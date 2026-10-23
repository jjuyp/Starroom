//! Native perceptual/statistical reference matching for M22.
//! Analysis produces existing Starroom adjustment semantics; it is not a second renderer.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use starroom_color::{
    BandAdjustment, ColorMixer, CurvePoint, LinearRgb, ToneParameters, oklab_to_oklch,
    rec2020_to_oklab,
};
use starroom_detail::LinearImage;
use starroom_grading::{ColorWheel, GradingParameters};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ReferenceError {
    #[error("reference image is empty or malformed")]
    InvalidImage,
    #[error("reference analysis produced non-finite statistics")]
    NonFinite,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RelativeWhiteBalance {
    pub temperature: f32,
    pub tint: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HueBandStatistic {
    pub weight: f32,
    pub mean_lightness: f32,
    pub mean_chroma: f32,
    pub mean_hue_degrees: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceAnalysis {
    pub fingerprint: String,
    pub luminance_quantiles: [f32; 7],
    pub oklab_mean: [f32; 3],
    pub oklab_covariance: [[f32; 3]; 3],
    pub hue_bands: [HueBandStatistic; 8],
    pub shadow_mean: [f32; 3],
    pub midtone_mean: [f32; 3],
    pub highlight_mean: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceMatchRecipe {
    pub tone: ToneParameters,
    pub curve: Vec<CurvePoint>,
    pub white_balance: RelativeWhiteBalance,
    pub color_mixer: ColorMixer,
    pub grading: GradingParameters,
    pub protect_skin: f32,
    pub confidence: f32,
    pub source_fingerprint: String,
    pub reference_fingerprint: String,
}

fn percentile(sorted: &[f32], q: f32) -> f32 {
    sorted[((sorted.len() - 1) as f32 * q).round() as usize]
}
fn circular_delta(target: f32, source: f32) -> f32 {
    (target - source + 180.0).rem_euclid(360.0) - 180.0
}
fn band_index(h: f32) -> usize {
    let centers = [25.0, 55.0, 95.0, 145.0, 195.0, 250.0, 300.0, 335.0];
    centers
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            circular_delta(**a, h)
                .abs()
                .total_cmp(&circular_delta(**b, h).abs())
        })
        .unwrap()
        .0
}

pub fn analyze(image: &LinearImage) -> Result<ReferenceAnalysis, ReferenceError> {
    if image.width == 0 || image.height == 0 || image.data.len() != image.width * image.height * 3 {
        return Err(ReferenceError::InvalidImage);
    }
    let mut samples = Vec::with_capacity(image.width * image.height);
    let mut lumas = Vec::with_capacity(samples.capacity());
    let mut hash = Sha256::new();
    for p in image.data.as_chunks::<3>().0 {
        for v in p {
            hash.update(v.to_le_bytes());
        }
        let lab = rec2020_to_oklab(LinearRgb {
            r: p[0],
            g: p[1],
            b: p[2],
        });
        if ![lab.l, lab.a, lab.b].iter().all(|v| v.is_finite()) {
            return Err(ReferenceError::NonFinite);
        }
        samples.push([lab.l, lab.a, lab.b]);
        lumas.push(lab.l.max(0.0));
    }
    lumas.sort_by(f32::total_cmp);
    let qs = [0.01, 0.05, 0.25, 0.50, 0.75, 0.95, 0.99].map(|q| percentile(&lumas, q));
    let n = samples.len() as f32;
    let mut mean = [0.0; 3];
    for s in &samples {
        for c in 0..3 {
            mean[c] += s[c] / n;
        }
    }
    let mut cov = [[0.0; 3]; 3];
    for s in &samples {
        for a in 0..3 {
            for b in 0..3 {
                cov[a][b] += (s[a] - mean[a]) * (s[b] - mean[b]) / n.max(1.0);
            }
        }
    }
    let mut sums = [[0.0; 4]; 8];
    let mut zones = [([0.0_f32; 3], 0.0_f32); 3];
    for s in &samples {
        let lch = oklab_to_oklch(starroom_color::Oklab {
            l: s[0],
            a: s[1],
            b: s[2],
        });
        if lch.c > 0.01 {
            let i = band_index(lch.h_deg);
            sums[i][0] += 1.0;
            sums[i][1] += lch.l;
            sums[i][2] += lch.c;
            sums[i][3] += lch.h_deg.to_radians();
        }
        let z = if s[0] < qs[2] {
            0
        } else if s[0] > qs[4] {
            2
        } else {
            1
        };
        for (channel, sample) in zones[z].0.iter_mut().zip(s.iter()) {
            *channel += *sample;
        }
        zones[z].1 += 1.0;
    }
    let hue_bands = std::array::from_fn(|i| {
        let n = sums[i][0].max(1.0);
        HueBandStatistic {
            weight: sums[i][0] / samples.len() as f32,
            mean_lightness: sums[i][1] / n,
            mean_chroma: sums[i][2] / n,
            mean_hue_degrees: (sums[i][3] / n).to_degrees().rem_euclid(360.0),
        }
    });
    let zone = |i: usize| {
        let n = zones[i].1.max(1.0);
        zones[i].0.map(|v| v / n)
    };
    Ok(ReferenceAnalysis {
        fingerprint: format!("{:x}", hash.finalize()),
        luminance_quantiles: qs,
        oklab_mean: mean,
        oklab_covariance: cov,
        hue_bands,
        shadow_mean: zone(0),
        midtone_mean: zone(1),
        highlight_mean: zone(2),
    })
}

pub fn match_reference(
    source: &ReferenceAnalysis,
    reference: &ReferenceAnalysis,
    protect_skin: f32,
) -> Result<ReferenceMatchRecipe, ReferenceError> {
    if source
        .luminance_quantiles
        .iter()
        .chain(reference.luminance_quantiles.iter())
        .any(|v| !v.is_finite())
    {
        return Err(ReferenceError::NonFinite);
    }
    let s = &source.luminance_quantiles;
    let r = &reference.luminance_quantiles;
    let exposure_ev = ((r[3] + 1e-4) / (s[3] + 1e-4)).log2().clamp(-4.0, 4.0);
    let source_span = (s[5] - s[1]).max(1e-4);
    let reference_span = (r[5] - r[1]).max(1e-4);
    let contrast = ((reference_span / source_span - 1.0) * 0.65).clamp(-1.0, 1.0);
    let tone = ToneParameters {
        exposure_ev,
        contrast,
        shadows: ((r[1] - s[1]) / source_span).clamp(-1.0, 1.0),
        highlights: ((r[5] - s[5]) / source_span).clamp(-1.0, 1.0),
        blacks: ((r[0] - s[0]) / source_span * 0.5).clamp(-1.0, 1.0),
        whites: ((r[6] - s[6]) / source_span * 0.5).clamp(-1.0, 1.0),
    };
    let mut curve = vec![CurvePoint {
        x: 0.0,
        y: r[0].clamp(0.0, 1.0),
    }];
    for i in 0..7 {
        let point = CurvePoint {
            x: s[i].clamp(0.0, 1.0),
            y: r[i].clamp(0.0, 1.0).max(curve.last().unwrap().y),
        };
        if point.x > curve.last().unwrap().x + 1e-4 && point.x < 1.0 - 1e-4 {
            curve.push(point);
        }
    }
    curve.push(CurvePoint {
        x: 1.0,
        y: r[6].clamp(0.0, 1.0).max(curve.last().unwrap().y),
    });
    let protection = protect_skin.clamp(0.0, 1.0);
    let color_scale = 1.0 - 0.65 * protection;
    let white_balance = RelativeWhiteBalance {
        temperature: ((reference.oklab_mean[2] - source.oklab_mean[2]) * 3.0 * color_scale)
            .clamp(-1.0, 1.0),
        tint: ((reference.oklab_mean[1] - source.oklab_mean[1]) * 3.0 * color_scale)
            .clamp(-1.0, 1.0),
    };
    let mut color_mixer = ColorMixer::default();
    for i in 0..8 {
        let a = source.hue_bands[i];
        let b = reference.hue_bands[i];
        color_mixer.bands[i] = BandAdjustment {
            hue_degrees: (circular_delta(b.mean_hue_degrees, a.mean_hue_degrees) * color_scale)
                .clamp(-30.0, 30.0),
            chroma: ((b.mean_chroma - a.mean_chroma) * 4.0 * color_scale).clamp(-1.0, 1.0),
            lightness: ((b.mean_lightness - a.mean_lightness) * 2.0).clamp(-1.0, 1.0),
        };
    }
    let wheel = |a: [f32; 3], b: [f32; 3]| {
        let da = (b[1] - a[1]) * color_scale;
        let db = (b[2] - a[2]) * color_scale;
        ColorWheel {
            hue_degrees: db.atan2(da).to_degrees().rem_euclid(360.0),
            chroma: (da * da + db * db).sqrt().mul_add(5.0, 0.0).clamp(0.0, 1.0),
            lightness: ((b[0] - a[0]) * 2.0).clamp(-1.0, 1.0),
        }
    };
    let grading = GradingParameters {
        shadows: wheel(source.shadow_mean, reference.shadow_mean),
        midtones: wheel(source.midtone_mean, reference.midtone_mean),
        highlights: wheel(source.highlight_mean, reference.highlight_mean),
        global: wheel(source.oklab_mean, reference.oklab_mean),
        balance: 0.0,
        blending: 0.5,
        amount: 0.55,
    };
    let confidence = (reference_span * 4.0).clamp(0.0, 1.0)
        * (1.0
            - (source.oklab_mean[0] - reference.oklab_mean[0])
                .abs()
                .min(1.0)
                * 0.2);
    Ok(ReferenceMatchRecipe {
        tone,
        curve,
        white_balance,
        color_mixer,
        grading,
        protect_skin: protection,
        confidence,
        source_fingerprint: source.fingerprint.clone(),
        reference_fingerprint: reference.fingerprint.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gradient(tint: [f32; 3]) -> LinearImage {
        LinearImage::new(
            64,
            32,
            (0..2048)
                .flat_map(|i| {
                    let v = (i % 64) as f32 / 63.0;
                    [v * tint[0], v * tint[1], v * tint[2]]
                })
                .collect(),
        )
        .unwrap()
    }
    #[test]
    fn analysis_is_deterministic_and_finite() {
        let a = analyze(&gradient([1.0, 0.8, 0.6])).unwrap();
        assert_eq!(a, analyze(&gradient([1.0, 0.8, 0.6])).unwrap());
        assert!(a.luminance_quantiles.windows(2).all(|w| w[0] <= w[1]));
    }
    #[test]
    fn identity_match_is_neutral_and_monotonic() {
        let a = analyze(&gradient([1.0, 1.0, 1.0])).unwrap();
        let m = match_reference(&a, &a, 0.0).unwrap();
        assert!(m.tone.exposure_ev.abs() < 1e-6);
        assert!(m.tone.contrast.abs() < 1e-6);
        assert!(m.tone.highlights.abs() < 1e-6);
        assert!(m.tone.shadows.abs() < 1e-6);
        assert!(m.tone.whites.abs() < 1e-6);
        assert!(m.tone.blacks.abs() < 1e-6);
        assert_eq!(m.white_balance, RelativeWhiteBalance::default());
        assert!(
            m.color_mixer
                .bands
                .iter()
                .all(|band| *band == BandAdjustment::default())
        );
        assert_eq!(m.grading.shadows.chroma, 0.0);
        assert_eq!(m.grading.midtones.chroma, 0.0);
        assert_eq!(m.grading.highlights.chroma, 0.0);
        assert_eq!(m.grading.global.chroma, 0.0);
        assert!(m.curve.windows(2).all(|w| w[0].y <= w[1].y));
    }
    #[test]
    fn exposure_color_and_skin_protection_are_bounded() {
        let a = analyze(&gradient([0.4, 0.5, 0.7])).unwrap();
        let b = analyze(&gradient([1.2, 0.7, 0.4])).unwrap();
        let loose = match_reference(&a, &b, 0.0).unwrap();
        let safe = match_reference(&a, &b, 1.0).unwrap();
        assert!(loose.tone.exposure_ev.is_finite());
        assert!(
            safe.white_balance.temperature.abs() <= loose.white_balance.temperature.abs() + 1e-6
        );
        assert!(
            safe.color_mixer
                .bands
                .iter()
                .all(|x| x.hue_degrees.abs() <= 30.0)
        );
    }

    #[test]
    fn portrait_landscape_key_contrast_neon_and_monochrome_matches_remain_finite() {
        let cases = [
            ([0.72, 0.48, 0.36], [0.55, 0.72, 1.1]),
            ([0.35, 0.75, 0.28], [1.0, 0.35, 0.75]),
            ([0.12, 0.12, 0.12], [1.4, 1.4, 1.4]),
            ([1.0, 0.05, 1.4], [0.05, 1.2, 1.3]),
            ([0.6, 0.6, 0.6], [0.9, 0.9, 0.9]),
        ];
        for (source_tint, reference_tint) in cases {
            let source = analyze(&gradient(source_tint)).unwrap();
            let reference = analyze(&gradient(reference_tint)).unwrap();
            let recipe = match_reference(&source, &reference, 0.8).unwrap();
            assert!(recipe.tone.exposure_ev.is_finite());
            assert!(
                recipe
                    .curve
                    .windows(2)
                    .all(|points| points[0].y <= points[1].y)
            );
            assert!(recipe.color_mixer.bands.iter().all(|band| {
                band.hue_degrees.is_finite()
                    && band.chroma.is_finite()
                    && band.lightness.is_finite()
            }));
        }
    }

    #[test]
    fn extreme_reference_is_bounded_without_nan_or_inf() {
        let source = analyze(&gradient([0.001, 0.002, 0.001])).unwrap();
        let reference = analyze(&gradient([12.0, 0.2, 5.0])).unwrap();
        let recipe = match_reference(&source, &reference, 0.0).unwrap();
        assert!((-4.0..=4.0).contains(&recipe.tone.exposure_ev));
        assert!(
            recipe
                .curve
                .iter()
                .all(|point| point.x.is_finite() && point.y.is_finite())
        );
        assert!((0.0..=1.0).contains(&recipe.confidence));
    }
}
