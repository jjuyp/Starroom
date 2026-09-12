//! Deterministic healing-brush reference for Starroom.
//! V1 copies nearby texture while adapting low-frequency color/luminance and feathering the
//! destination. AI/content-aware inpainting remains a replaceable future provider.

use serde::{Deserialize, Serialize};
use starroom_detail::{LinearImage, gaussian_blur};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HealPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HealStroke {
    pub source: HealPoint,
    pub destination: HealPoint,
    /// Radius in full-resolution pixels.
    pub radius: f32,
    /// 0 hard edge, 1 broad feather.
    pub feather: f32,
    /// 0..1 blend strength.
    pub opacity: f32,
}

/// M18 keeps Clone and frequency-aware Heal as explicit non-destructive operations. AI inpaint
/// is deliberately reserved by the serialized enum but has no implementation or fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum HealMode {
    Clone,
    #[default]
    Heal,
    AiInpaint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SourceMode {
    #[default]
    Auto,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealingOperation {
    pub id: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub mode: HealMode,
    pub target: HealPoint,
    pub source: Option<HealPoint>,
    /// Radius is source-image pixels, independent of preview zoom.
    pub radius: f32,
    pub feather: f32,
    pub opacity: f32,
    #[serde(default)]
    pub rotation_degrees: f32,
    #[serde(default = "one")]
    pub scale: f32,
    #[serde(default = "enabled")]
    pub tone_adaptation: bool,
    #[serde(default = "enabled")]
    pub texture_adaptation: bool,
    #[serde(default)]
    pub source_mode: SourceMode,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

const fn enabled() -> bool {
    true
}
const fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealError {
    InvalidOperation,
    MissingManualSource,
    AiInpaintUnavailable,
}

impl HealingOperation {
    pub fn validate(&self) -> Result<(), HealError> {
        if self.id.trim().is_empty()
            || ![
                self.target.x,
                self.target.y,
                self.radius,
                self.feather,
                self.opacity,
                self.rotation_degrees,
                self.scale,
            ]
            .into_iter()
            .all(f32::is_finite)
            || self
                .source
                .is_some_and(|source| ![source.x, source.y].into_iter().all(f32::is_finite))
            || !(0.0..=1.0).contains(&self.target.x)
            || !(0.0..=1.0).contains(&self.target.y)
            || self.source.is_some_and(|source| {
                !(0.0..=1.0).contains(&source.x) || !(0.0..=1.0).contains(&source.y)
            })
            || !(0.5..=2048.0).contains(&self.radius)
            || !(0.0..=1.0).contains(&self.feather)
            || !(0.0..=1.0).contains(&self.opacity)
            || !(-180.0..=180.0).contains(&self.rotation_degrees)
            || !(0.1..=4.0).contains(&self.scale)
        {
            return Err(HealError::InvalidOperation);
        }
        if self.source_mode == SourceMode::Manual && self.source.is_none() {
            return Err(HealError::MissingManualSource);
        }
        if self.mode == HealMode::AiInpaint {
            return Err(HealError::AiInpaintUnavailable);
        }
        Ok(())
    }
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if (edge1 - edge0).abs() < f32::EPSILON {
        return if value < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn normalized_to_pixel(point: HealPoint, image: &LinearImage) -> (f32, f32) {
    (
        point.x.clamp(0.0, 1.0) * image.width.saturating_sub(1) as f32,
        point.y.clamp(0.0, 1.0) * image.height.saturating_sub(1) as f32,
    )
}

fn sample_bilinear(image: &LinearImage, x: f32, y: f32, channel: usize) -> f32 {
    if image.width == 0 || image.height == 0 {
        return 0.0;
    }
    let x = x.clamp(0.0, image.width.saturating_sub(1) as f32);
    let y = y.clamp(0.0, image.height.saturating_sub(1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(image.width - 1);
    let y1 = (y0 + 1).min(image.height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let read = |px: usize, py: usize| image.data[(py * image.width + px) * 3 + channel];
    let top = read(x0, y0) * (1.0 - tx) + read(x1, y0) * tx;
    let bottom = read(x0, y1) * (1.0 - tx) + read(x1, y1) * tx;
    top * (1.0 - ty) + bottom * ty
}

/// Applies one circular healing stroke. Source high-frequency texture is preserved while the
/// source patch low frequency is shifted toward the destination low frequency.
pub fn apply_heal(image: &LinearImage, stroke: HealStroke) -> LinearImage {
    if image.width == 0 || image.height == 0 || stroke.radius <= 0.0 || stroke.opacity <= 0.0 {
        return image.clone();
    }
    let radius = stroke.radius.clamp(0.5, 512.0);
    let feather = stroke.feather.clamp(0.0, 1.0);
    let opacity = stroke.opacity.clamp(0.0, 1.0);
    let low = gaussian_blur(image, (radius * 0.2).clamp(1.0, 24.0));
    let (source_x, source_y) = normalized_to_pixel(stroke.source, image);
    let (destination_x, destination_y) = normalized_to_pixel(stroke.destination, image);
    let delta_x = source_x - destination_x;
    let delta_y = source_y - destination_y;

    let left = (destination_x - radius).floor().max(0.0) as usize;
    let right = (destination_x + radius)
        .ceil()
        .min(image.width.saturating_sub(1) as f32) as usize;
    let top = (destination_y - radius).floor().max(0.0) as usize;
    let bottom = (destination_y + radius)
        .ceil()
        .min(image.height.saturating_sub(1) as f32) as usize;
    let inner = radius * (1.0 - feather * 0.85);

    let mut output = image.clone();
    for y in top..=bottom {
        for x in left..=right {
            let dx = x as f32 - destination_x;
            let dy = y as f32 - destination_y;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > radius {
                continue;
            }
            let edge = if radius <= inner + 1.0e-5 {
                1.0
            } else {
                1.0 - smoothstep(inner, radius, distance)
            };
            let blend = edge * opacity;
            let source_sample_x = x as f32 + delta_x;
            let source_sample_y = y as f32 + delta_y;
            for channel in 0..3 {
                let source_value =
                    sample_bilinear(image, source_sample_x, source_sample_y, channel);
                let source_low = sample_bilinear(&low, source_sample_x, source_sample_y, channel);
                let destination_low = low.data[(y * image.width + x) * 3 + channel];
                let adapted = destination_low + (source_value - source_low);
                let index = (y * image.width + x) * 3 + channel;
                output.data[index] = image.data[index] * (1.0 - blend) + adapted * blend;
            }
        }
    }
    output
}

fn patch_statistics(image: &LinearImage, center: HealPoint, radius: f32) -> ([f32; 3], f32) {
    let (cx, cy) = normalized_to_pixel(center, image);
    let sample_radius = radius.clamp(1.0, 96.0);
    let left = (cx - sample_radius).floor().max(0.0) as usize;
    let right = (cx + sample_radius)
        .ceil()
        .min(image.width.saturating_sub(1) as f32) as usize;
    let top = (cy - sample_radius).floor().max(0.0) as usize;
    let bottom = (cy + sample_radius)
        .ceil()
        .min(image.height.saturating_sub(1) as f32) as usize;
    let mut mean = [0.0; 3];
    let mut count = 0.0_f32;
    for y in top..=bottom {
        for x in left..=right {
            if ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt() > sample_radius {
                continue;
            }
            for (channel, value) in mean.iter_mut().enumerate() {
                *value += image.data[(y * image.width + x) * 3 + channel];
            }
            count += 1.0;
        }
    }
    for value in &mut mean {
        *value /= count.max(1.0);
    }
    let mut variance = 0.0;
    for y in top..=bottom {
        for x in left..=right {
            let index = (y * image.width + x) * 3;
            let luma = image.data[index] * 0.2627
                + image.data[index + 1] * 0.6780
                + image.data[index + 2] * 0.0593;
            let mean_luma = mean[0] * 0.2627 + mean[1] * 0.6780 + mean[2] * 0.0593;
            variance += (luma - mean_luma).powi(2);
        }
    }
    (mean, variance / count.max(1.0))
}

/// Deterministic local-source selector. It minimizes luminance/chroma/texture mismatch while
/// excluding overlapping target samples; no AI model or non-reproducible heuristic is involved.
pub fn select_auto_source(
    image: &LinearImage,
    target: HealPoint,
    radius: f32,
) -> Option<HealPoint> {
    if image.width == 0 || image.height == 0 {
        return None;
    }
    let (target_mean, target_variance) = patch_statistics(image, target, radius);
    let mut best: Option<(f32, HealPoint)> = None;
    for grid_y in 1..8 {
        for grid_x in 1..8 {
            let candidate = HealPoint {
                x: grid_x as f32 / 8.0,
                y: grid_y as f32 / 8.0,
            };
            let distance = (candidate.x - target.x).hypot(candidate.y - target.y);
            let normalized_radius = radius / image.width.max(image.height) as f32;
            if distance < normalized_radius * 2.25 {
                continue;
            }
            let (mean, variance) = patch_statistics(image, candidate, radius);
            let chroma = (mean[0] - target_mean[0]).abs()
                + (mean[1] - target_mean[1]).abs()
                + (mean[2] - target_mean[2]).abs();
            let score = chroma * 2.0 + (variance - target_variance).abs() * 4.0 + distance * 0.05;
            if best.as_ref().is_none_or(|(current, _)| score < *current) {
                best = Some((score, candidate));
            }
        }
    }
    best.map(|(_, point)| point)
}

/// Applies a serializable M18 operation. Clone is exact high/low transfer; Heal uses
/// TargetLow + SourceHigh, then optional tone/texture adaptation, feather and opacity.
pub fn apply_operation(
    image: &LinearImage,
    operation: &HealingOperation,
) -> Result<LinearImage, HealError> {
    operation.validate()?;
    if !operation.enabled || operation.opacity <= 0.0 {
        return Ok(image.clone());
    }
    let source = match operation.source_mode {
        SourceMode::Manual => operation.source.ok_or(HealError::MissingManualSource)?,
        SourceMode::Auto => operation.source.unwrap_or_else(|| {
            select_auto_source(image, operation.target, operation.radius)
                .unwrap_or(operation.target)
        }),
    };
    let radius = operation.radius;
    let feather = operation.feather;
    let low = gaussian_blur(image, (radius * 0.2).clamp(1.0, 24.0));
    let (source_x, source_y) = normalized_to_pixel(source, image);
    let (target_x, target_y) = normalized_to_pixel(operation.target, image);
    let inner = radius * (1.0 - feather * 0.85);
    let left = (target_x - radius).floor().max(0.0) as usize;
    let right = (target_x + radius)
        .ceil()
        .min(image.width.saturating_sub(1) as f32) as usize;
    let top = (target_y - radius).floor().max(0.0) as usize;
    let bottom = (target_y + radius)
        .ceil()
        .min(image.height.saturating_sub(1) as f32) as usize;
    let angle = operation.rotation_degrees.to_radians();
    let (cos, sin) = (angle.cos(), angle.sin());
    let mut output = image.clone();
    for y in top..=bottom {
        for x in left..=right {
            let dx = x as f32 - target_x;
            let dy = y as f32 - target_y;
            let distance = dx.hypot(dy);
            if distance > radius {
                continue;
            }
            let edge = if radius <= inner + 1.0e-5 {
                1.0
            } else {
                1.0 - smoothstep(inner, radius, distance)
            };
            let sx = source_x + (dx * cos + dy * sin) / operation.scale;
            let sy = source_y + (-dx * sin + dy * cos) / operation.scale;
            for channel in 0..3 {
                let source_value = sample_bilinear(image, sx, sy, channel);
                let source_low = sample_bilinear(&low, sx, sy, channel);
                let target_low = low.data[(y * image.width + x) * 3 + channel];
                let transferred = match operation.mode {
                    HealMode::Clone => source_value,
                    HealMode::Heal => {
                        let high = if operation.texture_adaptation {
                            source_value - source_low
                        } else {
                            source_value - target_low
                        };
                        if operation.tone_adaptation {
                            target_low + high
                        } else {
                            source_low + high
                        }
                    }
                    HealMode::AiInpaint => return Err(HealError::AiInpaintUnavailable),
                };
                let index = (y * image.width + x) * 3 + channel;
                let blend = edge * operation.opacity;
                output.data[index] = image.data[index] * (1.0 - blend) + transferred * blend;
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb_index(width: usize, x: usize, y: usize) -> usize {
        (y * width + x) * 3
    }

    fn flat_with_spot() -> LinearImage {
        let mut data = vec![0.2; 7 * 3 * 3];
        let center = rgb_index(7, 3, 1);
        data[center] = 0.9;
        data[center + 1] = 0.1;
        data[center + 2] = 0.1;
        LinearImage::new(7, 3, data).expect("fixture")
    }

    #[test]
    fn zero_opacity_is_identity() {
        let image = flat_with_spot();
        let output = apply_heal(
            &image,
            HealStroke {
                source: HealPoint { x: 0.1, y: 0.5 },
                destination: HealPoint { x: 0.5, y: 0.5 },
                radius: 2.0,
                feather: 0.5,
                opacity: 0.0,
            },
        );
        assert_eq!(output, image);
    }

    #[test]
    fn healing_reduces_isolated_color_spot() {
        let image = flat_with_spot();
        let center = rgb_index(7, 3, 1);
        let output = apply_heal(
            &image,
            HealStroke {
                source: HealPoint { x: 0.0, y: 0.5 },
                destination: HealPoint { x: 0.5, y: 0.5 },
                radius: 1.5,
                feather: 0.4,
                opacity: 1.0,
            },
        );
        assert!(output.data[center] < image.data[center]);
        assert!(output.data[center + 1] > image.data[center + 1]);
    }

    #[test]
    fn healing_stays_finite_at_edges() {
        let image = flat_with_spot();
        let output = apply_heal(
            &image,
            HealStroke {
                source: HealPoint { x: 0.0, y: 0.0 },
                destination: HealPoint { x: 1.0, y: 1.0 },
                radius: 4.0,
                feather: 1.0,
                opacity: 1.0,
            },
        );
        assert!(output.data.iter().all(|value| value.is_finite()));
    }

    fn operation(mode: HealMode, source_mode: SourceMode) -> HealingOperation {
        HealingOperation {
            id: "heal-1".into(),
            enabled: true,
            mode,
            target: HealPoint { x: 0.5, y: 0.5 },
            source: Some(HealPoint { x: 0.0, y: 0.5 }),
            radius: 1.5,
            feather: 0.4,
            opacity: 1.0,
            rotation_degrees: 0.0,
            scale: 1.0,
            tone_adaptation: true,
            texture_adaptation: true,
            source_mode,
            metadata: BTreeMap::from([("tool".into(), "brush".into())]),
        }
    }

    #[test]
    fn m18_clone_and_heal_are_deterministic_non_destructive_operations() {
        let image = flat_with_spot();
        let clone = apply_operation(&image, &operation(HealMode::Clone, SourceMode::Manual))
            .expect("clone");
        let heal =
            apply_operation(&image, &operation(HealMode::Heal, SourceMode::Manual)).expect("heal");
        let repeat = apply_operation(&image, &operation(HealMode::Heal, SourceMode::Manual))
            .expect("repeat");
        let center = rgb_index(7, 3, 1);
        assert!(clone.data[center] < image.data[center]);
        assert!(heal.data[center].is_finite());
        assert_eq!(heal, repeat, "manual operations must be deterministic");
        assert_eq!(image.data[center], 0.9, "input image is never modified");
    }

    #[test]
    fn m18_auto_source_is_deterministic_and_ai_inpaint_is_typed_unavailable() {
        let image = flat_with_spot();
        let first =
            select_auto_source(&image, HealPoint { x: 0.5, y: 0.5 }, 1.5).expect("auto source");
        let second =
            select_auto_source(&image, HealPoint { x: 0.5, y: 0.5 }, 1.5).expect("auto source");
        assert_eq!(first, second);
        let mut ai = operation(HealMode::AiInpaint, SourceMode::Manual);
        assert_eq!(
            apply_operation(&image, &ai),
            Err(HealError::AiInpaintUnavailable)
        );
        ai.mode = HealMode::Heal;
        ai.source = None;
        assert_eq!(
            apply_operation(&image, &ai),
            Err(HealError::MissingManualSource)
        );
    }
}
