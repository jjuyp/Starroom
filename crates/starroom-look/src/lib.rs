//! Portable `.srlook` schema, semantic blending, deterministic grain, and HDR-safe vignette.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use starroom_color::{BandAdjustment, ColorMixer, CurvePoint, ToneParameters, map_monotone_curve};
use starroom_detail::{DenoiseParameters, LinearImage, LocalDetailParameters, SharpenParameters};
use starroom_grading::{ColorWheel, GradingParameters};
use std::collections::BTreeMap;
use thiserror::Error;

pub const LOOK_SCHEMA: &str = "https://starroom.app/schemas/look/v1";
/// Portable look schema limit, also used by the Native edit boundary.
pub const MAX_CURVE_POINTS: usize = 256;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LookMetadata {
    pub author: String,
    pub description: String,
    pub tags: Vec<String>,
    pub extensions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableRelativeColor {
    pub temperature: f32,
    pub tint: f32,
    pub vibrance: f32,
    pub saturation: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableCurves {
    pub master: Vec<CurvePoint>,
    pub red: Vec<CurvePoint>,
    pub green: Vec<CurvePoint>,
    pub blue: Vec<CurvePoint>,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrainSettings {
    pub amount: f32,
    pub size: f32,
    pub roughness: f32,
    /// 0 is monochrome grain, 1 is independent RGB grain.
    pub color: f32,
    pub seed: u64,
}
impl Default for GrainSettings {
    fn default() -> Self {
        Self {
            amount: 0.0,
            size: 0.5,
            roughness: 0.5,
            color: 0.0,
            seed: 0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VignetteSettings {
    pub amount: f32,
    pub midpoint: f32,
    pub roundness: f32,
    pub feather: f32,
    /// Reduces edge exposure change on scene-linear highlights, 0..1.
    pub highlight_protect: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableLook {
    pub schema: String,
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub metadata: LookMetadata,
    pub tone: ToneParameters,
    pub relative_color: PortableRelativeColor,
    pub curves: PortableCurves,
    pub color_mixer: ColorMixer,
    pub grading: GradingParameters,
    pub denoise: DenoiseParameters,
    pub local_detail: LocalDetailParameters,
    pub sharpen: SharpenParameters,
    pub grain: GrainSettings,
    pub vignette: VignetteSettings,
}
impl Default for PortableLook {
    fn default() -> Self {
        Self {
            schema: LOOK_SCHEMA.into(),
            schema_version: 1,
            id: "neutral".into(),
            name: "Neutral".into(),
            metadata: Default::default(),
            tone: Default::default(),
            relative_color: Default::default(),
            curves: Default::default(),
            color_mixer: Default::default(),
            grading: Default::default(),
            denoise: Default::default(),
            local_detail: Default::default(),
            sharpen: Default::default(),
            grain: Default::default(),
            vignette: Default::default(),
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum LookError {
    #[error("unsupported look schema/version")]
    UnsupportedSchema,
    #[error("look JSON is invalid: {0}")]
    InvalidJson(String),
    #[error("look contains non-finite or out-of-range values")]
    InvalidValues,
    #[error("look effect image is malformed")]
    InvalidImage,
}
impl PortableLook {
    pub fn from_json(json: &str) -> Result<Self, LookError> {
        let look: Self =
            serde_json::from_str(json).map_err(|e| LookError::InvalidJson(e.to_string()))?;
        look.validate()?;
        Ok(look)
    }
    pub fn to_json(&self) -> Result<String, LookError> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|e| LookError::InvalidJson(e.to_string()))
    }
    pub fn validate(&self) -> Result<(), LookError> {
        if self.schema != LOOK_SCHEMA || self.schema_version != 1 {
            return Err(LookError::UnsupportedSchema);
        }
        if self.id.trim().is_empty()
            || self.name.trim().is_empty()
            || self.name.chars().count() > 160
            || self.metadata.author.chars().count() > 160
            || self.metadata.description.chars().count() > 2_000
            || self.metadata.tags.len() > 64
            || self
                .metadata
                .tags
                .iter()
                .any(|tag| tag.chars().count() > 80)
            || self
                .metadata
                .extensions
                .iter()
                .any(|(key, value)| key.chars().count() > 80 || value.chars().count() > 500)
        {
            return Err(LookError::InvalidValues);
        }
        let values = [
            self.tone.exposure_ev,
            self.tone.contrast,
            self.tone.highlights,
            self.tone.shadows,
            self.tone.whites,
            self.tone.blacks,
            self.relative_color.temperature,
            self.relative_color.tint,
            self.relative_color.vibrance,
            self.relative_color.saturation,
            self.grain.amount,
            self.grain.size,
            self.grain.roughness,
            self.grain.color,
            self.vignette.amount,
            self.vignette.midpoint,
            self.vignette.roundness,
            self.vignette.feather,
            self.vignette.highlight_protect,
            self.color_mixer.band_width_degrees,
            self.grading.balance,
            self.grading.blending,
            self.grading.amount,
            self.denoise.luminance,
            self.denoise.chroma,
            self.denoise.radius,
            self.denoise.detail_protection,
            self.denoise.high_iso,
            self.local_detail.texture,
            self.local_detail.clarity,
            self.local_detail.dehaze,
            self.sharpen.amount,
            self.sharpen.radius,
            self.sharpen.detail,
            self.sharpen.masking,
            self.sharpen.halo_protection,
            self.sharpen.threshold,
        ];
        if values.iter().any(|v| !v.is_finite())
            || !signed(self.tone.contrast)
            || !signed(self.tone.highlights)
            || !signed(self.tone.shadows)
            || !signed(self.tone.whites)
            || !signed(self.tone.blacks)
            || !signed(self.relative_color.temperature)
            || !signed(self.relative_color.tint)
            || !signed(self.relative_color.vibrance)
            || !signed(self.relative_color.saturation)
            || !unit(self.grain.amount)
            || !(0.1..=1.0).contains(&self.grain.size)
            || !unit(self.grain.roughness)
            || !unit(self.grain.color)
            || self.vignette.amount.abs() > 1.0
            || !unit(self.vignette.midpoint)
            || !signed(self.vignette.roundness)
            || !unit(self.vignette.feather)
            || !unit(self.vignette.highlight_protect)
            || !(1.0..=180.0).contains(&self.color_mixer.band_width_degrees)
            || !self.color_mixer.bands.iter().all(|band| {
                band.hue_degrees.is_finite()
                    && (-180.0..=180.0).contains(&band.hue_degrees)
                    && signed(band.chroma)
                    && signed(band.lightness)
            })
            || !valid_wheel(self.grading.shadows)
            || !valid_wheel(self.grading.midtones)
            || !valid_wheel(self.grading.highlights)
            || !valid_wheel(self.grading.global)
            || !signed(self.grading.balance)
            || !unit(self.grading.blending)
            || !unit(self.grading.amount)
            || !unit(self.denoise.luminance)
            || !unit(self.denoise.chroma)
            || !(0.25..=24.0).contains(&self.denoise.radius)
            || !unit(self.denoise.detail_protection)
            || !unit(self.denoise.high_iso)
            || !signed(self.local_detail.texture)
            || !signed(self.local_detail.clarity)
            || !signed(self.local_detail.dehaze)
            || !unit(self.sharpen.amount)
            || !(0.25..=24.0).contains(&self.sharpen.radius)
            || !unit(self.sharpen.detail)
            || !unit(self.sharpen.masking)
            || !unit(self.sharpen.halo_protection)
            || !unit(self.sharpen.threshold)
            || !valid_curve(&self.curves.master)
            || !valid_curve(&self.curves.red)
            || !valid_curve(&self.curves.green)
            || !valid_curve(&self.curves.blue)
        {
            return Err(LookError::InvalidValues);
        }
        Ok(())
    }
}
fn unit(value: f32) -> bool {
    (0.0..=1.0).contains(&value)
}
fn signed(value: f32) -> bool {
    (-1.0..=1.0).contains(&value)
}
fn valid_wheel(wheel: ColorWheel) -> bool {
    wheel.hue_degrees.is_finite()
        && wheel.chroma.is_finite()
        && wheel.lightness.is_finite()
        && signed(wheel.chroma)
        && signed(wheel.lightness)
}
fn valid_curve(points: &[CurvePoint]) -> bool {
    points.len() <= MAX_CURVE_POINTS
        && points.iter().all(|point| {
            point.x.is_finite() && point.y.is_finite() && unit(point.x) && unit(point.y)
        })
        && points
            .windows(2)
            .all(|pair| pair[0].x < pair[1].x && pair[0].y <= pair[1].y)
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn hue_lerp(a: f32, b: f32, t: f32) -> f32 {
    (a + ((b - a + 180.0).rem_euclid(360.0) - 180.0) * t).rem_euclid(360.0)
}
fn wheel(a: ColorWheel, b: ColorWheel, t: f32) -> ColorWheel {
    ColorWheel {
        hue_degrees: hue_lerp(a.hue_degrees, b.hue_degrees, t),
        chroma: lerp(a.chroma, b.chroma, t),
        lightness: lerp(a.lightness, b.lightness, t),
    }
}
fn sample_curve(points: &[CurvePoint], x: f32) -> f32 {
    if points.is_empty() {
        x
    } else {
        map_monotone_curve(x, points)
    }
}
fn curve_blend(a: &[CurvePoint], b: &[CurvePoint], t: f32) -> Vec<CurvePoint> {
    (0..=32)
        .map(|i| {
            let x = i as f32 / 32.0;
            CurvePoint {
                x,
                y: lerp(sample_curve(a, x), sample_curve(b, x), t),
            }
        })
        .collect()
}
pub fn blend(
    a: &PortableLook,
    b: &PortableLook,
    amount: f32,
    name: impl Into<String>,
) -> PortableLook {
    let t = amount.clamp(0.0, 1.0);
    if t <= f32::EPSILON {
        return a.clone();
    }
    if t >= 1.0 - f32::EPSILON {
        return b.clone();
    }
    let l = |x, y| lerp(x, y, t);
    let tone = ToneParameters {
        exposure_ev: l(a.tone.exposure_ev, b.tone.exposure_ev),
        contrast: l(a.tone.contrast, b.tone.contrast),
        highlights: l(a.tone.highlights, b.tone.highlights),
        shadows: l(a.tone.shadows, b.tone.shadows),
        whites: l(a.tone.whites, b.tone.whites),
        blacks: l(a.tone.blacks, b.tone.blacks),
    };
    let mut mixer = ColorMixer::default();
    for i in 0..8 {
        mixer.bands[i] = BandAdjustment {
            // Mixer values are signed angular OFFSETS in -30..30, not absolute
            // wheel hues. Wrapping a negative offset into 0..360 makes it invalid.
            hue_degrees: l(
                a.color_mixer.bands[i].hue_degrees,
                b.color_mixer.bands[i].hue_degrees,
            ),
            chroma: l(a.color_mixer.bands[i].chroma, b.color_mixer.bands[i].chroma),
            lightness: l(
                a.color_mixer.bands[i].lightness,
                b.color_mixer.bands[i].lightness,
            ),
        };
    }
    mixer.hue_lock = if t < 0.5 {
        a.color_mixer.hue_lock
    } else {
        b.color_mixer.hue_lock
    };
    mixer.band_width_degrees = l(
        a.color_mixer.band_width_degrees,
        b.color_mixer.band_width_degrees,
    );
    let grading = GradingParameters {
        shadows: wheel(a.grading.shadows, b.grading.shadows, t),
        midtones: wheel(a.grading.midtones, b.grading.midtones, t),
        highlights: wheel(a.grading.highlights, b.grading.highlights, t),
        global: wheel(a.grading.global, b.grading.global, t),
        balance: l(a.grading.balance, b.grading.balance),
        blending: l(a.grading.blending, b.grading.blending),
        amount: l(a.grading.amount, b.grading.amount),
    };
    let name = name.into();
    let id = format!(
        "blend-{:x}",
        Sha256::digest(format!("{}:{}:{t:.6}", a.id, b.id).as_bytes())
    );
    PortableLook {
        schema: LOOK_SCHEMA.into(),
        schema_version: 1,
        id,
        name,
        metadata: if t < 0.5 {
            a.metadata.clone()
        } else {
            b.metadata.clone()
        },
        tone,
        relative_color: PortableRelativeColor {
            temperature: l(a.relative_color.temperature, b.relative_color.temperature),
            tint: l(a.relative_color.tint, b.relative_color.tint),
            vibrance: l(a.relative_color.vibrance, b.relative_color.vibrance),
            saturation: l(a.relative_color.saturation, b.relative_color.saturation),
        },
        curves: PortableCurves {
            master: curve_blend(&a.curves.master, &b.curves.master, t),
            red: curve_blend(&a.curves.red, &b.curves.red, t),
            green: curve_blend(&a.curves.green, &b.curves.green, t),
            blue: curve_blend(&a.curves.blue, &b.curves.blue, t),
        },
        color_mixer: mixer,
        grading,
        denoise: DenoiseParameters {
            luminance: l(a.denoise.luminance, b.denoise.luminance),
            chroma: l(a.denoise.chroma, b.denoise.chroma),
            radius: l(a.denoise.radius, b.denoise.radius),
            detail_protection: l(a.denoise.detail_protection, b.denoise.detail_protection),
            high_iso: l(a.denoise.high_iso, b.denoise.high_iso),
        },
        local_detail: LocalDetailParameters {
            texture: l(a.local_detail.texture, b.local_detail.texture),
            clarity: l(a.local_detail.clarity, b.local_detail.clarity),
            dehaze: l(a.local_detail.dehaze, b.local_detail.dehaze),
        },
        sharpen: SharpenParameters {
            amount: l(a.sharpen.amount, b.sharpen.amount),
            radius: l(a.sharpen.radius, b.sharpen.radius),
            detail: l(a.sharpen.detail, b.sharpen.detail),
            masking: l(a.sharpen.masking, b.sharpen.masking),
            halo_protection: l(a.sharpen.halo_protection, b.sharpen.halo_protection),
            threshold: l(a.sharpen.threshold, b.sharpen.threshold),
        },
        grain: GrainSettings {
            amount: l(a.grain.amount, b.grain.amount),
            size: l(a.grain.size, b.grain.size),
            roughness: l(a.grain.roughness, b.grain.roughness),
            color: l(a.grain.color, b.grain.color),
            seed: if t < 0.5 { a.grain.seed } else { b.grain.seed },
        },
        vignette: VignetteSettings {
            amount: l(a.vignette.amount, b.vignette.amount),
            midpoint: l(a.vignette.midpoint, b.vignette.midpoint),
            roundness: l(a.vignette.roundness, b.vignette.roundness),
            feather: l(a.vignette.feather, b.vignette.feather),
            highlight_protect: l(a.vignette.highlight_protect, b.vignette.highlight_protect),
        },
    }
}

pub fn mix_weighted(
    a: &PortableLook,
    b: &PortableLook,
    weight_a: f32,
    weight_b: f32,
    name: impl Into<String>,
) -> Result<PortableLook, LookError> {
    if !weight_a.is_finite()
        || !weight_b.is_finite()
        || weight_a < 0.0
        || weight_b < 0.0
        || weight_a + weight_b <= f32::EPSILON
    {
        return Err(LookError::InvalidValues);
    }
    Ok(blend(a, b, weight_b / (weight_a + weight_b), name))
}

fn random_unit(seed: u64, x: u64, y: u64, c: u64) -> f32 {
    let mut z = seed
        ^ x.wrapping_mul(0x9E3779B185EBCA87)
        ^ y.wrapping_mul(0xC2B2AE3D27D4EB4F)
        ^ c.wrapping_mul(0x165667B19E3779F9);
    z ^= z >> 30;
    z = z.wrapping_mul(0xBF58476D1CE4E5B9);
    z ^= z >> 27;
    z = z.wrapping_mul(0x94D049BB133111EB);
    ((z ^ (z >> 31)) >> 40) as f32 / 16_777_215.0
}
fn valid_finishing_layout(image: &LinearImage) -> bool {
    image
        .width
        .checked_mul(image.height)
        .and_then(|pixels| pixels.checked_mul(3))
        == Some(image.data.len())
}

pub fn apply_finishing_effects(
    image: &LinearImage,
    grain: GrainSettings,
    vignette: VignetteSettings,
    image_identity: &str,
) -> Result<LinearImage, LookError> {
    if !valid_finishing_layout(image) {
        return Err(LookError::InvalidImage);
    }
    apply_finishing_effects_owned(image.clone(), grain, vignette, image_identity)
}

/// Same deterministic finishing kernel, consuming the mutable working buffer instead of copying it.
pub fn apply_finishing_effects_owned(
    image: LinearImage,
    grain: GrainSettings,
    vignette: VignetteSettings,
    image_identity: &str,
) -> Result<LinearImage, LookError> {
    if !valid_finishing_layout(&image) {
        return Err(LookError::InvalidImage);
    }
    let LinearImage {
        width,
        height,
        mut data,
    } = image;
    // Off must not execute per-pixel vignette, noise hashes and grain arithmetic merely
    // because another detail control is enabled. Retain the same finite/shape validation.
    if grain.amount == 0.0 && vignette.amount == 0.0 {
        return LinearImage::new(width, height, data).map_err(|_| LookError::InvalidImage);
    }
    let identity = Sha256::digest(image_identity.as_bytes());
    let seed = grain.seed ^ u64::from_le_bytes(identity[..8].try_into().unwrap());
    let cx = (width.saturating_sub(1)) as f32 * 0.5;
    let cy = (height.saturating_sub(1)) as f32 * 0.5;
    let aspect = width as f32 / height.max(1) as f32;
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) * 3;
            let dx = (x as f32 - cx) / cx.max(1.0);
            let dy = (y as f32 - cy) / cy.max(1.0);
            let roundness = (vignette.roundness.clamp(-1.0, 1.0) + 1.0) * 0.5;
            let aspect_correction = 1.0 + (aspect.max(1.0) - 1.0) * (1.0 - roundness);
            let radius = ((dx * aspect_correction).powi(2) + dy.powi(2)).sqrt();
            let edge = ((radius - vignette.midpoint.clamp(0.0, 1.0))
                / (vignette.feather.abs().max(0.02)))
            .clamp(0.0, 1.0);
            let smooth = edge * edge * (3.0 - 2.0 * edge);
            let luminance =
                (0.2627 * data[i] + 0.6780 * data[i + 1] + 0.0593 * data[i + 2]).max(0.0);
            let highlight_weight = ((luminance - 0.55) / 1.45).clamp(0.0, 1.0);
            let protected = 1.0
                - vignette.highlight_protect.clamp(0.0, 1.0)
                    * highlight_weight
                    * highlight_weight
                    * (3.0 - 2.0 * highlight_weight);
            let vig_ev = -vignette.amount.clamp(-1.0, 1.0) * 2.0 * smooth * protected;
            let scale = 2.0f32.powf(vig_ev);
            let grain_gain = grain.amount.clamp(0.0, 1.0)
                * (0.003 + 0.025 * grain.size.clamp(0.1, 1.0))
                * (0.35 + 0.65 * luminance.sqrt());
            let cell = 1 + (grain.size.clamp(0.1, 1.0) * 4.0).round() as u64;
            let coarse_x = x as u64 / cell;
            let coarse_y = y as u64 / cell;
            let coarse = (random_unit(seed, coarse_x, coarse_y, 0) - 0.5) * 2.0;
            let fine = (random_unit(seed, x as u64, y as u64, 0) - 0.5) * 2.0;
            let mono = lerp(coarse, fine, grain.roughness.clamp(0.0, 1.0));
            for c in 0..3 {
                let colored = (random_unit(seed, x as u64, y as u64, c as u64 + 1) - 0.5) * 2.0;
                let noise = lerp(mono, colored, grain.color.clamp(0.0, 1.0));
                data[i + c] = data[i + c] * scale + noise * grain_gain;
            }
        }
    }
    LinearImage::new(width, height, data).map_err(|_| LookError::InvalidImage)
}

#[cfg(test)]
#[path = "../tests/oracles/borrowed_finishing_v1.rs"]
mod borrowed_finishing_reference;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_finishing_preserves_allocation_exact_frozen_bits_and_typed_validation() {
        for (width, height) in [(0, 0), (1, 1), (1, 17), (31, 19)] {
            let values = [-0.0, 0.0, -0.3, 0.18, 0.7, 1.0, 1.5, 9.0];
            let source = LinearImage::new(
                width,
                height,
                (0..width * height * 3)
                    .map(|i| values[(i * 13 + i / 3) % values.len()])
                    .collect(),
            )
            .unwrap();
            for amount in [0.0, 0.5, 1.0] {
                for vig in [-1.0, 0.0, 1.0] {
                    let grain = GrainSettings {
                        amount,
                        color: 0.7,
                        roughness: 0.3,
                        ..Default::default()
                    };
                    let vignette = VignetteSettings {
                        amount: vig,
                        highlight_protect: 0.8,
                        ..Default::default()
                    };
                    let input = source.clone();
                    let pointer = input.data.as_ptr();
                    let capacity = input.data.capacity();
                    let actual =
                        apply_finishing_effects_owned(input, grain, vignette, "owned-reference")
                            .unwrap();
                    let expected = borrowed_finishing_reference::finishing(
                        &source,
                        grain,
                        vignette,
                        "owned-reference",
                    )
                    .unwrap();
                    assert_eq!(actual.data.as_ptr(), pointer);
                    assert_eq!(actual.data.capacity(), capacity);
                    assert_eq!(
                        actual.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                        expected
                            .data
                            .iter()
                            .map(|v| v.to_bits())
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
        for invalid in [
            LinearImage {
                width: usize::MAX,
                height: 2,
                data: vec![],
            },
            LinearImage {
                width: 1,
                height: 1,
                data: vec![0.0, f32::NAN, 0.0],
            },
            LinearImage {
                width: 1,
                height: 1,
                data: vec![0.0, f32::INFINITY, 0.0],
            },
            LinearImage {
                width: 1,
                height: 1,
                data: vec![0.0],
            },
        ] {
            for amount in [0.0, 0.5] {
                assert_eq!(
                    apply_finishing_effects(
                        &invalid,
                        GrainSettings {
                            amount,
                            ..Default::default()
                        },
                        VignetteSettings::default(),
                        "invalid"
                    ),
                    Err(LookError::InvalidImage)
                );
                assert_eq!(
                    apply_finishing_effects_owned(
                        invalid.clone(),
                        GrainSettings {
                            amount,
                            ..Default::default()
                        },
                        VignetteSettings::default(),
                        "invalid"
                    ),
                    Err(LookError::InvalidImage)
                );
            }
        }
    }
    #[test]
    fn off_finishing_is_exact_hdr_identity_and_preserves_validation() {
        let source =
            LinearImage::new(3, 1, vec![-0.0, 0.0, 1.0e-9, -0.3, 1.5, 9.0, 0.2, 0.4, 0.9]).unwrap();
        let output = apply_finishing_effects(
            &source,
            GrainSettings::default(),
            VignetteSettings::default(),
            "off",
        )
        .unwrap();
        assert_eq!(
            source.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            output.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );
        let mut invalid = source.clone();
        invalid.data[0] = f32::NAN;
        assert_eq!(
            apply_finishing_effects(
                &invalid,
                GrainSettings::default(),
                VignetteSettings::default(),
                "off"
            ),
            Err(LookError::InvalidImage)
        );
        invalid.data.pop();
        assert_eq!(
            apply_finishing_effects(
                &invalid,
                GrainSettings::default(),
                VignetteSettings::default(),
                "off"
            ),
            Err(LookError::InvalidImage)
        );
    }
    fn image() -> LinearImage {
        LinearImage::new(32, 24, vec![0.5; 32 * 24 * 3]).unwrap()
    }
    #[test]
    fn look_json_round_trip_and_schema_validation() {
        let a = PortableLook::default();
        assert_eq!(a, PortableLook::from_json(&a.to_json().unwrap()).unwrap());
        let mut b = a;
        b.schema_version = 2;
        assert_eq!(b.validate(), Err(LookError::UnsupportedSchema));
    }
    #[test]
    fn amount_endpoints_and_circular_hue_are_semantic() {
        let a = PortableLook::default();
        let mut b = a.clone();
        b.id = "b".into();
        b.tone.exposure_ev = 2.0;
        b.grading.global.hue_degrees = 350.0;
        let mut c = a.clone();
        c.grading.global.hue_degrees = 10.0;
        assert_eq!(blend(&a, &b, 0.0, "x").tone, a.tone);
        assert_eq!(blend(&a, &b, 1.0, "x").tone, b.tone);
        assert!(
            blend(&c, &b, 0.5, "x").grading.global.hue_degrees.abs() < 1e-4
                || blend(&c, &b, 0.5, "x").grading.global.hue_degrees > 359.9
        );
    }
    #[test]
    fn mixer_blends_signed_offsets_without_wrapping_into_absolute_hues() {
        for a_offset in [-30.0, -10.0, 0.0, 10.0, 30.0] {
            for b_offset in [-30.0, -10.0, 0.0, 10.0, 30.0] {
                for amount in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    let mut a = PortableLook::default();
                    let mut b = a.clone();
                    b.id = "signed-offset-target".into();
                    for band in &mut a.color_mixer.bands {
                        band.hue_degrees = a_offset;
                    }
                    for band in &mut b.color_mixer.bands {
                        band.hue_degrees = b_offset;
                    }
                    a.validate().unwrap();
                    b.validate().unwrap();
                    let result = blend(&a, &b, amount, "signed offsets");
                    result.validate().unwrap();
                    for band in result.color_mixer.bands {
                        assert_eq!(band.hue_degrees, a_offset + (b_offset - a_offset) * amount);
                        assert!((-30.0..=30.0).contains(&band.hue_degrees));
                    }
                }
            }
        }
    }

    #[test]
    fn style_mixer_normalizes_seventy_thirty_weights() {
        let mut a = PortableLook::default();
        a.tone.exposure_ev = 0.0;
        let mut b = a.clone();
        b.id = "b".into();
        b.tone.exposure_ev = 1.0;
        let mixed = mix_weighted(&a, &b, 70.0, 30.0, "70/30").unwrap();
        assert!((mixed.tone.exposure_ev - 0.3).abs() < 1e-6);
        assert_eq!(
            mix_weighted(&a, &b, 0.0, 0.0, "invalid"),
            Err(LookError::InvalidValues)
        );
    }
    #[test]
    fn curve_blend_is_sampled_and_monotonic_for_monotonic_inputs() {
        let mut a = PortableLook::default();
        let mut b = a.clone();
        a.curves.master = vec![CurvePoint { x: 0.0, y: 0.0 }, CurvePoint { x: 1.0, y: 1.0 }];
        b.curves.master = vec![
            CurvePoint { x: 0.0, y: 0.1 },
            CurvePoint { x: 0.5, y: 0.7 },
            CurvePoint { x: 1.0, y: 1.0 },
        ];
        let c = blend(&a, &b, 0.5, "c");
        assert!(c.curves.master.windows(2).all(|p| p[0].y <= p[1].y));
    }
    #[test]
    fn grain_and_vignette_are_deterministic_finite_and_hdr_safe() {
        let mut a = image();
        a.data[0] = 4.0;
        let g = GrainSettings {
            amount: 0.7,
            size: 0.5,
            roughness: 0.2,
            color: 0.3,
            seed: 42,
        };
        let v = VignetteSettings {
            amount: 0.5,
            midpoint: 0.4,
            roundness: 0.0,
            feather: 0.5,
            highlight_protect: 0.8,
        };
        let x = apply_finishing_effects(&a, g, v, "id").unwrap();
        let y = apply_finishing_effects(&a, g, v, "id").unwrap();
        assert_eq!(x, y);
        assert!(x.data.iter().all(|v| v.is_finite()));
        assert!(x.data[0] > 1.0);
    }

    #[test]
    fn unknown_or_non_portable_fields_and_invalid_controls_are_rejected() {
        let json = PortableLook::default().to_json().unwrap();
        let forbidden = json.replacen("{", "{\"crop\":{},", 1);
        assert!(matches!(
            PortableLook::from_json(&forbidden),
            Err(LookError::InvalidJson(_))
        ));
        assert!(matches!(
            PortableLook::from_json("{not-json"),
            Err(LookError::InvalidJson(_))
        ));
        let mut look = PortableLook::default();
        look.grain.color = 1.1;
        assert_eq!(look.validate(), Err(LookError::InvalidValues));
        look.grain.color = 0.0;
        look.curves.master = vec![CurvePoint { x: 0.5, y: 0.8 }, CurvePoint { x: 0.4, y: 0.2 }];
        assert_eq!(look.validate(), Err(LookError::InvalidValues));
    }

    #[test]
    fn highlight_protect_reduces_vignette_at_hdr_edges() {
        let mut source = LinearImage::new(9, 9, vec![0.25; 9 * 9 * 3]).unwrap();
        source.data[0..3].fill(4.0);
        let vignette = VignetteSettings {
            amount: 1.0,
            midpoint: 0.2,
            roundness: 0.0,
            feather: 0.4,
            highlight_protect: 0.0,
        };
        let unprotected =
            apply_finishing_effects(&source, GrainSettings::default(), vignette, "id").unwrap();
        let protected = apply_finishing_effects(
            &source,
            GrainSettings::default(),
            VignetteSettings {
                highlight_protect: 1.0,
                ..vignette
            },
            "id",
        )
        .unwrap();
        assert!(protected.data[0] > unprotected.data[0]);
        assert!(protected.data.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn m23_signed_vignette_and_full_color_grain_extremes_remain_finite() {
        let source = LinearImage::new(
            17,
            13,
            (0..17 * 13)
                .flat_map(|index: usize| {
                    let value = if index.is_multiple_of(11) {
                        8.0
                    } else {
                        index as f32 / 221.0
                    };
                    [value, value * 0.7, value * 0.3]
                })
                .collect(),
        )
        .unwrap();
        for amount in [-1.0, 1.0] {
            let output = apply_finishing_effects(
                &source,
                GrainSettings {
                    amount: 1.0,
                    size: 1.0,
                    roughness: 1.0,
                    color: 1.0,
                    seed: u64::MAX,
                },
                VignetteSettings {
                    amount,
                    midpoint: 0.0,
                    roundness: if amount < 0.0 { -1.0 } else { 1.0 },
                    feather: 0.0,
                    highlight_protect: 1.0,
                },
                "m23-extreme-hdr",
            )
            .unwrap();
            assert!(output.data.iter().all(|value| value.is_finite()));
            assert!(output.data.iter().any(|value| *value > 1.0));
        }
    }
}
