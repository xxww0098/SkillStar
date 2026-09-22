//! Qwen3-0.6B trunk with explicit KV state.
//!
//! The reference runtime encodes the shared `[STATE] … [QUESTION] …` prefix
//! once and then branches every candidate off that prefix, which is where its
//! ~2× wide-candidate speedup comes from. Candle's stock Qwen3 keeps its cache
//! private and only ever appends, so a branch could not be rewound to the
//! prefix. This module therefore owns the forward pass: the KV state is a
//! plain `Vec<Kv>` the caller passes back in, and a prefix is never mutated by
//! the branches read against it.
//!
//! Numerics follow `transformers`' Qwen3: RMSNorm computed in f32 with the
//! configured eps, per-head q/k RMSNorm, half-split RoPE, GQA expansion by
//! repeating each KV head `num_attention_heads / num_key_value_heads` times.

use candle_core::{DType, Device, Module, Tensor};
use candle_nn::{linear_no_bias, Linear, VarBuilder};
use serde::Deserialize;

use crate::error::{DecisionError, Result};
use crate::ops::{apply_rope, rms_norm, softmax_last_dim};

/// Model geometry, read from the checkpoint's `config.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct Qwen3Config {
    /// Residual stream width.
    pub hidden_size: usize,
    /// SwiGLU inner width.
    pub intermediate_size: usize,
    /// Decoder layer count.
    pub num_hidden_layers: usize,
    /// Query heads.
    pub num_attention_heads: usize,
    /// Key/value heads (GQA).
    pub num_key_value_heads: usize,
    /// Per-head width.
    pub head_dim: usize,
    /// RMSNorm epsilon.
    pub rms_norm_eps: f64,
    /// RoPE base.
    pub rope_theta: f64,
    /// Embedding rows.
    pub vocab_size: usize,
    /// RoPE table length.
    pub max_position_embeddings: usize,
}

/// Key/value state for one layer, shaped `[1, kv_heads, tokens, head_dim]`.
#[derive(Debug, Clone)]
pub struct Kv {
    /// Keys.
    pub k: Tensor,
    /// Values.
    pub v: Tensor,
}

/// Prefix state: one entry per decoder layer.
pub type PrefixKv = Vec<Kv>;

struct Attention {
    q_proj: Linear,
    k_proj: Linear,
    v_proj: Linear,
    o_proj: Linear,
    q_norm: Tensor,
    k_norm: Tensor,
}

struct Mlp {
    gate_proj: Linear,
    up_proj: Linear,
    down_proj: Linear,
}

struct Layer {
    input_ln: Tensor,
    post_attention_ln: Tensor,
    attention: Attention,
    mlp: Mlp,
}

/// The trunk. Holds weights for the lifetime of the engine.
pub struct Backbone {
    embed_tokens: Tensor,
    layers: Vec<Layer>,
    norm: Tensor,
    cos: Tensor,
    sin: Tensor,
    config: Qwen3Config,
    num_kv_groups: usize,
    dtype: DType,
    device: Device,
}

impl Backbone {
    /// Load the trunk from `path_encoder.backbone.*` tensors in `vb`.
    pub fn load(config: Qwen3Config, vb: VarBuilder) -> Result<Self> {
        let dtype = vb.dtype();
        let device = vb.device().clone();
        let vb = vb.pp("path_encoder").pp("backbone");

        let embed_tokens = vb.get((config.vocab_size, config.hidden_size), "embed_tokens.weight")?;
        let norm = vb.get(config.hidden_size, "norm.weight")?;

        let mut layers = Vec::with_capacity(config.num_hidden_layers);
        for index in 0..config.num_hidden_layers {
            let vb = vb.pp(format!("layers.{index}"));
            let attention = Attention {
                q_proj: linear_no_bias(
                    config.hidden_size,
                    config.num_attention_heads * config.head_dim,
                    vb.pp("self_attn").pp("q_proj"),
                )?,
                k_proj: linear_no_bias(
                    config.hidden_size,
                    config.num_key_value_heads * config.head_dim,
                    vb.pp("self_attn").pp("k_proj"),
                )?,
                v_proj: linear_no_bias(
                    config.hidden_size,
                    config.num_key_value_heads * config.head_dim,
                    vb.pp("self_attn").pp("v_proj"),
                )?,
                o_proj: linear_no_bias(
                    config.num_attention_heads * config.head_dim,
                    config.hidden_size,
                    vb.pp("self_attn").pp("o_proj"),
                )?,
                q_norm: vb.pp("self_attn").pp("q_norm").get(config.head_dim, "weight")?,
                k_norm: vb.pp("self_attn").pp("k_norm").get(config.head_dim, "weight")?,
            };
            let mlp = Mlp {
                gate_proj: linear_no_bias(
                    config.hidden_size,
                    config.intermediate_size,
                    vb.pp("mlp").pp("gate_proj"),
                )?,
                up_proj: linear_no_bias(
                    config.hidden_size,
                    config.intermediate_size,
                    vb.pp("mlp").pp("up_proj"),
                )?,
                down_proj: linear_no_bias(
                    config.intermediate_size,
                    config.hidden_size,
                    vb.pp("mlp").pp("down_proj"),
                )?,
            };
            layers.push(Layer {
                input_ln: vb.get(config.hidden_size, "input_layernorm.weight")?,
                post_attention_ln: vb
                    .get(config.hidden_size, "post_attention_layernorm.weight")?,
                attention,
                mlp,
            });
        }

        let (cos, sin) = rotary_tables(&config, dtype, &device)?;

        Ok(Self {
            embed_tokens,
            layers,
            norm,
            cos,
            sin,
            num_kv_groups: config.num_attention_heads / config.num_key_value_heads,
            dtype,
            device,
            config,
        })
    }

    /// Encode `tokens` at absolute positions `offset..offset + tokens.len()`.
    ///
    /// When `prefix` is given, every layer attends to it as well; the prefix
    /// tensors are read, never written. The returned KV belongs to this call
    /// only, and its second element is the hidden state of the final token
    /// after the model's closing RMSNorm — which is exactly the vector the
    /// candidate head scores.
    pub fn forward(
        &self,
        tokens: &[u32],
        offset: usize,
        prefix: Option<&PrefixKv>,
    ) -> Result<(Tensor, PrefixKv)> {
        if tokens.is_empty() {
            return Err(DecisionError::Inference(
                "cannot encode an empty token path".to_string(),
            ));
        }
        if let Some(prefix) = prefix
            && prefix.len() != self.layers.len()
        {
            return Err(DecisionError::Inference(format!(
                "prefix has {} layers but the trunk has {}",
                prefix.len(),
                self.layers.len()
            )));
        }
        let seq_len = tokens.len();
        let ids = Tensor::from_vec(tokens.to_vec(), seq_len, &self.device)?;
        let mut hidden = self.embed_tokens.index_select(&ids, 0)?;

        let scale = 1.0 / (self.config.head_dim as f64).sqrt();
        let mask = causal_mask(seq_len, offset, self.dtype, &self.device)?;

        let mut kv_out = Vec::with_capacity(self.layers.len());
        for (index, layer) in self.layers.iter().enumerate() {
            let residual = hidden.clone();
            let normed = rms_norm(&hidden, &layer.input_ln, self.config.rms_norm_eps)?;

            let q = layer
                .attention
                .q_proj
                .forward(&normed)?
                .reshape((seq_len, self.config.num_attention_heads, self.config.head_dim))?
                .transpose(0, 1)?
                .unsqueeze(0)?;
            let k = layer
                .attention
                .k_proj
                .forward(&normed)?
                .reshape((seq_len, self.config.num_key_value_heads, self.config.head_dim))?
                .transpose(0, 1)?
                .unsqueeze(0)?;
            let v = layer
                .attention
                .v_proj
                .forward(&normed)?
                .reshape((seq_len, self.config.num_key_value_heads, self.config.head_dim))?
                .transpose(0, 1)?
                .unsqueeze(0)?;

            let q = rms_norm(&q, &layer.attention.q_norm, self.config.rms_norm_eps)?;
            let k = rms_norm(&k, &layer.attention.k_norm, self.config.rms_norm_eps)?;

            let cos = self.cos.narrow(0, offset, seq_len)?;
            let sin = self.sin.narrow(0, offset, seq_len)?;
            let q = apply_rope(&q, &cos, &sin)?;
            let k = apply_rope(&k, &cos, &sin)?;

            // Branch against the shared prefix without copying it: keys and
            // values of this call are concatenated for attention only, and the
            // un-concatenated `k`/`v` are what the caller keeps.
            let (keys, values) = match prefix {
                Some(prefix) => (
                    Tensor::cat(&[&prefix[index].k, &k], 2)?,
                    Tensor::cat(&[&prefix[index].v, &v], 2)?,
                ),
                None => (k.clone(), v.clone()),
            };
            kv_out.push(Kv { k, v });

            let keys = repeat_kv(&keys, self.num_kv_groups)?;
            let repeated_values = repeat_kv(&values, self.num_kv_groups)?;
            let scores = (q
                .matmul(&keys.transpose(2, 3)?.contiguous()?)?
                .affine(scale, 0.0)?)
            .broadcast_add(&mask)?;
            let probs = softmax_last_dim(&scores)?;
            let context = probs
                .matmul(&repeated_values)?
                .transpose(1, 2)?
                .contiguous()?
                .reshape((seq_len, self.config.num_attention_heads * self.config.head_dim))?;
            hidden = (residual + layer.attention.o_proj.forward(&context)?)?;

            let residual = hidden.clone();
            let normed = rms_norm(
                &hidden,
                &layer.post_attention_ln,
                self.config.rms_norm_eps,
            )?;
            // SwiGLU: the gate is SiLU in Qwen3 (`hidden_act: "silu"`), not the
            // GELU the candidate set encoder uses.
            let gate = layer.mlp.gate_proj.forward(&normed)?.silu()?;
            let up = layer.mlp.up_proj.forward(&normed)?;
            hidden = (residual + layer.mlp.down_proj.forward(&(gate * up)?)?)?;
        }

        let last = hidden.narrow(0, seq_len - 1, 1)?;
        let last = rms_norm(&last, &self.norm, self.config.rms_norm_eps)?;
        Ok((last.reshape(self.config.hidden_size)?, kv_out))
    }
}

/// RoPE tables: `[max_position, head_dim / 2]`, f32 while computed.
fn rotary_tables(config: &Qwen3Config, dtype: DType, device: &Device) -> Result<(Tensor, Tensor)> {
    let dim = config.head_dim;
    let inv_freq: Vec<f32> = (0..dim)
        .step_by(2)
        .map(|index| 1f32 / config.rope_theta.powf(index as f64 / dim as f64) as f32)
        .collect();
    let inv_freq_len = inv_freq.len();
    let inv_freq = Tensor::from_vec(inv_freq, (1, inv_freq_len), device)?.to_dtype(DType::F32)?;
    let positions = Tensor::arange(0u32, config.max_position_embeddings as u32, device)?
        .to_dtype(DType::F32)?
        .reshape((config.max_position_embeddings, 1))?;
    let freqs = positions.matmul(&inv_freq)?;
    Ok((
        freqs.cos()?.to_dtype(dtype)?,
        freqs.sin()?.to_dtype(dtype)?,
    ))
}

/// Additive causal mask for `seq_len` queries whose keys end at `offset + seq_len`.
fn causal_mask(seq_len: usize, offset: usize, dtype: DType, device: &Device) -> Result<Tensor> {
    let keys = offset + seq_len;
    let mut values = vec![0f32; seq_len * keys];
    for query in 0..seq_len {
        let first_masked = offset + query + 1;
        for key in first_masked..keys {
            values[query * keys + key] = f32::NEG_INFINITY;
        }
    }
    Ok(Tensor::from_vec(values, (1, 1, seq_len, keys), device)?.to_dtype(dtype)?)
}

/// Expand each KV head into `groups` query heads, as `repeat_kv` does in HF.
fn repeat_kv(x: &Tensor, groups: usize) -> Result<Tensor> {
    if groups == 1 {
        return Ok(x.clone());
    }
    let (batch, heads, seq_len, head_dim) = x.dims4()?;
    Ok(x.unsqueeze(2)?
        .broadcast_as((batch, heads, groups, seq_len, head_dim))?
        .contiguous()?
        .reshape((batch, heads * groups, seq_len, head_dim))?)
}

