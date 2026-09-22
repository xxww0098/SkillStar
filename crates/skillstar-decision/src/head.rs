//! Permutation-equivariant candidate head.
//!
//! Per question: candidate vectors `[C, hidden]` → `proj_in` to `set_dim` →
//! a two-layer transformer **without positional embeddings** (so the answer
//! cannot depend on the order the caller listed the options in) → `proj_out`
//! residual back into the trunk width → an RMSNorm/SiLU MLP that emits one
//! scalar logit per candidate. The per-question softmax is applied by the
//! engine, after calibration temperatures.

use candle_core::{Module, Tensor, D};
use candle_nn::{linear, Linear, VarBuilder};

use crate::error::Result;
use crate::ops::{layer_norm, rms_norm, softmax_last_dim};

/// `nn.LayerNorm` epsilon — PyTorch's default, and what the set encoder uses.
const SET_LAYER_NORM_EPS: f64 = 1e-5;
/// `RMSNorm` epsilon of the scalar scorer (the module default, not the trunk's).
const SCORER_NORM_EPS: f64 = 1e-6;

struct SetLayer {
    norm1_weight: Tensor,
    norm1_bias: Tensor,
    in_proj_weight: Tensor,
    in_proj_bias: Tensor,
    out_proj: Linear,
    norm2_weight: Tensor,
    norm2_bias: Tensor,
    linear1: Linear,
    linear2: Linear,
    heads: usize,
    head_dim: usize,
}

impl SetLayer {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let candidates = x.dim(0)?;
        let width = x.dim(D::Minus1)?;

        // Pre-norm attention (`norm_first=True`).
        let normed = layer_norm(
            x,
            &self.norm1_weight,
            &self.norm1_bias,
            SET_LAYER_NORM_EPS,
        )?;
        let packed = normed
            .matmul(&self.in_proj_weight.t()?)?
            .broadcast_add(&self.in_proj_bias)?;
        // `.contiguous()` after the transpose is not cosmetic: candle's Metal
        // GEMM rejects the strided views a transpose leaves behind, and the
        // failure is a runtime error, not a compile error.
        let query = packed
            .narrow(1, 0, width)?
            .contiguous()?
            .reshape((candidates, self.heads, self.head_dim))?
            .transpose(0, 1)?
            .contiguous()?;
        let key = packed
            .narrow(1, width, width)?
            .contiguous()?
            .reshape((candidates, self.heads, self.head_dim))?
            .transpose(0, 1)?
            .contiguous()?;
        let value = packed
            .narrow(1, 2 * width, width)?
            .contiguous()?
            .reshape((candidates, self.heads, self.head_dim))?
            .transpose(0, 1)?
            .contiguous()?;

        let scale = 1.0 / (self.head_dim as f64).sqrt();
        let scores = query
            .matmul(&key.transpose(1, 2)?.contiguous()?)?
            .affine(scale, 0.0)?;
        let probs = softmax_last_dim(&scores)?;
        let context = probs
            .matmul(&value)?
            .transpose(0, 1)?
            .contiguous()?
            .reshape((candidates, width))?;

        let x = (x + self.out_proj.forward(&context)?)?;

        // Pre-norm feed-forward, GELU (erf form, PyTorch's default).
        let normed = layer_norm(
            &x,
            &self.norm2_weight,
            &self.norm2_bias,
            SET_LAYER_NORM_EPS,
        )?;
        let hidden = self.linear1.forward(&normed)?.gelu_erf()?;
        Ok((&x + self.linear2.forward(&hidden)?)?)
    }
}

/// Scores candidate vectors for a single question.
pub struct CandidateHead {
    proj_in: Linear,
    set_layers: Vec<SetLayer>,
    proj_out: Linear,
    scorer_norm: Tensor,
    scorer_fc1: Linear,
    scorer_fc2: Linear,
}

impl CandidateHead {
    /// Load the head tensors (`proj_in` / `set_encoder` / `proj_out` /
    /// `scorer`) from the checkpoint root.
    pub fn load(
        vb: VarBuilder,
        hidden_size: usize,
        set_dim: usize,
        set_layers: usize,
        set_heads: usize,
    ) -> Result<Self> {
        let proj_in = linear(hidden_size, set_dim, vb.pp("proj_in"))?;
        let proj_out = linear(set_dim, hidden_size, vb.pp("proj_out"))?;
        let scorer_norm = vb.pp("scorer").pp("norm").get(hidden_size, "weight")?;
        let scorer_fc1 = linear(
            hidden_size,
            set_dim,
            vb.pp("scorer").pp("fc1"),
        )?;
        let scorer_fc2 = linear(set_dim, 1, vb.pp("scorer").pp("fc2"))?;

        let head_dim = set_dim / set_heads;
        let mut layers = Vec::with_capacity(set_layers);
        for index in 0..set_layers {
            let layer = vb.pp("set_encoder").pp("encoder").pp(format!("layers.{index}"));
            let attention = layer.pp("self_attn");
            layers.push(SetLayer {
                norm1_weight: layer.get(set_dim, "norm1.weight")?,
                norm1_bias: layer.get(set_dim, "norm1.bias")?,
                in_proj_weight: attention.get((3 * set_dim, set_dim), "in_proj_weight")?,
                in_proj_bias: attention.get(3 * set_dim, "in_proj_bias")?,
                out_proj: linear(set_dim, set_dim, attention.pp("out_proj"))?,
                norm2_weight: layer.get(set_dim, "norm2.weight")?,
                norm2_bias: layer.get(set_dim, "norm2.bias")?,
                linear1: linear(set_dim, 4 * set_dim, layer.pp("linear1"))?,
                linear2: linear(4 * set_dim, set_dim, layer.pp("linear2"))?,
                heads: set_heads,
                head_dim,
            });
        }

        Ok(Self {
            proj_in,
            set_layers: layers,
            proj_out,
            scorer_norm,
            scorer_fc1,
            scorer_fc2,
        })
    }

    /// One scalar logit per candidate, in the order the candidates were given.
    pub fn score(&self, candidates: &Tensor) -> Result<Tensor> {
        let mut x = self.proj_in.forward(candidates)?;
        for layer in &self.set_layers {
            x = layer.forward(&x)?;
        }
        let residual = (candidates + self.proj_out.forward(&x)?)?;
        let normed = rms_norm(&residual, &self.scorer_norm, SCORER_NORM_EPS)?;
        let hidden = self.scorer_fc1.forward(&normed)?.silu()?;
        Ok(self.scorer_fc2.forward(&hidden)?.reshape(candidates.dim(0)?)?)
    }
}

