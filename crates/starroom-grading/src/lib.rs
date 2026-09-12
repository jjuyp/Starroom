//! Four-way OKLab color grading for Starroom.
//! The zone-overlap and balance semantics follow the mature design of darktable Color Balance
//! RGB, while this compact CPU operator is independently implemented against Starroom's linear
//! Rec.2020 D65 graph. UI code only transports parameters.

use serde::{Deserialize, Serialize};
use starroom_color::{LinearRgb, Oklab, oklab_to_rec2020, rec2020_to_oklab};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ColorWheel {
    /// 0..360 degrees.
    pub hue_degrees: f32,
    /// 0..1 creative amount.
    #[serde(alias = "saturation")]
    pub chroma: f32,
    /// Relative lightness offset, -1..1.
    #[serde(alias = "luminance")]
    pub lightness: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradingParameters {
    pub shadows: ColorWheel,
    pub midtones: ColorWheel,
    pub highlights: ColorWheel,
    pub global: ColorWheel,
    /// -1 moves tonal crossover toward shadows, +1 toward highlights.
    pub balance: f32,
    /// 0 keeps zones tighter, 1 maximizes overlap.
    pub blending: f32,
    /// Master effect amount, 0..1.
    pub amount: f32,
}

impl Default for GradingParameters {
    fn default() -> Self {
        Self {
            shadows: ColorWheel::default(),
            midtones: ColorWheel::default(),
            highlights: ColorWheel::default(),
            global: ColorWheel::default(),
            balance: 0.0,
            blending: 0.5,
            amount: 1.0,
        }
    }
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let width = edge1 - edge0;
    if width.abs() < f32::EPSILON {
        return if value < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((value - edge0) / width).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn wheel_vector(wheel: ColorWheel) -> (f32, f32, f32) {
    let angle = wheel.hue_degrees.rem_euclid(360.0).to_radians();
    let chroma = wheel.chroma.clamp(-1.0, 1.0) * 0.12;
    (
        angle.cos() * chroma,
        angle.sin() * chroma,
        wheel.lightness.clamp(-1.0, 1.0) * 0.12,
    )
}

fn tonal_weights(lightness: f32, balance: f32, blending: f32) -> (f32, f32, f32) {
    let balance_shift = balance.clamp(-1.0, 1.0) * 0.12;
    let overlap = 0.08 + blending.clamp(0.0, 1.0) * 0.18;
    let shadow_end = 0.42 + balance_shift;
    let highlight_start = 0.58 + balance_shift;

    let shadow = 1.0 - smoothstep(shadow_end - overlap, shadow_end + overlap, lightness);
    let highlight = smoothstep(
        highlight_start - overlap,
        highlight_start + overlap,
        lightness,
    );
    let midtone = (1.0 - shadow.max(highlight)).clamp(0.0, 1.0);
    let sum = (shadow + midtone + highlight).max(f32::EPSILON);
    (shadow / sum, midtone / sum, highlight / sum)
}

fn apply_wheel(lab: &mut Oklab, wheel: ColorWheel, weight: f32, amount: f32) {
    if weight <= f32::EPSILON || amount <= f32::EPSILON {
        return;
    }
    let (a, b, l) = wheel_vector(wheel);
    lab.a += a * weight * amount;
    lab.b += b * weight * amount;
    lab.l += l * weight * amount;
}

pub fn apply_grading(rgb: LinearRgb, parameters: GradingParameters) -> LinearRgb {
    if ![rgb.r, rgb.g, rgb.b].into_iter().all(f32::is_finite) {
        return LinearRgb {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        };
    }
    let amount = parameters.amount.clamp(0.0, 1.0);
    let wheels = [
        parameters.shadows,
        parameters.midtones,
        parameters.highlights,
        parameters.global,
    ];
    if amount <= f32::EPSILON
        || wheels.into_iter().all(|wheel| {
            wheel.chroma.abs() <= f32::EPSILON && wheel.lightness.abs() <= f32::EPSILON
        })
    {
        return rgb;
    }
    let mut lab = rec2020_to_oklab(rgb);
    let source_lightness = lab.l;
    let (shadow_weight, midtone_weight, highlight_weight) =
        tonal_weights(source_lightness, parameters.balance, parameters.blending);

    apply_wheel(&mut lab, parameters.shadows, shadow_weight, amount);
    apply_wheel(&mut lab, parameters.midtones, midtone_weight, amount);
    apply_wheel(&mut lab, parameters.highlights, highlight_weight, amount);
    apply_wheel(&mut lab, parameters.global, 1.0, amount);
    lab.l = lab.l.max(0.0);
    let output = oklab_to_rec2020(lab);
    if [output.r, output.g, output.b]
        .into_iter()
        .all(f32::is_finite)
    {
        output
    } else {
        rgb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distance(a: LinearRgb, b: LinearRgb) -> f32 {
        (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs()
    }

    #[test]
    fn zero_amount_is_identity() {
        let source = LinearRgb {
            r: 0.2,
            g: 0.15,
            b: 0.1,
        };
        let output = apply_grading(
            source,
            GradingParameters {
                amount: 0.0,
                ..Default::default()
            },
        );
        assert_eq!(source, output);
    }

    #[test]
    fn shadow_wheel_affects_dark_pixel_more_than_bright_pixel() {
        let parameters = GradingParameters {
            shadows: ColorWheel {
                hue_degrees: 220.0,
                chroma: 0.7,
                lightness: 0.0,
            },
            ..Default::default()
        };
        let dark = LinearRgb {
            r: 0.03,
            g: 0.025,
            b: 0.02,
        };
        let bright = LinearRgb {
            r: 0.8,
            g: 0.75,
            b: 0.7,
        };
        let dark_delta = distance(dark, apply_grading(dark, parameters));
        let bright_delta = distance(bright, apply_grading(bright, parameters));
        assert!(dark_delta > bright_delta);
    }

    #[test]
    fn global_wheel_affects_all_tones() {
        let parameters = GradingParameters {
            global: ColorWheel {
                hue_degrees: 35.0,
                chroma: 0.4,
                lightness: 0.0,
            },
            ..Default::default()
        };
        let source = LinearRgb {
            r: 0.45,
            g: 0.35,
            b: 0.25,
        };
        assert!(distance(source, apply_grading(source, parameters)) > 1.0e-4);
    }

    #[test]
    fn grading_stays_finite_at_extreme_controls() {
        let parameters = GradingParameters {
            shadows: ColorWheel {
                hue_degrees: 3600.0,
                chroma: 2.0,
                lightness: -3.0,
            },
            midtones: ColorWheel {
                hue_degrees: -720.0,
                chroma: 1.0,
                lightness: 1.0,
            },
            highlights: ColorWheel {
                hue_degrees: 120.0,
                chroma: 1.0,
                lightness: 1.0,
            },
            global: ColorWheel {
                hue_degrees: 300.0,
                chroma: 1.0,
                lightness: 1.0,
            },
            balance: 5.0,
            blending: 5.0,
            amount: 5.0,
        };
        let output = apply_grading(
            LinearRgb {
                r: 0.3,
                g: 0.2,
                b: 0.1,
            },
            parameters,
        );
        assert!(output.r.is_finite() && output.g.is_finite() && output.b.is_finite());
    }

    #[test]
    fn m8_neutral_is_exact_identity_and_hdr_is_finite() {
        for source in [
            LinearRgb {
                r: 0.18,
                g: 0.18,
                b: 0.18,
            },
            LinearRgb {
                r: 6.0,
                g: 1.7,
                b: 0.2,
            },
        ] {
            assert_eq!(apply_grading(source, GradingParameters::default()), source);
            let output = apply_grading(
                source,
                GradingParameters {
                    highlights: ColorWheel {
                        hue_degrees: 42.0,
                        chroma: 1.0,
                        lightness: -1.0,
                    },
                    amount: 1.0,
                    ..Default::default()
                },
            );
            assert!(
                [output.r, output.g, output.b]
                    .into_iter()
                    .all(f32::is_finite)
            );
        }
    }

    #[test]
    fn m8_balance_and_blending_change_zone_distribution_not_global_wheel() {
        let source = LinearRgb {
            r: 0.18,
            g: 0.12,
            b: 0.08,
        };
        let base = GradingParameters {
            shadows: ColorWheel {
                hue_degrees: 220.0,
                chroma: 0.8,
                lightness: 0.0,
            },
            highlights: ColorWheel {
                hue_degrees: 45.0,
                chroma: 0.8,
                lightness: 0.0,
            },
            ..Default::default()
        };
        let left = apply_grading(
            source,
            GradingParameters {
                balance: -1.0,
                blending: 0.0,
                ..base
            },
        );
        let right = apply_grading(
            source,
            GradingParameters {
                balance: 1.0,
                blending: 1.0,
                ..base
            },
        );
        assert!(distance(left, right) > 1.0e-4);
    }

    #[test]
    fn m8_skin_gray_neon_and_wide_gamut_vectors_are_stable() {
        let grading = GradingParameters {
            midtones: ColorWheel {
                hue_degrees: 32.0,
                chroma: 0.22,
                lightness: 0.04,
            },
            shadows: ColorWheel {
                hue_degrees: 235.0,
                chroma: 0.18,
                lightness: -0.03,
            },
            highlights: ColorWheel {
                hue_degrees: 55.0,
                chroma: 0.12,
                lightness: 0.02,
            },
            global: ColorWheel {
                hue_degrees: 310.0,
                chroma: 0.05,
                lightness: 0.0,
            },
            balance: 0.1,
            blending: 0.65,
            amount: 0.8,
        };
        for source in [
            LinearRgb {
                r: 0.42,
                g: 0.23,
                b: 0.14,
            },
            LinearRgb {
                r: 0.18,
                g: 0.18,
                b: 0.18,
            },
            LinearRgb {
                r: 0.9,
                g: 0.01,
                b: 1.4,
            },
            LinearRgb {
                r: -0.02,
                g: 0.7,
                b: 2.8,
            },
        ] {
            let output = apply_grading(source, grading);
            assert!(
                [output.r, output.g, output.b]
                    .into_iter()
                    .all(f32::is_finite)
            );
        }
    }
}
