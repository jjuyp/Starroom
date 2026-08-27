//! Starroom v0.2 CPU reference color engine.
//! Published color science and independent Starroom code are used here. The module is the
//! authoritative CPU reference for future wgpu shaders.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LinearRgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Oklab {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Oklch {
    pub l: f32,
    pub c: f32,
    pub h_deg: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ToneParameters {
    pub exposure_ev: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
}

impl Default for ToneParameters {
    fn default() -> Self {
        Self {
            exposure_ev: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColorBand {
    Red,
    Orange,
    Yellow,
    Green,
    #[serde(alias = "aqua")]
    Cyan,
    Blue,
    Purple,
    Magenta,
}

impl ColorBand {
    pub const ALL: [Self; 8] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Cyan,
        Self::Blue,
        Self::Purple,
        Self::Magenta,
    ];

    fn center_degrees(self) -> f32 {
        match self {
            Self::Red => 25.0,
            Self::Orange => 55.0,
            Self::Yellow => 95.0,
            Self::Green => 145.0,
            Self::Cyan => 195.0,
            Self::Blue => 250.0,
            Self::Purple => 300.0,
            Self::Magenta => 335.0,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Red => 0,
            Self::Orange => 1,
            Self::Yellow => 2,
            Self::Green => 3,
            Self::Cyan => 4,
            Self::Blue => 5,
            Self::Purple => 6,
            Self::Magenta => 7,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BandAdjustment {
    /// Relative hue rotation in degrees. UI target range: -30..30.
    pub hue_degrees: f32,
    /// Relative chroma adjustment. UI target range: -1..1.
    pub chroma: f32,
    /// Relative perceptual lightness adjustment. UI target range: -1..1.
    pub lightness: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorMixer {
    pub bands: [BandAdjustment; 8],
    /// Hue lock means chroma/lightness controls preserve the source hue exactly; only the
    /// explicitly weighted Hue control may rotate it. Enabled by default.
    #[serde(default = "default_hue_lock")]
    pub hue_lock: bool,
    /// Smooth transition half-width in degrees. The inner half has full influence and the
    /// outer half uses cubic smoothstep overlap with adjacent bands.
    #[serde(default = "default_band_width")]
    pub band_width_degrees: f32,
}

const fn default_hue_lock() -> bool {
    true
}
const fn default_band_width() -> f32 {
    52.0
}

impl Default for ColorMixer {
    fn default() -> Self {
        Self {
            bands: [BandAdjustment::default(); 8],
            hue_lock: true,
            band_width_degrees: default_band_width(),
        }
    }
}

impl ColorMixer {
    pub fn with_band(mut self, band: ColorBand, adjustment: BandAdjustment) -> Self {
        self.bands[band.index()] = adjustment;
        self
    }
}

fn clamp_unit_control(value: f32) -> f32 {
    value.clamp(-1.0, 1.0)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if (edge1 - edge0).abs() < f32::EPSILON {
        return if value < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Rec.2020/D65 relative luminance for Starroom's linear working RGB baseline.
pub fn luminance(rgb: LinearRgb) -> f32 {
    0.2627 * rgb.r + 0.6780 * rgb.g + 0.0593 * rgb.b
}

fn zone_weights(y: f32) -> (f32, f32, f32, f32) {
    let safe_y = y.max(0.0);
    // Preserve true black and fade shadow influence before the midtones. This avoids the
    // v0.1 failure where Shadows behaved like a broad white veil.
    let shadow = smoothstep(0.004, 0.012, safe_y) * (1.0 - smoothstep(0.06, 0.18, safe_y));
    let black = 1.0 - smoothstep(0.0, 0.11, safe_y);
    // The scene-linear highlight shoulder must continue into HDR values above display white.
    // It gently relaxes there, but never falls to zero as the old prototype did.
    let highlight = smoothstep(0.34, 0.62, safe_y) * (1.0 - 0.25 * smoothstep(1.10, 8.0, safe_y));
    let white = smoothstep(0.72, 1.02, safe_y);
    (shadow, black, highlight, white)
}

// GPL-derived / private-use: adapted from darktable `src/iop/sigmoid.c`,
// release-5.6.0 (commit 3c17b2976793303c186a5f64e8c9635ecf8b15d3), GPL-3.0-or-later.
// This is the numerically stable generalized-loglogistic film/paper response,
// isolated here as a typed Starroom adapter rather than importing darktable's runtime.
fn darktable_generalized_loglogistic(
    value: f32,
    magnitude: f32,
    paper_exposure: f32,
    film_fog: f32,
    film_power: f32,
    paper_power: f32,
) -> f32 {
    let film_response = (film_fog + value.max(0.0)).powf(film_power.max(1.0e-4));
    let denominator = (paper_exposure + film_response).max(1.0e-12);
    let response = magnitude * (film_response / denominator).powf(paper_power.max(1.0e-4));
    if response.is_finite() {
        response
    } else {
        magnitude
    }
}

/// A scene-linear highlight shoulder adapted from darktable's sigmoid foundation.
/// It is blended only in the declared highlight zone, so `Highlights -100` recovers
/// bright values without globally dimming shadows or midtones.
fn darktable_highlight_rolloff(y: f32, amount: f32, highlight_weight: f32) -> f32 {
    if amount >= 0.0 || highlight_weight <= 0.0 {
        return y;
    }
    let strength = (-amount).clamp(0.0, 1.0) * highlight_weight;
    // Parameters follow the neutral-gray normalized sigmoid construction. The black target
    // avoids the zero pole while the output remains scene-linear until the final gamut stage.
    let mapped = darktable_generalized_loglogistic(
        y,
        1.0,
        0.84,
        0.000_152,
        1.22 + strength * 1.45,
        1.0 + strength * 0.55,
    );
    // Preserve 18% middle gray exactly by operating on excess above the pivot.
    let pivot = 0.1845;
    let shoulder = if y > pivot {
        pivot + (mapped - pivot).max(0.0)
    } else {
        y
    };
    y + (shoulder - y) * strength
}

fn tone_luminance(y: f32, parameters: ToneParameters) -> f32 {
    if !y.is_finite() {
        return 0.0;
    }

    let mut output = y.max(0.0) * 2.0_f32.powf(parameters.exposure_ev.clamp(-5.0, 5.0));
    let (shadow_weight, black_weight, highlight_weight, white_weight) = zone_weights(output);

    let shadows = clamp_unit_control(parameters.shadows);
    if shadows >= 0.0 {
        output += shadows * shadow_weight * (0.24 + 0.18 * output.sqrt()) * (1.0 - output.min(1.0));
    } else {
        output *= 1.0 + shadows * shadow_weight * 0.72;
    }

    let highlights = clamp_unit_control(parameters.highlights);
    if highlights < 0.0 {
        output = darktable_highlight_rolloff(output, highlights, highlight_weight);
    } else {
        output += highlights * highlight_weight * (1.0 - output.min(1.0)) * 0.22;
    }

    let blacks = clamp_unit_control(parameters.blacks);
    if blacks >= 0.0 {
        output += blacks * black_weight * 0.055;
    } else {
        output *= 1.0 + blacks * black_weight * 0.82;
    }

    let whites = clamp_unit_control(parameters.whites);
    if whites >= 0.0 {
        output += whites * white_weight * (0.10 + 0.10 * output.min(1.0));
    } else {
        output *= 1.0 + whites * white_weight * 0.48;
    }

    let contrast = clamp_unit_control(parameters.contrast);
    if contrast.abs() > f32::EPSILON {
        let pivot = 0.18_f32;
        let safe = output.max(1.0e-6);
        let stops = (safe / pivot).log2();
        output = pivot * 2.0_f32.powf(stops * (1.0 + contrast * 0.62));
    }

    if output.is_finite() {
        output.max(0.0)
    } else {
        0.0
    }
}

/// Applies tone by remapping luminance and scaling RGB together. This keeps hue/chroma much
/// more stable than moving each RGB channel independently toward white or black.
pub fn apply_tone(rgb: LinearRgb, parameters: ToneParameters) -> LinearRgb {
    let source_luminance = luminance(rgb).max(0.0);
    let target_luminance = tone_luminance(source_luminance, parameters);
    if source_luminance <= 1.0e-7 {
        return LinearRgb {
            r: target_luminance,
            g: target_luminance,
            b: target_luminance,
        };
    }

    let scale = target_luminance / source_luminance;
    LinearRgb {
        r: rgb.r * scale,
        g: rgb.g * scale,
        b: rgb.b * scale,
    }
}

// OKLab XYZ conversion matrices follow Bjorn Ottosson's published reference:
// https://bottosson.github.io/posts/oklab/ (public domain / MIT).
fn rec2020_to_xyz(rgb: LinearRgb) -> (f32, f32, f32) {
    (
        0.636_958_06 * rgb.r + 0.144_616_9 * rgb.g + 0.168_880_98 * rgb.b,
        0.262_700_2 * rgb.r + 0.677_998_07 * rgb.g + 0.059_301_72 * rgb.b,
        0.028_072_693 * rgb.g + 1.060_985_1 * rgb.b,
    )
}

fn xyz_to_rec2020(x: f32, y: f32, z: f32) -> LinearRgb {
    LinearRgb {
        r: 1.716_651_2 * x - 0.355_670_78 * y - 0.253_366_3 * z,
        g: -0.666_684_3 * x + 1.616_481_2 * y + 0.015_768_546 * z,
        b: 0.017_639_857 * x - 0.042_770_613 * y + 0.942_103_1 * z,
    }
}

pub fn rec2020_to_oklab(rgb: LinearRgb) -> Oklab {
    let (x, y, z) = rec2020_to_xyz(rgb);
    let l = (0.818_933 * x + 0.361_866_74 * y - 0.128_859_71 * z).cbrt();
    let m = (0.032_984_544 * x + 0.929_311_9 * y + 0.036_145_64 * z).cbrt();
    let s = (0.048_200_3 * x + 0.264_366_27 * y + 0.633_851_7 * z).cbrt();
    Oklab {
        l: 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        a: 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        b: 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    }
}

pub fn oklab_to_rec2020(lab: Oklab) -> LinearRgb {
    let l_prime = lab.l + 0.396_337_78 * lab.a + 0.215_803_76 * lab.b;
    let m_prime = lab.l - 0.105_561_346 * lab.a - 0.063_854_17 * lab.b;
    let s_prime = lab.l - 0.089_484_18 * lab.a - 1.291_485_5 * lab.b;
    let l = l_prime * l_prime * l_prime;
    let m = m_prime * m_prime * m_prime;
    let s = s_prime * s_prime * s_prime;
    let x = 1.227_014 * l - 0.557_8 * m + 0.281_256_14 * s;
    let y = -0.040_580_18 * l + 1.112_256_9 * m - 0.071_676_68 * s;
    let z = -0.076_381_29 * l - 0.421_481_97 * m + 1.586_163_2 * s;
    xyz_to_rec2020(x, y, z)
}

pub fn oklab_to_oklch(lab: Oklab) -> Oklch {
    let chroma = (lab.a * lab.a + lab.b * lab.b).sqrt();
    let mut hue = lab.b.atan2(lab.a).to_degrees();
    if hue < 0.0 {
        hue += 360.0;
    }
    Oklch {
        l: lab.l,
        c: chroma,
        h_deg: hue,
    }
}

pub fn oklch_to_oklab(lch: Oklch) -> Oklab {
    let angle = lch.h_deg.to_radians();
    Oklab {
        l: lch.l,
        a: lch.c * angle.cos(),
        b: lch.c * angle.sin(),
    }
}

pub fn rotate_hue(rgb: LinearRgb, degrees: f32) -> LinearRgb {
    let mut lch = oklab_to_oklch(rec2020_to_oklab(rgb));
    lch.h_deg = (lch.h_deg + degrees).rem_euclid(360.0);
    oklab_to_rec2020(oklch_to_oklab(lch))
}

fn circular_distance_degrees(a: f32, b: f32) -> f32 {
    let difference = (a - b).rem_euclid(360.0).abs();
    difference.min(360.0 - difference)
}

fn color_band_weight(hue: f32, band: ColorBand, width: f32) -> f32 {
    let distance = circular_distance_degrees(hue, band.center_degrees());
    let outer = width.clamp(30.0, 80.0);
    1.0 - smoothstep(outer * 0.42, outer, distance)
}

/// Picks the strongest of the eight circular bands. Achromatic samples are rejected because
/// assigning an arbitrary hue to neutral grey would make the targeted tool unstable.
pub fn sample_color_band(rgb: LinearRgb) -> Option<ColorBand> {
    let lch = oklab_to_oklch(rec2020_to_oklab(rgb));
    if !lch.l.is_finite() || !lch.c.is_finite() || lch.c < 1.0e-4 {
        return None;
    }
    ColorBand::ALL.into_iter().max_by(|left, right| {
        color_band_weight(lch.h_deg, *left, default_band_width()).total_cmp(&color_band_weight(
            lch.h_deg,
            *right,
            default_band_width(),
        ))
    })
}

/// Eight-band selective color editing in OKLCh. Hue adjustment keeps L and C fixed before
/// gamut mapping; chroma and lightness are explicit independent controls.
pub fn apply_color_mixer(rgb: LinearRgb, mixer: ColorMixer) -> LinearRgb {
    if ![rgb.r, rgb.g, rgb.b].into_iter().all(f32::is_finite) {
        return LinearRgb {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        };
    }
    let mut lch = oklab_to_oklch(rec2020_to_oklab(rgb));
    if lch.c < 1.0e-4 {
        return rgb;
    }

    let original_hue = lch.h_deg;
    let mut weight_total = 0.0;
    let mut hue_delta = 0.0;
    let mut chroma_delta = 0.0;
    let mut lightness_delta = 0.0;

    for band in ColorBand::ALL {
        let weight = color_band_weight(original_hue, band, mixer.band_width_degrees);
        if weight <= 0.0 {
            continue;
        }
        let adjustment = mixer.bands[band.index()];
        weight_total += weight;
        hue_delta += adjustment.hue_degrees.clamp(-30.0, 30.0) * weight;
        chroma_delta += clamp_unit_control(adjustment.chroma) * weight;
        lightness_delta += clamp_unit_control(adjustment.lightness) * weight;
    }

    if weight_total > 1.0e-7 {
        let inverse = 1.0 / weight_total;
        let explicit_hue_delta = hue_delta * inverse;
        lch.h_deg = (original_hue + explicit_hue_delta).rem_euclid(360.0);
        lch.c *= 1.0 + chroma_delta * inverse * 0.75;
        lch.c = lch.c.max(0.0);
        lch.l += lightness_delta * inverse * 0.16;
    }

    let output = oklab_to_rec2020(oklch_to_oklab(lch));
    if [output.r, output.g, output.b]
        .into_iter()
        .all(f32::is_finite)
    {
        output
    } else {
        rgb
    }
}

/// Smoothly reduces OKLCh chroma until RGB fits a normalized display/output gamut. Starroom's
/// internal creative pipeline remains unbounded; call this only at a bounded output boundary.
pub fn compress_to_unit_gamut(rgb: LinearRgb) -> LinearRgb {
    if [rgb.r, rgb.g, rgb.b]
        .into_iter()
        .all(|channel| (0.0..=1.0).contains(&channel))
    {
        return rgb;
    }

    let mut lch = oklab_to_oklch(rec2020_to_oklab(rgb));
    let original_chroma = lch.c;
    let mut low = 0.0;
    let mut high = original_chroma;
    let mut best = LinearRgb {
        r: lch.l,
        g: lch.l,
        b: lch.l,
    };

    for _ in 0..14 {
        lch.c = (low + high) * 0.5;
        let candidate = oklab_to_rec2020(oklch_to_oklab(lch));
        let in_gamut = [candidate.r, candidate.g, candidate.b]
            .into_iter()
            .all(|channel| (0.0..=1.0).contains(&channel));
        if in_gamut {
            best = candidate;
            low = lch.c;
        } else {
            high = lch.c;
        }
    }

    best
}

/// Monotone cubic Hermite curve mapping. For monotone control points this avoids spline
/// overshoot and the harsh piecewise-linear bends from the v0.1 browser prototype.
pub fn map_monotone_curve(value: f32, points: &[CurvePoint]) -> f32 {
    let mut points: Vec<CurvePoint> = points
        .iter()
        .copied()
        .filter(|point| point.x.is_finite() && point.y.is_finite())
        .collect();
    points.sort_by(|left, right| left.x.total_cmp(&right.x));
    points.dedup_by(|left, right| (left.x - right.x).abs() < 1.0e-6);

    if points.len() < 2 {
        return value;
    }

    let segment_count = points.len() - 1;
    let mut slopes = vec![0.0_f32; segment_count];
    for index in 0..segment_count {
        let dx = (points[index + 1].x - points[index].x).max(1.0e-6);
        slopes[index] = (points[index + 1].y - points[index].y) / dx;
    }

    let mut tangents = vec![0.0_f32; points.len()];
    tangents[0] = slopes[0];
    tangents[points.len() - 1] = slopes[segment_count - 1];
    for index in 1..points.len() - 1 {
        let left = slopes[index - 1];
        let right = slopes[index];
        tangents[index] = if left * right <= 0.0 {
            0.0
        } else {
            2.0 * left * right / (left + right)
        };
    }

    // Preserve scene-linear HDR values by extrapolating endpoint tangents rather than
    // clamping to 0..1 before the output transform.
    if value <= points[0].x {
        return points[0].y + (value - points[0].x) * tangents[0];
    }
    if value >= points[points.len() - 1].x {
        let last = points.len() - 1;
        return points[last].y + (value - points[last].x) * tangents[last];
    }

    for index in 0..segment_count {
        let left = points[index];
        let right = points[index + 1];
        if value > right.x {
            continue;
        }

        let width = (right.x - left.x).max(1.0e-6);
        let t = ((value - left.x) / width).clamp(0.0, 1.0);
        let t2 = t * t;
        let t3 = t2 * t;
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        return h00 * left.y
            + h10 * width * tangents[index]
            + h01 * right.y
            + h11 * width * tangents[index + 1];
    }

    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(a: f32, b: f32) -> f32 {
        (a - b).abs()
    }

    #[test]
    fn neutral_tone_is_identity() {
        let rgb = LinearRgb {
            r: 0.21,
            g: 0.13,
            b: 0.07,
        };
        let output = apply_tone(rgb, ToneParameters::default());
        assert!(delta(output.r, rgb.r) < 1e-6);
        assert!(delta(output.g, rgb.g) < 1e-6);
        assert!(delta(output.b, rgb.b) < 1e-6);
    }

    #[test]
    fn positive_shadows_lift_dark_region_without_washing_midtones() {
        let parameters = ToneParameters {
            shadows: 0.5,
            ..Default::default()
        };
        let dark = LinearRgb {
            r: 0.018,
            g: 0.014,
            b: 0.010,
        };
        let mid = LinearRgb {
            r: 0.23,
            g: 0.19,
            b: 0.16,
        };
        let dark_gain = luminance(apply_tone(dark, parameters)) - luminance(dark);
        let mid_gain = luminance(apply_tone(mid, parameters)) - luminance(mid);
        assert!(dark_gain > 0.0);
        assert!(dark_gain > mid_gain * 2.0);
    }

    #[test]
    fn shadow_lift_keeps_black_anchor() {
        let parameters = ToneParameters {
            shadows: 1.0,
            ..Default::default()
        };
        let black = LinearRgb {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        };
        let output = apply_tone(black, parameters);
        assert!(output.r.abs() < 1e-6 && output.g.abs() < 1e-6 && output.b.abs() < 1e-6);
    }

    #[test]
    fn highlights_recover_only_bright_region_without_global_dimming() {
        let parameters = ToneParameters {
            highlights: -1.0,
            ..Default::default()
        };
        let shadow = LinearRgb {
            r: 0.03,
            g: 0.03,
            b: 0.03,
        };
        let mid = LinearRgb {
            r: 0.18,
            g: 0.18,
            b: 0.18,
        };
        let bright = LinearRgb {
            r: 1.8,
            g: 1.5,
            b: 1.2,
        };
        assert!(delta(luminance(apply_tone(shadow, parameters)), luminance(shadow)) < 1.0e-5);
        assert!(delta(luminance(apply_tone(mid, parameters)), luminance(mid)) < 1.0e-4);
        assert!(luminance(apply_tone(bright, parameters)) < luminance(bright));
    }

    #[test]
    fn tone_extremes_remain_finite_and_scene_linear() {
        let source = LinearRgb {
            r: 8.0,
            g: 2.0,
            b: 0.4,
        };
        for control in [-1.0, 1.0] {
            let output = apply_tone(
                source,
                ToneParameters {
                    exposure_ev: control * 5.0,
                    contrast: control,
                    highlights: control,
                    shadows: control,
                    whites: control,
                    blacks: control,
                },
            );
            assert!(output.r.is_finite() && output.g.is_finite() && output.b.is_finite());
        }
        let hdr = apply_tone(
            source,
            ToneParameters {
                exposure_ev: 5.0,
                ..Default::default()
            },
        );
        assert!(hdr.r > 1.0 && hdr.g > 1.0 && hdr.b > 1.0);
    }

    #[test]
    fn whites_and_blacks_have_narrower_target_than_shadow_and_highlight_controls() {
        let low = LinearRgb {
            r: 0.035,
            g: 0.035,
            b: 0.035,
        };
        let lower_mid = LinearRgb {
            r: 0.12,
            g: 0.12,
            b: 0.12,
        };
        let high = LinearRgb {
            r: 0.82,
            g: 0.82,
            b: 0.82,
        };
        let upper_mid = LinearRgb {
            r: 0.48,
            g: 0.48,
            b: 0.48,
        };
        let black_low = (luminance(apply_tone(
            low,
            ToneParameters {
                blacks: 1.0,
                ..Default::default()
            },
        )) - luminance(low))
        .abs();
        let black_mid = (luminance(apply_tone(
            lower_mid,
            ToneParameters {
                blacks: 1.0,
                ..Default::default()
            },
        )) - luminance(lower_mid))
        .abs();
        let white_high = (luminance(apply_tone(
            high,
            ToneParameters {
                whites: 1.0,
                ..Default::default()
            },
        )) - luminance(high))
        .abs();
        let white_mid = (luminance(apply_tone(
            upper_mid,
            ToneParameters {
                whites: 1.0,
                ..Default::default()
            },
        )) - luminance(upper_mid))
        .abs();
        assert!(black_low > black_mid * 2.0);
        assert!(white_high > white_mid * 2.0);
    }

    #[test]
    fn hue_rotation_preserves_oklch_lightness_and_chroma_before_gamut_mapping() {
        let rgb = LinearRgb {
            r: 0.30,
            g: 0.16,
            b: 0.08,
        };
        let before = oklab_to_oklch(rec2020_to_oklab(rgb));
        let after = oklab_to_oklch(rec2020_to_oklab(rotate_hue(rgb, 42.0)));
        assert!(delta(before.l, after.l) < 2e-4);
        assert!(delta(before.c, after.c) < 2e-4);
    }

    #[test]
    fn rec2020_oklab_round_trip_preserves_linear_rgb() {
        let samples = [
            LinearRgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
            },
            LinearRgb {
                r: 1.0,
                g: 1.0,
                b: 1.0,
            },
            LinearRgb {
                r: 0.30,
                g: 0.16,
                b: 0.08,
            },
            LinearRgb {
                r: 0.02,
                g: 0.42,
                b: 0.91,
            },
        ];

        for source in samples {
            let restored = oklab_to_rec2020(rec2020_to_oklab(source));
            assert!(restored.r.is_finite() && restored.g.is_finite() && restored.b.is_finite());
            assert!(delta(source.r, restored.r) < 2e-5);
            assert!(delta(source.g, restored.g) < 2e-5);
            assert!(delta(source.b, restored.b) < 2e-5);
        }
    }

    #[test]
    fn monotone_curve_is_smooth_and_bounded_for_monotone_points() {
        let points = [
            CurvePoint { x: 0.0, y: 0.0 },
            CurvePoint { x: 0.25, y: 0.18 },
            CurvePoint { x: 0.50, y: 0.58 },
            CurvePoint { x: 1.0, y: 1.0 },
        ];
        let mut previous = map_monotone_curve(0.0, &points);
        for sample in 1..=100 {
            let value = sample as f32 / 100.0;
            let output = map_monotone_curve(value, &points);
            assert!(output >= previous - 1.0e-5);
            assert!((0.0..=1.0).contains(&output));
            previous = output;
        }
    }

    #[test]
    fn color_mixer_changes_selected_hue_without_nan() {
        let mixer = ColorMixer::default().with_band(
            ColorBand::Red,
            BandAdjustment {
                hue_degrees: 15.0,
                chroma: 0.20,
                lightness: 0.05,
            },
        );
        let source = LinearRgb {
            r: 0.35,
            g: 0.08,
            b: 0.05,
        };
        let output = apply_color_mixer(source, mixer);
        assert!(output.r.is_finite() && output.g.is_finite() && output.b.is_finite());
        assert!(
            delta(source.r, output.r) + delta(source.g, output.g) + delta(source.b, output.b)
                > 1.0e-4
        );
    }

    #[test]
    fn m7_hue_lock_preserves_hue_for_chroma_and_lightness_only_edits() {
        let source = LinearRgb {
            r: 0.62,
            g: 0.22,
            b: 0.08,
        };
        let before = oklab_to_oklch(rec2020_to_oklab(source));
        let band = sample_color_band(source).expect("chromatic source");
        let output = apply_color_mixer(
            source,
            ColorMixer::default().with_band(
                band,
                BandAdjustment {
                    hue_degrees: 0.0,
                    chroma: 0.7,
                    lightness: -0.35,
                },
            ),
        );
        let after = oklab_to_oklch(rec2020_to_oklab(output));
        assert!(circular_distance_degrees(before.h_deg, after.h_deg) < 0.02);
        assert!(after.c > before.c);
        assert!(after.l < before.l);
    }

    #[test]
    fn m7_circular_red_wrap_has_smooth_overlap() {
        let near_zero = oklab_to_rec2020(oklch_to_oklab(Oklch {
            l: 0.65,
            c: 0.16,
            h_deg: 359.5,
        }));
        let near_wrap = oklab_to_rec2020(oklch_to_oklab(Oklch {
            l: 0.65,
            c: 0.16,
            h_deg: 0.5,
        }));
        let mixer = ColorMixer::default().with_band(
            ColorBand::Red,
            BandAdjustment {
                hue_degrees: 12.0,
                ..Default::default()
            },
        );
        let left = oklab_to_oklch(rec2020_to_oklab(apply_color_mixer(near_zero, mixer)));
        let right = oklab_to_oklch(rec2020_to_oklab(apply_color_mixer(near_wrap, mixer)));
        assert!(circular_distance_degrees(left.h_deg, right.h_deg) < 1.5);
    }

    #[test]
    fn m7_achromatic_and_hdr_extremes_are_stable_and_finite() {
        let mixer = ColorMixer::default().with_band(
            ColorBand::Blue,
            BandAdjustment {
                hue_degrees: 30.0,
                chroma: 1.0,
                lightness: 1.0,
            },
        );
        let gray = LinearRgb {
            r: 0.18,
            g: 0.18,
            b: 0.18,
        };
        assert_eq!(apply_color_mixer(gray, mixer), gray);
        assert_eq!(sample_color_band(gray), None);
        for source in [
            LinearRgb {
                r: 4.0,
                g: 0.02,
                b: 1.7,
            },
            LinearRgb {
                r: -0.05,
                g: 0.4,
                b: 2.0,
            },
        ] {
            let output = apply_color_mixer(source, mixer);
            assert!(
                [output.r, output.g, output.b]
                    .into_iter()
                    .all(f32::is_finite)
            );
        }
    }
}
