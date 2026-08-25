//! Native rendered-image CPU pipeline for Starroom v0.2.
//! This is the executable reference graph for JPEG/PNG/TIFF editing. Future wgpu stages must
//! match this pipeline within documented tolerances before replacing the CPU reference.

use serde::{Deserialize, Serialize};
use starroom_ai_denoise::{AiDenoiseError, AiDenoiseParameters, AiDenoiseResidual, apply_residual};
use starroom_color::{
    ColorBand, ColorMixer, CurvePoint, LinearRgb, ToneParameters, apply_color_mixer, apply_tone,
    compress_to_unit_gamut, map_monotone_curve, oklab_to_oklch, oklab_to_rec2020, oklch_to_oklab,
    rec2020_to_oklab, sample_color_band,
};
use starroom_color_management::{
    ColorManagementError, InputProfileSource, LittleCmsProvider, OutputProfileSource,
    RenderingIntent,
};
use starroom_detail::{
    DenoiseParameters, LinearImage, LocalDetailParameters, SharpenParameters, denoise,
    local_detail, sharpen,
};
use starroom_geometry::{
    GeometryParameters, UprightMode, analyze_upright, apply_geometry, apply_upright,
};
use starroom_grading::{GradingParameters, apply_grading};
use starroom_heal::{HealingOperation, apply_operation};
use starroom_imageio::{DecodedRenderedImage, DecodedSourceImage, lens_metadata};
use starroom_look::{GrainSettings, LookError, VignetteSettings, apply_finishing_effects};
use starroom_optics::{
    LensIdentity, LensProfileResolution, LensProfileStatus, LensfunProvider, OpticsSettings,
    apply_lens_correction,
};
use starroom_portrait::{SkinRetouchParameters, apply_skin_retouch};
use starroom_project::{
    GeneratedMaskSemantic, MaskDefinition, MaskOperation, MaskTree, PortraitMaskRegion,
};
use starroom_raw::{CameraProfileDescriptor, CameraProfileStatus, DecodedRawImage};
use starroom_render::gpu::{GpuError, GpuRenderer};
use starroom_render::profiling::{self, ProfileStage};
use std::time::Instant;
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

/// Normalized source-space rectangle used by the native Neutral Picker.
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
    fn weight_at(&self, x: f32, y: f32) -> Result<f32, PipelineError> {
        if self.width == 0
            || self.height == 0
            || self.values.len() != self.width as usize * self.height as usize
            || self.values.iter().any(|value| !value.is_finite())
        {
            return Err(PipelineError::InvalidMask("portrait raster is malformed"));
        }
        let px = (x.clamp(0.0, 1.0) * self.width.saturating_sub(1) as f32).round() as usize;
        let py = (y.clamp(0.0, 1.0) * self.height.saturating_sub(1) as f32).round() as usize;
        Ok(self.values[py * self.width as usize + px].clamp(0.0, 1.0))
    }
}

impl GeneratedMaskRaster {
    fn weight_at(&self, x: f32, y: f32) -> Result<f32, PipelineError> {
        if self.width == 0
            || self.height == 0
            || self.values.len() != self.width as usize * self.height as usize
            || self.values.iter().any(|value| !value.is_finite())
        {
            return Err(PipelineError::InvalidMask(
                "generated AI raster is malformed",
            ));
        }
        let px = (x.clamp(0.0, 1.0) * self.width.saturating_sub(1) as f32).round() as usize;
        let py = (y.clamp(0.0, 1.0) * self.height.saturating_sub(1) as f32).round() as usize;
        Ok(self.values[py * self.width as usize + px].clamp(0.0, 1.0))
    }
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
        }
    }
}

#[derive(Debug, Error)]
pub enum PipelineError {
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

fn neutral_scale(sum: [f32; 3], count: usize) -> Option<[f32; 3]> {
    if count == 0 || !sum.into_iter().all(f32::is_finite) {
        return None;
    }
    let mean = sum.map(|channel| channel / count as f32);
    // Green is the stable reference used by common RAW pipelines; refuse black/non-finite
    // samples instead of inventing a white point.
    if mean.iter().any(|channel| *channel <= 1.0e-6) {
        return None;
    }
    Some([mean[1] / mean[0], 1.0, mean[1] / mean[2]])
}

fn apply_diagonal_white_balance(pixels: &mut [[f32; 3]], scale: [f32; 3]) {
    for pixel in pixels {
        pixel[0] *= scale[0];
        pixel[1] *= scale[1];
        pixel[2] *= scale[2];
    }
}

pub trait AutoWhiteBalanceProvider {
    fn estimate_scale(&self, pixels: &[[f32; 3]]) -> Option<[f32; 3]>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GrayWorldAutoWhiteBalance;

impl AutoWhiteBalanceProvider for GrayWorldAutoWhiteBalance {
    fn estimate_scale(&self, pixels: &[[f32; 3]]) -> Option<[f32; 3]> {
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
        neutral_scale(sum, count)
    }
}

fn picker_white_balance_scale(
    pixels: &[[f32; 3]],
    width: u32,
    height: u32,
    sample: WhiteBalanceSample,
) -> Option<[f32; 3]> {
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
    neutral_scale(sum, count)
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
            let scale = GrayWorldAutoWhiteBalance
                .estimate_scale(pixels)
                .ok_or(PipelineError::InvalidWhiteBalanceSample)?;
            apply_diagonal_white_balance(pixels, scale);
            Ok(())
        }
        (_, WhiteBalanceMode::NeutralPicker) => {
            let sample = settings
                .sample
                .ok_or(PipelineError::InvalidWhiteBalanceSample)?;
            let scale = picker_white_balance_scale(pixels, width, height, sample)
                .ok_or(PipelineError::InvalidWhiteBalanceSample)?;
            apply_diagonal_white_balance(pixels, scale);
            Ok(())
        }
        (SourceKind::Raw, WhiteBalanceMode::Relative) => Err(PipelineError::WhiteBalanceSemantic {
            mode: settings.mode,
            input_kind: "RAW",
        }),
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
                .into_iter()
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

    let mut lch = oklab_to_oklch(lab);
    let saturation = parameters.saturation.clamp(-1.0, 1.0);
    let vibrance = parameters.vibrance.clamp(-1.0, 1.0);
    let normalized_chroma = (lch.c / 0.32).clamp(0.0, 1.0);
    let vibrance_weight = 1.0 - normalized_chroma;
    let scale = (1.0 + saturation * 0.85 + vibrance * vibrance_weight * 0.65).max(0.0);
    lch.c *= scale;
    oklab_to_rec2020(oklch_to_oklab(lch))
}

fn apply_one_curve(value: f32, curve: &[CurvePoint]) -> f32 {
    if curve.len() < 2 {
        value
    } else {
        map_monotone_curve(value, curve)
    }
}
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
        } => {
            if face_id.trim().is_empty()
                || cache_key.trim().is_empty()
                || model_id.trim().is_empty()
                || model_version.trim().is_empty()
                || model_hash.len() != 64
                || ![*threshold, *feather].into_iter().all(f32::is_finite)
                || !(0.0..=1.0).contains(threshold)
                || *feather < 0.0
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
            let value = raster.weight_at(x, y)?;
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
            let probability = raster.weight_at(x, y)?;
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

fn mask_weight(
    mask: &MaskTree,
    x: f32,
    y: f32,
    rgb: LinearRgb,
    portrait_masks: &[PortraitMaskRaster],
    generated_masks: &[GeneratedMaskRaster],
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
        MaskTree::Leaf(leaf) => mask_leaf_weight(leaf, x, y, rgb, portrait_masks, generated_masks),
        MaskTree::Composite(composite) => {
            let mut children = composite.children.iter();
            let Some(first) = children.next() else {
                return Ok(0.0);
            };
            let first_weight = mask_weight(first, x, y, rgb, portrait_masks, generated_masks)?;
            match composite.operation {
                MaskOperation::Add => children.try_fold(first_weight, |value, child| {
                    Ok(value.max(mask_weight(
                        child,
                        x,
                        y,
                        rgb,
                        portrait_masks,
                        generated_masks,
                    )?))
                }),
                MaskOperation::Subtract => children.try_fold(first_weight, |value, child| {
                    Ok(value
                        * (1.0 - mask_weight(child, x, y, rgb, portrait_masks, generated_masks)?))
                }),
                MaskOperation::Intersect => children.try_fold(first_weight, |value, child| {
                    Ok(value.min(mask_weight(
                        child,
                        x,
                        y,
                        rgb,
                        portrait_masks,
                        generated_masks,
                    )?))
                }),
                MaskOperation::Invert => Ok(1.0 - first_weight),
            }
        }
    }
}

fn apply_layers(
    mut rgb: LinearRgb,
    layers: &[NativeAdjustmentLayer],
    x: f32,
    y: f32,
    portrait_masks: &[PortraitMaskRaster],
    generated_masks: &[GeneratedMaskRaster],
) -> Result<LinearRgb, PipelineError> {
    for layer in layers {
        if !layer.enabled {
            continue;
        }
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
        let adjusted = apply_grading(
            apply_color_mixer(
                apply_curve(
                    apply_tone(
                        apply_relative_color(rgb, layer.adjustments.relative_color),
                        layer.adjustments.tone,
                    ),
                    &[],
                    &layer.adjustments.curves,
                ),
                layer.adjustments.color_mixer,
            ),
            layer.adjustments.grading,
        );
        // M14 deliberately supports only Normal. Any future mode must earn an explicit
        // scene-linear implementation rather than quietly behaving like Normal.
        let weight =
            layer.opacity * mask_weight(&layer.mask, x, y, rgb, portrait_masks, generated_masks)?;
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

fn skin_retouch_is_identity(parameters: SkinRetouchParameters) -> bool {
    parameters == SkinRetouchParameters::default()
}

fn apply_skin_retouch_stage(
    data: Vec<f32>,
    width: usize,
    height: usize,
    settings: &RenderSettings,
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
                let x = (pixel % width) as f32 / width.max(1) as f32;
                let y = (pixel / width) as f32 / height.max(1) as f32;
                let value = raster.weight_at(x, y)?;
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
        image = apply_operation(&image, operation).map_err(|error| {
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

fn apply_creative_graph(
    pixels: Vec<[f32; 3]>,
    width: usize,
    height: usize,
    settings: &RenderSettings,
    gpu: Option<&GpuRenderer>,
) -> Result<Vec<f32>, PipelineError> {
    // The GPU accelerates the existing scene-linear Exposure node only. Subsequent stages keep
    // the established CPU reference math until each earns its own parity gate; this avoids a
    // second color-science implementation.
    let pixel_count = pixels.len();
    let working_bytes = (pixel_count as u64).saturating_mul(3 * u64::from(f32::BITS / 8));
    let prepared = profiling::measure(ProfileStage::WhiteBalance, working_bytes, || {
        pixels
            .into_iter()
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
    });
    let (prepared, tone_parameters) = if let Some(renderer) = gpu {
        let input: Vec<[f32; 4]> = prepared
            .iter()
            .map(|rgb| [rgb.r, rgb.g, rgb.b, 1.0])
            .collect();
        let started = Instant::now();
        let exposed = profiling::measure(ProfileStage::Tone, working_bytes, || {
            renderer.apply_exposure(&input, settings.tone.exposure_ev)
        })?;
        let elapsed = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        profiling::record_gpu(ProfileStage::Tone, elapsed);
        let mut remainder = settings.tone;
        remainder.exposure_ev = 0.0;
        (
            exposed
                .into_iter()
                .map(|pixel| LinearRgb {
                    r: pixel[0],
                    g: pixel[1],
                    b: pixel[2],
                })
                .collect::<Vec<_>>(),
            remainder,
        )
    } else {
        (prepared, settings.tone)
    };
    let mut prepared = prepared;
    profiling::measure(ProfileStage::Tone, working_bytes, || {
        prepared
            .iter_mut()
            .for_each(|rgb| *rgb = apply_tone(*rgb, tone_parameters));
    });
    profiling::measure(ProfileStage::Curve, working_bytes, || {
        prepared
            .iter_mut()
            .for_each(|rgb| *rgb = apply_curve(*rgb, &settings.curve, &settings.curves));
    });
    profiling::measure(ProfileStage::ColorMixer, working_bytes, || {
        prepared
            .iter_mut()
            .for_each(|rgb| *rgb = apply_color_mixer(*rgb, settings.color_mixer));
    });
    profiling::measure(ProfileStage::ColorGrading, working_bytes, || {
        prepared
            .iter_mut()
            .for_each(|rgb| *rgb = apply_grading(*rgb, settings.grading));
    });
    profiling::measure(ProfileStage::Mask, working_bytes, || {
        for (index, rgb) in prepared.iter_mut().enumerate() {
            let x = (index % width) as f32 / width.max(1) as f32;
            let y = (index / width) as f32 / height.max(1) as f32;
            *rgb = apply_layers(
                *rgb,
                &settings.layers,
                x,
                y,
                &settings.portrait_masks,
                &settings.generated_masks,
            )?;
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
    let data = profiling::measure(ProfileStage::Skin, working_bytes, || {
        apply_skin_retouch_stage(data, width, height, settings)
    })?;
    profiling::measure(ProfileStage::Healing, working_bytes, || {
        apply_healing_stage(data, width, height, settings)
    })
}

fn to_working_image(
    decoded: &DecodedRenderedImage,
    settings: &RenderSettings,
) -> Result<(LinearImage, InputProfileSource), PipelineError> {
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
    let working_bytes = (pixels.len() as u64).saturating_mul(3 * u64::from(f32::BITS / 8));
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
            settings.white_balance,
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
    let working_bytes = (pixels.len() as u64).saturating_mul(3 * u64::from(f32::BITS / 8));
    profiling::measure(ProfileStage::WhiteBalance, working_bytes, || {
        apply_white_balance(
            &mut pixels,
            decoded.width,
            decoded.height,
            SourceKind::Raw,
            settings.white_balance,
        )
    })?;
    let data = pixels.into_iter().flatten().collect();
    LinearImage::new(decoded.width as usize, decoded.height as usize, data)
        .map_err(|_| PipelineError::DetailBuffer)
}

/// Samples the actual native working graph at normalized image coordinates for M7's targeted
/// Color Mixer tool. The browser transports only the selected enum, never image pixels or color
/// science. RAW and encoded inputs therefore use exactly the same decode/WB/creative stages as
/// preview and export.
pub fn sample_source_color_band(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
    x: f32,
    y: f32,
) -> Result<Option<ColorBand>, PipelineError> {
    if !x.is_finite() || !y.is_finite() || !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return Err(PipelineError::InvalidDecodedBuffer);
    }
    let (image, width, height) = match decoded {
        DecodedSourceImage::Rendered(source) => {
            let (image, _) = to_working_image(source, settings)?;
            (image, source.width as usize, source.height as usize)
        }
        DecodedSourceImage::Raw(source) => (
            to_working_raw(source, settings)?,
            source.width as usize,
            source.height as usize,
        ),
    };
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
    let geometry_image = apply_precreative_geometry(working, settings, optics_resolution)?;
    render_prepared_working_graph(
        geometry_image,
        input_source,
        camera_profile,
        settings,
        output_icc,
        gpu,
    )
}

fn apply_precreative_geometry(
    working: LinearImage,
    settings: &RenderSettings,
    optics_resolution: Option<&LensProfileResolution>,
) -> Result<LinearImage, PipelineError> {
    let working_bytes = (working.data.len() as u64).saturating_mul(u64::from(f32::BITS / 8));
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
        LinearImage::new(working.width, working.height, corrected.data)
            .map_err(|_| PipelineError::DetailBuffer)?
    } else {
        working
    };
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
    let geometrically_corrected = profiling::measure(ProfileStage::Geometry, working_bytes, || {
        apply_geometry(
            optically_corrected.width,
            optically_corrected.height,
            &optically_corrected.data,
            geometry_parameters,
        )
    })
    .map_err(|_| PipelineError::Geometry)?;
    LinearImage::new(
        geometrically_corrected.width,
        geometrically_corrected.height,
        geometrically_corrected.data,
    )
    .map_err(|_| PipelineError::DetailBuffer)
}

fn render_prepared_working_graph(
    geometry_image: LinearImage,
    input_source: InputProfileSource,
    camera_profile: Option<&CameraProfileDescriptor>,
    settings: &RenderSettings,
    output_icc: Option<&[u8]>,
    gpu: Option<&GpuRenderer>,
) -> Result<RenderedRgbF32, PipelineError> {
    // M21 is intentionally before tone/curve/mixer/grading. Inference and control adjustment
    // caches are separate; an enabled request without its native residual is a typed failure.
    let working_bytes = (geometry_image.data.len() as u64).saturating_mul(u64::from(f32::BITS / 8));
    let model_adjusted = if settings.ai_denoise.enabled {
        let residual = settings
            .ai_denoise_residual
            .as_ref()
            .ok_or(AiDenoiseError::ResidualMismatch)?;
        let skin = settings.portrait_masks.iter().find(|mask| {
            mask.region == PortraitMaskRegion::Skin
                && mask.width as usize == geometry_image.width
                && mask.height as usize == geometry_image.height
        });
        profiling::measure(ProfileStage::AiDenoise, working_bytes, || {
            apply_residual(
                &geometry_image,
                residual,
                settings.ai_denoise,
                skin.map(|mask| mask.values.as_slice()),
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
        apply_creative_graph(
            pixels,
            model_adjusted.width,
            model_adjusted.height,
            settings,
            gpu,
        )?,
    )
    .map_err(|_| PipelineError::DetailBuffer)?;
    let detailed = profiling::measure(ProfileStage::Detail, working_bytes, || {
        let denoised = denoise(&creative, settings.denoise);
        let locally_adjusted = local_detail(&denoised, settings.local_detail);
        let detailed = sharpen(&locally_adjusted, settings.sharpen);
        apply_finishing_effects(
            &detailed,
            settings.grain,
            settings.vignette,
            &settings.image_identity,
        )
    })?;
    let mut pixels = Vec::with_capacity(width as usize * height as usize);
    for pixel in detailed.data.as_chunks::<3>().0 {
        let working_rgb = compress_to_unit_gamut(LinearRgb {
            r: pixel[0],
            g: pixel[1],
            b: pixel[2],
        });
        pixels.push([working_rgb.r, working_rgb.g, working_rgb.b]);
    }
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

/// Returns the exact Linear Rec.2020 D65 image presented to M21, after source colour, optics,
/// orientation/crop and geometry, but before NAFNet and every tone/creative/detail stage.
pub fn prepare_source_for_ai_denoise(
    decoded: &DecodedSourceImage,
    settings: &RenderSettings,
) -> Result<LinearImage, PipelineError> {
    let optics_resolution = if settings.optics.parameters.enabled {
        Some(resolve_source_lens_profile(decoded, &settings.optics)?)
    } else {
        None
    };
    let working = match decoded {
        DecodedSourceImage::Rendered(image) => to_working_image(image, settings)?.0,
        DecodedSourceImage::Raw(image) => to_working_raw(image, settings)?,
    };
    apply_precreative_geometry(working, settings, optics_resolution.as_ref())
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
            radius: 0.3,
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
                assert!(
                    hybrid
                        .data
                        .iter()
                        .zip(cpu.data)
                        .all(|(gpu, cpu)| { i16::from(*gpu).abs_diff(i16::from(cpu)) <= 1 })
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
            .chunks_exact(3)
            .map(|pixel| (pixel[0] * 65_535.0).round() as u16)
            .collect::<std::collections::BTreeSet<_>>();
        let unique_eight = eight
            .data
            .chunks_exact(3)
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
}
