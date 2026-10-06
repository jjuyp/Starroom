//! Native rendered-image CPU pipeline for Starroom v0.2.
//! This is the executable reference graph for JPEG/PNG/TIFF editing. Future wgpu stages must
//! match this pipeline within documented tolerances before replacing the CPU reference.

pub mod cancellation;
use cancellation::checkpoint;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use starroom_ai_denoise::{AiDenoiseError, AiDenoiseParameters, AiDenoiseResidual, apply_residual};
use starroom_color::{
    ColorBand, ColorMixer, CurvePoint, LinearRgb, PreparedCurve, ToneParameters,
    apply_chroma_controls, apply_color_mixer, apply_tone, compress_to_unit_gamut, oklab_to_rec2020,
    rec2020_to_oklab, sample_color_band,
};
use starroom_color_management::{
    ColorManagementError, InputProfileSource, LittleCmsProvider, Matrix3 as ColorMatrix3,
    OutputProfileSource, RenderingIntent, Xyz, measured_neutral_adaptation,
};
use starroom_detail::{
    DenoiseParameters, LinearImage, LocalDetailParameters, SharpenParameters, denoise,
    local_detail, sharpen,
};
use starroom_geometry::{
    CropRect, GeometryParameters, Matrix3, Point2, UprightMode, analyze_upright, apply_geometry,
    apply_upright, constrain_crop_aspect,
};
use starroom_grading::{GradingParameters, apply_grading};
use starroom_heal::{HealPoint, HealingOperation, apply_operation};
use starroom_imageio::{DecodedRenderedImage, DecodedSourceImage, lens_metadata};
use starroom_look::{GrainSettings, LookError, VignetteSettings, apply_finishing_effects};
use starroom_optics::{
    LensCorrection, LensIdentity, LensProfileResolution, LensProfileStatus, LensfunProvider,
    NormalizedPoint, OpticsSettings, apply_lens_correction, distort,
};
use starroom_portrait::{SkinRetouchParameters, apply_skin_retouch};
use starroom_project::{
    GeneratedMaskSemantic, MaskDefinition, MaskOperation, MaskTree, PortraitMaskRegion,
    PortraitSourceCrop,
};
use starroom_raw::{CameraProfileDescriptor, CameraProfileStatus, DecodedRawImage};
use starroom_render::profiling::{self, ProfileStage};
use starroom_render::{
    GpuStageCacheKeys,
    gpu::{GpuCreativeParameters, GpuError, GpuRenderer},
};
use std::time::Instant;

const F32_BYTES: u64 = 4;
/// Reproducible render-policy identity. Change whenever authoritative color semantics change.
pub const COLOR_POLICY_VERSION: &str = "starroom-color-v3-measured-neutral-cat";
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorManagementSettings {
    pub intent: RenderingIntent,
    pub black_point_compensation: bool,
}

impl Default for ColorManagementSettings {
    fn default() -> Self {
        Self {
            intent: RenderingIntent::RelativeColorimetric,
            black_point_compensation: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct RelativeColorParameters {
    /// Encoded-image relative warm/cool correction in -1..1. Not a physical Kelvin value.
    pub temperature: f32,
    /// Encoded-image relative green/magenta correction in -1..1.
    pub tint: f32,
    pub vibrance: f32,
    pub saturation: f32,
}

/// White-balance intent is persisted independently from the creative colour controls.
/// `SourceDefault` means LibRaw camera WB for RAW and relative controls for encoded sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum WhiteBalanceMode {
    #[default]
    SourceDefault,
    AsShot,
    Camera,
    Auto,
    NeutralPicker,
    Relative,
}

/// Normalized post-lens/post-geometry rectangle used by the native Neutral Picker.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhiteBalanceSample {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl WhiteBalanceSample {
    fn validated(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .into_iter()
            .all(f32::is_finite)
            && self.x >= 0.0
            && self.y >= 0.0
            && self.width > 0.0
            && self.height > 0.0
            && self.x + self.width <= 1.0
            && self.y + self.height <= 1.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WhiteBalanceSettings {
    pub mode: WhiteBalanceMode,
    pub sample: Option<WhiteBalanceSample>,
}

/// M6 native tone curves.  Each curve uses the tested monotone cubic Hermite mapper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ToneCurveSet {
    #[serde(default)]
    pub master: Vec<CurvePoint>,
    #[serde(default)]
    pub red: Vec<CurvePoint>,
    #[serde(default)]
    pub green: Vec<CurvePoint>,
    #[serde(default)]
    pub blue: Vec<CurvePoint>,
}

/// M14 native adjustment-layer intent. Layer math remains entirely in the shared Rust graph;
/// the frontend transports only this small, serializable edit description.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LayerBlendMode {
    #[default]
    Normal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LayerAdjustments {
    #[serde(default)]
    pub tone: ToneParameters,
    #[serde(default)]
    pub relative_color: RelativeColorParameters,
    #[serde(default)]
    pub curves: ToneCurveSet,
    #[serde(default)]
    pub color_mixer: ColorMixer,
    #[serde(default)]
    pub grading: GradingParameters,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeAdjustmentLayer {
    pub id: String,
    pub name: String,
    #[serde(default = "layer_enabled")]
    pub enabled: bool,
    #[serde(default = "layer_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub blend_mode: LayerBlendMode,
    #[serde(default = "default_mask_tree")]
    pub mask: MaskTree,
    #[serde(default)]
    pub adjustments: LayerAdjustments,
}

/// Native-only M16 semantic-mask cache entry. The project's MaskTree persists a compact
/// reference; source-image R16Float-compatible weights are resolved here in the shared graph.
/// Tauri obtains these values from the local ONNX cache, not from a browser JSON pixel payload.
#[derive(Debug, Clone, PartialEq)]
pub struct PortraitMaskRaster {
    pub cache_key: String,
    pub face_id: String,
    pub region: PortraitMaskRegion,
    pub width: u32,
    pub height: u32,
    pub values: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedMaskRaster {
    pub cache_identity: String,
    pub semantic: GeneratedMaskSemantic,
    pub width: u32,
    pub height: u32,
    pub values: Vec<f32>,
}

/// Compact persistent identity for a face selected for M17 skin retouch. The actual semantic
/// R16Float-compatible raster is resolved only from the native M16 cache.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinRetouchFaceReference {
    pub face_id: String,
    pub cache_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_crop: Option<PortraitSourceCrop>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkinRetouchSettings {
    #[serde(default)]
    pub parameters: SkinRetouchParameters,
    #[serde(default)]
    pub faces: Vec<SkinRetouchFaceReference>,
}

impl PortraitMaskRaster {
    fn validate(&self) -> Result<(), PipelineError> {
        validate_soft_raster(
            self.width,
            self.height,
            &self.values,
            "portrait raster is malformed",
        )
    }

    fn weight_at(&self, x: f32, y: f32) -> Result<f32, PipelineError> {
        sample_soft_raster(
            self.width,
            self.height,
            &self.values,
            x,
            y,
            "portrait raster is malformed",
        )
    }
}

impl GeneratedMaskRaster {
    fn validate(&self) -> Result<(), PipelineError> {
        validate_soft_raster(
            self.width,
            self.height,
            &self.values,
            "generated AI raster is malformed",
        )
    }

    fn weight_at(&self, x: f32, y: f32) -> Result<f32, PipelineError> {
        sample_soft_raster(
            self.width,
            self.height,
            &self.values,
            x,
            y,
            "generated AI raster is malformed",
        )
    }
}

fn soft_raster_has_valid_shape(width: u32, height: u32, values: &[f32]) -> bool {
    width > 0 && height > 0 && (width as usize).checked_mul(height as usize) == Some(values.len())
}

/// Validate every cell once at the shared graph boundary, including cells outside the current
/// viewport. Scanning a full mask from each pixel sample previously made Face/Skin/AI O(N²).
fn validate_soft_raster(
    width: u32,
    height: u32,
    values: &[f32],
    reason: &'static str,
) -> Result<(), PipelineError> {
    if !soft_raster_has_valid_shape(width, height, values) {
        return Err(PipelineError::InvalidMask(reason));
    }
    for chunk in values.chunks(65_536) {
        checkpoint()?;
        if chunk.iter().any(|value| !value.is_finite()) {
            return Err(PipelineError::InvalidMask(reason));
        }
    }
    Ok(())
}

/// Sampling remains O(1). Shape and the selected cell are checked defensively, without weakening
/// the complete finite-data validation performed before a production graph starts sampling.
fn sample_soft_raster(
    width: u32,
    height: u32,
    values: &[f32],
    x: f32,
    y: f32,
    reason: &'static str,
) -> Result<f32, PipelineError> {
    if !soft_raster_has_valid_shape(width, height, values) || !x.is_finite() || !y.is_finite() {
        return Err(PipelineError::InvalidMask(reason));
    }
    let px = (x.clamp(0.0, 1.0) * width.saturating_sub(1) as f32).round() as usize;
    let py = (y.clamp(0.0, 1.0) * height.saturating_sub(1) as f32).round() as usize;
    let value = values
        .get(py * width as usize + px)
        .copied()
        .filter(|value| value.is_finite())
        .ok_or(PipelineError::InvalidMask(reason))?;
    Ok(value.clamp(0.0, 1.0))
}

fn validate_mask_rasters(
    portrait: &[PortraitMaskRaster],
    generated: &[GeneratedMaskRaster],
) -> Result<(), PipelineError> {
    for raster in portrait {
        raster.validate()?;
    }
    for raster in generated {
        raster.validate()?;
    }
    Ok(())
}

fn layer_enabled() -> bool {
    true
}
fn layer_opacity() -> f32 {
    1.0
}

fn default_mask_tree() -> MaskTree {
    MaskDefinition::None.into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderSettings {
    pub color_management: ColorManagementSettings,
    pub tone: ToneParameters,
    pub relative_color: RelativeColorParameters,
    pub white_balance: WhiteBalanceSettings,
    pub curve: Vec<CurvePoint>,
    #[serde(default)]
    pub curves: ToneCurveSet,
    pub color_mixer: ColorMixer,
    pub grading: GradingParameters,
    pub denoise: DenoiseParameters,
    /// M21 NAFNet adjustment controls. Native code resolves the matching residual cache entry.
    #[serde(default)]
    pub ai_denoise: AiDenoiseParameters,
    /// Native-only model residual. Never serialized or transported as JSON pixels.
    #[serde(skip)]
    pub ai_denoise_residual: Option<AiDenoiseResidual>,
    #[serde(default)]
    pub local_detail: LocalDetailParameters,
    pub sharpen: SharpenParameters,
    #[serde(default)]
    pub optics: OpticsSettings,
    #[serde(default)]
    pub geometry: GeometryParameters,
    /// Evaluated after global creative adjustments and before detail/output. Layer order is the
    /// vector order, which is part of the graph cache identity.
    #[serde(default)]
    pub layers: Vec<NativeAdjustmentLayer>,
    /// Runtime cache bindings for M16 PortraitSemantic leaves. This deliberately is not
    /// serialized into sidecars or accepted from the frontend transport.
    #[serde(skip)]
    pub portrait_masks: Vec<PortraitMaskRaster>,
    #[serde(skip)]
    pub generated_masks: Vec<GeneratedMaskRaster>,
    #[serde(default)]
    pub skin_retouch: SkinRetouchSettings,
    /// M18 operations run after colour/portrait work and before detail/output, identically for
    /// preview and export. They contain coordinates and parameters only, never raster pixels.
    #[serde(default)]
    pub healing_operations: Vec<HealingOperation>,
    /// M23 portable finishing effects, evaluated in the shared graph.
    #[serde(default)]
    pub grain: GrainSettings,
    #[serde(default)]
    pub vignette: VignetteSettings,
    /// Native source identity makes grain stable across preview/export without exposing pixels.
    #[serde(skip)]
    pub image_identity: String,
    /// Runtime-only mapping for source-resolution viewport tiles. Local masks continue to use
    /// full-image normalized coordinates while only the visible tile is evaluated.
    #[serde(skip)]
    pub source_region: Option<SourceRegion>,
    #[serde(skip)]
    pub gpu_cache_keys: Option<GpuStageCacheKeys>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRegion {
    pub full_width: u32,
    pub full_height: u32,
    pub x: u32,
    pub y: u32,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            color_management: ColorManagementSettings::default(),
            tone: ToneParameters::default(),
            relative_color: RelativeColorParameters::default(),
            white_balance: WhiteBalanceSettings::default(),
            curve: Vec::new(),
            curves: ToneCurveSet::default(),
            color_mixer: ColorMixer::default(),
            grading: GradingParameters::default(),
            denoise: DenoiseParameters::default(),
            ai_denoise: AiDenoiseParameters::default(),
            ai_denoise_residual: None,
            local_detail: LocalDetailParameters::default(),
            sharpen: SharpenParameters {
                amount: 0.0,
                ..Default::default()
            },
            optics: OpticsSettings::default(),
            geometry: GeometryParameters::default(),
            layers: Vec::new(),
            portrait_masks: Vec::new(),
            generated_masks: Vec::new(),
            skin_retouch: SkinRetouchSettings::default(),
            healing_operations: Vec::new(),
            grain: GrainSettings::default(),
            vignette: VignetteSettings::default(),
            image_identity: String::new(),
            source_region: None,
            gpu_cache_keys: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("PreviewCancelled: request was superseded")]
    Cancelled,
    #[error("decoded RGBA buffer length does not match dimensions")]
    InvalidDecodedBuffer,
    #[error("detail image buffer is invalid")]
    DetailBuffer,
    #[error("Lensfun profile is unavailable: {0:?}")]
    OpticsProfile(LensProfileStatus),
    #[error("Lensfun correction failed")]
    OpticsCorrection,
    #[error("Lensfun database failed: {0}")]
    OpticsDatabase(String),
    #[error("geometry transform failed")]
    Geometry,
    #[error("white-balance mode {mode:?} is not valid for {input_kind} input")]
    WhiteBalanceSemantic {
        mode: WhiteBalanceMode,
        input_kind: &'static str,
    },
    #[error("neutral-picker sample is missing or invalid")]
    InvalidWhiteBalanceSample,
    #[error("adjustment layer {id} is invalid: {reason}")]
    InvalidLayer { id: String, reason: &'static str },
    #[error("mask is invalid: {0}")]
    InvalidMask(&'static str),
    #[error("mask provider {provider:?} is unavailable for this native graph")]
    MaskProviderUnavailable { provider: String },
    #[error("GPU acceleration failed: {0}")]
    Gpu(#[from] GpuError),
    #[error("AI denoise failed: {0}")]
    AiDenoise(#[from] AiDenoiseError),
    #[error("look finishing effect failed: {0}")]
    Look(#[from] LookError),
    #[error(transparent)]
    ColorManagement(#[from] ColorManagementError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    Raw,
    Encoded,
}

fn measured_neutral(sum: [f32; 3], count: usize) -> Option<LinearRgb> {
    if count == 0 || !sum.into_iter().all(f32::is_finite) {
        return None;
    }
    let mean = sum.map(|channel| channel / count as f32);
    // Refuse black/non-finite measurements rather than inventing a white point. The color
    // provider determines chromatic adaptation; an RGB diagonal is not a working-space CAT.
    if mean.iter().any(|channel| *channel <= 1.0e-6) {
        return None;
    }
    Some(LinearRgb {
        r: mean[0],
        g: mean[1],
        b: mean[2],
    })
}

fn apply_white_balance_matrix(pixels: &mut [[f32; 3]], matrix: ColorMatrix3) {
    pixels.par_iter_mut().for_each(|pixel| {
        let adapted = matrix.multiply_vec(Xyz {
            x: pixel[0],
            y: pixel[1],
            z: pixel[2],
        });
        *pixel = [adapted.x, adapted.y, adapted.z];
    });
}

fn sampled_white_balance_matrix(sample: LinearRgb) -> Result<ColorMatrix3, PipelineError> {
    measured_neutral_adaptation(sample).map_err(|_| PipelineError::InvalidWhiteBalanceSample)
}

pub trait AutoWhiteBalanceProvider {
    /// Estimate a measured neutral in Linear Rec.2020 D65. The color-management stage, not
    /// this provider, owns adaptation. Providers must never return encoded RGB channel gains.
    fn estimate_neutral(&self, pixels: &[[f32; 3]]) -> Option<LinearRgb>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GrayWorldAutoWhiteBalance;

impl AutoWhiteBalanceProvider for GrayWorldAutoWhiteBalance {
    fn estimate_neutral(&self, pixels: &[[f32; 3]]) -> Option<LinearRgb> {
        // Deterministic grey-world provider: reject very dark and clipped samples so highlights
        // and empty black borders do not define the estimated neutral. It is an active provider,
        // not a fallback for Camera/As-Shot WB.
        let mut sum = [0.0; 3];
        let mut count = 0;
        for pixel in pixels {
            let y = pixel[0] * 0.2627 + pixel[1] * 0.6780 + pixel[2] * 0.0593;
            if y.is_finite()
                && (0.01..=0.85).contains(&y)
                && pixel.iter().all(|v| v.is_finite() && *v > 0.0)
            {
                for (target, source) in sum.iter_mut().zip(pixel) {
                    *target += *source;
                }
                count += 1;
            }
        }
        measured_neutral(sum, count)
    }
}

fn picker_white_balance_neutral(
    pixels: &[[f32; 3]],
    width: u32,
    height: u32,
    sample: WhiteBalanceSample,
) -> Option<LinearRgb> {
    if !sample.validated() {
        return None;
    }
    let left = (sample.x * width as f32).floor() as usize;
    let top = (sample.y * height as f32).floor() as usize;
    let right = ((sample.x + sample.width) * width as f32).ceil() as usize;
    let bottom = ((sample.y + sample.height) * height as f32).ceil() as usize;
    let mut sum = [0.0; 3];
    let mut count = 0;
    for y in top.min(height as usize)..bottom.min(height as usize) {
        for x in left.min(width as usize)..right.min(width as usize) {
            let pixel = pixels[y * width as usize + x];
            if pixel.iter().all(|v| v.is_finite() && *v > 1.0e-6) {
                for (target, source) in sum.iter_mut().zip(pixel) {
                    *target += source;
                }
                count += 1;
            }
        }
    }
    measured_neutral(sum, count)
}

fn apply_white_balance(
    pixels: &mut [[f32; 3]],
    width: u32,
    height: u32,
    source: SourceKind,
    settings: WhiteBalanceSettings,
) -> Result<(), PipelineError> {
    match (source, settings.mode) {
        // LibRaw has already applied the recorded Camera Neutral / As-Shot multipliers before
        // the RAW data reaches the linear Rec.2020 graph. The modes stay explicit so projects
        // preserve the photographer's intent and no encoded-image WB is silently substituted.
        (
            SourceKind::Raw,
            WhiteBalanceMode::SourceDefault | WhiteBalanceMode::AsShot | WhiteBalanceMode::Camera,
        ) => Ok(()),
        (SourceKind::Encoded, WhiteBalanceMode::SourceDefault | WhiteBalanceMode::Relative) => {
            Ok(())
        }
        (SourceKind::Encoded, WhiteBalanceMode::AsShot | WhiteBalanceMode::Camera) => {
            Err(PipelineError::WhiteBalanceSemantic {
                mode: settings.mode,
                input_kind: "encoded",
            })
        }
        (_, WhiteBalanceMode::Auto) => {
            let neutral = GrayWorldAutoWhiteBalance
                .estimate_neutral(pixels)
                .ok_or(PipelineError::InvalidWhiteBalanceSample)?;
            apply_white_balance_matrix(pixels, sampled_white_balance_matrix(neutral)?);
            Ok(())
        }
        (_, WhiteBalanceMode::NeutralPicker) => {
            let sample = settings
                .sample
                .ok_or(PipelineError::InvalidWhiteBalanceSample)?;
            let neutral = picker_white_balance_neutral(pixels, width, height, sample)
                .ok_or(PipelineError::InvalidWhiteBalanceSample)?;
            apply_white_balance_matrix(pixels, sampled_white_balance_matrix(neutral)?);
            Ok(())
        }
        (SourceKind::Raw, WhiteBalanceMode::Relative) => Err(PipelineError::WhiteBalanceSemantic {
            mode: settings.mode,
            input_kind: "RAW",
        }),
    }
}

/// Picker rectangles are post-geometry/display coordinates. Camera/As-Shot and Auto retain
/// their source-space semantics; only the explicit picker is deferred until the same native
/// lens/geometry stage used by preview/export has prepared the visible image.
fn source_white_balance(settings: WhiteBalanceSettings) -> WhiteBalanceSettings {
    if settings.mode == WhiteBalanceMode::NeutralPicker {
        WhiteBalanceSettings::default()
    } else {
        settings
    }
}

/// Runtime-only output -> source mapping for immutable source-space AI probability caches.
/// Reuses the existing mature projective geometry transform and Lensfun distortion equations;
/// no raster copies, cache/model identity changes, or browser image mathematics are involved.
#[derive(Debug, Clone, Copy)]
struct SemanticSamplingMap {
    inverse_geometry: Matrix3,
    crop: CropRect,
    output_width: usize,
    output_height: usize,
    lens: Option<(LensCorrection, f32, bool)>,
}

impl SemanticSamplingMap {
    fn source_point(self, x: f32, y: f32) -> Option<Point2> {
        const EDGE_EPSILON: f32 = 1.0e-6;
        let covered = |point: Point2| {
            point.x.is_finite()
                && point.y.is_finite()
                && (-EDGE_EPSILON..=1.0 + EDGE_EPSILON).contains(&point.x)
                && (-EDGE_EPSILON..=1.0 + EDGE_EPSILON).contains(&point.y)
        };
        // Local/mask iteration uses pixel-index / frame-size. Geometry's resampler uses
        // pixel-index / (frame-size - 1); match its exact pixel grid, including edge pixels.
        let target = Point2 {
            x: self.crop.left
                + x * self.output_width as f32 / self.output_width.saturating_sub(1).max(1) as f32
                    * (self.crop.right - self.crop.left),
            y: self.crop.top
                + y * self.output_height as f32
                    / self.output_height.saturating_sub(1).max(1) as f32
                    * (self.crop.bottom - self.crop.top),
        };
        let mut source = self.inverse_geometry.transform(target);
        // Geometry's RGB sampler treats uncovered post-lens pixels as empty. Do not let an
        // extrapolated radial polynomial fold those coordinates back into a source subject.
        if !covered(source) {
            return None;
        }
        if let Some((correction, auto_scale, distortion_enabled)) = self.lens {
            let point = NormalizedPoint {
                x: (source.x * 2.0 - 1.0) * auto_scale,
                y: (source.y * 2.0 - 1.0) * auto_scale,
            };
            let green = if distortion_enabled {
                distort(point, correction.distortion)
            } else {
                point
            };
            source = Point2 {
                x: (green.x + 1.0) * 0.5,
                y: (green.y + 1.0) * 0.5,
            };
        }
        // Floating quarter-turn endpoints may differ from zero/one by a few ULPs. Real
        // uncovered borders must remain unselected rather than clamping to source edges.
        if !covered(source) {
            return None;
        }
        Some(Point2 {
            x: source.x.clamp(0.0, 1.0),
            y: source.y.clamp(0.0, 1.0),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorTransformReport {
    pub input: InputProfileSource,
    pub output: OutputProfileSource,
    pub working_space: &'static str,
    pub camera_profile_id: Option<String>,
    pub camera_profile_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedRgb8 {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub color: ColorTransformReport,
}

/// High-precision encoded output of the shared Native graph. Values have already passed through
/// the selected LittleCMS output transform, but have not been quantized for a file codec.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedRgbF32 {
    pub width: u32,
    pub height: u32,
    pub data: Vec<f32>,
    pub color: ColorTransformReport,
}

impl RenderedRgbF32 {
    fn into_rgb8(self) -> RenderedRgb8 {
        RenderedRgb8 {
            width: self.width,
            height: self.height,
            data: self
                .data
                .into_par_iter()
                .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect(),
            color: self.color,
        }
    }
}

fn apply_relative_color(rgb: LinearRgb, parameters: RelativeColorParameters) -> LinearRgb {
    let temperature = parameters.temperature.clamp(-1.0, 1.0);
    let tint = parameters.tint.clamp(-1.0, 1.0);
    let mut lab = rec2020_to_oklab(rgb);
    // This is deliberately labeled relative editing rather than Kelvin. Positive b is warmer;
    // positive a is more magenta in Oklab opponent coordinates.
    lab.b += temperature * 0.035;
    lab.a += tint * 0.025;

    oklab_to_rec2020(apply_chroma_controls(
        lab,
        parameters.saturation,
        parameters.vibrance,
    ))
}

#[cfg(test)]
fn apply_one_curve(value: f32, curve: &[CurvePoint]) -> f32 {
    if curve.len() < 2 {
        value
    } else {
        starroom_color::map_monotone_curve(value, curve)
    }
}
#[cfg(test)]
fn apply_curve(rgb: LinearRgb, legacy: &[CurvePoint], curves: &ToneCurveSet) -> LinearRgb {
    let master = if curves.master.len() >= 2 {
        &curves.master
    } else {
        legacy
    };
    let rgb = LinearRgb {
        r: apply_one_curve(rgb.r, master),
        g: apply_one_curve(rgb.g, master),
        b: apply_one_curve(rgb.b, master),
    };
    LinearRgb {
        r: apply_one_curve(rgb.r, &curves.red),
        g: apply_one_curve(rgb.g, &curves.green),
        b: apply_one_curve(rgb.b, &curves.blue),
    }
}

struct PreparedLayer<'a> {
    layer: &'a NativeAdjustmentLayer,
    master: PreparedCurve,
    red: PreparedCurve,
    green: PreparedCurve,
    blue: PreparedCurve,
}

impl<'a> PreparedLayer<'a> {
    fn new(layer: &'a NativeAdjustmentLayer) -> Result<Self, PipelineError> {
        if !layer_is_finite(layer) {
            return Err(PipelineError::InvalidLayer {
                id: layer.id.clone(),
                reason: "non-finite control",
            });
        }
        if !(0.0..=1.0).contains(&layer.opacity) {
            return Err(PipelineError::InvalidLayer {
                id: layer.id.clone(),
                reason: "opacity must be 0..1",
            });
        }
        Ok(Self {
            layer,
            master: PreparedCurve::new(&layer.adjustments.curves.master),
            red: PreparedCurve::new(&layer.adjustments.curves.red),
            green: PreparedCurve::new(&layer.adjustments.curves.green),
            blue: PreparedCurve::new(&layer.adjustments.curves.blue),
        })
    }

    fn apply_curve(&self, rgb: LinearRgb) -> LinearRgb {
        LinearRgb {
            r: self.red.map(self.master.map(rgb.r)),
            g: self.green.map(self.master.map(rgb.g)),
            b: self.blue.map(self.master.map(rgb.b)),
        }
    }
}

fn layer_is_finite(layer: &NativeAdjustmentLayer) -> bool {
    let tone = layer.adjustments.tone;
    let color = layer.adjustments.relative_color;
    layer.opacity.is_finite()
        && [
            tone.exposure_ev,
            tone.contrast,
            tone.highlights,
            tone.shadows,
            tone.whites,
            tone.blacks,
            color.temperature,
            color.tint,
            color.vibrance,
            color.saturation,
        ]
        .into_iter()
        .all(f32::is_finite)
        && [
            &layer.adjustments.curves.master,
            &layer.adjustments.curves.red,
            &layer.adjustments.curves.green,
            &layer.adjustments.curves.blue,
        ]
        .into_iter()
        .flatten()
        .all(|point| point.x.is_finite() && point.y.is_finite())
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if edge1 <= edge0 {
        return if value >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mask_leaf_weight(
    mask: &MaskDefinition,
    x: f32,
    y: f32,
    rgb: LinearRgb,
    portrait_masks: &[PortraitMaskRaster],
    generated_masks: &[GeneratedMaskRaster],
    semantic_map: Option<SemanticSamplingMap>,
) -> Result<f32, PipelineError> {
    let weight = match mask {
        MaskDefinition::None => 1.0,
        MaskDefinition::Radial {
            x: center_x,
            y: center_y,
            width,
            height,
            rotation,
            feather,
            invert,
        } => {
            if ![*center_x, *center_y, *width, *height, *rotation, *feather]
                .into_iter()
                .all(f32::is_finite)
                || *width <= 0.0
                || *height <= 0.0
                || *feather < 0.0
            {
                return Err(PipelineError::InvalidMask(
                    "radial values must be finite with positive size",
                ));
            }
            let angle = -*rotation * std::f32::consts::PI / 180.0;
            let dx = x - *center_x;
            let dy = y - *center_y;
            let local_x = dx * angle.cos() - dy * angle.sin();
            let local_y = dx * angle.sin() + dy * angle.cos();
            let distance = (local_x / (*width * 0.5)).hypot(local_y / (*height * 0.5));
            let result = 1.0 - smoothstep(1.0, 1.0 + *feather * 2.0, distance);
            if *invert { 1.0 - result } else { result }
        }
        MaskDefinition::Linear {
            start_x,
            start_y,
            end_x,
            end_y,
            feather,
            invert,
        } => {
            if ![*start_x, *start_y, *end_x, *end_y, *feather]
                .into_iter()
                .all(f32::is_finite)
                || *feather < 0.0
            {
                return Err(PipelineError::InvalidMask("linear values must be finite"));
            }
            let dx = *end_x - *start_x;
            let dy = *end_y - *start_y;
            let length = dx.hypot(dy);
            if length <= 1.0e-6 {
                return Err(PipelineError::InvalidMask("linear endpoints must differ"));
            }
            let along = ((x - *start_x) * dx + (y - *start_y) * dy) / length;
            let result = smoothstep(0.0, (*feather).max(0.001), along);
            if *invert { 1.0 - result } else { result }
        }
        MaskDefinition::Brush {
            points,
            radius,
            feather,
            flow,
            erase,
        } => {
            if ![*radius, *feather, *flow].into_iter().all(f32::is_finite)
                || *radius <= 0.0
                || *feather < 0.0
                || !(0.0..=1.0).contains(flow)
            {
                return Err(PipelineError::InvalidMask(
                    "brush values are outside supported ranges",
                ));
            }
            let mut coverage: f32 = 0.0;
            for point in points {
                if ![point.x, point.y, point.pressure]
                    .into_iter()
                    .all(f32::is_finite)
                    || point.pressure < 0.0
                {
                    return Err(PipelineError::InvalidMask("brush point is invalid"));
                }
                let distance = (x - point.x).hypot(y - point.y);
                coverage = coverage.max(
                    (1.0 - smoothstep(*radius, *radius * (1.0 + *feather), distance))
                        * point.pressure,
                );
            }
            let result = (coverage * *flow).clamp(0.0, 1.0);
            if *erase { 1.0 - result } else { result }
        }
        MaskDefinition::Luminance {
            minimum,
            maximum,
            feather,
            invert,
        } => {
            if ![*minimum, *maximum, *feather]
                .into_iter()
                .all(f32::is_finite)
                || *minimum > *maximum
                || *feather < 0.0
            {
                return Err(PipelineError::InvalidMask("luminance range is invalid"));
            }
            let luma = rgb.r * 0.2627 + rgb.g * 0.6780 + rgb.b * 0.0593;
            let soft = (*feather).max(0.0001);
            let result = smoothstep(*minimum - soft, *minimum + soft, luma)
                * (1.0 - smoothstep(*maximum - soft, *maximum + soft, luma));
            if *invert { 1.0 - result } else { result }
        }
        MaskDefinition::ColorRange {
            reference,
            tolerance,
            feather,
            invert,
        } => {
            if !reference
                .iter()
                .copied()
                .chain([*tolerance, *feather])
                .all(f32::is_finite)
                || *tolerance < 0.0
                || *feather < 0.0
            {
                return Err(PipelineError::InvalidMask("color range is invalid"));
            }
            let distance = ((rgb.r - reference[0]).powi(2)
                + (rgb.g - reference[1]).powi(2)
                + (rgb.b - reference[2]).powi(2))
            .sqrt();
            let result =
                1.0 - smoothstep(*tolerance, *tolerance + (*feather).max(0.0001), distance);
            if *invert { 1.0 - result } else { result }
        }
        MaskDefinition::PortraitSemantic {
            face_id,
            region,
            threshold,
            feather,
            model_id,
            model_version,
            model_hash,
            cache_key,
            source_crop,
        } => {
            if face_id.trim().is_empty()
                || cache_key.trim().is_empty()
                || model_id.trim().is_empty()
                || model_version.trim().is_empty()
                || model_hash.len() != 64
                || ![*threshold, *feather].into_iter().all(f32::is_finite)
                || !(0.0..=1.0).contains(threshold)
                || *feather < 0.0
                || source_crop.is_some_and(|crop| !crop.is_valid())
            {
                return Err(PipelineError::InvalidMask(
                    "portrait semantic reference is invalid",
                ));
            }
            let raster = portrait_masks
                .iter()
                .find(|candidate| {
                    candidate.cache_key == *cache_key
                        && candidate.face_id == *face_id
                        && candidate.region == *region
                })
                .ok_or_else(|| PipelineError::MaskProviderUnavailable {
                    provider: format!("portrait semantic cache: {cache_key}"),
                })?;
            let value = if let Some(map) = semantic_map {
                let Some(point) = map.source_point(x, y) else {
                    return Ok(0.0);
                };
                raster.weight_at(point.x, point.y)?
            } else {
                raster.weight_at(x, y)?
            };
            smoothstep(*threshold - *feather, *threshold + *feather + 1.0e-5, value)
        }
        MaskDefinition::Generated {
            provider_id,
            model_id,
            model_version,
            model_hash,
            semantic_class,
            threshold,
            feather,
            invert,
            cache_identity,
            ..
        } => {
            if provider_id.trim().is_empty()
                || model_id.trim().is_empty()
                || model_version.trim().is_empty()
                || model_hash.len() != 64
                || cache_identity.trim().is_empty()
                || ![*threshold, *feather].into_iter().all(f32::is_finite)
                || !(0.0..=1.0).contains(threshold)
                || *feather < 0.0
            {
                return Err(PipelineError::InvalidMask(
                    "generated AI mask reference is invalid",
                ));
            }
            let raster = generated_masks
                .iter()
                .find(|candidate| {
                    candidate.cache_identity == *cache_identity
                        && candidate.semantic == *semantic_class
                })
                .ok_or_else(|| PipelineError::MaskProviderUnavailable {
                    provider: format!("AI mask cache: {cache_identity}"),
                })?;
            let probability = if let Some(map) = semantic_map {
                let Some(point) = map.source_point(x, y) else {
                    return Ok(0.0);
                };
                raster.weight_at(point.x, point.y)?
            } else {
                raster.weight_at(x, y)?
            };
            let refined = smoothstep(
                *threshold - *feather,
                *threshold + *feather + 1.0e-5,
                probability,
            );
            if *invert { 1.0 - refined } else { refined }
        }
        MaskDefinition::Provider { provider, .. } => {
            return Err(PipelineError::MaskProviderUnavailable {
                provider: provider.clone(),
            });
        }
    };
    Ok(weight.clamp(0.0, 1.0))
}

fn mask_weight_mapped(
    mask: &MaskTree,
    x: f32,
    y: f32,
    rgb: LinearRgb,
    portrait_masks: &[PortraitMaskRaster],
    generated_masks: &[GeneratedMaskRaster],
    semantic_map: Option<SemanticSamplingMap>,
) -> Result<f32, PipelineError> {
    if !x.is_finite()
        || !y.is_finite()
        || !rgb.r.is_finite()
        || !rgb.g.is_finite()
        || !rgb.b.is_finite()
    {
        return Err(PipelineError::InvalidMask(
            "coordinates and input must be finite",
        ));
    }
    match mask {
        MaskTree::Leaf(leaf) => mask_leaf_weight(
            leaf,
            x,
            y,
            rgb,
            portrait_masks,
            generated_masks,
            semantic_map,
        ),
        MaskTree::Composite(composite) => {
            let mut children = composite.children.iter();
            let Some(first) = children.next() else {
                return Ok(0.0);
            };
            let first_weight = mask_weight_mapped(
                first,
                x,
                y,
                rgb,
                portrait_masks,
                generated_masks,
                semantic_map,
            )?;
            match composite.operation {
                MaskOperation::Add => children.try_fold(first_weight, |value, child| {
                    Ok(value.max(mask_weight_mapped(
                        child,
                        x,
                        y,
                        rgb,
                        portrait_masks,
                        generated_masks,
                        semantic_map,
                    )?))
                }),
                MaskOperation::Subtract => children.try_fold(first_weight, |value, child| {
                    Ok(value
                        * (1.0
                            - mask_weight_mapped(
                                child,
                                x,
                                y,
                                rgb,
                                portrait_masks,
                                generated_masks,
                                semantic_map,
                            )?))
                }),
                MaskOperation::Intersect => children.try_fold(first_weight, |value, child| {
                    Ok(value.min(mask_weight_mapped(
                        child,
                        x,
                        y,
                        rgb,
                        portrait_masks,
                        generated_masks,
                        semantic_map,
                    )?))
                }),
                MaskOperation::Invert => Ok(1.0 - first_weight),
            }
        }
    }
}

#[cfg(test)]
fn mask_weight(
    mask: &MaskTree,
    x: f32,
    y: f32,
    rgb: LinearRgb,
    portrait_masks: &[PortraitMaskRaster],
    generated_masks: &[GeneratedMaskRaster],
) -> Result<f32, PipelineError> {
    mask_weight_mapped(mask, x, y, rgb, portrait_masks, generated_masks, None)
}

fn apply_prepared_layers(
    mut rgb: LinearRgb,
    layers: &[PreparedLayer<'_>],
    x: f32,
    y: f32,
    portrait_masks: &[PortraitMaskRaster],
    generated_masks: &[GeneratedMaskRaster],
    semantic_map: Option<SemanticSamplingMap>,
) -> Result<LinearRgb, PipelineError> {
    for prepared_layer in layers {
        let layer = prepared_layer.layer;
        if !layer.enabled {
            continue;
        }
        let adjusted = apply_grading(
            apply_color_mixer(
                prepared_layer.apply_curve(apply_tone(
                    apply_relative_color(rgb, layer.adjustments.relative_color),
                    layer.adjustments.tone,
                )),
                layer.adjustments.color_mixer,
            ),
            layer.adjustments.grading,
        );
        // M14 deliberately supports only Normal. Any future mode must earn an explicit
        // scene-linear implementation rather than quietly behaving like Normal.
        let weight = layer.opacity
            * mask_weight_mapped(
                &layer.mask,
                x,
                y,
                rgb,
                portrait_masks,
                generated_masks,
                semantic_map,
            )?;
        rgb = LinearRgb {
            r: rgb.r + (adjusted.r - rgb.r) * weight,
            g: rgb.g + (adjusted.g - rgb.g) * weight,
            b: rgb.b + (adjusted.b - rgb.b) * weight,
        };
        if !rgb.r.is_finite() || !rgb.g.is_finite() || !rgb.b.is_finite() {
            return Err(PipelineError::InvalidLayer {
                id: layer.id.clone(),
                reason: "produced non-finite output",
            });
        }
    }
    Ok(rgb)
}

#[cfg(test)]
fn apply_layers(
    rgb: LinearRgb,
    layers: &[NativeAdjustmentLayer],
    x: f32,
    y: f32,
    portrait_masks: &[PortraitMaskRaster],
    generated_masks: &[GeneratedMaskRaster],
) -> Result<LinearRgb, PipelineError> {
    validate_mask_rasters(portrait_masks, generated_masks)?;
    let prepared = layers
        .iter()
        .map(PreparedLayer::new)
        .collect::<Result<Vec<_>, _>>()?;
    apply_prepared_layers(rgb, &prepared, x, y, portrait_masks, generated_masks, None)
}

fn skin_retouch_is_identity(parameters: SkinRetouchParameters) -> bool {
    parameters == SkinRetouchParameters::default()
}

fn apply_skin_retouch_stage(
    data: Vec<f32>,
    width: usize,
    height: usize,
    settings: &RenderSettings,
    semantic_map: Option<SemanticSamplingMap>,
) -> Result<Vec<f32>, PipelineError> {
    let parameters = settings.skin_retouch.parameters;
    if skin_retouch_is_identity(parameters) {
        return Ok(data);
    }
    if settings.skin_retouch.faces.is_empty() {
        return Err(PipelineError::MaskProviderUnavailable {
            provider: "M17 skin retouch requires a detected portrait face".into(),
        });
    }
    let image = LinearImage::new(width, height, data).map_err(|_| PipelineError::DetailBuffer)?;
    let count = width.saturating_mul(height);
    let mut skin = vec![0.0_f32; count];
    let mut protected = vec![0.0_f32; count];
    for reference in &settings.skin_retouch.faces {
        let found = settings.portrait_masks.iter().filter(|raster| {
            raster.cache_key == reference.cache_key && raster.face_id == reference.face_id
        });
        let mut matched = false;
        for raster in found {
            matched = true;
            for pixel in 0..count {
                let local_x = pixel % width;
                let local_y = pixel / width;
                let (x, y) = settings.source_region.map_or_else(
                    || {
                        (
                            local_x as f32 / width.max(1) as f32,
                            local_y as f32 / height.max(1) as f32,
                        )
                    },
                    |region| {
                        (
                            (region.x as usize + local_x) as f32 / region.full_width.max(1) as f32,
                            (region.y as usize + local_y) as f32 / region.full_height.max(1) as f32,
                        )
                    },
                );
                let value = if let Some(map) = semantic_map {
                    map.source_point(x, y)
                        .map_or(Ok(0.0), |point| raster.weight_at(point.x, point.y))?
                } else {
                    raster.weight_at(x, y)?
                };
                match raster.region {
                    PortraitMaskRegion::Skin => skin[pixel] = skin[pixel].max(value),
                    PortraitMaskRegion::Eyes
                    | PortraitMaskRegion::LeftEye
                    | PortraitMaskRegion::RightEye
                    | PortraitMaskRegion::Brows
                    | PortraitMaskRegion::LeftBrow
                    | PortraitMaskRegion::RightBrow
                    | PortraitMaskRegion::Lips
                    | PortraitMaskRegion::Mouth
                    | PortraitMaskRegion::Hair => protected[pixel] = protected[pixel].max(value),
                    PortraitMaskRegion::Face => {}
                }
            }
        }
        if !matched {
            return Err(PipelineError::MaskProviderUnavailable {
                provider: format!("M17 portrait cache: {}", reference.cache_key),
            });
        }
    }
    apply_skin_retouch(&image, parameters, &skin, &protected)
        .map(|image| image.data)
        .map_err(|_| PipelineError::InvalidMask("M17 skin retouch data is invalid"))
}

fn apply_healing_stage(
    data: Vec<f32>,
    width: usize,
    height: usize,
    settings: &RenderSettings,
) -> Result<Vec<f32>, PipelineError> {
    if settings.healing_operations.len() > 256 {
        return Err(PipelineError::InvalidMask(
            "M18 operation count exceeds 256",
        ));
    }
    let mut image =
        LinearImage::new(width, height, data).map_err(|_| PipelineError::DetailBuffer)?;
    for operation in &settings.healing_operations {
        checkpoint()?;
        let operation = settings.source_region.map_or_else(
            || operation.clone(),
            |region| {
                let map_point = |point: HealPoint| HealPoint {
                    x: (point.x * region.full_width.saturating_sub(1) as f32 - region.x as f32)
                        / width.saturating_sub(1).max(1) as f32,
                    y: (point.y * region.full_height.saturating_sub(1) as f32 - region.y as f32)
                        / height.saturating_sub(1).max(1) as f32,
                };
                let mut local = operation.clone();
                local.target = map_point(local.target);
                local.source = local.source.map(map_point);
                local
            },
        );
        image = apply_operation(&image, &operation).map_err(|error| {
            PipelineError::InvalidMask(match error {
                starroom_heal::HealError::InvalidOperation => "M18 healing operation is invalid",
                starroom_heal::HealError::MissingManualSource => "M18 manual source is missing",
                starroom_heal::HealError::AiInpaintUnavailable => {
                    "M18 AI inpaint is reserved and unavailable"
                }
            })
        })?;
    }
    Ok(image.data)
}

fn gpu_curve_luts(settings: &RenderSettings) -> [f32; 8192] {
    let master = PreparedCurve::new(if settings.curves.master.len() >= 2 {
        &settings.curves.master
    } else {
        &settings.curve
    });
    let curves = [
        master,
        PreparedCurve::new(&settings.curves.red),
        PreparedCurve::new(&settings.curves.green),
        PreparedCurve::new(&settings.curves.blue),
    ];
    let mut lut = [0.0; 8192];
    for (channel, curve) in curves.iter().enumerate() {
        for sample in 0..1024 {
            let value = sample as f32 / 1023.0;
            lut[channel * 1024 + sample] = curve.map(value);
        }
    }
    lut[4096] = apply_tone(
        LinearRgb {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        },
        settings.tone,
    )
    .r;
    for sample in 1..4096 {
        let exponent = -24.0 + 40.0 * (sample - 1) as f32 / 4094.0;
        let value = 2.0_f32.powf(exponent);
        lut[4096 + sample] = apply_tone(
            LinearRgb {
                r: value,
                g: value,
                b: value,
            },
            settings.tone,
        )
        .r;
    }
    lut
}

/// Finishing vignette belongs after layers/Skin/Healing/spatial detail/grain. It can join the
/// earlier global GPU pass only when every intervening operator is an identity. Otherwise the
/// existing shared CPU finishing stage owns it; no duplicate or reordered creative operation.
fn gpu_can_fuse_vignette(settings: &RenderSettings) -> bool {
    settings
        .layers
        .iter()
        .all(|layer| !layer.enabled || layer.opacity <= f32::EPSILON)
        && skin_retouch_is_identity(settings.skin_retouch.parameters)
        && settings
            .healing_operations
            .iter()
            .all(|operation| !operation.enabled)
        && settings.denoise.luminance.abs() <= f32::EPSILON
        && settings.denoise.chroma.abs() <= f32::EPSILON
        && settings.denoise.high_iso.abs() <= f32::EPSILON
        && settings.local_detail.texture.abs() <= f32::EPSILON
        && settings.local_detail.clarity.abs() <= f32::EPSILON
        && settings.local_detail.dehaze.abs() <= f32::EPSILON
        && settings.sharpen.amount.abs() <= f32::EPSILON
        && settings.grain.amount.abs() <= f32::EPSILON
}

fn gpu_creative_parameters(
    settings: &RenderSettings,
    pixel_count: usize,
    width: usize,
    height: usize,
) -> GpuCreativeParameters {
    let mut values = [[0.0; 4]; 20];
    values[0] = [
        settings.tone.exposure_ev,
        settings.tone.contrast,
        settings.tone.highlights,
        settings.tone.shadows,
    ];
    values[1] = [
        settings.tone.whites,
        settings.tone.blacks,
        settings.relative_color.temperature,
        settings.relative_color.tint,
    ];
    values[2] = [
        settings.relative_color.vibrance,
        settings.relative_color.saturation,
        if gpu_can_fuse_vignette(settings) {
            settings.vignette.amount
        } else {
            0.0
        },
        settings.vignette.midpoint,
    ];
    values[3] = [
        settings.vignette.roundness,
        settings.vignette.feather,
        settings.vignette.highlight_protect,
        0.0,
    ];
    values[4][0] = settings.color_mixer.band_width_degrees;
    for (index, band) in settings.color_mixer.bands.iter().enumerate() {
        values[5 + index] = [band.hue_degrees, band.chroma, band.lightness, 0.0];
    }
    for (index, wheel) in [
        settings.grading.shadows,
        settings.grading.midtones,
        settings.grading.highlights,
        settings.grading.global,
    ]
    .into_iter()
    .enumerate()
    {
        values[13 + index] = [wheel.hue_degrees, wheel.chroma, wheel.lightness, 0.0];
    }
    values[17] = [
        settings.grading.balance,
        settings.grading.blending,
        settings.grading.amount,
        0.0,
    ];
    values[18] = [pixel_count as f32, width as f32, height as f32, 0.0];
    values[19] = [
        (!settings.curve.is_empty() || settings.curves != ToneCurveSet::default()) as u8 as f32,
        (settings.color_mixer != ColorMixer::default()) as u8 as f32,
        (settings.grading != GradingParameters::default()) as u8 as f32,
        (gpu_can_fuse_vignette(settings) && settings.vignette.amount.abs() > f32::EPSILON) as u8
            as f32,
    ];
    GpuCreativeParameters { values }
}

fn apply_creative_graph_mapped(
    pixels: Vec<[f32; 3]>,
    width: usize,
    height: usize,
    settings: &RenderSettings,
    gpu: Option<&GpuRenderer>,
    semantic_map: Option<SemanticSamplingMap>,
) -> Result<Vec<f32>, PipelineError> {
    validate_mask_rasters(&settings.portrait_masks, &settings.generated_masks)?;
    // Encoded relative-WB is prepared by the CPU color oracle. The complete global creative
    // chain after that boundary is fused on GPU; local layers remain a separate cached composite.
    let pixel_count = pixels.len();
    checkpoint()?;
    let working_bytes = (pixel_count as u64).saturating_mul(3 * F32_BYTES);
    let prepared = profiling::measure(ProfileStage::WhiteBalance, working_bytes, || {
        if gpu.is_some() || settings.relative_color == RelativeColorParameters::default() {
            pixels
                .into_par_iter()
                .map(|pixel| LinearRgb {
                    r: pixel[0],
                    g: pixel[1],
                    b: pixel[2],
                })
                .collect::<Vec<_>>()
        } else {
            pixels
                .into_par_iter()
                .map(|pixel| {
                    apply_relative_color(
                        LinearRgb {
                            r: pixel[0],
                            g: pixel[1],
                            b: pixel[2],
                        },
                        settings.relative_color,
                    )
                })
                .collect::<Vec<_>>()
        }
    });
    let (prepared, tone_parameters, gpu_creative) = if let Some(renderer) = gpu {
        let input: Vec<[f32; 4]> = prepared
            .iter()
            .map(|rgb| [rgb.r, rgb.g, rgb.b, 1.0])
            .collect();
        let started = Instant::now();
        let parameters = gpu_creative_parameters(settings, pixel_count, width, height);
        let luts = gpu_curve_luts(settings);
        let exposed = profiling::measure(ProfileStage::Tone, working_bytes, || {
            renderer.apply_creative(
                &input,
                &parameters,
                &luts,
                settings
                    .gpu_cache_keys
                    .as_ref()
                    .map(|keys| keys.global_creative.as_str()),
            )
        })?;
        let elapsed = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        profiling::record_gpu(ProfileStage::Tone, elapsed);
        (
            exposed
                .into_iter()
                .map(|pixel| LinearRgb {
                    r: pixel[0],
                    g: pixel[1],
                    b: pixel[2],
                })
                .collect::<Vec<_>>(),
            ToneParameters::default(),
            true,
        )
    } else {
        (prepared, settings.tone, false)
    };
    let mut prepared = prepared;
    checkpoint()?;
    profiling::measure(ProfileStage::Tone, working_bytes, || {
        if tone_parameters != ToneParameters::default() {
            prepared
                .par_iter_mut()
                .for_each(|rgb| *rgb = apply_tone(*rgb, tone_parameters));
        }
    });
    checkpoint()?;
    profiling::measure(ProfileStage::Curve, working_bytes, || {
        if !gpu_creative
            && (!settings.curve.is_empty() || settings.curves != ToneCurveSet::default())
        {
            let master = starroom_color::PreparedCurve::new(if settings.curves.master.len() >= 2 {
                &settings.curves.master
            } else {
                &settings.curve
            });
            let red = starroom_color::PreparedCurve::new(&settings.curves.red);
            let green = starroom_color::PreparedCurve::new(&settings.curves.green);
            let blue = starroom_color::PreparedCurve::new(&settings.curves.blue);
            prepared.par_iter_mut().for_each(|rgb| {
                rgb.r = red.map(master.map(rgb.r));
                rgb.g = green.map(master.map(rgb.g));
                rgb.b = blue.map(master.map(rgb.b));
            });
        }
    });
    checkpoint()?;
    profiling::measure(ProfileStage::ColorMixer, working_bytes, || {
        if !gpu_creative && settings.color_mixer != ColorMixer::default() {
            prepared
                .par_iter_mut()
                .for_each(|rgb| *rgb = apply_color_mixer(*rgb, settings.color_mixer));
        }
    });
    checkpoint()?;
    profiling::measure(ProfileStage::ColorGrading, working_bytes, || {
        if !gpu_creative && settings.grading != GradingParameters::default() {
            prepared
                .par_iter_mut()
                .for_each(|rgb| *rgb = apply_grading(*rgb, settings.grading));
        }
    });
    checkpoint()?;
    profiling::measure(ProfileStage::Mask, working_bytes, || {
        if !settings.layers.is_empty() {
            let layers = settings
                .layers
                .iter()
                .map(PreparedLayer::new)
                .collect::<Result<Vec<_>, _>>()?;
            for (index, rgb) in prepared.iter_mut().enumerate() {
                if index % 4096 == 0 {
                    checkpoint()?;
                }
                let local_x = index % width;
                let local_y = index / width;
                let (x, y) = settings.source_region.map_or_else(
                    || {
                        (
                            local_x as f32 / width.max(1) as f32,
                            local_y as f32 / height.max(1) as f32,
                        )
                    },
                    |region| {
                        (
                            (region.x as usize + local_x) as f32 / region.full_width.max(1) as f32,
                            (region.y as usize + local_y) as f32 / region.full_height.max(1) as f32,
                        )
                    },
                );
                *rgb = apply_prepared_layers(
                    *rgb,
                    &layers,
                    x,
                    y,
                    &settings.portrait_masks,
                    &settings.generated_masks,
                    semantic_map,
                )?;
            }
        }
        Ok::<_, PipelineError>(())
    })?;
    let mut data = Vec::with_capacity(pixel_count * 3);
    for rgb in prepared {
        if !rgb.r.is_finite() || !rgb.g.is_finite() || !rgb.b.is_finite() {
            return Err(PipelineError::InvalidDecodedBuffer);
        }
        data.extend_from_slice(&[rgb.r, rgb.g, rgb.b]);
    }
    checkpoint()?;
    let data = profiling::measure(ProfileStage::Skin, working_bytes, || {
        apply_skin_retouch_stage(data, width, height, settings, semantic_map)
    })?;
    checkpoint()?;
    profiling::measure(ProfileStage::Healing, working_bytes, || {
        apply_healing_stage(data, width, height, settings)
    })
}

#[cfg(test)]
fn apply_creative_graph(
    pixels: Vec<[f32; 3]>,
    width: usize,
    height: usize,
    settings: &RenderSettings,
    gpu: Option<&GpuRenderer>,
) -> Result<Vec<f32>, PipelineError> {
    apply_creative_graph_mapped(pixels, width, height, settings, gpu, None)
}

fn to_working_image(
    decoded: &DecodedRenderedImage,
    settings: &RenderSettings,
) -> Result<(LinearImage, InputProfileSource), PipelineError> {
    checkpoint()?;
    let expected = decoded.width as usize * decoded.height as usize * 4;
    if decoded.rgba.len() != expected {
        return Err(PipelineError::InvalidDecodedBuffer);
    }

    let mut pixels: Vec<[f32; 3]> = decoded
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|rgba| [rgba[0], rgba[1], rgba[2]])
        .collect();
    let working_bytes = (pixels.len() as u64).saturating_mul(3 * F32_BYTES);
    let input_source = profiling::measure(ProfileStage::CameraTransform, working_bytes, || {
        LittleCmsProvider.input_to_working(
            &mut pixels,
            decoded.embedded_icc.as_deref(),
            settings.color_management.intent,
            settings.color_management.black_point_compensation,
        )
    })?;

    profiling::measure(ProfileStage::WhiteBalance, working_bytes, || {
        apply_white_balance(
            &mut pixels,
            decoded.width,
            decoded.height,
            SourceKind::Encoded,
            source_white_balance(settings.white_balance),
        )
    })?;
    let data = pixels.into_iter().flatten().collect();

    let image = LinearImage::new(decoded.width as usize, decoded.height as usize, data)
        .map_err(|_| PipelineError::DetailBuffer)?;
    Ok((image, input_source))
}

fn to_working_raw(
    decoded: &DecodedRawImage,
    settings: &RenderSettings,
) -> Result<LinearImage, PipelineError> {
    let expected = decoded.width as usize * decoded.height as usize * 3;
    if decoded.rgb.len() != expected {
        return Err(PipelineError::InvalidDecodedBuffer);
    }
    let mut pixels: Vec<[f32; 3]> = decoded
        .rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    let working_bytes = (pixels.len() as u64).saturating_mul(3 * F32_BYTES);
    profiling::measure(ProfileStage::WhiteBalance, working_bytes, || {
        apply_white_balance(
            &mut pixels,
            decoded.width,
            decoded.height,
            SourceKind::Raw,
            source_white_balance(settings.white_balance),
        )
    })?;
    let data = pixels.into_iter().flatten().collect();
    LinearImage::new(decoded.width as usize, decoded.height as usize, data)
        .map_err(|_| PipelineError::DetailBuffer)
}

/// Samples the native post-lens/post-geometry working image for M7's targeted Color Mixer tool.
/// Sampling precedes creative adjustments to avoid a circular Mixer target. RAW and encoded
/// inputs use the same source profile, WB and geometry graph as preview/export; the browser
/// receives only the selected enum, never image pixels or color-science computations.
pub fn sample_source_color_band(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
    x: f32,
    y: f32,
) -> Result<Option<ColorBand>, PipelineError> {
    if !x.is_finite() || !y.is_finite() || !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return Err(PipelineError::InvalidDecodedBuffer);
    }
    // The pointer lives on the post-Lens/post-Geometry canvas, not the sensor/source frame.
    // Reuse the actual native graph instead of maintaining a second coordinate approximation.
    let image = prepare_source_for_ai_denoise(decoded, settings)?;
    let (width, height) = (image.width, image.height);
    let px = ((x * width as f32).floor() as usize).min(width.saturating_sub(1));
    let py = ((y * height as f32).floor() as usize).min(height.saturating_sub(1));
    let offset = (py * width + px) * 3;
    Ok(sample_color_band(LinearRgb {
        r: image.data[offset],
        g: image.data[offset + 1],
        b: image.data[offset + 2],
    }))
}

fn render_working_graph(
    working: LinearImage,
    input_source: InputProfileSource,
    camera_profile: Option<&CameraProfileDescriptor>,
    settings: &RenderSettings,
    optics_resolution: Option<&LensProfileResolution>,
    output_icc: Option<&[u8]>,
    gpu: Option<&GpuRenderer>,
) -> Result<RenderedRgbF32, PipelineError> {
    let (geometry_image, semantic_map) =
        apply_precreative_geometry_mapped(working, settings, optics_resolution)?;
    render_prepared_working_graph(
        geometry_image,
        input_source,
        camera_profile,
        settings,
        output_icc,
        gpu,
        semantic_map,
    )
}

#[cfg(test)]
fn apply_precreative_geometry(
    working: LinearImage,
    settings: &RenderSettings,
    optics_resolution: Option<&LensProfileResolution>,
) -> Result<LinearImage, PipelineError> {
    apply_precreative_geometry_mapped(working, settings, optics_resolution).map(|(image, _)| image)
}

fn apply_precreative_geometry_mapped(
    working: LinearImage,
    settings: &RenderSettings,
    optics_resolution: Option<&LensProfileResolution>,
) -> Result<(LinearImage, SemanticSamplingMap), PipelineError> {
    checkpoint()?;
    let working_bytes = (working.data.len() as u64).saturating_mul(F32_BYTES);
    let mut lens_mapping = None;
    let optically_corrected = if settings.optics.parameters.enabled {
        let resolution = optics_resolution.ok_or(PipelineError::OpticsProfile(
            LensProfileStatus::MissingMetadata,
        ))?;
        let correction = resolution
            .correction
            .ok_or_else(|| PipelineError::OpticsProfile(resolution.status.clone()))?;
        let corrected = profiling::measure(ProfileStage::Lens, working_bytes, || {
            apply_lens_correction(
                working.width,
                working.height,
                &working.data,
                correction,
                settings.optics.parameters,
            )
        })
        .map_err(|_| PipelineError::OpticsCorrection)?;
        lens_mapping = Some((
            correction,
            corrected.auto_scale,
            settings.optics.parameters.distortion,
        ));
        LinearImage::new(working.width, working.height, corrected.data)
            .map_err(|_| PipelineError::DetailBuffer)?
    } else {
        working
    };
    checkpoint()?;
    let geometry_parameters = if settings.geometry.upright_mode != UprightMode::Off {
        let analysis = analyze_upright(
            optically_corrected.width,
            optically_corrected.height,
            &optically_corrected.data,
            settings.geometry.upright_mode,
        );
        apply_upright(settings.geometry, analysis)
    } else {
        settings.geometry
    };
    let crop = constrain_crop_aspect(
        geometry_parameters.crop,
        optically_corrected.width,
        optically_corrected.height,
        geometry_parameters.crop_aspect_width,
        geometry_parameters.crop_aspect_height,
    );
    let (mut image, inverse_geometry) = if geometry_parameters == GeometryParameters::default() {
        (
            profiling::measure(ProfileStage::Geometry, working_bytes, || {
                optically_corrected
            }),
            Matrix3::IDENTITY,
        )
    } else {
        let geometrically_corrected =
            profiling::measure(ProfileStage::Geometry, working_bytes, || {
                apply_geometry(
                    optically_corrected.width,
                    optically_corrected.height,
                    &optically_corrected.data,
                    geometry_parameters,
                )
            })
            .map_err(|_| PipelineError::Geometry)?;
        let inverse = geometrically_corrected
            .transform
            .inverse()
            .ok_or(PipelineError::Geometry)?;
        (
            LinearImage::new(
                geometrically_corrected.width,
                geometrically_corrected.height,
                geometrically_corrected.data,
            )
            .map_err(|_| PipelineError::DetailBuffer)?,
            inverse,
        )
    };
    if settings.white_balance.mode == WhiteBalanceMode::NeutralPicker {
        profiling::measure(ProfileStage::WhiteBalance, working_bytes, || {
            let sample = settings
                .white_balance
                .sample
                .ok_or(PipelineError::InvalidWhiteBalanceSample)?;
            let neutral = picker_white_balance_neutral(
                image.data.as_chunks::<3>().0,
                image.width as u32,
                image.height as u32,
                sample,
            )
            .ok_or(PipelineError::InvalidWhiteBalanceSample)?;
            apply_white_balance_matrix(
                image.data.as_chunks_mut::<3>().0,
                sampled_white_balance_matrix(neutral)?,
            );
            Ok::<_, PipelineError>(())
        })?;
    }
    let semantic_map = SemanticSamplingMap {
        inverse_geometry,
        crop,
        output_width: settings
            .source_region
            .map_or(image.width, |region| region.full_width as usize),
        output_height: settings
            .source_region
            .map_or(image.height, |region| region.full_height as usize),
        lens: lens_mapping,
    };
    Ok((image, semantic_map))
}

fn render_prepared_working_graph(
    geometry_image: LinearImage,
    input_source: InputProfileSource,
    camera_profile: Option<&CameraProfileDescriptor>,
    settings: &RenderSettings,
    output_icc: Option<&[u8]>,
    gpu: Option<&GpuRenderer>,
    semantic_map: SemanticSamplingMap,
) -> Result<RenderedRgbF32, PipelineError> {
    checkpoint()?;
    // M21 is intentionally before tone/curve/mixer/grading. Inference and control adjustment
    // caches are separate; an enabled request without its native residual is a typed failure.
    let working_bytes = (geometry_image.data.len() as u64).saturating_mul(F32_BYTES);
    let model_adjusted = if settings.ai_denoise.enabled {
        let residual = settings
            .ai_denoise_residual
            .as_ref()
            .ok_or(AiDenoiseError::ResidualMismatch)?;
        let skin = mapped_portrait_region_weights(
            geometry_image.width,
            geometry_image.height,
            settings,
            semantic_map,
            PortraitMaskRegion::Skin,
        )?;
        profiling::measure(ProfileStage::AiDenoise, working_bytes, || {
            apply_residual(
                &geometry_image,
                residual,
                settings.ai_denoise,
                skin.as_deref(),
            )
        })?
    } else {
        geometry_image
    };
    let width = model_adjusted.width as u32;
    let height = model_adjusted.height as u32;
    let pixels: Vec<[f32; 3]> = model_adjusted
        .data
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    let creative = LinearImage::new(
        model_adjusted.width,
        model_adjusted.height,
        apply_creative_graph_mapped(
            pixels,
            model_adjusted.width,
            model_adjusted.height,
            settings,
            gpu,
            Some(semantic_map),
        )?,
    )
    .map_err(|_| PipelineError::DetailBuffer)?;
    checkpoint()?;
    let detailed = profiling::measure(ProfileStage::Detail, working_bytes, || {
        if gpu.is_some()
            && gpu_can_fuse_vignette(settings)
            && settings.vignette.amount.abs() > f32::EPSILON
        {
            // Only a mathematically safe identity-tail vignette is already fused. Non-commuting
            // combinations retain the canonical shared finishing order below.
            let mut detail_settings = settings.clone();
            detail_settings.vignette = VignetteSettings::default();
            apply_detail_stage(creative, &detail_settings)
        } else {
            apply_detail_stage(creative, settings)
        }
    })?;
    let mut pixels = detailed
        .data
        .par_chunks_exact(3)
        .map(|pixel| {
            let working_rgb = compress_to_unit_gamut(LinearRgb {
                r: pixel[0],
                g: pixel[1],
                b: pixel[2],
            });
            [working_rgb.r, working_rgb.g, working_rgb.b]
        })
        .collect::<Vec<_>>();
    checkpoint()?;
    let output_source = profiling::measure(ProfileStage::ColorTransform, working_bytes, || {
        LittleCmsProvider.working_to_output(
            &mut pixels,
            output_icc,
            settings.color_management.intent,
            settings.color_management.black_point_compensation,
        )
    })?;
    let output = pixels.into_iter().flatten().collect();
    Ok(RenderedRgbF32 {
        width,
        height,
        data: output,
        color: ColorTransformReport {
            input: input_source,
            output: output_source,
            working_space: "linear Rec.2020 D65",
            camera_profile_id: camera_profile.map(|profile| profile.id.clone()),
            camera_profile_hash: camera_profile.map(|profile| profile.hash.clone()),
        },
    })
}

fn detail_stage_is_identity(settings: &RenderSettings) -> bool {
    settings.denoise.luminance.abs() <= f32::EPSILON
        && settings.denoise.chroma.abs() <= f32::EPSILON
        && settings.denoise.high_iso.abs() <= f32::EPSILON
        && settings.local_detail.texture.abs() <= f32::EPSILON
        && settings.local_detail.clarity.abs() <= f32::EPSILON
        && settings.local_detail.dehaze.abs() <= f32::EPSILON
        && settings.sharpen.amount.abs() <= f32::EPSILON
        && settings.grain.amount.abs() <= f32::EPSILON
        && settings.vignette.amount.abs() <= f32::EPSILON
}

fn apply_detail_stage(
    image: LinearImage,
    settings: &RenderSettings,
) -> Result<LinearImage, PipelineError> {
    if detail_stage_is_identity(settings) {
        return Ok(image);
    }
    let denoised = denoise(&image, settings.denoise);
    let locally_adjusted = local_detail(&denoised, settings.local_detail);
    let detailed = sharpen(&locally_adjusted, settings.sharpen);
    apply_finishing_effects(
        &detailed,
        settings.grain,
        settings.vignette,
        &settings.image_identity,
    )
    .map_err(PipelineError::from)
}

/// Returns the exact Linear Rec.2020 D65 image presented to M21, after source colour, optics,
/// orientation/crop and geometry, but before NAFNet and every tone/creative/detail stage.
pub fn prepare_source_for_ai_denoise(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
) -> Result<LinearImage, PipelineError> {
    prepare_source_with_sampling_map(decoded, settings).map(|(image, _)| image)
}

fn prepare_source_with_sampling_map(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
) -> Result<(LinearImage, SemanticSamplingMap), PipelineError> {
    let optics_resolution = if settings.optics.parameters.enabled {
        Some(resolve_source_lens_profile(decoded, &settings.optics)?)
    } else {
        None
    };
    let working = match decoded {
        DecodedSourceImage::Rendered(image) => to_working_image(image, settings)?.0,
        DecodedSourceImage::Raw(image) => to_working_raw(image, settings)?,
    };
    apply_precreative_geometry_mapped(working, settings, optics_resolution.as_ref())
}

fn mapped_portrait_region_weights(
    width: usize,
    height: usize,
    settings: &RenderSettings,
    semantic_map: SemanticSamplingMap,
    region: PortraitMaskRegion,
) -> Result<Option<Vec<f32>>, PipelineError> {
    let rasters = settings
        .portrait_masks
        .iter()
        .filter(|mask| mask.region == region)
        .collect::<Vec<_>>();
    if rasters.is_empty() {
        return Ok(None);
    }
    for raster in &rasters {
        raster.validate()?;
    }
    let mut weights = Vec::with_capacity(width * height);
    for y in 0..height {
        checkpoint()?;
        for x in 0..width {
            let (x, y) = settings.source_region.map_or_else(
                || (x as f32 / width as f32, y as f32 / height as f32),
                |region| {
                    (
                        (region.x as usize + x) as f32 / region.full_width.max(1) as f32,
                        (region.y as usize + y) as f32 / region.full_height.max(1) as f32,
                    )
                },
            );
            let mut weight = 0.0_f32;
            if let Some(source) = semantic_map.source_point(x, y) {
                for raster in &rasters {
                    weight = weight.max(raster.weight_at(source.x, source.y)?);
                }
            }
            weights.push(weight);
        }
    }
    Ok(Some(weights))
}

/// Native-only coverage in the same post-lens/post-geometry pixel grid as the rendered image.
/// The advisor and denoise protection reuse the shared mapping rather than separately guessing
/// source coordinates. Source rasters and their model/cache identities remain immutable.
pub fn sample_source_portrait_weights(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
    region: PortraitMaskRegion,
) -> Result<Option<Vec<f32>>, PipelineError> {
    if !settings
        .portrait_masks
        .iter()
        .any(|mask| mask.region == region)
    {
        return Ok(None);
    }
    let (image, map) = prepare_source_with_sampling_map(decoded, settings)?;
    mapped_portrait_region_weights(image.width, image.height, settings, map, region)
}

fn render_shared_graph(
    decoded: &DecodedRenderedImage,
    settings: &RenderSettings,
    output_icc: Option<&[u8]>,
    gpu: Option<&GpuRenderer>,
) -> Result<RenderedRgbF32, PipelineError> {
    let (working, input_source) = to_working_image(decoded, settings)?;
    let resolution = if settings.optics.parameters.enabled {
        Some(resolve_rendered_optics(decoded, &settings.optics)?)
    } else {
        None
    };
    render_working_graph(
        working,
        input_source,
        None,
        settings,
        resolution.as_ref(),
        output_icc,
        gpu,
    )
}

fn render_shared_source_graph(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
    output_icc: Option<&[u8]>,
    gpu: Option<&GpuRenderer>,
) -> Result<RenderedRgbF32, PipelineError> {
    let optics_resolution = if settings.optics.parameters.enabled {
        Some(resolve_source_lens_profile(decoded, &settings.optics)?)
    } else {
        None
    };
    match decoded {
        DecodedSourceImage::Rendered(image) => {
            let (working, input_source) = to_working_image(image, settings)?;
            render_working_graph(
                working,
                input_source,
                None,
                settings,
                optics_resolution.as_ref(),
                output_icc,
                gpu,
            )
        }
        DecodedSourceImage::Raw(image) => {
            let input_source = match image.metadata.camera_profile.status {
                CameraProfileStatus::Resolved => InputProfileSource::RawCameraMatrix,
                CameraProfileStatus::Generic => InputProfileSource::RawGenericProfile,
            };
            render_working_graph(
                to_working_raw(image, settings)?,
                input_source,
                Some(&image.metadata.camera_profile),
                settings,
                optics_resolution.as_ref(),
                output_icc,
                gpu,
            )
        }
    }
}

fn rendered_lens_identity(image: &DecodedRenderedImage) -> LensIdentity {
    let metadata = lens_metadata(image);
    LensIdentity {
        camera_make: metadata.camera_make,
        camera_model: metadata.camera_model,
        lens_make: metadata.lens_make,
        lens_model: metadata.lens_model,
        focal_length_mm: metadata.focal_length_mm.unwrap_or(0.0),
        aperture: metadata.aperture.unwrap_or(0.0),
        focus_distance_m: metadata.focus_distance_m,
    }
}

fn raw_lens_identity(image: &DecodedRawImage) -> LensIdentity {
    LensIdentity {
        camera_make: image.metadata.make.clone(),
        camera_model: image.metadata.model.clone(),
        lens_make: image.metadata.lens_make.clone(),
        lens_model: image.metadata.lens_model.clone(),
        focal_length_mm: image.metadata.focal_length_mm,
        aperture: image.metadata.aperture,
        focus_distance_m: image.metadata.focus_distance_m,
    }
}

fn resolve_rendered_optics(
    image: &DecodedRenderedImage,
    settings: &OpticsSettings,
) -> Result<LensProfileResolution, PipelineError> {
    let identity = settings
        .manual_identity
        .as_ref()
        .cloned()
        .unwrap_or_else(|| rendered_lens_identity(image));
    LensfunProvider
        .resolve_profile(&identity, settings.match_mode)
        .map_err(|error| PipelineError::OpticsDatabase(format!("{error:?}")))
}

pub fn resolve_source_lens_profile(
    decoded: &DecodedSourceImage,
    settings: &OpticsSettings,
) -> Result<LensProfileResolution, PipelineError> {
    let identity = settings
        .manual_identity
        .as_ref()
        .cloned()
        .unwrap_or_else(|| match decoded {
            DecodedSourceImage::Rendered(image) => rendered_lens_identity(image),
            DecodedSourceImage::Raw(image) => raw_lens_identity(image),
        });
    LensfunProvider
        .resolve_profile(&identity, settings.match_mode)
        .map_err(|error| PipelineError::OpticsDatabase(format!("{error:?}")))
}

/// Preview and export deliberately enter the same graph; only the requested output profile differs.
pub fn render_preview_to_srgb8(
    decoded: &DecodedRenderedImage,
    settings: &RenderSettings,
) -> Result<RenderedRgb8, PipelineError> {
    render_shared_graph(decoded, settings, None, None).map(RenderedRgbF32::into_rgb8)
}

pub fn render_preview_to_display_icc8(
    decoded: &DecodedRenderedImage,
    settings: &RenderSettings,
    display_icc: &[u8],
) -> Result<RenderedRgb8, PipelineError> {
    render_shared_graph(decoded, settings, Some(display_icc), None).map(RenderedRgbF32::into_rgb8)
}

pub fn render_export_to_srgb8(
    decoded: &DecodedRenderedImage,
    settings: &RenderSettings,
) -> Result<RenderedRgb8, PipelineError> {
    render_shared_graph(decoded, settings, None, None).map(RenderedRgbF32::into_rgb8)
}

pub fn render_export_to_icc8(
    decoded: &DecodedRenderedImage,
    settings: &RenderSettings,
    output_icc: &[u8],
) -> Result<RenderedRgb8, PipelineError> {
    render_shared_graph(decoded, settings, Some(output_icc), None).map(RenderedRgbF32::into_rgb8)
}

pub fn render_source_preview_to_srgb8(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
) -> Result<RenderedRgb8, PipelineError> {
    render_shared_source_graph(decoded, settings, None, None).map(RenderedRgbF32::into_rgb8)
}

pub fn render_source_export_to_srgb8(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
) -> Result<RenderedRgb8, PipelineError> {
    render_shared_source_graph(decoded, settings, None, None).map(RenderedRgbF32::into_rgb8)
}

pub fn render_source_export_to_icc8(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
    output_icc: &[u8],
) -> Result<RenderedRgb8, PipelineError> {
    render_shared_source_graph(decoded, settings, Some(output_icc), None)
        .map(RenderedRgbF32::into_rgb8)
}

/// Full-resolution export surface that preserves the float output of the shared graph until the
/// selected file encoder performs its one and only quantization step.
pub fn render_source_export_to_srgb_f32(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
) -> Result<RenderedRgbF32, PipelineError> {
    render_shared_source_graph(decoded, settings, None, None)
}

pub fn render_source_export_to_icc_f32(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
    output_icc: &[u8],
) -> Result<RenderedRgbF32, PipelineError> {
    render_shared_source_graph(decoded, settings, Some(output_icc), None)
}

pub fn profile_source_export_to_srgb_f32(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
) -> (
    Result<RenderedRgbF32, PipelineError>,
    starroom_render::profiling::RenderProfile,
) {
    profiling::capture(|| render_shared_source_graph(decoded, settings, None, None))
}

pub fn profile_source_preview_to_srgb8(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
) -> (
    Result<RenderedRgb8, PipelineError>,
    starroom_render::profiling::RenderProfile,
) {
    profiling::capture(|| {
        render_shared_source_graph(decoded, settings, None, None).map(RenderedRgbF32::into_rgb8)
    })
}

/// M12 preview entry point. It shares all decode, colour-management, tone, geometry, detail and
/// output stages with export; only the exposure node is delegated to a parity-checked GPU kernel.
/// A caller must decide and report fallback rather than silently retrying this path on CPU.
pub fn render_source_preview_with_gpu_to_srgb8(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
    gpu: &GpuRenderer,
) -> Result<RenderedRgb8, PipelineError> {
    render_shared_source_graph(decoded, settings, None, Some(gpu)).map(RenderedRgbF32::into_rgb8)
}

/// Compatibility entry point. New callers should name preview or export explicitly.
pub fn render_to_srgb8(
    decoded: &DecodedRenderedImage,
    settings: &RenderSettings,
) -> Result<RenderedRgb8, PipelineError> {
    render_preview_to_srgb8(decoded, settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use starroom_ai_denoise::ExecutionProvider as AiExecutionProvider;
    use starroom_color::{BandAdjustment, ColorBand};
    use starroom_grading::ColorWheel;
    use starroom_imageio::RenderedFormat;
    use starroom_optics::LensMatchMode;

    fn fixture(values: &[[f32; 4]]) -> DecodedRenderedImage {
        DecodedRenderedImage {
            width: values.len() as u32,
            height: 1,
            format: RenderedFormat::Png,
            rgba: values
                .iter()
                .flat_map(|pixel| pixel.iter().copied())
                .collect(),
            embedded_icc: None,
            exif: None,
        }
    }

    fn geometry_fixture() -> DecodedSourceImage {
        let mut image = fixture(&[
            [0.72, 0.12, 0.09, 1.0],
            [0.12, 0.68, 0.15, 1.0],
            [0.10, 0.14, 0.73, 1.0],
            [0.68, 0.58, 0.10, 1.0],
        ]);
        image.width = 2;
        image.height = 2;
        DecodedSourceImage::Rendered(image)
    }

    fn sampling_geometries() -> [GeometryParameters; 4] {
        [
            GeometryParameters {
                rotation_degrees: 90.0,
                ..Default::default()
            },
            GeometryParameters {
                flip_horizontal: true,
                ..Default::default()
            },
            GeometryParameters {
                flip_vertical: true,
                ..Default::default()
            },
            GeometryParameters {
                crop: CropRect {
                    left: 0.5,
                    ..Default::default()
                },
                ..Default::default()
            },
        ]
    }

    #[test]
    fn native_sampling_uses_post_geometry_rotation_crop_and_flip() {
        let source = geometry_fixture();
        let settings = RenderSettings::default();
        let red = sample_source_color_band(&source, &settings, 0.0, 0.0).unwrap();
        let green = sample_source_color_band(&source, &settings, 0.75, 0.0).unwrap();
        let blue = sample_source_color_band(&source, &settings, 0.0, 0.75).unwrap();
        assert!(red.is_some() && green.is_some() && blue.is_some());
        assert_ne!(red, green);
        assert_ne!(red, blue);
        for (geometry, expected) in sampling_geometries().into_iter().zip([blue, green, blue]) {
            let settings = RenderSettings {
                geometry,
                ..Default::default()
            };
            assert_eq!(
                sample_source_color_band(&source, &settings, 0.0, 0.0).unwrap(),
                expected
            );
        }
        // A narrow crop removes the red column; its first visible pixel is the original green.
        let crop = RenderSettings {
            geometry: GeometryParameters {
                crop: CropRect {
                    left: 0.9,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            sample_source_color_band(&source, &crop, 0.0, 0.0).unwrap(),
            green
        );
        assert!(sample_source_color_band(&source, &settings, f32::NAN, 0.0).is_err());
    }

    #[test]
    fn neutral_picker_samples_the_visible_rotated_cropped_flipped_pixel() {
        let source = geometry_fixture();
        for geometry in sampling_geometries() {
            let settings = RenderSettings {
                geometry,
                white_balance: WhiteBalanceSettings {
                    mode: WhiteBalanceMode::NeutralPicker,
                    sample: Some(WhiteBalanceSample {
                        x: 0.0,
                        y: 0.0,
                        width: 0.1,
                        height: 0.1,
                    }),
                },
                ..Default::default()
            };
            let visible = prepare_source_for_ai_denoise(&source, &settings).unwrap();
            let picked = &visible.data[..3];
            assert!(
                (picked[0] - picked[1]).abs() < 1.0e-6,
                "geometry={geometry:?} picked={picked:?}"
            );
            assert!((picked[1] - picked[2]).abs() < 1.0e-6);
            assert_eq!(
                render_source_preview_to_srgb8(&source, &settings).unwrap(),
                render_source_export_to_srgb8(&source, &settings).unwrap()
            );
        }
    }

    #[test]
    fn auto_white_balance_remains_full_source_and_ignores_picker_rectangles() {
        let source = geometry_fixture();
        let base = RenderSettings {
            white_balance: WhiteBalanceSettings {
                mode: WhiteBalanceMode::Auto,
                sample: None,
            },
            ..Default::default()
        };
        let full = prepare_source_for_ai_denoise(&source, &base).unwrap();
        for geometry in sampling_geometries() {
            let expected = apply_geometry(full.width, full.height, &full.data, geometry).unwrap();
            let settings = RenderSettings {
                geometry,
                white_balance: WhiteBalanceSettings {
                    mode: WhiteBalanceMode::Auto,
                    sample: Some(WhiteBalanceSample {
                        x: 0.0,
                        y: 0.0,
                        width: 0.1,
                        height: 0.1,
                    }),
                },
                ..base.clone()
            };
            let actual = prepare_source_for_ai_denoise(&source, &settings).unwrap();
            assert_eq!(
                actual.data, expected.data,
                "Auto WB must not become a one-point picker"
            );
        }
    }

    fn semantic_selection_settings(mask: MaskDefinition) -> RenderSettings {
        RenderSettings {
            layers: vec![NativeAdjustmentLayer {
                id: "source-semantic".into(),
                name: "Source semantic".into(),
                enabled: true,
                opacity: 1.0,
                blend_mode: LayerBlendMode::Normal,
                mask: mask.into(),
                adjustments: LayerAdjustments {
                    tone: ToneParameters {
                        exposure_ev: 0.6,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            }],
            portrait_masks: vec![PortraitMaskRaster {
                cache_key: "immutable-source-face".into(),
                face_id: "face".into(),
                region: PortraitMaskRegion::Skin,
                width: 2,
                height: 2,
                values: vec![1.0, 0.0, 0.0, 0.0],
            }],
            generated_masks: vec![GeneratedMaskRaster {
                cache_identity: "immutable-source-subject".into(),
                semantic: GeneratedMaskSemantic::Subject,
                width: 2,
                height: 2,
                values: vec![1.0, 0.0, 0.0, 0.0],
            }],
            ..Default::default()
        }
    }

    fn semantic_test_masks() -> [MaskDefinition; 2] {
        [
            MaskDefinition::PortraitSemantic {
                face_id: "face".into(),
                region: PortraitMaskRegion::Skin,
                threshold: 0.5,
                feather: 0.0,
                model_id: "local-face".into(),
                model_version: "verified-local".into(),
                model_hash: "a".repeat(64),
                cache_key: "immutable-source-face".into(),
                source_crop: None,
            },
            MaskDefinition::Generated {
                provider_id: "local-subject".into(),
                model_id: "verified-subject".into(),
                model_version: "fixture".into(),
                model_hash: "b".repeat(64),
                semantic_class: GeneratedMaskSemantic::Subject,
                threshold: 0.5,
                feather: 0.0,
                invert: false,
                cache_identity: "immutable-source-subject".into(),
                metadata: Default::default(),
            },
        ]
    }

    #[test]
    fn source_semantic_rasters_follow_rotation_crop_flip_without_cache_mutation() {
        let source = geometry_fixture();
        for mask in semantic_test_masks() {
            for (geometry, selected_pixel) in
                sampling_geometries()
                    .into_iter()
                    .zip([Some(1), Some(1), Some(2), None])
            {
                let mut settings = semantic_selection_settings(mask.clone());
                settings.geometry = geometry;
                let portrait_before = settings.portrait_masks.clone();
                let generated_before = settings.generated_masks.clone();
                let base = RenderSettings {
                    geometry,
                    ..Default::default()
                };
                let original = render_source_preview_to_srgb8(&source, &base).unwrap();
                let changed = render_source_preview_to_srgb8(&source, &settings).unwrap();
                for (index, (original, changed)) in original
                    .data
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .zip(changed.data.as_chunks::<3>().0)
                    .enumerate()
                {
                    if selected_pixel == Some(index) {
                        assert_ne!(
                            original, changed,
                            "selected source pixel must follow geometry"
                        );
                    } else {
                        assert_eq!(
                            original, changed,
                            "unselected pixel {index} changed after {geometry:?}"
                        );
                    }
                }
                assert_eq!(
                    changed,
                    render_source_export_to_srgb8(&source, &settings).unwrap()
                );
                assert_eq!(settings.portrait_masks, portrait_before);
                assert_eq!(settings.generated_masks, generated_before);
            }
        }
    }

    #[test]
    fn advisor_portrait_weights_use_the_shared_visible_grid_and_preserve_source_cache() {
        let source = geometry_fixture();
        let mut settings = semantic_selection_settings(semantic_test_masks()[0].clone());
        let source_cache = settings.portrait_masks.clone();
        for (geometry, expected) in sampling_geometries().into_iter().zip([
            vec![0.0, 1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0, 0.0],
            vec![0.0, 0.0, 1.0, 0.0],
            vec![0.0, 0.0],
        ]) {
            settings.geometry = geometry;
            let weights =
                sample_source_portrait_weights(&source, &settings, PortraitMaskRegion::Skin)
                    .unwrap()
                    .unwrap();
            assert_eq!(
                weights, expected,
                "advisor and denoise must use actual visible geometry"
            );
        }
        assert_eq!(settings.portrait_masks, source_cache);
        assert!(
            sample_source_portrait_weights(&source, &settings, PortraitMaskRegion::Hair)
                .unwrap()
                .is_none()
        );
        settings.portrait_masks[0].values[3] = f32::NAN;
        assert!(
            sample_source_portrait_weights(&source, &settings, PortraitMaskRegion::Skin).is_err()
        );
    }

    #[test]
    fn source_semantic_mapping_uses_exact_lensfun_green_channel_geometry() {
        let width = 7;
        let height = 5;
        let input: Vec<f32> = (0..height)
            .flat_map(|_| (0..width).flat_map(|x| [0.2, x as f32 / (width - 1) as f32, 0.2]))
            .collect();
        let correction = LensCorrection {
            distortion: starroom_optics::DistortionCoefficients {
                model: starroom_optics::DistortionModel::Poly3,
                k1: -0.12,
                ..Default::default()
            },
            ..Default::default()
        };
        let parameters = starroom_optics::OpticsParameters {
            enabled: true,
            tca: false,
            vignette: false,
            ..Default::default()
        };
        let corrected =
            apply_lens_correction(width, height, &input, correction, parameters).unwrap();
        let map = SemanticSamplingMap {
            inverse_geometry: Matrix3::IDENTITY,
            crop: CropRect::default(),
            output_width: width,
            output_height: height,
            lens: Some((correction, corrected.auto_scale, true)),
        };
        for y in 0..height {
            for x in 0..width {
                let point = map.source_point(x as f32 / width as f32, y as f32 / height as f32);
                if let Some(point) = point {
                    let green = corrected.data[(y * width + x) * 3 + 1];
                    assert!((point.x - green).abs() < 1.0e-6, "same Lensfun sample grid");
                }
            }
        }
        let outside = SemanticSamplingMap {
            inverse_geometry: Matrix3 {
                m: [[1.0, 0.0, 2.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            },
            ..map
        };
        assert!(
            outside.source_point(0.0, 0.0).is_none(),
            "do not clamp uncovered borders to a face"
        );
    }

    #[test]
    fn skin_retouch_and_gpu_local_layers_remap_source_rasters_after_geometry() {
        let source = geometry_fixture();
        for (geometry, selected_pixel) in
            sampling_geometries()
                .into_iter()
                .zip([Some(1), Some(1), Some(2), None])
        {
            let mut settings = semantic_selection_settings(semantic_test_masks()[0].clone());
            settings.geometry = geometry;
            settings.skin_retouch = SkinRetouchSettings {
                parameters: SkinRetouchParameters {
                    exposure_ev: 0.25,
                    ..Default::default()
                },
                faces: vec![SkinRetouchFaceReference {
                    face_id: "face".into(),
                    cache_key: "immutable-source-face".into(),
                    source_crop: None,
                }],
            };
            let original = render_source_preview_to_srgb8(
                &source,
                &RenderSettings {
                    geometry,
                    ..Default::default()
                },
            )
            .unwrap();
            let cpu = render_source_preview_to_srgb8(&source, &settings).unwrap();
            for (index, (original, changed)) in original
                .data
                .as_chunks::<3>()
                .0
                .iter()
                .zip(cpu.data.as_chunks::<3>().0)
                .enumerate()
            {
                assert_eq!(
                    original != changed,
                    selected_pixel == Some(index),
                    "skin/local selection {index} after {geometry:?}"
                );
            }
            let mut only_skin = settings.clone();
            only_skin.layers.clear();
            let skin = render_source_preview_to_srgb8(&source, &only_skin).unwrap();
            assert_eq!(
                skin,
                render_source_export_to_srgb8(&source, &only_skin).unwrap()
            );
            for (index, (original, changed)) in original
                .data
                .as_chunks::<3>()
                .0
                .iter()
                .zip(skin.data.as_chunks::<3>().0)
                .enumerate()
            {
                assert_eq!(
                    original != changed,
                    selected_pixel == Some(index),
                    "Skin alone must track source selection"
                );
            }
            match GpuRenderer::try_new() {
                Ok(gpu) => {
                    let accelerated =
                        render_source_preview_with_gpu_to_srgb8(&source, &settings, &gpu).unwrap();
                    assert_eq!(
                        (accelerated.width, accelerated.height),
                        (cpu.width, cpu.height)
                    );
                    assert!(
                        accelerated
                            .data
                            .iter()
                            .zip(&cpu.data)
                            .all(|(a, b)| i16::from(*a).abs_diff(i16::from(*b)) <= 1)
                    );
                }
                Err(error) => assert!(
                    !error.to_string().is_empty(),
                    "GPU unavailability must be explicit"
                ),
            }
        }
    }

    #[test]
    fn ai_denoise_preserve_skin_remaps_full_source_mask_to_preview_geometry() {
        let source = geometry_fixture();
        let mut settings = semantic_selection_settings(semantic_test_masks()[0].clone());
        settings.layers.clear();
        settings.geometry.rotation_degrees = 90.0;
        // A higher-resolution source mask is deliberately different from the preview size.
        settings.portrait_masks[0].width = 4;
        settings.portrait_masks[0].height = 4;
        settings.portrait_masks[0].values = vec![
            1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        ];
        settings.ai_denoise = AiDenoiseParameters {
            enabled: true,
            amount: 1.0,
            detail: 0.0,
            color_noise: 1.0,
            preserve_skin: 1.0,
        };
        settings.ai_denoise_residual = Some(AiDenoiseResidual {
            width: 2,
            height: 2,
            values: vec![-0.04; 12],
            model_hash: "fixture".into(),
            source_identity: "source".into(),
            inference_cache_key: "post-geometry-residual".into(),
            execution_provider: AiExecutionProvider::Cpu,
        });
        let base = render_source_preview_to_srgb8(
            &source,
            &RenderSettings {
                geometry: settings.geometry,
                ..Default::default()
            },
        )
        .unwrap();
        let protected = render_source_preview_to_srgb8(&source, &settings).unwrap();
        assert_eq!(
            &base.data[3..6],
            &protected.data[3..6],
            "rotated skin must retain its original detail"
        );
        assert_ne!(
            &base.data[..3],
            &protected.data[..3],
            "non-skin must receive denoise"
        );
        assert_eq!(
            protected,
            render_source_export_to_srgb8(&source, &settings).unwrap()
        );
        settings.ai_denoise.preserve_skin = 0.0;
        let unprotected = render_source_preview_to_srgb8(&source, &settings).unwrap();
        assert_ne!(&unprotected.data[3..6], &protected.data[3..6]);
    }

    #[test]
    fn m14_layers_apply_in_order_with_linear_normal_opacity() {
        let initial = LinearRgb {
            r: 0.18,
            g: 0.18,
            b: 0.18,
        };
        let brighten = NativeAdjustmentLayer {
            id: "brighten".into(),
            name: "Brighten".into(),
            enabled: true,
            opacity: 1.0,
            blend_mode: LayerBlendMode::Normal,
            mask: MaskDefinition::None.into(),
            adjustments: LayerAdjustments {
                tone: ToneParameters {
                    exposure_ev: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            },
        };
        let contrast_half = NativeAdjustmentLayer {
            id: "contrast".into(),
            name: "Contrast".into(),
            enabled: true,
            opacity: 0.5,
            blend_mode: LayerBlendMode::Normal,
            mask: MaskDefinition::None.into(),
            adjustments: LayerAdjustments {
                tone: ToneParameters {
                    contrast: 0.75,
                    ..Default::default()
                },
                ..Default::default()
            },
        };
        let output = apply_layers(
            initial,
            &[brighten.clone(), contrast_half.clone()],
            0.5,
            0.5,
            &[],
            &[],
        )
        .expect("layers");
        let reversed = apply_layers(initial, &[contrast_half, brighten], 0.5, 0.5, &[], &[])
            .expect("reversed layers");
        assert!(output.r.is_finite() && output.g.is_finite() && output.b.is_finite());
        assert!(
            (output.r - reversed.r).abs() > 0.01,
            "layer order is meaningful"
        );
    }

    #[test]
    fn source_region_keeps_local_mask_coordinates_and_renders_only_requested_tile() {
        let layer = NativeAdjustmentLayer {
            id: "dirty-radial".into(),
            name: "Dirty radial".into(),
            enabled: true,
            opacity: 1.0,
            blend_mode: LayerBlendMode::Normal,
            mask: MaskDefinition::Radial {
                x: 0.5,
                y: 0.5,
                width: 0.45,
                height: 0.45,
                rotation: 0.0,
                feather: 0.2,
                invert: false,
            }
            .into(),
            adjustments: LayerAdjustments {
                tone: ToneParameters {
                    exposure_ev: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            },
        };
        let mut full_settings = RenderSettings {
            layers: vec![layer.clone()],
            ..Default::default()
        };
        let source = vec![[0.18, 0.16, 0.14]; 16];
        let full = apply_creative_graph(source.clone(), 4, 4, &full_settings, None).unwrap();
        full_settings.source_region = Some(SourceRegion {
            full_width: 4,
            full_height: 4,
            x: 1,
            y: 1,
        });
        let crop = vec![source[5], source[6], source[9], source[10]];
        let tile = apply_creative_graph(crop, 2, 2, &full_settings, None).unwrap();
        let expected = [5usize, 6, 9, 10]
            .into_iter()
            .flat_map(|index| full[index * 3..index * 3 + 3].iter().copied())
            .collect::<Vec<_>>();
        assert_eq!(tile, expected);
    }

    #[test]
    fn source_region_maps_manual_healing_coordinates_without_full_frame_work() {
        let width = 12usize;
        let height = 5usize;
        let data = (0..width * height)
            .flat_map(|index| {
                let value = index as f32 / (width * height) as f32;
                [value, value * 0.8, value * 0.6]
            })
            .collect::<Vec<_>>();
        let operation = HealingOperation {
            id: "tile-clone".into(),
            enabled: true,
            mode: starroom_heal::HealMode::Clone,
            target: HealPoint { x: 0.72, y: 0.5 },
            source: Some(HealPoint { x: 0.28, y: 0.5 }),
            radius: 1.0,
            feather: 0.0,
            opacity: 1.0,
            rotation_degrees: 0.0,
            scale: 1.0,
            tone_adaptation: false,
            texture_adaptation: false,
            source_mode: starroom_heal::SourceMode::Manual,
            metadata: Default::default(),
        };
        let settings = RenderSettings {
            healing_operations: vec![operation],
            ..Default::default()
        };
        let full = apply_healing_stage(data.clone(), width, height, &settings).unwrap();
        let crop_x = 1usize;
        let crop_width = 10usize;
        let crop = (0..height)
            .flat_map(|y| {
                let start = (y * width + crop_x) * 3;
                data[start..start + crop_width * 3].iter().copied()
            })
            .collect::<Vec<_>>();
        let tile_settings = RenderSettings {
            source_region: Some(SourceRegion {
                full_width: width as u32,
                full_height: height as u32,
                x: crop_x as u32,
                y: 0,
            }),
            ..settings
        };
        let tile = apply_healing_stage(crop, crop_width, height, &tile_settings).unwrap();
        for y in 0..height {
            for x in 0..crop_width {
                let full_index = (y * width + x + crop_x) * 3;
                let tile_index = (y * crop_width + x) * 3;
                assert_eq!(
                    &tile[tile_index..tile_index + 3],
                    &full[full_index..full_index + 3]
                );
            }
        }
    }

    #[test]
    fn m14_layer_rejects_invalid_opacity_and_non_finite_controls() {
        let mut invalid = NativeAdjustmentLayer {
            id: "bad".into(),
            name: "Bad".into(),
            enabled: true,
            opacity: 1.2,
            blend_mode: LayerBlendMode::Normal,
            mask: MaskDefinition::None.into(),
            adjustments: LayerAdjustments::default(),
        };
        assert!(matches!(
            apply_layers(
                LinearRgb {
                    r: 0.2,
                    g: 0.2,
                    b: 0.2
                },
                &[invalid.clone()],
                0.5,
                0.5,
                &[],
                &[],
            ),
            Err(PipelineError::InvalidLayer { .. })
        ));
        invalid.opacity = 1.0;
        invalid.adjustments.tone.exposure_ev = f32::NAN;
        assert!(matches!(
            apply_layers(
                LinearRgb {
                    r: 0.2,
                    g: 0.2,
                    b: 0.2
                },
                &[invalid],
                0.5,
                0.5,
                &[],
                &[],
            ),
            Err(PipelineError::InvalidLayer { .. })
        ));
    }

    #[test]
    fn m15_mask_tree_supports_radial_brush_and_boolean_operations() {
        let radial = MaskDefinition::Radial {
            x: 0.5,
            y: 0.5,
            width: 0.4,
            height: 0.4,
            rotation: 0.0,
            feather: 0.1,
            invert: false,
        };
        let brush = MaskDefinition::Brush {
            points: vec![starroom_project::BrushPoint {
                x: 0.5,
                y: 0.5,
                pressure: 1.0,
            }],
            radius: 0.1,
            feather: 0.5,
            flow: 1.0,
            erase: false,
        };
        let tree = MaskTree::Composite(starroom_project::MaskComposite {
            operation: MaskOperation::Subtract,
            children: vec![radial.into(), brush.into()],
        });
        let center = mask_weight(
            &tree,
            0.5,
            0.5,
            LinearRgb {
                r: 0.3,
                g: 0.3,
                b: 0.3,
            },
            &[],
            &[],
        )
        .expect("mask");
        let edge = mask_weight(
            &tree,
            0.65,
            0.5,
            LinearRgb {
                r: 0.3,
                g: 0.3,
                b: 0.3,
            },
            &[],
            &[],
        )
        .expect("mask");
        assert!(center < 0.01);
        assert!(edge.is_finite() && (0.0..=1.0).contains(&edge));
    }

    #[test]
    fn m15_luminance_and_color_masks_are_finite_and_invertible() {
        let rgb = LinearRgb {
            r: 0.4,
            g: 0.4,
            b: 0.4,
        };
        let luminance = MaskDefinition::Luminance {
            minimum: 0.3,
            maximum: 0.5,
            feather: 0.02,
            invert: false,
        };
        let inverted = MaskTree::Composite(starroom_project::MaskComposite {
            operation: MaskOperation::Invert,
            children: vec![luminance.clone().into()],
        });
        let selected = mask_weight(&luminance.into(), 0.5, 0.5, rgb, &[], &[]).expect("luma");
        let inverse = mask_weight(&inverted, 0.5, 0.5, rgb, &[], &[]).expect("inverse");
        assert!((selected + inverse - 1.0).abs() < 1.0e-5);
        let color = MaskDefinition::ColorRange {
            reference: [0.4, 0.4, 0.4],
            tolerance: 0.02,
            feather: 0.1,
            invert: false,
        };
        assert!(mask_weight(&color.into(), 0.5, 0.5, rgb, &[], &[]).expect("color") > 0.99);
    }

    #[test]
    fn m15_provider_mask_never_silently_substitutes() {
        let provider = MaskDefinition::Provider {
            provider: "subject".into(),
            request: "person".into(),
            fingerprint: None,
        };
        assert!(matches!(
            mask_weight(
                &provider.into(),
                0.5,
                0.5,
                LinearRgb {
                    r: 0.2,
                    g: 0.2,
                    b: 0.2
                },
                &[],
                &[],
            ),
            Err(PipelineError::MaskProviderUnavailable { .. })
        ));
    }

    #[test]
    fn m15_layer_compositing_uses_mask_weight_before_opacity() {
        let layer = NativeAdjustmentLayer {
            id: "local-lift".into(),
            name: "Local lift".into(),
            enabled: true,
            opacity: 1.0,
            blend_mode: LayerBlendMode::Normal,
            mask: MaskDefinition::Radial {
                x: 0.5,
                y: 0.5,
                width: 0.3,
                height: 0.3,
                rotation: 0.0,
                feather: 0.0,
                invert: false,
            }
            .into(),
            adjustments: LayerAdjustments {
                tone: ToneParameters {
                    exposure_ev: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            },
        };
        let source = LinearRgb {
            r: 0.2,
            g: 0.2,
            b: 0.2,
        };
        let center =
            apply_layers(source, std::slice::from_ref(&layer), 0.5, 0.5, &[], &[]).expect("center");
        let outside = apply_layers(source, &[layer], 0.0, 0.0, &[], &[]).expect("outside");
        assert!(center.r > source.r * 1.8);
        assert!((outside.r - source.r).abs() < 1.0e-6);
    }

    #[test]
    fn m16_portrait_semantic_leaf_uses_native_cache_and_m15_boolean_algebra() {
        let leaf: MaskTree = MaskDefinition::PortraitSemantic {
            face_id: "face-1".into(),
            region: PortraitMaskRegion::Skin,
            threshold: 0.5,
            feather: 0.0,
            model_id: "parser".into(),
            model_version: "pin".into(),
            model_hash: "a".repeat(64),
            cache_key: "parse-1".into(),
            source_crop: None,
        }
        .into();
        let raster = PortraitMaskRaster {
            cache_key: "parse-1".into(),
            face_id: "face-1".into(),
            region: PortraitMaskRegion::Skin,
            width: 2,
            height: 1,
            values: vec![1.0, 0.0],
        };
        let rgb = LinearRgb {
            r: 0.2,
            g: 0.2,
            b: 0.2,
        };
        assert!(
            mask_weight(&leaf, 0.0, 0.0, rgb, std::slice::from_ref(&raster), &[]).expect("cache")
                > 0.99
        );
        assert!(mask_weight(&leaf, 1.0, 0.0, rgb, &[raster], &[]).expect("cache") < 0.01);
        assert!(matches!(
            mask_weight(&leaf, 0.0, 0.0, rgb, &[], &[]),
            Err(PipelineError::MaskProviderUnavailable { .. })
        ));
    }

    #[test]
    fn m20_generated_soft_mask_uses_m15_boolean_algebra_and_requires_cache() {
        let generated: MaskTree = MaskDefinition::Generated {
            provider_id: "foreground".into(),
            model_id: "birefnet-subject".into(),
            model_version: "v1/pinned".into(),
            model_hash: "c".repeat(64),
            semantic_class: GeneratedMaskSemantic::Subject,
            threshold: 0.5,
            feather: 0.1,
            invert: false,
            cache_identity: "subject-cache".into(),
            metadata: Default::default(),
        }
        .into();
        let tree = MaskTree::Composite(starroom_project::MaskComposite {
            operation: MaskOperation::Intersect,
            children: vec![
                generated.clone(),
                MaskDefinition::Luminance {
                    minimum: 0.1,
                    maximum: 0.8,
                    feather: 0.02,
                    invert: false,
                }
                .into(),
            ],
        });
        let raster = GeneratedMaskRaster {
            cache_identity: "subject-cache".into(),
            semantic: GeneratedMaskSemantic::Subject,
            width: 2,
            height: 1,
            values: vec![0.9, 0.1],
        };
        let rgb = LinearRgb {
            r: 0.4,
            g: 0.4,
            b: 0.4,
        };
        assert!(
            mask_weight(&tree, 0.0, 0.0, rgb, &[], std::slice::from_ref(&raster))
                .expect("generated")
                > 0.9
        );
        assert!(mask_weight(&tree, 1.0, 0.0, rgb, &[], &[raster]).expect("generated") < 0.01);
        assert!(matches!(
            mask_weight(&generated, 0.0, 0.0, rgb, &[], &[]),
            Err(PipelineError::MaskProviderUnavailable { .. })
        ));
    }

    #[test]
    fn neutral_pipeline_preserves_rendered_gray_nearly_exactly() {
        let decoded = fixture(&[[0.25, 0.25, 0.25, 1.0], [0.7, 0.7, 0.7, 1.0]]);
        let output = render_to_srgb8(&decoded, &RenderSettings::default()).expect("render");
        assert!((i16::from(output.data[0]) - 64).abs() <= 1);
        assert!((i16::from(output.data[3]) - 179).abs() <= 1);
        assert_eq!(output.data[0], output.data[1]);
        assert_eq!(output.data[1], output.data[2]);
    }

    #[test]
    fn m12_gpu_preview_uses_the_same_native_graph_as_cpu_reference() {
        let source = DecodedSourceImage::Rendered(fixture(&[
            [0.18, 0.18, 0.18, 1.0],
            [0.68, 0.32, 0.21, 1.0],
            [3.5, 0.08, 1.7, 1.0],
            [0.003, 0.007, 0.012, 1.0],
        ]));
        let settings = RenderSettings {
            tone: ToneParameters {
                exposure_ev: -1.25,
                shadows: 0.5,
                highlights: -0.5,
                contrast: 0.2,
                ..Default::default()
            },
            ..Default::default()
        };
        let cpu = render_source_preview_to_srgb8(&source, &settings).expect("CPU reference");
        match GpuRenderer::try_new() {
            Ok(gpu) => {
                let accelerated = render_source_preview_with_gpu_to_srgb8(&source, &settings, &gpu)
                    .expect("GPU graph");
                // The GPU node is compared before output quantisation in `starroom-render`.
                // This integration guard permits at most one final 8-bit code value of rounding
                // difference, rather than hiding a colour/tone drift with a broad visual metric.
                assert!(
                    accelerated
                        .data
                        .iter()
                        .zip(&cpu.data)
                        .all(
                            |(gpu, reference)| i16::from(*gpu).abs_diff(i16::from(*reference)) <= 1
                        )
                );
                assert_eq!(accelerated.width, cpu.width);
                assert_eq!(accelerated.height, cpu.height);
            }
            Err(error) => {
                // Adapter-unavailable test hosts are a supported, explicit CPU fallback state.
                assert!(!error.to_string().is_empty());
            }
        }
    }

    #[test]
    fn nasa_portrait_shadow_lift_preserves_tone_order_and_cpu_gpu_parity() {
        let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/golden/sources/astronaut-eileen-collins.png");
        let source_bytes = std::fs::read(&source).expect("immutable NASA portrait");
        let decoded = starroom_imageio::decode_source_preview(&source, 512)
            .expect("decode real NASA portrait");
        assert_eq!((decoded.width(), decoded.height()), (512, 512));
        let settings = RenderSettings {
            tone: ToneParameters {
                exposure_ev: -0.07,
                shadows: 0.49,
                ..Default::default()
            },
            ..Default::default()
        };
        let neutral = render_source_preview_to_srgb8(&decoded, &RenderSettings::default())
            .expect("neutral portrait");
        let cpu = render_source_preview_to_srgb8(&decoded, &settings).expect("CPU portrait");
        let export = render_source_export_to_srgb8(&decoded, &settings).expect("portrait export");
        assert_eq!(cpu.data, export.data, "CPU preview/export parity");
        assert!(
            neutral
                .data
                .iter()
                .zip(&cpu.data)
                .filter(|(a, b)| a != b)
                .count()
                > cpu.data.len() / 100
        );
        let working = prepare_source_for_ai_denoise(&decoded, &RenderSettings::default())
            .expect("real portrait linear working pixels");
        let mut detail_pairs = 0;
        let mut smallest_ratio = f32::INFINITY;
        // Hair and dark suit/collar from the real photograph, not a synthetic face. Preserve
        // actual adjacent-pixel luminance order and at least 20% of its local contrast before
        // output quantization; a posterized transfer can have excellent CPU/GPU parity.
        for (left, top, right, bottom) in [(125, 0, 310, 130), (145, 150, 330, 300)] {
            for y in top..bottom {
                for x in left..right - 1 {
                    let sample = |x| {
                        let index = (y * working.width + x) * 3;
                        LinearRgb {
                            r: working.data[index],
                            g: working.data[index + 1],
                            b: working.data[index + 2],
                        }
                    };
                    let before = [sample(x), sample(x + 1)];
                    let luminances = before.map(starroom_color::luminance);
                    let original_delta = luminances[1] - luminances[0];
                    if original_delta.abs() > 0.001
                        && luminances.iter().all(|value| (0.002..0.18).contains(value))
                    {
                        let after = before.map(|pixel| {
                            starroom_color::luminance(apply_tone(pixel, settings.tone))
                        });
                        let ratio = (after[1] - after[0]) / original_delta;
                        assert!(
                            ratio >= 0.2,
                            "real portrait local tone inversion/flattening: {luminances:?}, ratio {ratio}"
                        );
                        smallest_ratio = smallest_ratio.min(ratio);
                        detail_pairs += 1;
                    }
                }
            }
        }
        assert!(
            detail_pairs > 1000,
            "must exercise real portrait dark texture"
        );
        eprintln!(
            "NASA_SHADOW_DETAIL pairs={detail_pairs} minimum_contrast_ratio={smallest_ratio}"
        );
        let output = std::env::temp_dir().join(format!(
            "starroom-nasa-shadow-regression-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&output).unwrap();
        for (name, image) in [("neutral", &neutral), ("cpu", &cpu)] {
            let encoded = starroom_imageio::encode_jpeg_rgb8(
                &image.data,
                image.width,
                image.height,
                95,
                None,
            )
            .unwrap();
            std::fs::write(output.join(format!("{name}.jpg")), encoded).unwrap();
        }
        match GpuRenderer::try_new() {
            Ok(gpu) => {
                let accelerated =
                    render_source_preview_with_gpu_to_srgb8(&decoded, &settings, &gpu)
                        .expect("GPU portrait");
                let maximum_delta = accelerated
                    .data
                    .iter()
                    .zip(&cpu.data)
                    .map(|(actual, expected)| actual.abs_diff(*expected))
                    .max()
                    .unwrap();
                let large_differences = accelerated
                    .data
                    .iter()
                    .zip(&cpu.data)
                    .filter(|(actual, expected)| actual.abs_diff(**expected) > 2)
                    .count();
                eprintln!(
                    "NASA_SHADOW_PARITY backend={:?} max_delta={maximum_delta} over_two={large_differences} output={}",
                    gpu.status(),
                    output.display()
                );
                let encoded = starroom_imageio::encode_jpeg_rgb8(
                    &accelerated.data,
                    accelerated.width,
                    accelerated.height,
                    95,
                    None,
                )
                .unwrap();
                std::fs::write(output.join("gpu.jpg"), encoded).unwrap();
                assert!(
                    maximum_delta <= 1,
                    "real portrait CPU/GPU delta {maximum_delta}"
                );
            }
            Err(error) => eprintln!("NASA_SHADOW_PARITY explicit GPU unavailable: {error}"),
        }
        let mut previous = 0.0;
        for index in 0..=65536 {
            let value = index as f32 * 0.3 / 65536.0;
            let mapped = starroom_color::luminance(apply_tone(
                LinearRgb {
                    r: value,
                    g: value,
                    b: value,
                },
                settings.tone,
            ));
            assert!(mapped.is_finite());
            assert!(
                mapped >= previous,
                "shadow response reversed brightness at {value}: {previous} -> {mapped}"
            );
            previous = mapped;
        }
        assert_eq!(source_bytes, std::fs::read(source).unwrap());
    }

    #[test]
    fn fused_gpu_creative_stages_match_cpu_oracle_before_spatial_processing() {
        let pixels = [
            LinearRgb {
                r: 0.02,
                g: 0.01,
                b: 0.005,
            },
            LinearRgb {
                r: 0.68,
                g: 0.32,
                b: 0.21,
            },
            LinearRgb {
                r: 3.5,
                g: 0.08,
                b: 1.7,
            },
        ];
        let settings = RenderSettings {
            tone: ToneParameters {
                exposure_ev: -0.65,
                contrast: 0.25,
                highlights: -0.3,
                shadows: 0.2,
                whites: 0.1,
                blacks: -0.1,
            },
            curves: ToneCurveSet {
                master: vec![
                    CurvePoint { x: 0.0, y: 0.0 },
                    CurvePoint { x: 0.5, y: 0.54 },
                    CurvePoint { x: 1.0, y: 1.0 },
                ],
                red: vec![
                    CurvePoint { x: 0.0, y: 0.0 },
                    CurvePoint { x: 1.0, y: 0.95 },
                ],
                ..Default::default()
            },
            color_mixer: ColorMixer::default().with_band(
                ColorBand::Orange,
                BandAdjustment {
                    hue_degrees: 4.0,
                    chroma: 0.08,
                    lightness: 0.03,
                },
            ),
            grading: GradingParameters {
                midtones: ColorWheel {
                    hue_degrees: 32.0,
                    chroma: 0.12,
                    lightness: 0.02,
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let input: Vec<[f32; 4]> = pixels.iter().map(|v| [v.r, v.g, v.b, 1.0]).collect();
        let expected: Vec<LinearRgb> = pixels
            .into_iter()
            .map(|v| {
                let v = apply_tone(v, settings.tone);
                let v = apply_curve(v, &settings.curve, &settings.curves);
                apply_grading(apply_color_mixer(v, settings.color_mixer), settings.grading)
            })
            .collect();
        if let Ok(gpu) = GpuRenderer::try_new() {
            let actual = gpu
                .apply_creative(
                    &input,
                    &gpu_creative_parameters(&settings, input.len(), input.len(), 1),
                    &gpu_curve_luts(&settings),
                    None,
                )
                .expect("creative GPU");
            for (index, (cpu, gpu)) in expected.iter().zip(actual).enumerate() {
                let delta = (cpu.r - gpu[0])
                    .abs()
                    .max((cpu.g - gpu[1]).abs())
                    .max((cpu.b - gpu[2]).abs());
                assert!(
                    delta <= 0.003,
                    "pixel {index} delta {delta}: cpu={cpu:?} gpu={gpu:?}"
                );
            }
        }
    }

    #[test]
    fn fused_gpu_vignette_matches_shared_cpu_finishing_reference() {
        let mut source = Vec::new();
        for y in 0..7 {
            for x in 0..11 {
                source.push([
                    0.08 + x as f32 * 0.035,
                    0.12 + y as f32 * 0.045,
                    0.22 + (x + y) as f32 * 0.015,
                    1.0,
                ]);
            }
        }
        let decoded = DecodedSourceImage::Rendered(fixture(&source));
        let settings = RenderSettings {
            vignette: VignetteSettings {
                amount: 0.72,
                midpoint: 0.38,
                roundness: -0.35,
                feather: 0.47,
                highlight_protect: 0.64,
            },
            ..Default::default()
        };
        let cpu = render_source_preview_to_srgb8(&decoded, &settings).expect("CPU vignette");
        if let Ok(gpu) = GpuRenderer::try_new() {
            let accelerated = render_source_preview_with_gpu_to_srgb8(&decoded, &settings, &gpu)
                .expect("GPU vignette");
            assert!(
                accelerated
                    .data
                    .iter()
                    .zip(&cpu.data)
                    .all(
                        |(actual, expected)| i16::from(*actual).abs_diff(i16::from(*expected)) <= 1
                    )
            );
        }
    }

    #[test]
    fn saturation_minimum_is_achromatic_in_cpu_and_gpu_shared_outputs() {
        let decoded = DecodedSourceImage::Rendered(fixture(&[
            [0.9, 0.3, 0.1, 1.0],
            [0.2, 0.5, 0.9, 1.0],
            [0.6, 0.32, 0.24, 1.0],
            [0.08, 0.03, 0.02, 1.0],
            [1.0, 0.0, 1.0, 1.0],
        ]));
        let gpu = GpuRenderer::try_new().ok();
        for vibrance in [-1.0, 0.0, 1.0] {
            let mut settings = RenderSettings::default();
            settings.relative_color.saturation = -1.0;
            settings.relative_color.vibrance = vibrance;
            let cpu = render_source_export_to_srgb8(&decoded, &settings).unwrap();
            for pixel in cpu.data.chunks_exact(3) {
                assert!(
                    pixel[0].abs_diff(pixel[1]) <= 1 && pixel[1].abs_diff(pixel[2]) <= 1,
                    "not gray: {pixel:?}"
                );
            }
            if let Some(gpu) = &gpu {
                let accelerated =
                    render_source_preview_with_gpu_to_srgb8(&decoded, &settings, gpu).unwrap();
                for pixel in accelerated.data.chunks_exact(3) {
                    assert!(pixel[0].abs_diff(pixel[1]) <= 1 && pixel[1].abs_diff(pixel[2]) <= 1);
                }
                assert!(
                    cpu.data
                        .iter()
                        .zip(&accelerated.data)
                        .all(|(a, b)| a.abs_diff(*b) <= 1)
                );
            }
        }
    }

    #[test]
    fn skin_protected_vibrance_has_native_cpu_gpu_identity_and_extreme_parity() {
        let decoded = DecodedSourceImage::Rendered(fixture(&[
            [0.7, 0.45, 0.32, 1.0],
            [0.4, 0.27, 0.18, 1.0],
            [0.12, 0.08, 0.05, 1.0],
            [0.4, 0.5, 0.7, 1.0],
            [0.98, 0.03, 0.05, 1.0],
            [0.3, 0.3, 0.3, 1.0],
        ]));
        let gpu = GpuRenderer::try_new().ok();
        for vibrance in [-1.0, 0.0, 0.5, 1.0] {
            let mut settings = RenderSettings::default();
            settings.relative_color.vibrance = vibrance;
            let cpu = render_source_export_to_srgb8(&decoded, &settings).unwrap();
            assert_eq!(
                cpu.data,
                render_source_preview_to_srgb8(&decoded, &settings)
                    .unwrap()
                    .data
            );
            if let Some(gpu) = &gpu {
                let accelerated =
                    render_source_preview_with_gpu_to_srgb8(&decoded, &settings, gpu).unwrap();
                assert!(
                    cpu.data
                        .iter()
                        .zip(&accelerated.data)
                        .all(|(a, b)| a.abs_diff(*b) <= 1)
                );
            }
        }
    }

    #[test]
    fn vignette_fusion_rejects_every_nonidentity_tail_and_keeps_original_settings() {
        let mut base = RenderSettings::default();
        base.vignette.amount = 0.72;
        assert!(gpu_can_fuse_vignette(&base));
        let mut cases = Vec::new();
        let mut settings = base.clone();
        settings.sharpen.amount = 0.5;
        cases.push(settings);
        let mut settings = base.clone();
        settings.denoise.luminance = 0.5;
        cases.push(settings);
        let mut settings = base.clone();
        settings.denoise.chroma = 0.5;
        cases.push(settings);
        let mut settings = base.clone();
        settings.denoise.high_iso = 0.5;
        cases.push(settings);
        let mut settings = base.clone();
        settings.local_detail.texture = 0.5;
        cases.push(settings);
        let mut settings = base.clone();
        settings.local_detail.clarity = 0.5;
        cases.push(settings);
        let mut settings = base.clone();
        settings.local_detail.dehaze = 0.5;
        cases.push(settings);
        let mut settings = base.clone();
        settings.grain.amount = 0.3;
        cases.push(settings);
        let mut settings = base.clone();
        settings.skin_retouch.parameters.smooth = 0.5;
        cases.push(settings);
        for settings in cases {
            assert!(!gpu_can_fuse_vignette(&settings));
            let parameters = gpu_creative_parameters(&settings, 32, 8, 4);
            assert_eq!(parameters.values[2][2], 0.0);
            assert_eq!(parameters.values[19][3], 0.0);
            assert_eq!(settings.vignette.amount, 0.72);
        }
    }

    #[test]
    fn vignette_with_local_tone_colour_spatial_and_grain_matches_cpu_export() {
        let mut pixels = Vec::new();
        for y in 0..24 {
            for x in 0..32 {
                let value = 0.15 + 0.6 * (x + y) as f32 / 54.0;
                pixels.push([value, value * 0.8, value * 0.6, 1.0]);
            }
        }
        let decoded = DecodedSourceImage::Rendered(DecodedRenderedImage {
            width: 32,
            height: 24,
            ..fixture(&pixels)
        });
        let base = RenderSettings {
            vignette: VignetteSettings {
                amount: 0.8,
                midpoint: 0.2,
                feather: 0.5,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut layer = LayerAdjustments::default();
        layer.tone.contrast = 0.7;
        layer.tone.shadows = 0.5;
        let local = NativeAdjustmentLayer {
            id: "vignette-tail".into(),
            name: "Local".into(),
            enabled: true,
            opacity: 1.0,
            blend_mode: LayerBlendMode::Normal,
            mask: MaskDefinition::None.into(),
            adjustments: layer,
        };
        let mut cases = Vec::new();
        let mut settings = base.clone();
        settings.layers.push(local.clone());
        cases.push(settings);
        let mut settings = base.clone();
        let mut colour = local;
        colour.adjustments.tone = ToneParameters::default();
        colour.adjustments.relative_color.temperature = 0.4;
        colour.adjustments.relative_color.tint = -0.3;
        settings.layers.push(colour);
        cases.push(settings);
        let mut settings = base.clone();
        settings.sharpen.amount = 0.6;
        settings.sharpen.radius = 1.2;
        cases.push(settings);
        let mut settings = base.clone();
        settings.local_detail.texture = 0.4;
        settings.local_detail.clarity = 0.25;
        settings.local_detail.dehaze = 0.3;
        cases.push(settings);
        let mut settings = base.clone();
        settings.denoise.luminance = 0.4;
        settings.denoise.chroma = 0.3;
        cases.push(settings);
        let mut settings = base;
        settings.grain.amount = 0.25;
        settings.grain.seed = 42;
        settings.image_identity = "vignette-grain".into();
        cases.push(settings);
        let gpu = match GpuRenderer::try_new() {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("GPU parity unavailable: {error}");
                return;
            }
        };
        for (index, settings) in cases.iter().enumerate() {
            assert!(!gpu_can_fuse_vignette(settings));
            let cpu = render_source_export_to_srgb8(&decoded, settings).unwrap();
            let accelerated =
                render_source_preview_with_gpu_to_srgb8(&decoded, settings, &gpu).unwrap();
            let difference = cpu
                .data
                .iter()
                .zip(&accelerated.data)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(
                difference <= 1,
                "case {index}: GPU/Export max code difference {difference}"
            );
        }
    }

    #[test]
    fn shadow_control_targets_dark_pixel_more_than_mid_pixel() {
        let decoded = fixture(&[[0.12, 0.10, 0.08, 1.0], [0.5, 0.45, 0.4, 1.0]]);
        let baseline = render_to_srgb8(&decoded, &RenderSettings::default()).expect("baseline");
        let settings = RenderSettings {
            tone: ToneParameters {
                shadows: 0.5,
                ..Default::default()
            },
            ..Default::default()
        };
        let adjusted = render_to_srgb8(&decoded, &settings).expect("adjusted");
        let dark_gain = i16::from(adjusted.data[0]) - i16::from(baseline.data[0]);
        let mid_gain = i16::from(adjusted.data[3]) - i16::from(baseline.data[3]);
        assert!(dark_gain > 0);
        assert!(dark_gain > mid_gain * 2);
    }

    #[test]
    fn oklch_color_mixer_changes_selected_color() {
        let decoded = fixture(&[[0.8, 0.2, 0.12, 1.0]]);
        let baseline = render_to_srgb8(&decoded, &RenderSettings::default()).expect("baseline");
        let settings = RenderSettings {
            color_mixer: ColorMixer::default().with_band(
                ColorBand::Red,
                BandAdjustment {
                    hue_degrees: 20.0,
                    chroma: 0.2,
                    lightness: 0.0,
                },
            ),
            ..Default::default()
        };
        let adjusted = render_to_srgb8(&decoded, &settings).expect("adjusted");
        assert_ne!(baseline.data, adjusted.data);
    }

    #[test]
    fn m8_four_way_grading_preview_export_share_native_stage() {
        let decoded = fixture(&[
            [0.62, 0.35, 0.24, 1.0],
            [0.08, 0.1, 0.18, 1.0],
            [1.4, 0.1, 0.9, 1.0],
        ]);
        let settings = RenderSettings {
            grading: GradingParameters {
                shadows: ColorWheel {
                    hue_degrees: 225.0,
                    chroma: 0.35,
                    lightness: -0.08,
                },
                midtones: ColorWheel {
                    hue_degrees: 35.0,
                    chroma: 0.2,
                    lightness: 0.04,
                },
                highlights: ColorWheel {
                    hue_degrees: 55.0,
                    chroma: 0.12,
                    lightness: -0.02,
                },
                global: ColorWheel {
                    hue_degrees: 310.0,
                    chroma: 0.04,
                    lightness: 0.0,
                },
                balance: 0.1,
                blending: 0.7,
                amount: 0.85,
            },
            ..Default::default()
        };
        let preview = render_preview_to_srgb8(&decoded, &settings).expect("preview");
        let export = render_export_to_srgb8(&decoded, &settings).expect("export");
        assert_eq!(preview, export);
        assert_ne!(
            preview.data,
            render_to_srgb8(&decoded, &RenderSettings::default())
                .expect("baseline")
                .data
        );
    }

    #[test]
    fn m9_detail_engine_preview_export_share_spatial_pipeline() {
        let decoded = DecodedRenderedImage {
            width: 5,
            height: 3,
            format: RenderedFormat::Png,
            rgba: (0..15)
                .flat_map(|index: usize| {
                    let edge = if index % 5 < 2 { 0.12 } else { 0.68 };
                    let noise = if index.is_multiple_of(2) {
                        0.025
                    } else {
                        -0.02
                    };
                    [edge + noise, edge, edge - noise, 1.0]
                })
                .collect(),
            embedded_icc: None,
            exif: None,
        };
        let settings = RenderSettings {
            denoise: DenoiseParameters {
                luminance: 0.45,
                chroma: 0.7,
                radius: 1.2,
                detail_protection: 0.65,
                high_iso: 0.5,
            },
            local_detail: LocalDetailParameters {
                texture: 0.25,
                clarity: 0.2,
                dehaze: 0.1,
            },
            sharpen: SharpenParameters {
                amount: 0.7,
                radius: 1.0,
                detail: 0.65,
                masking: 0.4,
                halo_protection: 0.8,
                threshold: 0.002,
            },
            ..Default::default()
        };
        let preview = render_preview_to_srgb8(&decoded, &settings).expect("preview");
        let export = render_export_to_srgb8(&decoded, &settings).expect("export");
        assert_eq!(preview, export);
        assert_eq!(preview.data.len(), 45);
    }

    #[test]
    fn m10_lensfun_manual_profile_preview_export_parity() {
        let decoded = fixture(&[
            [0.1, 0.1, 0.1, 1.0],
            [0.3, 0.25, 0.2, 1.0],
            [0.7, 0.6, 0.5, 1.0],
            [0.2, 0.3, 0.4, 1.0],
            [0.5, 0.5, 0.5, 1.0],
            [0.9, 0.7, 0.4, 1.0],
        ]);
        let settings = RenderSettings {
            optics: OpticsSettings {
                parameters: starroom_optics::OpticsParameters {
                    enabled: true,
                    ..Default::default()
                },
                match_mode: LensMatchMode::Manual,
                manual_identity: Some(LensIdentity {
                    camera_make: "Nikon".into(),
                    camera_model: "Nikon D750".into(),
                    lens_make: "Nikon".into(),
                    lens_model: "Nikon AF-S Nikkor 16-35mm f/4G ED VR".into(),
                    focal_length_mm: 24.0,
                    aperture: 5.6,
                    focus_distance_m: Some(10.0),
                }),
            },
            ..Default::default()
        };
        let preview = render_preview_to_srgb8(&decoded, &settings).expect("preview");
        let export = render_export_to_srgb8(&decoded, &settings).expect("export");
        assert_eq!(preview, export);
    }

    #[test]
    fn m10_unknown_lens_never_silently_uses_generic_profile() {
        let decoded = fixture(&[[0.2, 0.2, 0.2, 1.0]]);
        let settings = RenderSettings {
            optics: OpticsSettings {
                parameters: starroom_optics::OpticsParameters {
                    enabled: true,
                    ..Default::default()
                },
                match_mode: LensMatchMode::Manual,
                manual_identity: Some(LensIdentity {
                    camera_make: "Nikon".into(),
                    camera_model: "Nikon D750".into(),
                    lens_make: "Missing".into(),
                    lens_model: "Definitely Missing Lens".into(),
                    focal_length_mm: 50.0,
                    aperture: 2.0,
                    focus_distance_m: None,
                }),
            },
            ..Default::default()
        };
        assert!(matches!(
            render_preview_to_srgb8(&decoded, &settings),
            Err(PipelineError::OpticsProfile(LensProfileStatus::UnknownLens))
        ));
    }

    #[test]
    fn m11_geometry_preview_export_share_native_stage_and_dimensions() {
        let values: Vec<[f32; 4]> = (0..48)
            .map(|index| {
                let value = index as f32 / 48.0;
                [value, value * 0.8, value * 0.6, 1.0]
            })
            .collect();
        let mut decoded = fixture(&values);
        decoded.width = 8;
        decoded.height = 6;
        let settings = RenderSettings {
            geometry: GeometryParameters {
                rotation_degrees: 3.0,
                vertical_keystone: 0.12,
                horizontal_keystone: -0.08,
                crop: starroom_geometry::CropRect {
                    left: 0.125,
                    top: 0.0,
                    right: 0.875,
                    bottom: 1.0,
                },
                crop_aspect_width: 1.0,
                crop_aspect_height: 1.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let preview = render_preview_to_srgb8(&decoded, &settings).expect("preview");
        let export = render_export_to_srgb8(&decoded, &settings).expect("export");
        assert_eq!(preview, export);
        assert_eq!(preview.width, preview.height);
        assert!(preview.data.iter().any(|value| *value != 0));
    }

    #[test]
    fn relative_temperature_warms_neutral_pixel_without_kelvin_claim() {
        let decoded = fixture(&[[0.5, 0.5, 0.5, 1.0]]);
        let settings = RenderSettings {
            relative_color: RelativeColorParameters {
                temperature: 0.7,
                ..Default::default()
            },
            ..Default::default()
        };
        let output = render_to_srgb8(&decoded, &settings).expect("render");
        assert!(output.data[0] > output.data[2]);
    }

    #[test]
    fn native_rgb_curves_are_channel_specific_and_preview_export_match() {
        let decoded = fixture(&[[0.5, 0.5, 0.5, 1.0]]);
        let settings = RenderSettings {
            curves: ToneCurveSet {
                red: vec![CurvePoint { x: 0.0, y: 0.0 }, CurvePoint { x: 1.0, y: 0.7 }],
                ..Default::default()
            },
            ..Default::default()
        };
        let preview = render_preview_to_srgb8(&decoded, &settings).expect("preview");
        let export = render_export_to_srgb8(&decoded, &settings).expect("export");
        assert_eq!(preview, export);
        assert!(preview.data[0] < preview.data[1]);
    }

    #[test]
    fn master_identity_curve_preserves_portrait_and_gradient_golden_vector() {
        let decoded = fixture(&[
            [0.62, 0.35, 0.24, 1.0],
            [0.05, 0.05, 0.05, 1.0],
            [0.25, 0.25, 0.25, 1.0],
            [0.75, 0.75, 0.75, 1.0],
        ]);
        let identity = vec![CurvePoint { x: 0.0, y: 0.0 }, CurvePoint { x: 1.0, y: 1.0 }];
        let baseline = render_to_srgb8(&decoded, &RenderSettings::default()).expect("baseline");
        let curved = render_to_srgb8(
            &decoded,
            &RenderSettings {
                curves: ToneCurveSet {
                    master: identity,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .expect("identity");
        assert_eq!(baseline.data, curved.data);
    }

    #[test]
    fn s_curve_changes_gradient_ends_while_preserving_midpoint() {
        let curves = ToneCurveSet {
            master: vec![
                CurvePoint { x: 0.0, y: 0.0 },
                CurvePoint { x: 0.25, y: 0.16 },
                CurvePoint { x: 0.5, y: 0.5 },
                CurvePoint { x: 0.75, y: 0.86 },
                CurvePoint { x: 1.0, y: 1.0 },
            ],
            ..Default::default()
        };
        let dark = apply_curve(
            LinearRgb {
                r: 0.2,
                g: 0.2,
                b: 0.2,
            },
            &[],
            &curves,
        );
        let middle = apply_curve(
            LinearRgb {
                r: 0.5,
                g: 0.5,
                b: 0.5,
            },
            &[],
            &curves,
        );
        let bright = apply_curve(
            LinearRgb {
                r: 0.8,
                g: 0.8,
                b: 0.8,
            },
            &[],
            &curves,
        );
        assert!(dark.r < 0.2);
        assert!((middle.r - 0.5).abs() <= 1.0e-6);
        assert!(bright.r > 0.8);
    }

    #[test]
    fn extreme_curves_remain_finite_for_hdr_working_values() {
        let curves = ToneCurveSet {
            master: vec![
                CurvePoint { x: 0.0, y: 0.2 },
                CurvePoint { x: 0.5, y: 0.95 },
                CurvePoint { x: 1.0, y: 1.0 },
            ],
            red: vec![CurvePoint { x: 0.0, y: 1.0 }, CurvePoint { x: 1.0, y: 0.0 }],
            ..Default::default()
        };
        for rgb in [
            LinearRgb {
                r: -0.25,
                g: 0.0,
                b: 0.2,
            },
            LinearRgb {
                r: 1.5,
                g: 4.0,
                b: 12.0,
            },
        ] {
            let result = apply_curve(rgb, &[], &curves);
            assert!(
                [result.r, result.g, result.b]
                    .into_iter()
                    .all(f32::is_finite)
            );
        }
    }

    #[test]
    fn measured_white_balance_uses_prepared_lcms_cat_for_all_pixels_before_creative_processing() {
        let original = [[0.4, 0.3, 0.2], [0.7, 0.42, 0.25], [3.0, 1.0, 0.2]];
        let mut pixels = original;
        let settings = WhiteBalanceSettings {
            mode: WhiteBalanceMode::NeutralPicker,
            sample: Some(WhiteBalanceSample {
                x: 0.0,
                y: 0.0,
                width: 0.3,
                height: 1.0,
            }),
        };
        apply_white_balance(&mut pixels, 3, 1, SourceKind::Encoded, settings).unwrap();
        let matrix = measured_neutral_adaptation(LinearRgb {
            r: 0.4,
            g: 0.3,
            b: 0.2,
        })
        .unwrap();
        for (actual, input) in pixels.into_iter().zip(original) {
            let expected = matrix.multiply_vec(Xyz {
                x: input[0],
                y: input[1],
                z: input[2],
            });
            assert_eq!(actual, [expected.x, expected.y, expected.z]);
        }
        assert!((pixels[0][0] - pixels[0][1]).abs() < 1.0e-6);
        assert!((pixels[0][1] - pixels[0][2]).abs() < 1.0e-6);
        assert!(
            pixels[2][0] > 1.0,
            "scene-linear highlights must not be clipped by CAT"
        );
        let mut black = [[0.0; 3]];
        assert!(matches!(
            apply_white_balance(
                &mut black,
                1,
                1,
                SourceKind::Encoded,
                WhiteBalanceSettings {
                    mode: WhiteBalanceMode::Auto,
                    sample: None
                }
            ),
            Err(PipelineError::InvalidWhiteBalanceSample)
        ));
    }

    #[test]
    fn neutral_picker_removes_a_measured_encoded_colour_cast() {
        let decoded = fixture(&[[0.48, 0.36, 0.24, 1.0], [0.48, 0.36, 0.24, 1.0]]);
        let output = render_to_srgb8(
            &decoded,
            &RenderSettings {
                white_balance: WhiteBalanceSettings {
                    mode: WhiteBalanceMode::NeutralPicker,
                    sample: Some(WhiteBalanceSample {
                        x: 0.0,
                        y: 0.0,
                        width: 1.0,
                        height: 1.0,
                    }),
                },
                ..Default::default()
            },
        )
        .expect("picker render");
        assert!((i16::from(output.data[0]) - i16::from(output.data[1])).abs() <= 1);
        assert!((i16::from(output.data[1]) - i16::from(output.data[2])).abs() <= 1);
    }

    #[test]
    fn auto_white_balance_is_active_and_extreme_pixels_stay_finite() {
        let decoded = fixture(&[
            [0.9, 0.6, 0.3, 1.0],
            [0.72, 0.48, 0.24, 1.0],
            [4.0, 1.0, 0.1, 1.0],
            [0.001, 0.001, 0.001, 1.0],
        ]);
        let output = render_to_srgb8(
            &decoded,
            &RenderSettings {
                white_balance: WhiteBalanceSettings {
                    mode: WhiteBalanceMode::Auto,
                    sample: None,
                },
                ..Default::default()
            },
        )
        .expect("auto render");
        assert!(output.data.iter().any(|value| *value > 0));
    }

    #[test]
    fn skin_and_mixed_lighting_white_balance_regression_stays_warm_and_finite() {
        let decoded = fixture(&[
            [0.68, 0.42, 0.30, 1.0],
            [0.55, 0.37, 0.29, 1.0],
            [0.22, 0.31, 0.58, 1.0],
            [0.62, 0.48, 0.25, 1.0],
        ]);
        let output = render_to_srgb8(
            &decoded,
            &RenderSettings {
                white_balance: WhiteBalanceSettings {
                    mode: WhiteBalanceMode::Auto,
                    sample: None,
                },
                ..Default::default()
            },
        )
        .expect("mixed-light Auto WB");
        assert!(
            output.data[0] > output.data[2],
            "skin sample must retain warm ordering"
        );
        assert_eq!(output.data.len(), 12);
    }

    #[test]
    fn encoded_camera_white_balance_is_a_typed_error_not_a_silent_fallback() {
        let decoded = fixture(&[[0.4, 0.4, 0.4, 1.0]]);
        let result = render_to_srgb8(
            &decoded,
            &RenderSettings {
                white_balance: WhiteBalanceSettings {
                    mode: WhiteBalanceMode::Camera,
                    sample: None,
                },
                ..Default::default()
            },
        );
        assert!(matches!(
            result,
            Err(PipelineError::WhiteBalanceSemantic {
                mode: WhiteBalanceMode::Camera,
                input_kind: "encoded"
            })
        ));
    }

    #[test]
    fn invalid_picker_sample_is_rejected_before_rendering() {
        let decoded = fixture(&[[0.4, 0.4, 0.4, 1.0]]);
        let result = render_to_srgb8(
            &decoded,
            &RenderSettings {
                white_balance: WhiteBalanceSettings {
                    mode: WhiteBalanceMode::NeutralPicker,
                    sample: Some(WhiteBalanceSample {
                        x: 0.8,
                        y: 0.0,
                        width: 0.4,
                        height: 1.0,
                    }),
                },
                ..Default::default()
            },
        );
        assert!(matches!(
            result,
            Err(PipelineError::InvalidWhiteBalanceSample)
        ));
    }

    #[test]
    fn preview_and_export_share_identical_srgb_graph() {
        let decoded = fixture(&[[0.15, 0.3, 0.8, 1.0], [0.8, 0.45, 0.1, 1.0]]);
        let settings = RenderSettings {
            tone: ToneParameters {
                exposure_ev: 0.4,
                ..Default::default()
            },
            ..Default::default()
        };
        let preview = render_preview_to_srgb8(&decoded, &settings).expect("preview");
        let export = render_export_to_srgb8(&decoded, &settings).expect("export");
        assert_eq!(preview, export);
        assert_eq!(preview.color.input, InputProfileSource::AssumedSrgb);
        assert_eq!(preview.color.output, OutputProfileSource::Srgb);
    }

    #[test]
    fn mask_raster_validation_rejects_unsampled_non_finite_cells_before_render() {
        // A one-pixel image samples only cell zero of a two-cell native raster. Validation must
        // still reject corrupt cell one before either Preview or Export enters the graph.
        let decoded = fixture(&[[0.4, 0.25, 0.18, 1.0]]);
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let portrait = RenderSettings {
                portrait_masks: vec![PortraitMaskRaster {
                    cache_key: "real-cache".into(),
                    face_id: "face-a".into(),
                    region: PortraitMaskRegion::Skin,
                    width: 2,
                    height: 1,
                    values: vec![1.0, invalid],
                }],
                ..Default::default()
            };
            for result in [
                render_preview_to_srgb8(&decoded, &portrait),
                render_export_to_srgb8(&decoded, &portrait),
            ] {
                assert!(matches!(
                    result,
                    Err(PipelineError::InvalidMask("portrait raster is malformed"))
                ));
            }
            let generated = RenderSettings {
                generated_masks: vec![GeneratedMaskRaster {
                    cache_identity: "real-ai-cache".into(),
                    semantic: GeneratedMaskSemantic::Subject,
                    width: 2,
                    height: 1,
                    values: vec![1.0, invalid],
                }],
                ..Default::default()
            };
            for result in [
                render_preview_to_srgb8(&decoded, &generated),
                render_export_to_srgb8(&decoded, &generated),
            ] {
                assert!(matches!(
                    result,
                    Err(PipelineError::InvalidMask(
                        "generated AI raster is malformed"
                    ))
                ));
            }
        }
    }

    #[test]
    fn mask_raster_shape_errors_remain_typed_and_sampling_is_exact() {
        for (width, height, values) in [
            (0, 1, vec![]),
            (1, 0, vec![]),
            (2, 1, vec![0.5]),
            (u32::MAX, u32::MAX, vec![0.5]),
        ] {
            assert!(matches!(
                validate_soft_raster(width, height, &values, "bad shape"),
                Err(PipelineError::InvalidMask("bad shape"))
            ));
            assert!(matches!(
                sample_soft_raster(width, height, &values, 0.0, 0.0, "bad shape"),
                Err(PipelineError::InvalidMask("bad shape"))
            ));
        }
        let raster = PortraitMaskRaster {
            cache_key: "sampling".into(),
            face_id: "face".into(),
            region: PortraitMaskRegion::Skin,
            width: 512,
            height: 512,
            values: (0..512 * 512)
                .map(|index| (index % 512) as f32 / 511.0)
                .collect(),
        };
        raster.validate().unwrap();
        let started = Instant::now();
        for y in 0..512 {
            for x in 0..512 {
                let sampled = raster
                    .weight_at(x as f32 / 511.0, y as f32 / 511.0)
                    .unwrap();
                assert_eq!(sampled, raster.values[y * 512 + x]);
            }
        }
        eprintln!(
            "512² validated mask sampling: {:.3}s",
            started.elapsed().as_secs_f64()
        );
    }

    #[test]
    fn m17_skin_retouch_uses_native_portrait_masks_and_preview_export_graph() {
        let decoded = fixture(&[[0.18, 0.12, 0.09, 1.0], [0.88, 0.58, 0.38, 1.0]]);
        let mut settings = RenderSettings {
            skin_retouch: SkinRetouchSettings {
                parameters: SkinRetouchParameters {
                    smooth: 0.75,
                    texture: 0.70,
                    tone_evenness: 0.35,
                    hue_degrees: 4.0,
                    chroma: -0.1,
                    exposure_ev: 0.2,
                },
                faces: vec![SkinRetouchFaceReference {
                    face_id: "face-a".into(),
                    cache_key: "cache-a".into(),
                    source_crop: None,
                }],
            },
            ..Default::default()
        };
        for (region, values) in [
            (PortraitMaskRegion::Skin, vec![1.0, 1.0]),
            (PortraitMaskRegion::Eyes, vec![0.0, 1.0]),
            (PortraitMaskRegion::Brows, vec![0.0, 0.0]),
            (PortraitMaskRegion::Lips, vec![0.0, 0.0]),
            (PortraitMaskRegion::Hair, vec![0.0, 0.0]),
            (PortraitMaskRegion::LeftEye, vec![0.0, 0.0]),
            (PortraitMaskRegion::RightEye, vec![0.0, 0.0]),
            (PortraitMaskRegion::LeftBrow, vec![0.0, 0.0]),
            (PortraitMaskRegion::RightBrow, vec![0.0, 0.0]),
            (PortraitMaskRegion::Mouth, vec![0.0, 0.0]),
        ] {
            settings.portrait_masks.push(PortraitMaskRaster {
                cache_key: "cache-a".into(),
                face_id: "face-a".into(),
                region,
                width: 2,
                height: 1,
                values,
            });
        }
        let preview = render_preview_to_srgb8(&decoded, &settings).expect("M17 preview");
        let export = render_export_to_srgb8(&decoded, &settings).expect("M17 export");
        assert_eq!(
            preview, export,
            "M17 cannot diverge between preview and export"
        );
        assert_eq!(preview.data.len(), 6);
    }

    #[test]
    fn m17_enabled_skin_retouch_without_native_cache_is_typed_error() {
        let decoded = fixture(&[[0.4, 0.25, 0.18, 1.0]]);
        let settings = RenderSettings {
            skin_retouch: SkinRetouchSettings {
                parameters: SkinRetouchParameters {
                    smooth: 0.5,
                    ..Default::default()
                },
                faces: vec![SkinRetouchFaceReference {
                    face_id: "face-a".into(),
                    cache_key: "missing".into(),
                    source_crop: None,
                }],
            },
            ..Default::default()
        };
        assert!(matches!(
            render_preview_to_srgb8(&decoded, &settings),
            Err(PipelineError::MaskProviderUnavailable { .. })
        ));
    }

    #[test]
    fn m18_healing_is_shared_by_preview_export_and_rejects_reserved_inpaint() {
        let decoded = fixture(&[
            [0.2, 0.2, 0.2, 1.0],
            [0.9, 0.1, 0.1, 1.0],
            [0.2, 0.2, 0.2, 1.0],
        ]);
        let operation = HealingOperation {
            id: "spot".into(),
            enabled: true,
            mode: starroom_heal::HealMode::Heal,
            target: starroom_heal::HealPoint { x: 0.5, y: 0.0 },
            source: Some(starroom_heal::HealPoint { x: 0.0, y: 0.0 }),
            radius: 1.0,
            feather: 0.3,
            opacity: 1.0,
            rotation_degrees: 0.0,
            scale: 1.0,
            tone_adaptation: true,
            texture_adaptation: true,
            source_mode: starroom_heal::SourceMode::Manual,
            metadata: std::collections::BTreeMap::new(),
        };
        let settings = RenderSettings {
            healing_operations: vec![operation.clone()],
            ..Default::default()
        };
        assert_eq!(
            render_preview_to_srgb8(&decoded, &settings).expect("preview"),
            render_export_to_srgb8(&decoded, &settings).expect("export")
        );
        let mut unavailable = operation;
        unavailable.mode = starroom_heal::HealMode::AiInpaint;
        assert!(matches!(
            render_preview_to_srgb8(
                &decoded,
                &RenderSettings {
                    healing_operations: vec![unavailable],
                    ..Default::default()
                }
            ),
            Err(PipelineError::InvalidMask(
                "M18 AI inpaint is reserved and unavailable"
            ))
        ));
    }

    #[test]
    fn m21_ai_denoise_stage_is_required_and_shared_by_preview_export() {
        let decoded = fixture(&[[0.18, 0.20, 0.22, 1.0], [0.8, 0.7, 0.6, 1.0]]);
        let mut settings = RenderSettings {
            ai_denoise: AiDenoiseParameters {
                enabled: true,
                amount: 0.7,
                detail: 0.4,
                color_noise: 0.8,
                preserve_skin: 0.5,
            },
            ..Default::default()
        };
        assert!(matches!(
            render_preview_to_srgb8(&decoded, &settings),
            Err(PipelineError::AiDenoise(AiDenoiseError::ResidualMismatch))
        ));
        settings.ai_denoise_residual = Some(AiDenoiseResidual {
            width: 2,
            height: 1,
            values: vec![-0.01, -0.008, -0.006, -0.02, -0.01, -0.005],
            model_hash: "fixture-model".into(),
            source_identity: "fixture".into(),
            inference_cache_key: "fixture-cache".into(),
            execution_provider: AiExecutionProvider::Cpu,
        });
        let preview = render_preview_to_srgb8(&decoded, &settings).expect("M21 preview");
        let export = render_export_to_srgb8(&decoded, &settings).expect("M21 export");
        assert_eq!(preview, export);
    }

    #[test]
    fn m21_denoise_detail_and_m16_m17_portrait_path_share_one_export_graph() {
        let decoded = fixture(&[
            [0.12, 0.08, 0.06, 1.0],
            [0.68, 0.42, 0.30, 1.0],
            [1.8, 1.2, 0.7, 1.0],
        ]);
        let mut settings = RenderSettings {
            ai_denoise: AiDenoiseParameters {
                enabled: true,
                amount: 0.65,
                detail: 0.6,
                color_noise: 0.7,
                preserve_skin: 0.85,
            },
            ai_denoise_residual: Some(AiDenoiseResidual {
                width: 3,
                height: 1,
                values: vec![
                    -0.012, -0.009, -0.007, -0.006, -0.004, -0.003, -0.018, -0.012, -0.008,
                ],
                model_hash: "fixture-model".into(),
                source_identity: "high-iso-portrait".into(),
                inference_cache_key: "high-iso-portrait-cache".into(),
                execution_provider: AiExecutionProvider::Cpu,
            }),
            local_detail: LocalDetailParameters {
                texture: 0.35,
                clarity: 0.15,
                dehaze: 0.05,
            },
            skin_retouch: SkinRetouchSettings {
                parameters: SkinRetouchParameters {
                    smooth: 0.35,
                    texture: 0.65,
                    tone_evenness: 0.2,
                    hue_degrees: 2.0,
                    chroma: -0.05,
                    exposure_ev: 0.1,
                },
                faces: vec![SkinRetouchFaceReference {
                    face_id: "face-a".into(),
                    cache_key: "portrait-cache".into(),
                    source_crop: None,
                }],
            },
            ..Default::default()
        };
        for (region, values) in [
            (PortraitMaskRegion::Skin, vec![0.0, 1.0, 0.0]),
            (PortraitMaskRegion::Eyes, vec![0.0, 0.0, 0.0]),
            (PortraitMaskRegion::Brows, vec![0.0, 0.0, 0.0]),
            (PortraitMaskRegion::Lips, vec![0.0, 0.0, 0.0]),
            (PortraitMaskRegion::Hair, vec![0.0, 0.0, 0.0]),
        ] {
            settings.portrait_masks.push(PortraitMaskRaster {
                cache_key: "portrait-cache".into(),
                face_id: "face-a".into(),
                region,
                width: 3,
                height: 1,
                values,
            });
        }
        let preview = render_preview_to_srgb8(&decoded, &settings).expect("integrated preview");
        let export = render_export_to_srgb8(&decoded, &settings).expect("integrated export");
        assert_eq!(preview, export);
        assert!(preview.data.iter().any(|value| *value > 0));
    }

    #[test]
    fn m28_hybrid_gpu_path_matches_full_cpu_graph_with_local_and_geometry_stages() {
        let source = DecodedSourceImage::Rendered(fixture(&[
            [0.12, 0.08, 0.06, 1.0],
            [0.68, 0.42, 0.30, 1.0],
            [1.8, 1.2, 0.7, 1.0],
        ]));
        let healing = HealingOperation {
            id: "parity-spot".into(),
            enabled: true,
            mode: starroom_heal::HealMode::Heal,
            target: starroom_heal::HealPoint { x: 1.0, y: 0.0 },
            source: Some(starroom_heal::HealPoint { x: 0.0, y: 0.0 }),
            // Healing radii are expressed in source pixels and production validation deliberately
            // rejects sub-pixel operations. Keep this tiny fixture on the valid boundary so the
            // test exercises the complete CPU/hybrid graph instead of failing during validation.
            radius: 0.5,
            feather: 0.4,
            opacity: 0.65,
            rotation_degrees: 0.0,
            scale: 1.0,
            tone_adaptation: true,
            texture_adaptation: true,
            source_mode: starroom_heal::SourceMode::Manual,
            metadata: std::collections::BTreeMap::new(),
        };
        let mut settings = RenderSettings {
            tone: ToneParameters {
                exposure_ev: -0.65,
                contrast: 0.25,
                highlights: -0.3,
                shadows: 0.2,
                whites: 0.1,
                blacks: -0.1,
            },
            relative_color: RelativeColorParameters {
                temperature: 0.15,
                tint: -0.1,
                vibrance: 0.2,
                saturation: 0.05,
            },
            curves: ToneCurveSet {
                master: vec![
                    CurvePoint { x: 0.0, y: 0.0 },
                    CurvePoint { x: 0.5, y: 0.54 },
                    CurvePoint { x: 1.0, y: 1.0 },
                ],
                red: vec![
                    CurvePoint { x: 0.0, y: 0.0 },
                    CurvePoint { x: 1.0, y: 0.95 },
                ],
                ..Default::default()
            },
            color_mixer: ColorMixer::default().with_band(
                ColorBand::Orange,
                BandAdjustment {
                    hue_degrees: 4.0,
                    chroma: 0.08,
                    lightness: 0.03,
                },
            ),
            grading: GradingParameters {
                midtones: ColorWheel {
                    hue_degrees: 32.0,
                    chroma: 0.12,
                    lightness: 0.02,
                },
                ..Default::default()
            },
            denoise: DenoiseParameters {
                luminance: 0.2,
                chroma: 0.15,
                ..Default::default()
            },
            local_detail: LocalDetailParameters {
                texture: 0.2,
                clarity: 0.1,
                dehaze: 0.05,
            },
            sharpen: SharpenParameters {
                amount: 0.3,
                ..Default::default()
            },
            geometry: GeometryParameters {
                flip_horizontal: true,
                ..Default::default()
            },
            layers: vec![NativeAdjustmentLayer {
                id: "local-mask".into(),
                name: "Local mask".into(),
                enabled: true,
                opacity: 0.55,
                blend_mode: LayerBlendMode::Normal,
                mask: MaskDefinition::Radial {
                    x: 0.5,
                    y: 0.5,
                    width: 0.8,
                    height: 1.0,
                    rotation: 12.0,
                    feather: 0.3,
                    invert: false,
                }
                .into(),
                adjustments: LayerAdjustments {
                    tone: ToneParameters {
                        exposure_ev: 0.25,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            }],
            skin_retouch: SkinRetouchSettings {
                parameters: SkinRetouchParameters {
                    smooth: 0.25,
                    texture: 0.7,
                    tone_evenness: 0.15,
                    hue_degrees: 1.0,
                    chroma: -0.03,
                    exposure_ev: 0.05,
                },
                faces: vec![SkinRetouchFaceReference {
                    face_id: "face-a".into(),
                    cache_key: "parity-face".into(),
                    source_crop: None,
                }],
            },
            healing_operations: vec![healing],
            ..Default::default()
        };
        for (region, values) in [
            (PortraitMaskRegion::Skin, vec![0.0, 1.0, 0.0]),
            (PortraitMaskRegion::Eyes, vec![0.0, 0.0, 0.0]),
            (PortraitMaskRegion::Brows, vec![0.0, 0.0, 0.0]),
            (PortraitMaskRegion::Lips, vec![0.0, 0.0, 0.0]),
            (PortraitMaskRegion::Hair, vec![0.0, 0.0, 0.0]),
        ] {
            settings.portrait_masks.push(PortraitMaskRaster {
                cache_key: "parity-face".into(),
                face_id: "face-a".into(),
                region,
                width: 3,
                height: 1,
                values,
            });
        }
        let cpu = render_source_preview_to_srgb8(&source, &settings).expect("CPU graph");
        match GpuRenderer::try_new() {
            Ok(gpu) => {
                let hybrid = render_source_preview_with_gpu_to_srgb8(&source, &settings, &gpu)
                    .expect("hybrid GPU graph");
                assert_eq!((hybrid.width, hybrid.height), (cpu.width, cpu.height));
                let maximum_delta = hybrid
                    .data
                    .iter()
                    .zip(&cpu.data)
                    .map(|(gpu, cpu)| i16::from(*gpu).abs_diff(i16::from(*cpu)))
                    .max()
                    .unwrap_or(0);
                assert!(
                    hybrid
                        .data
                        .iter()
                        .zip(cpu.data)
                        .all(|(gpu, cpu)| { i16::from(*gpu).abs_diff(i16::from(cpu)) <= 1 }),
                    "maximum CPU/GPU byte delta was {maximum_delta}"
                );
            }
            Err(error) => assert!(!error.to_string().is_empty()),
        }
    }

    #[test]
    fn m23_grain_vignette_are_shared_deterministic_and_not_baked_into_before() {
        let decoded = fixture(&[
            [0.3, 0.4, 0.5, 1.0],
            [0.5, 0.6, 0.7, 1.0],
            [2.0, 1.5, 1.0, 1.0],
        ]);
        let settings = RenderSettings {
            grain: GrainSettings {
                amount: 0.4,
                size: 0.5,
                roughness: 0.2,
                color: 0.25,
                seed: 77,
            },
            vignette: VignetteSettings {
                amount: 0.5,
                midpoint: 0.35,
                roundness: 0.2,
                feather: 0.5,
                highlight_protect: 0.8,
            },
            image_identity: "same-source".into(),
            ..Default::default()
        };
        let preview = render_preview_to_srgb8(&decoded, &settings).expect("M23 preview");
        let repeat = render_preview_to_srgb8(&decoded, &settings).expect("repeat");
        let export = render_export_to_srgb8(&decoded, &settings).expect("M23 export");
        assert_eq!(preview, repeat);
        assert_eq!(preview, export);
        assert_ne!(
            preview,
            render_preview_to_srgb8(&decoded, &RenderSettings::default()).expect("before")
        );
    }

    #[test]
    fn m23_weighted_look_layer_mask_and_finishing_keep_preview_export_parity() {
        let decoded = fixture(&[
            [0.16, 0.20, 0.28, 1.0],
            [0.55, 0.42, 0.30, 1.0],
            [2.4, 1.6, 0.9, 1.0],
        ]);
        let look_a = starroom_look::PortableLook {
            id: "look-a".into(),
            name: "Look A".into(),
            tone: ToneParameters {
                exposure_ev: 0.35,
                ..Default::default()
            },
            relative_color: starroom_look::PortableRelativeColor {
                temperature: 0.25,
                ..Default::default()
            },
            grain: GrainSettings {
                amount: 0.3,
                size: 0.45,
                roughness: 0.6,
                color: 0.2,
                seed: 23,
            },
            ..Default::default()
        };
        let look_b = starroom_look::PortableLook {
            id: "look-b".into(),
            name: "Look B".into(),
            tone: ToneParameters {
                contrast: 0.4,
                ..Default::default()
            },
            relative_color: starroom_look::PortableRelativeColor {
                tint: -0.2,
                ..Default::default()
            },
            vignette: VignetteSettings {
                amount: 0.45,
                midpoint: 0.35,
                roundness: 0.2,
                feather: 0.55,
                highlight_protect: 0.8,
            },
            ..Default::default()
        };
        let mixed = starroom_look::mix_weighted(&look_a, &look_b, 70.0, 30.0, "A70 B30")
            .expect("weighted Look");
        let settings = RenderSettings {
            tone: mixed.tone,
            relative_color: RelativeColorParameters {
                temperature: mixed.relative_color.temperature,
                tint: mixed.relative_color.tint,
                vibrance: mixed.relative_color.vibrance,
                saturation: mixed.relative_color.saturation,
            },
            curves: ToneCurveSet {
                master: mixed.curves.master,
                red: mixed.curves.red,
                green: mixed.curves.green,
                blue: mixed.curves.blue,
            },
            color_mixer: mixed.color_mixer,
            grading: mixed.grading,
            denoise: mixed.denoise,
            local_detail: mixed.local_detail,
            sharpen: mixed.sharpen,
            grain: mixed.grain,
            vignette: mixed.vignette,
            layers: vec![NativeAdjustmentLayer {
                id: "masked-look-layer".into(),
                name: "Masked Look Layer".into(),
                enabled: true,
                opacity: 0.65,
                blend_mode: LayerBlendMode::Normal,
                mask: MaskDefinition::Radial {
                    x: 1.0 / 3.0,
                    y: 0.0,
                    width: 0.45,
                    height: 1.0,
                    rotation: 18.0,
                    feather: 0.25,
                    invert: false,
                }
                .into(),
                adjustments: LayerAdjustments {
                    tone: ToneParameters {
                        exposure_ev: 0.3,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            }],
            image_identity: "m23-style-mixer-layer-mask".into(),
            ..Default::default()
        };
        let preview = render_preview_to_srgb8(&decoded, &settings).expect("M23 preview");
        let export = render_export_to_srgb8(&decoded, &settings).expect("M23 export");
        assert_eq!(preview, export);
        assert_ne!(
            preview,
            render_preview_to_srgb8(&decoded, &RenderSettings::default()).expect("identity")
        );
    }

    #[test]
    fn embedded_icc_is_used_by_shared_graph() {
        let mut decoded = fixture(&[[0.3, 0.5, 0.7, 1.0]]);
        decoded.embedded_icc = Some(
            LittleCmsProvider
                .srgb_profile_bytes()
                .expect("serialize sRGB profile"),
        );
        let output = render_preview_to_srgb8(&decoded, &RenderSettings::default())
            .expect("profiled preview");
        assert_eq!(output.color.input, InputProfileSource::EmbeddedIcc);
    }

    #[test]
    fn invalid_embedded_icc_fails_the_shared_graph() {
        let mut decoded = fixture(&[[0.3, 0.5, 0.7, 1.0]]);
        decoded.embedded_icc = Some(b"broken profile".to_vec());
        let result = render_preview_to_srgb8(&decoded, &RenderSettings::default());
        assert!(matches!(
            result,
            Err(PipelineError::ColorManagement(
                ColorManagementError::InvalidProfile { .. }
            ))
        ));
    }

    #[test]
    fn supplied_output_profile_is_applied_and_reported() {
        let decoded = fixture(&[[0.2, 0.4, 0.6, 1.0]]);
        let output_profile = LittleCmsProvider
            .srgb_profile_bytes()
            .expect("serialize sRGB profile");
        let output = render_export_to_icc8(&decoded, &RenderSettings::default(), &output_profile)
            .expect("profiled export");
        assert_eq!(output.color.output, OutputProfileSource::SuppliedIcc);
    }

    #[test]
    fn display_profile_uses_the_same_preview_graph() {
        let decoded = fixture(&[[0.2, 0.4, 0.6, 1.0]]);
        let display_profile = LittleCmsProvider
            .srgb_profile_bytes()
            .expect("serialize display profile");
        let display =
            render_preview_to_display_icc8(&decoded, &RenderSettings::default(), &display_profile)
                .expect("display preview");
        let fallback = render_preview_to_srgb8(&decoded, &RenderSettings::default())
            .expect("fallback preview");
        assert_eq!(display.data, fallback.data);
        assert_eq!(display.color.output, OutputProfileSource::SuppliedIcc);
    }

    #[test]
    fn invalid_output_icc_fails_instead_of_falling_back() {
        let decoded = fixture(&[[0.2, 0.4, 0.6, 1.0]]);
        let result = render_export_to_icc8(
            &decoded,
            &RenderSettings::default(),
            b"broken output profile",
        );
        assert!(matches!(
            result,
            Err(PipelineError::ColorManagement(
                ColorManagementError::InvalidProfile { .. }
            ))
        ));
    }

    #[test]
    fn m27_high_precision_export_quantizes_only_after_the_shared_graph() {
        let values: Vec<[f32; 4]> = (0..1024)
            .map(|index| {
                let value = index as f32 / 1023.0;
                [value, value, value, 1.0]
            })
            .collect();
        let source = DecodedSourceImage::Rendered(fixture(&values));
        let high = render_source_export_to_srgb_f32(&source, &RenderSettings::default())
            .expect("float shared graph");
        let eight = render_source_export_to_srgb8(&source, &RenderSettings::default())
            .expect("8-bit compatibility surface");
        assert!(high.data.iter().all(|value| value.is_finite()));
        let unique_high = high
            .data
            .as_chunks::<3>()
            .0
            .iter()
            .map(|pixel| (pixel[0] * 65_535.0).round() as u16)
            .collect::<std::collections::BTreeSet<_>>();
        let unique_eight = eight
            .data
            .as_chunks::<3>()
            .0
            .iter()
            .map(|pixel| pixel[0])
            .collect::<std::collections::BTreeSet<_>>();
        assert!(unique_high.len() > unique_eight.len());
        for (float, quantized) in high.data.iter().zip(eight.data.iter()) {
            assert_eq!((float.clamp(0.0, 1.0) * 255.0).round() as u8, *quantized);
        }
    }

    #[test]
    fn m28_profiled_graph_measures_real_stages_without_changing_pixels() {
        let source = DecodedSourceImage::Rendered(fixture(&[
            [0.1, 0.2, 0.3, 1.0],
            [0.4, 0.5, 0.6, 1.0],
            [0.7, 0.8, 0.9, 1.0],
        ]));
        let settings = RenderSettings::default();
        let reference = render_source_preview_to_srgb8(&source, &settings).expect("reference");
        let (profiled, profile) = profile_source_preview_to_srgb8(&source, &settings);
        assert_eq!(profiled.expect("profiled"), reference);
        println!(
            "M28_PREVIEW_PROFILE {}",
            serde_json::to_string(&profile).expect("profile JSON")
        );
        for stage in [
            ProfileStage::CameraTransform,
            ProfileStage::WhiteBalance,
            ProfileStage::Tone,
            ProfileStage::Curve,
            ProfileStage::ColorMixer,
            ProfileStage::ColorGrading,
            ProfileStage::Mask,
            ProfileStage::Skin,
            ProfileStage::Healing,
            ProfileStage::Detail,
            ProfileStage::Geometry,
            ProfileStage::ColorTransform,
        ] {
            assert!(profile.stages.contains_key(&stage), "missing {stage:?}");
        }
        assert!(profile.total_cpu_nanoseconds > 0);
        assert!(profile.peak_working_bytes >= 3 * 3 * size_of::<f32>() as u64);
    }

    #[test]
    fn m30_identity_geometry_and_detail_take_exact_no_op_paths() {
        let source = LinearImage::new(
            2,
            2,
            vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2],
        )
        .expect("linear source");
        let settings = RenderSettings::default();
        let geometry_input = source.clone();
        let geometry_buffer = geometry_input.data.as_ptr();
        let prepared =
            apply_precreative_geometry(geometry_input, &settings, None).expect("identity geometry");
        assert_eq!(prepared.data.as_ptr(), geometry_buffer);
        let legacy_geometry = apply_geometry(
            source.width,
            source.height,
            &source.data,
            GeometryParameters::default(),
        )
        .expect("legacy identity geometry");
        assert_eq!(
            (prepared.width, prepared.height),
            (legacy_geometry.width, legacy_geometry.height)
        );
        assert!(
            prepared
                .data
                .iter()
                .zip(&legacy_geometry.data)
                .all(|(optimized, reference)| (optimized - reference).abs() <= 1.0e-6)
        );
        assert!(detail_stage_is_identity(&settings));
        let detail_input = source.clone();
        let detail_buffer = detail_input.data.as_ptr();
        let optimized_detail =
            apply_detail_stage(detail_input, &settings).expect("optimized identity detail");
        assert_eq!(optimized_detail.data.as_ptr(), detail_buffer);
        let denoised = denoise(&source, settings.denoise);
        let locally_adjusted = local_detail(&denoised, settings.local_detail);
        let detailed = sharpen(&locally_adjusted, settings.sharpen);
        let legacy_detail = apply_finishing_effects(
            &detailed,
            settings.grain,
            settings.vignette,
            &settings.image_identity,
        )
        .expect("legacy identity detail");
        assert_eq!(optimized_detail, legacy_detail);

        let mut adjusted = settings.clone();
        adjusted.local_detail.clarity = 0.01;
        assert!(!detail_stage_is_identity(&adjusted));
        adjusted = settings.clone();
        adjusted.grain.amount = 0.01;
        assert!(!detail_stage_is_identity(&adjusted));
        adjusted = settings;
        adjusted.vignette.amount = -0.01;
        assert!(!detail_stage_is_identity(&adjusted));
    }

    #[test]
    fn rc2_identity_creative_stages_preserve_pixels_exactly() {
        let pixels = vec![
            [0.0, 0.1, 0.2],
            [0.4, 0.5, 0.6],
            [0.9, 1.0, 1.5],
            [-0.1, 0.2, 0.3],
        ];
        let expected = pixels.iter().flatten().copied().collect::<Vec<_>>();
        let actual = apply_creative_graph(pixels, 2, 2, &RenderSettings::default(), None)
            .expect("identity creative graph");
        assert_eq!(actual, expected);

        let mut exposure = RenderSettings::default();
        exposure.tone.exposure_ev = 1.0;
        let adjusted = apply_creative_graph(vec![[0.1, 0.2, 0.3]], 1, 1, &exposure, None)
            .expect("exposure creative graph");
        assert_ne!(adjusted, vec![0.1, 0.2, 0.3]);
    }
}
