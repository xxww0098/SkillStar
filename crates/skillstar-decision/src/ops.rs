//! Numerical primitives expressed with portable candle ops.
//!
//! candle 0.11's Metal backend does not implement everything candle-nn offers:
//! `ops::softmax_last_dim` is a `CustomOp` with a CPU kernel only, and
//! `rotary_emb::rope` has no Metal kernel at all. Both fail at *runtime* with
//! "no metal implementation for …", so a model that merely compiles is not a
//! model that runs on the GPU. The helpers here compute the same functions
//! from ops every backend implements, which keeps one code path for CPU and
//! Metal instead of a per-op CPU fallback that would silently serialize the
//! whole forward pass.

use candle_core::{DType, Tensor, D};

use crate::error::Result;

/// Softmax over the last dimension.
///
/// The max shift is not decoration: logits arrive masked with `-inf` and a
/// plain `exp` would overflow on any value above ~88.
pub(crate) fn softmax_last_dim(x: &Tensor) -> Result<Tensor> {
    let max = x.max_keepdim(D::Minus1)?;
    let shifted = x.broadcast_sub(&max)?;
    let exponent = shifted.exp()?;
    let total = exponent.sum_keepdim(D::Minus1)?;
    Ok(exponent.broadcast_div(&total)?)
}

/// RMSNorm in f32, then back to the input dtype — the numerics Qwen3 ships.
pub(crate) fn rms_norm(x: &Tensor, weight: &Tensor, eps: f64) -> Result<Tensor> {
    let dtype = x.dtype();
    let width = x.dim(D::Minus1)?;
    let x = x.to_dtype(DType::F32)?;
    let variance = (x.sqr()?.sum_keepdim(D::Minus1)? / width as f64)?;
    let normed = x.broadcast_div(&variance.affine(1.0, eps)?.sqrt()?)?;
    let weight = weight.to_dtype(DType::F32)?;
    Ok(normed.broadcast_mul(&weight)?.to_dtype(dtype)?)
}

/// `nn.LayerNorm` over the last dimension, computed in f32.
pub(crate) fn layer_norm(x: &Tensor, weight: &Tensor, bias: &Tensor, eps: f64) -> Result<Tensor> {
    let dtype = x.dtype();
    let width = x.dim(D::Minus1)?;
    let x = x.to_dtype(DType::F32)?;
    let mean = (x.sum_keepdim(D::Minus1)? / width as f64)?;
    let centered = x.broadcast_sub(&mean)?;
    let variance = (centered.sqr()?.sum_keepdim(D::Minus1)? / width as f64)?;
    let normed = centered.broadcast_div(&variance.affine(1.0, eps)?.sqrt()?)?;
    let weight = weight.to_dtype(DType::F32)?;
    let bias = bias.to_dtype(DType::F32)?;
    Ok(normed
        .broadcast_mul(&weight)?
        .broadcast_add(&bias)?
        .to_dtype(dtype)?)
}

/// Half-split RoPE over `x` shaped `[1, heads, seq, head_dim]`.
///
/// Same rotation HF applies: pair the first and second half of each head and
/// rotate with the precomputed cos/sin tables.
pub(crate) fn apply_rope(x: &Tensor, cos: &Tensor, sin: &Tensor) -> Result<Tensor> {
    let (_, _, _, head_dim) = x.dims4()?;
    let half = head_dim / 2;
    // Tables are `[seq, head_dim / 2]`; both halves need the same rotation.
    let cos = Tensor::cat(&[cos, cos], D::Minus1)?
        .unsqueeze(0)?
        .unsqueeze(0)?;
    let sin = Tensor::cat(&[sin, sin], D::Minus1)?
        .unsqueeze(0)?
        .unsqueeze(0)?;
    let first = x.narrow(D::Minus1, 0, half)?;
    let second = x.narrow(D::Minus1, half, half)?;
    let rotated = Tensor::cat(&[second.neg()?, first], D::Minus1)?;
    Ok((x.broadcast_mul(&cos)? + rotated.broadcast_mul(&sin)?)?)
}
