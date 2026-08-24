//! NAFNet-SIDD native denoise provider and shared-graph adjustment stage.
//! Model pixels never cross the Tauri JSON boundary: native code owns inference and residuals.

use ndarray::Array4;
use ort::{
    execution_providers::{CPUExecutionProvider, DirectMLExecutionProvider},
    session::Session,
    value::TensorRef,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use starroom_detail::LinearImage;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use thiserror::Error;

pub const MODEL_ID: &str = "nafnet-sidd-width32";
pub const MODEL_VERSION: &str = "upstream-2b4af71-opset20-tile512-fp32";
pub const MODEL_SHA256: &str = "0e522d6de607958c283c834e6459a37b2fccbf5c19223a289393b4745f0cb633";
pub const TILE_EDGE: usize = 512;
pub const TILE_OVERLAP: usize = 64;
pub const TILE_STRIDE: usize = TILE_EDGE - TILE_OVERLAP;
/// Conservative process budget for source/domain/accumulator/residual buffers. ORT owns its
/// tile tensor separately, so the estimate intentionally includes one extra 512 RGB tile pair.
pub const MAX_WORKING_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionProvider {
    DirectMl,
    Cpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiDenoiseParameters {
    pub enabled: bool,
    /// Strength of the cached NAFNet residual, 0..1.
    pub amount: f32,
    /// Restores luminance high-frequency detail, 0..1.
    pub detail: f32,
    /// Chroma residual strength, 0..1.
    pub color_noise: f32,
    /// Reduces denoise residual over a supplied portrait skin mask, 0..1.
    pub preserve_skin: f32,
}

impl Default for AiDenoiseParameters {
    fn default() -> Self {
        Self {
            enabled: false,
            amount: 0.5,
            detail: 0.5,
            color_noise: 0.5,
            preserve_skin: 0.5,
        }
    }
}

impl AiDenoiseParameters {
    pub fn validate(self) -> Result<Self, AiDenoiseError> {
        if ![
            self.amount,
            self.detail,
            self.color_noise,
            self.preserve_skin,
        ]
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            return Err(AiDenoiseError::InvalidParameters);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AiDenoiseResidual {
    pub width: usize,
    pub height: usize,
    /// Denoised minus source, interleaved Linear Rec.2020 D65.
    pub values: Vec<f32>,
    pub model_hash: String,
    pub source_identity: String,
    pub inference_cache_key: String,
    pub execution_provider: ExecutionProvider,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelDomain {
    pub black: f32,
    pub scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Error)]
pub enum AiDenoiseError {
    #[error("NAFNet model is missing: {0}")]
    ModelMissing(PathBuf),
    #[error("NAFNet model hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("NAFNet runtime initialization failed: {0}")]
    RuntimeUnavailable(String),
    #[error("deterministic NAFNet ONNX export is invalid: {0}")]
    ExportInvalid(String),
    #[error("DirectML was requested but unavailable: {0}")]
    DirectMlUnavailable(String),
    #[error("NAFNet inference failed: {0}")]
    InferenceFailed(String),
    #[error("NAFNet tensor is malformed: {0}")]
    InvalidTensor(String),
    #[error("NAFNet output is malformed: {0}")]
    InvalidOutput(String),
    #[error("NAFNet inference exhausted available memory")]
    OutOfMemory,
    #[error("AI denoise parameters are outside finite supported ranges")]
    InvalidParameters,
    #[error("AI denoise residual does not match the working image")]
    ResidualMismatch,
    #[error("AI denoise was cancelled")]
    Cancelled,
}

pub fn directml_failure_allows_cpu_fallback(error: &AiDenoiseError) -> bool {
    matches!(
        error,
        AiDenoiseError::DirectMlUnavailable(_)
            | AiDenoiseError::RuntimeUnavailable(_)
            | AiDenoiseError::InferenceFailed(_)
            | AiDenoiseError::OutOfMemory
    )
}

pub fn inference_cache_key(source_identity: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!("{source_identity}:m21:{MODEL_VERSION}:{MODEL_SHA256}:domain-v1").as_bytes()
        )
    )
}

pub fn adjustment_cache_key(
    inference_key: &str,
    parameters: AiDenoiseParameters,
) -> Result<String, AiDenoiseError> {
    let p = parameters.validate()?;
    Ok(format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{inference_key}:{:.6}:{:.6}:{:.6}:{:.6}",
                p.amount, p.detail, p.color_noise, p.preserve_skin
            )
            .as_bytes()
        )
    ))
}

fn quantile(mut values: Vec<f32>, q: f32) -> f32 {
    values.retain(|v| v.is_finite());
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    values[((values.len() - 1) as f32 * q.clamp(0.0, 1.0)).round() as usize]
}

fn rec2020_to_linear_srgb([r, g, b]: [f32; 3]) -> [f32; 3] {
    [
        1.660_491 * r - 0.587_641 * g - 0.072_850 * b,
        -0.124_550 * r + 1.132_9 * g - 0.008_349 * b,
        -0.018_151 * r - 0.100_579 * g + 1.118_73 * b,
    ]
}
fn linear_srgb_to_rec2020([r, g, b]: [f32; 3]) -> [f32; 3] {
    [
        0.627_404 * r + 0.329_283 * g + 0.043_313 * b,
        0.069_097 * r + 0.919_540 * g + 0.011_362 * b,
        0.016_391 * r + 0.088_013 * g + 0.895_595 * b,
    ]
}
fn srgb_encode(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}
fn srgb_decode(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn compress(v: f32) -> f32 {
    v / (1.0 + v.abs())
}
fn expand(v: f32) -> f32 {
    v / (1.0 - v.abs()).max(1.0e-6)
}

pub fn encode_model_domain(image: &LinearImage) -> Result<(Vec<f32>, ModelDomain), AiDenoiseError> {
    let luma = image
        .data
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| (0.2627 * p[0] + 0.6780 * p[1] + 0.0593 * p[2]).max(0.0))
        .collect::<Vec<_>>();
    let black = quantile(luma.clone(), 0.01);
    let white = quantile(luma, 0.99);
    let scale = (white - black).max(1.0e-4);
    let mut chw = vec![0.0; image.width * image.height * 3];
    let plane = image.width * image.height;
    for (i, p) in image.data.as_chunks::<3>().0.iter().enumerate() {
        let s = rec2020_to_linear_srgb([p[0], p[1], p[2]]);
        for c in 0..3 {
            chw[c * plane + i] = srgb_encode(compress((s[c] - black) / scale)).clamp(0.0, 1.0);
        }
    }
    if chw.iter().any(|v| !v.is_finite()) {
        return Err(AiDenoiseError::InvalidTensor(
            "non-finite encoded input".into(),
        ));
    }
    Ok((chw, ModelDomain { black, scale }))
}

pub fn decode_model_domain(
    chw: &[f32],
    width: usize,
    height: usize,
    domain: ModelDomain,
) -> Result<LinearImage, AiDenoiseError> {
    let plane = width * height;
    if chw.len() != plane * 3 || chw.iter().any(|v| !v.is_finite()) {
        return Err(AiDenoiseError::InvalidTensor("invalid model output".into()));
    }
    let mut data = vec![0.0; plane * 3];
    for i in 0..plane {
        let mut s = [0.0; 3];
        for c in 0..3 {
            s[c] = expand(srgb_decode(chw[c * plane + i].clamp(0.0, 1.0))) * domain.scale
                + domain.black;
        }
        let r = linear_srgb_to_rec2020(s);
        data[i * 3..i * 3 + 3].copy_from_slice(&r);
    }
    LinearImage::new(width, height, data)
        .map_err(|_| AiDenoiseError::InvalidTensor("decoded output is invalid".into()))
}

pub fn tile_plan(width: usize, height: usize) -> Vec<Tile> {
    fn origins(length: usize) -> Vec<usize> {
        if length <= TILE_EDGE {
            return vec![0];
        }
        let mut values = (0..)
            .map(|i| i * TILE_STRIDE)
            .take_while(|x| x + TILE_EDGE < length)
            .collect::<Vec<_>>();
        values.push(length - TILE_EDGE);
        values.dedup();
        values
    }
    origins(height)
        .into_iter()
        .flat_map(|y| {
            origins(width).into_iter().map(move |x| Tile {
                x,
                y,
                width: (width - x).min(TILE_EDGE),
                height: (height - y).min(TILE_EDGE),
            })
        })
        .collect()
}

/// Stable visible-first ordering used by interactive preview. The full plan is unchanged, so
/// export coverage and overlap are identical; only job priority differs.
pub fn tile_plan_prioritized(
    width: usize,
    height: usize,
    viewport_x: f32,
    viewport_y: f32,
) -> Vec<Tile> {
    let target_x = viewport_x.clamp(0.0, 1.0) * width as f32;
    let target_y = viewport_y.clamp(0.0, 1.0) * height as f32;
    let mut tiles = tile_plan(width, height);
    tiles.sort_by(|left, right| {
        let distance = |tile: &Tile| {
            let x = tile.x as f32 + tile.width as f32 * 0.5 - target_x;
            let y = tile.y as f32 + tile.height as f32 * 0.5 - target_y;
            x * x + y * y
        };
        distance(left).total_cmp(&distance(right))
    });
    tiles
}

fn raised_cosine(position: usize, length: usize, at_start: bool, at_end: bool) -> f32 {
    let edge = TILE_OVERLAP.min(length / 2);
    if edge == 0 {
        return 1.0;
    }
    if !at_start && position < edge {
        return 0.5 - 0.5 * (std::f32::consts::PI * position as f32 / edge as f32).cos();
    }
    if !at_end && position >= length - edge {
        let d = (length - 1 - position) as f32;
        return 0.5 - 0.5 * (std::f32::consts::PI * d / edge as f32).cos();
    }
    1.0
}

pub trait TileInferencer {
    fn infer_tile(&mut self, chw_512: &[f32]) -> Result<Vec<f32>, AiDenoiseError>;
}

pub fn estimated_working_bytes(width: usize, height: usize) -> Result<u64, AiDenoiseError> {
    let pixels = (width as u64)
        .checked_mul(height as u64)
        .ok_or(AiDenoiseError::OutOfMemory)?;
    let full_frame_buffers = pixels
        .checked_mul(3 * std::mem::size_of::<f32>() as u64)
        .and_then(|bytes| bytes.checked_mul(5))
        .ok_or(AiDenoiseError::OutOfMemory)?;
    let tile_buffers = (TILE_EDGE * TILE_EDGE * 3 * std::mem::size_of::<f32>() * 2) as u64;
    full_frame_buffers
        .checked_add(tile_buffers)
        .ok_or(AiDenoiseError::OutOfMemory)
}

pub fn validate_memory_budget(width: usize, height: usize) -> Result<(), AiDenoiseError> {
    if estimated_working_bytes(width, height)? > MAX_WORKING_BYTES {
        return Err(AiDenoiseError::OutOfMemory);
    }
    Ok(())
}

pub fn infer_tiled<I: TileInferencer>(
    provider: &mut I,
    image: &LinearImage,
    source_identity: &str,
    cancellation: &AtomicBool,
    execution_provider: ExecutionProvider,
) -> Result<AiDenoiseResidual, AiDenoiseError> {
    validate_memory_budget(image.width, image.height)?;
    let (encoded, domain) = encode_model_domain(image)?;
    let plane = image.width * image.height;
    let mut accum = vec![0.0; plane * 3];
    let mut weights = vec![0.0; plane];
    for tile in tile_plan_prioritized(image.width, image.height, 0.5, 0.5) {
        if cancellation.load(Ordering::Relaxed) {
            return Err(AiDenoiseError::Cancelled);
        }
        let mut input = vec![0.0; 3 * TILE_EDGE * TILE_EDGE];
        for c in 0..3 {
            for ty in 0..TILE_EDGE {
                for tx in 0..TILE_EDGE {
                    let sx = (tile.x + tx.min(tile.width - 1)).min(image.width - 1);
                    let sy = (tile.y + ty.min(tile.height - 1)).min(image.height - 1);
                    input[c * TILE_EDGE * TILE_EDGE + ty * TILE_EDGE + tx] =
                        encoded[c * plane + sy * image.width + sx];
                }
            }
        }
        let output = provider.infer_tile(&input)?;
        if output.len() != input.len() || output.iter().any(|v| !v.is_finite()) {
            return Err(AiDenoiseError::InvalidOutput(
                "expected finite [1,3,512,512] output".into(),
            ));
        }
        for ty in 0..tile.height {
            for tx in 0..tile.width {
                let dst = (tile.y + ty) * image.width + tile.x + tx;
                let w = raised_cosine(
                    tx,
                    tile.width,
                    tile.x == 0,
                    tile.x + tile.width == image.width,
                ) * raised_cosine(
                    ty,
                    tile.height,
                    tile.y == 0,
                    tile.y + tile.height == image.height,
                );
                weights[dst] += w;
                for c in 0..3 {
                    accum[c * plane + dst] +=
                        output[c * TILE_EDGE * TILE_EDGE + ty * TILE_EDGE + tx] * w;
                }
            }
        }
    }
    for i in 0..plane {
        let w = weights[i].max(1.0e-6);
        for c in 0..3 {
            accum[c * plane + i] /= w;
        }
    }
    let denoised = decode_model_domain(&accum, image.width, image.height, domain)?;
    let values = denoised
        .data
        .iter()
        .zip(&image.data)
        .map(|(a, b)| a - b)
        .collect();
    Ok(AiDenoiseResidual {
        width: image.width,
        height: image.height,
        values,
        model_hash: MODEL_SHA256.into(),
        source_identity: source_identity.into(),
        inference_cache_key: inference_cache_key(source_identity),
        execution_provider,
    })
}

pub fn apply_residual(
    image: &LinearImage,
    residual: &AiDenoiseResidual,
    parameters: AiDenoiseParameters,
    skin_mask: Option<&[f32]>,
) -> Result<LinearImage, AiDenoiseError> {
    let p = parameters.validate()?;
    if residual.width != image.width
        || residual.height != image.height
        || residual.values.len() != image.data.len()
        || skin_mask.is_some_and(|m| m.len() != image.width * image.height)
    {
        return Err(AiDenoiseError::ResidualMismatch);
    }
    if !p.enabled || p.amount == 0.0 {
        return Ok(image.clone());
    }
    let mut data = Vec::with_capacity(image.data.len());
    for (i, (src, res)) in image
        .data
        .as_chunks::<3>()
        .0
        .iter()
        .zip(residual.values.as_chunks::<3>().0.iter())
        .enumerate()
    {
        let skin = skin_mask.map_or(0.0, |m| m[i].clamp(0.0, 1.0));
        let strength = p.amount * (1.0 - p.preserve_skin * skin);
        let luma_res = 0.2627 * res[0] + 0.6780 * res[1] + 0.0593 * res[2];
        for c in 0..3 {
            let chroma = res[c] - luma_res;
            let adjusted = luma_res * (1.0 - p.detail) + chroma * p.color_noise;
            data.push(src[c] + strength * adjusted);
        }
    }
    LinearImage::new(image.width, image.height, data)
        .map_err(|_| AiDenoiseError::InvalidTensor("non-finite adjusted image".into()))
}

pub struct NafNetOnnxProvider {
    session: Session,
    pub execution_provider: ExecutionProvider,
}
impl NafNetOnnxProvider {
    pub fn initialize(
        path: impl AsRef<Path>,
        requested: ExecutionProvider,
    ) -> Result<Self, AiDenoiseError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(AiDenoiseError::ModelMissing(path.into()));
        }
        let actual = format!(
            "{:x}",
            Sha256::digest(
                std::fs::read(path)
                    .map_err(|e| AiDenoiseError::RuntimeUnavailable(e.to_string()))?
            )
        );
        if actual != MODEL_SHA256 {
            return Err(AiDenoiseError::HashMismatch {
                expected: MODEL_SHA256.into(),
                actual,
            });
        }
        let ep = match requested {
            ExecutionProvider::Cpu => CPUExecutionProvider::default().build(),
            ExecutionProvider::DirectMl => DirectMLExecutionProvider::default().build(),
        };
        let session = Session::builder()
            .map_err(|e| AiDenoiseError::RuntimeUnavailable(e.to_string()))?
            .with_execution_providers([ep])
            .map_err(|e| match requested {
                ExecutionProvider::DirectMl => AiDenoiseError::DirectMlUnavailable(e.to_string()),
                ExecutionProvider::Cpu => AiDenoiseError::RuntimeUnavailable(e.to_string()),
            })?
            .commit_from_file(path)
            .map_err(|e| {
                let detail = e.to_string();
                if requested == ExecutionProvider::DirectMl {
                    AiDenoiseError::DirectMlUnavailable(detail)
                } else if detail.to_ascii_lowercase().contains("invalid")
                    || detail.to_ascii_lowercase().contains("protobuf")
                {
                    AiDenoiseError::ExportInvalid(detail)
                } else {
                    AiDenoiseError::RuntimeUnavailable(detail)
                }
            })?;
        Ok(Self {
            session,
            execution_provider: requested,
        })
    }
}
impl TileInferencer for NafNetOnnxProvider {
    fn infer_tile(&mut self, chw: &[f32]) -> Result<Vec<f32>, AiDenoiseError> {
        let tensor = Array4::from_shape_vec((1, 3, TILE_EDGE, TILE_EDGE), chw.to_vec())
            .map_err(|e| AiDenoiseError::InvalidTensor(e.to_string()))?;
        let outputs = self
            .session
            .run(ort::inputs![
                TensorRef::from_array_view(&tensor)
                    .map_err(|e| AiDenoiseError::InvalidTensor(e.to_string()))?
            ])
            .map_err(|e| {
                let detail = e.to_string();
                if detail.to_ascii_lowercase().contains("memory") {
                    AiDenoiseError::OutOfMemory
                } else {
                    AiDenoiseError::InferenceFailed(detail)
                }
            })?;
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| AiDenoiseError::InvalidTensor(e.to_string()))?;
        if shape.len() != 4
            || shape[0] != 1
            || shape[1] != 3
            || shape[2] != TILE_EDGE as i64
            || shape[3] != TILE_EDGE as i64
        {
            return Err(AiDenoiseError::InvalidOutput(format!(
                "unexpected output {shape:?}"
            )));
        }
        Ok(data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Identity;
    impl TileInferencer for Identity {
        fn infer_tile(&mut self, v: &[f32]) -> Result<Vec<f32>, AiDenoiseError> {
            Ok(v.to_vec())
        }
    }
    struct ConstantResidual(f32);
    impl TileInferencer for ConstantResidual {
        fn infer_tile(&mut self, values: &[f32]) -> Result<Vec<f32>, AiDenoiseError> {
            Ok(values
                .iter()
                .map(|value| (value + self.0).clamp(0.0, 1.0))
                .collect())
        }
    }
    fn image(w: usize, h: usize) -> LinearImage {
        LinearImage::new(
            w,
            h,
            (0..w * h)
                .flat_map(|i| {
                    let v = (i % 31) as f32 / 17.0;
                    [v, v * 0.8, v * 0.6]
                })
                .collect(),
        )
        .unwrap()
    }
    #[test]
    fn domain_round_trip_is_finite_and_hdr_safe() {
        let a = image(17, 13);
        let (e, d) = encode_model_domain(&a).unwrap();
        let b = decode_model_domain(&e, 17, 13, d).unwrap();
        assert!(b.data.iter().all(|v| v.is_finite()));
        assert!(
            b.data
                .iter()
                .zip(&a.data)
                .all(|(x, y)| (x - y).abs() < 2e-4)
        );
    }
    #[test]
    fn tile_plan_covers_small_portrait_landscape_and_large() {
        for (w, h) in [(64, 64), (300, 700), (900, 300), (1200, 900)] {
            let t = tile_plan(w, h);
            assert!(!t.is_empty());
            for y in 0..h {
                for x in 0..w {
                    assert!(
                        t.iter().any(|q| x >= q.x
                            && x < q.x + q.width
                            && y >= q.y
                            && y < q.y + q.height)
                    );
                }
            }
        }
    }
    #[test]
    fn visible_first_priority_starts_nearest_the_viewport() {
        let tiles = tile_plan_prioritized(1600, 1200, 0.5, 0.5);
        let first = tiles.first().unwrap();
        assert!(first.x > 0 && first.y > 0);
        let corner = tile_plan_prioritized(1600, 1200, 0.0, 0.0)[0];
        assert_eq!((corner.x, corner.y), (0, 0));
    }
    #[test]
    fn identity_inference_and_adjustment_are_stable() {
        let a = image(530, 521);
        let r = infer_tiled(
            &mut Identity,
            &a,
            "fixture",
            &AtomicBool::new(false),
            ExecutionProvider::Cpu,
        )
        .unwrap();
        let b = apply_residual(
            &a,
            &r,
            AiDenoiseParameters {
                enabled: true,
                amount: 1.0,
                ..Default::default()
            },
            None,
        )
        .unwrap();
        assert!(b.data.iter().zip(a.data).all(|(x, y)| (x - y).abs() < 3e-4));
    }
    #[test]
    fn inference_and_adjustment_cache_keys_are_separate() {
        let a = inference_cache_key("x");
        let p = adjustment_cache_key(&a, AiDenoiseParameters::default()).unwrap();
        assert_ne!(a, p);
        assert_eq!(a, inference_cache_key("x"));
    }
    #[test]
    fn cancellation_and_invalid_parameters_are_typed() {
        let a = image(2, 2);
        assert!(matches!(
            infer_tiled(
                &mut Identity,
                &a,
                "x",
                &AtomicBool::new(true),
                ExecutionProvider::Cpu
            ),
            Err(AiDenoiseError::Cancelled)
        ));
        assert!(matches!(
            AiDenoiseParameters {
                amount: f32::NAN,
                ..Default::default()
            }
            .validate(),
            Err(AiDenoiseError::InvalidParameters)
        ));
        assert_eq!(
            validate_memory_budget(usize::MAX, usize::MAX)
                .unwrap_err()
                .to_string(),
            AiDenoiseError::OutOfMemory.to_string()
        );
    }
    #[test]
    fn directml_fallback_policy_is_explicit_and_never_hides_invalid_models() {
        assert!(directml_failure_allows_cpu_fallback(
            &AiDenoiseError::DirectMlUnavailable("device".into())
        ));
        assert!(directml_failure_allows_cpu_fallback(
            &AiDenoiseError::InferenceFailed("provider".into())
        ));
        assert!(!directml_failure_allows_cpu_fallback(
            &AiDenoiseError::HashMismatch {
                expected: "a".into(),
                actual: "b".into(),
            }
        ));
        assert!(!directml_failure_allows_cpu_fallback(
            &AiDenoiseError::InvalidOutput("shape".into())
        ));
    }
    #[test]
    fn preserve_skin_reduces_residual() {
        let a = image(2, 1);
        let r = AiDenoiseResidual {
            width: 2,
            height: 1,
            values: vec![0.2; 6],
            model_hash: "m".into(),
            source_identity: "s".into(),
            inference_cache_key: "k".into(),
            execution_provider: ExecutionProvider::Cpu,
        };
        let p = AiDenoiseParameters {
            enabled: true,
            amount: 1.0,
            detail: 0.0,
            color_noise: 1.0,
            preserve_skin: 1.0,
        };
        let b = apply_residual(&a, &r, p, Some(&[1.0, 0.0])).unwrap();
        assert!((b.data[0] - a.data[0]).abs() < 1e-6);
        assert!(b.data[3] > a.data[3]);
    }
    #[test]
    fn m21_amount_zero_is_exact_source_parity_without_adjustment_recompute() {
        let source = image(8, 6);
        let residual = AiDenoiseResidual {
            width: source.width,
            height: source.height,
            values: vec![0.75; source.data.len()],
            model_hash: MODEL_SHA256.into(),
            source_identity: "amount-zero".into(),
            inference_cache_key: inference_cache_key("amount-zero"),
            execution_provider: ExecutionProvider::Cpu,
        };
        let output = apply_residual(
            &source,
            &residual,
            AiDenoiseParameters {
                enabled: true,
                amount: 0.0,
                ..Default::default()
            },
            None,
        )
        .unwrap();
        assert_eq!(output, source);
    }
    #[test]
    fn noisy_flat_color_hdr_and_overlap_outputs_remain_finite_without_seams() {
        for mut source in [
            LinearImage::new(520, 24, vec![0.18; 520 * 24 * 3]).unwrap(),
            image(700, 80),
            LinearImage::new(40, 40, vec![3.5; 40 * 40 * 3]).unwrap(),
        ] {
            for (index, value) in source.data.iter_mut().enumerate() {
                *value += ((index * 17 % 23) as f32 - 11.0) * 0.0005;
            }
            let result = infer_tiled(
                &mut ConstantResidual(0.01),
                &source,
                "noise-fixture",
                &AtomicBool::new(false),
                ExecutionProvider::Cpu,
            )
            .unwrap();
            assert!(result.values.iter().all(|value| value.is_finite()));
            if source.width > TILE_EDGE {
                let seam = TILE_STRIDE;
                let left = result.values[(seam - 1) * 3];
                let right = result.values[seam * 3];
                assert!((left - right).abs() < 0.03);
            }
        }
    }

    #[test]
    fn high_iso_portrait_hair_fabric_foliage_night_and_neon_vectors_are_finite() {
        for label in [
            "high-iso",
            "portrait-skin",
            "hair",
            "fine-fabric",
            "foliage",
            "night",
            "neon",
        ] {
            let mut source = image(48, 40);
            for (index, value) in source.data.iter_mut().enumerate() {
                let x = (index / 3) % source.width;
                let y = (index / 3) / source.width;
                let channel = index % 3;
                let texture = match label {
                    "hair" => ((x * 13 + y) % 17) as f32 / 100.0,
                    "fine-fabric" => ((x + y) % 2) as f32 * 0.08,
                    "foliage" => ((x * 7 + y * 11) % 23) as f32 / 80.0,
                    "night" => -0.7 + ((x + y) % 5) as f32 * 0.01,
                    "neon" => {
                        if channel == x % 3 {
                            2.5
                        } else {
                            -0.2
                        }
                    }
                    "portrait-skin" => [0.32, 0.18, 0.12][channel],
                    _ => ((index * 29 % 31) as f32 - 15.0) * 0.008,
                };
                *value = (*value + texture).max(-0.5);
            }
            let residual = infer_tiled(
                &mut Identity,
                &source,
                label,
                &AtomicBool::new(false),
                ExecutionProvider::Cpu,
            )
            .unwrap();
            assert!(
                residual.values.iter().all(|value| value.is_finite()),
                "{label}"
            );
        }
    }
}
