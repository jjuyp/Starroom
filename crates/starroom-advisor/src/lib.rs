//! Explainable, local-first editing suggestions for Starroom v0.2.
//! This crate contains deterministic statistics/rules only. No network or generative AI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct AnalysisStats {
    pub shadow_fraction: f32,
    pub highlight_fraction: f32,
    pub black_clip_fraction: f32,
    pub white_clip_fraction: f32,
    pub median_luminance: f32,
    pub estimated_warmth_bias: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    pub id: String,
    pub control: String,
    pub value: f32,
    pub confidence: f32,
    pub reason: String,
}

/// M19 expands the legacy compact statistics with auditable percentiles and colour evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DetailedAnalysis {
    pub luminance_mean: f32,
    pub luminance_median: f32,
    pub p01: f32,
    pub p05: f32,
    pub p25: f32,
    pub p50: f32,
    pub p75: f32,
    pub p95: f32,
    pub p99: f32,
    pub black_clip_fraction: f32,
    pub white_clip_fraction: f32,
    pub global_contrast: f32,
    pub mean_chroma: f32,
    pub high_chroma_fraction: f32,
    pub warmth_bias: f32,
    pub green_magenta_bias: f32,
    pub portrait_luminance_mean: f32,
    pub portrait_chroma_mean: f32,
    pub portrait_sample_fraction: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AdvisorCategory {
    Light,
    WhiteBalance,
    Color,
    Detail,
    Portrait,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfidenceLabel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisorSuggestion {
    pub id: String,
    pub category: AdvisorCategory,
    /// Existing native parameter key, never a second render graph setting.
    pub control: String,
    pub amount: f32,
    pub what: String,
    pub why: String,
    pub confidence: ConfidenceLabel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AdvisorResult {
    pub analysis: DetailedAnalysis,
    pub suggestions: Vec<AdvisorSuggestion>,
}

fn percentile(values: &[f32], fraction: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let position =
        (fraction.clamp(0.0, 1.0) * (values.len().saturating_sub(1)) as f32).round() as usize;
    values[position]
}

/// Deterministic image analysis over native working-space samples. It intentionally reports
/// descriptive evidence only—there is no learned probability or cloud inference.
pub fn analyze_detailed(samples: &[[f32; 3]]) -> DetailedAnalysis {
    if samples.is_empty() {
        return DetailedAnalysis::default();
    }
    let mut luma = Vec::with_capacity(samples.len());
    let mut chroma_sum = 0.0;
    let mut high_chroma = 0usize;
    let mut warmth = 0.0;
    let mut green = 0.0;
    let mut black = 0usize;
    let mut white = 0usize;
    for [r, g, b] in samples.iter().copied() {
        let (r, g, b) = (r.max(0.0), g.max(0.0), b.max(0.0));
        let y = 0.2627 * r + 0.6780 * g + 0.0593 * b;
        luma.push(y);
        let chroma = ((r - g).powi(2) + (g - b).powi(2) + (b - r).powi(2)).sqrt();
        chroma_sum += chroma;
        if chroma > 0.22 {
            high_chroma += 1;
        }
        warmth += (r - b) / (r + b).max(1.0e-5);
        green += (g - (r + b) * 0.5) / (r + g + b).max(1.0e-5);
        if y <= 0.003 {
            black += 1;
        }
        if y >= 0.985 || r >= 0.995 || g >= 0.995 || b >= 0.995 {
            white += 1;
        }
    }
    luma.sort_by(|left, right| left.total_cmp(right));
    let count = samples.len() as f32;
    DetailedAnalysis {
        luminance_mean: luma.iter().sum::<f32>() / count,
        luminance_median: percentile(&luma, 0.5),
        p01: percentile(&luma, 0.01),
        p05: percentile(&luma, 0.05),
        p25: percentile(&luma, 0.25),
        p50: percentile(&luma, 0.5),
        p75: percentile(&luma, 0.75),
        p95: percentile(&luma, 0.95),
        p99: percentile(&luma, 0.99),
        black_clip_fraction: black as f32 / count,
        white_clip_fraction: white as f32 / count,
        global_contrast: percentile(&luma, 0.95) - percentile(&luma, 0.05),
        mean_chroma: chroma_sum / count,
        high_chroma_fraction: high_chroma as f32 / count,
        warmth_bias: warmth / count,
        green_magenta_bias: green / count,
        portrait_luminance_mean: 0.0,
        portrait_chroma_mean: 0.0,
        portrait_sample_fraction: 0.0,
    }
}

pub fn advise_detailed(analysis: DetailedAnalysis) -> Vec<AdvisorSuggestion> {
    let mut output = Vec::new();
    let push = |output: &mut Vec<AdvisorSuggestion>,
                id: &str,
                category,
                control: &str,
                amount,
                what: &str,
                why: String,
                confidence| {
        output.push(AdvisorSuggestion {
            id: id.into(),
            category,
            control: control.into(),
            amount,
            what: what.into(),
            why,
            confidence,
        })
    };
    if analysis.p50 < 0.14 && analysis.white_clip_fraction < 0.01 {
        push(
            &mut output,
            "m19-open-exposure",
            AdvisorCategory::Light,
            "exposure",
            0.25,
            "Open exposure",
            format!(
                "Median luminance is {:.3} with {:.1}% white clipping.",
                analysis.p50,
                analysis.white_clip_fraction * 100.0
            ),
            ConfidenceLabel::High,
        );
    }
    if analysis.black_clip_fraction > 0.025 && analysis.p25 < 0.08 {
        push(
            &mut output,
            "m19-open-shadows",
            AdvisorCategory::Light,
            "shadows",
            18.0,
            "Recover shadow detail",
            format!(
                "Black clipping is {:.1}% and lower-quarter luminance is {:.3}.",
                analysis.black_clip_fraction * 100.0,
                analysis.p25
            ),
            ConfidenceLabel::High,
        );
    }
    if analysis.white_clip_fraction > 0.012 && analysis.p95 > 0.78 {
        push(
            &mut output,
            "m19-recover-highlights",
            AdvisorCategory::Light,
            "highlights",
            -22.0,
            "Recover highlights",
            format!(
                "White clipping is {:.1}% while p95 is {:.3}.",
                analysis.white_clip_fraction * 100.0,
                analysis.p95
            ),
            ConfidenceLabel::High,
        );
    }
    if analysis.warmth_bias.abs() > 0.16 {
        push(
            &mut output,
            "m19-neutralize-temperature",
            AdvisorCategory::WhiteBalance,
            "temperature",
            (-analysis.warmth_bias * 22.0).clamp(-20.0, 20.0),
            "Reduce colour cast",
            format!(
                "Relative warm/cool evidence is {:.3}; encoded images remain relative, not Kelvin.",
                analysis.warmth_bias
            ),
            ConfidenceLabel::Medium,
        );
    }
    if analysis.green_magenta_bias.abs() > 0.08 {
        push(
            &mut output,
            "m19-neutralize-tint",
            AdvisorCategory::WhiteBalance,
            "tint",
            (-analysis.green_magenta_bias * 35.0).clamp(-15.0, 15.0),
            "Reduce green/magenta cast",
            format!(
                "Green/magenta evidence is {:.3}.",
                analysis.green_magenta_bias
            ),
            ConfidenceLabel::Medium,
        );
    }
    if analysis.global_contrast < 0.16
        && analysis.black_clip_fraction < 0.01
        && analysis.white_clip_fraction < 0.01
    {
        push(
            &mut output,
            "m19-restore-contrast",
            AdvisorCategory::Light,
            "contrast",
            12.0,
            "Restore midtone contrast",
            format!(
                "p95−p05 contrast is only {:.3} without clipping.",
                analysis.global_contrast
            ),
            ConfidenceLabel::Medium,
        );
    }
    if analysis.portrait_sample_fraction > 0.01 && analysis.portrait_luminance_mean < 0.18 {
        push(
            &mut output,
            "m19-face-exposure",
            AdvisorCategory::Portrait,
            "exposure",
            0.18,
            "Lift face exposure",
            format!(
                "Detected skin-weighted luminance is {:.3} across {:.1}% of analyzed pixels.",
                analysis.portrait_luminance_mean,
                analysis.portrait_sample_fraction * 100.0
            ),
            ConfidenceLabel::Medium,
        );
    }
    output
}

/// Builds deterministic editing statistics from linear Rec.2020 RGB samples.
/// The thresholds are intentionally explicit so the advisor remains explainable and testable.
pub fn analyze_linear_rgb(samples: &[[f32; 3]]) -> AnalysisStats {
    if samples.is_empty() {
        return AnalysisStats::default();
    }

    let mut luminance_values = Vec::with_capacity(samples.len());
    let mut shadow_count = 0usize;
    let mut highlight_count = 0usize;
    let mut black_clip_count = 0usize;
    let mut white_clip_count = 0usize;
    let mut warmth_sum = 0.0_f32;

    for [red, green, blue] in samples.iter().copied() {
        let red = if red.is_finite() { red.max(0.0) } else { 0.0 };
        let green = if green.is_finite() {
            green.max(0.0)
        } else {
            0.0
        };
        let blue = if blue.is_finite() { blue.max(0.0) } else { 0.0 };
        let luminance = 0.2627 * red + 0.6780 * green + 0.0593 * blue;
        luminance_values.push(luminance);

        if luminance < 0.12 {
            shadow_count += 1;
        }
        if luminance > 0.72 {
            highlight_count += 1;
        }
        if luminance <= 0.003 {
            black_clip_count += 1;
        }
        if luminance >= 0.985 || red >= 0.995 || green >= 0.995 || blue >= 0.995 {
            white_clip_count += 1;
        }

        let chromatic_energy = red + blue;
        if chromatic_energy > 1.0e-5 {
            warmth_sum += (red - blue) / chromatic_energy;
        }
    }

    luminance_values.sort_by(|left, right| left.total_cmp(right));
    let middle = luminance_values.len() / 2;
    let median_luminance = if luminance_values.len().is_multiple_of(2) {
        (luminance_values[middle - 1] + luminance_values[middle]) * 0.5
    } else {
        luminance_values[middle]
    };

    let count = samples.len() as f32;
    AnalysisStats {
        shadow_fraction: shadow_count as f32 / count,
        highlight_fraction: highlight_count as f32 / count,
        black_clip_fraction: black_clip_count as f32 / count,
        white_clip_fraction: white_clip_count as f32 / count,
        median_luminance,
        estimated_warmth_bias: (warmth_sum / count).clamp(-1.0, 1.0),
    }
}

pub fn advise(stats: AnalysisStats) -> Vec<Suggestion> {
    let mut output = Vec::new();

    if stats.shadow_fraction >= 0.34 && stats.black_clip_fraction < 0.02 {
        let value = ((stats.shadow_fraction - 0.30) * 130.0).clamp(8.0, 32.0);
        output.push(Suggestion {
            id: "lift-shadows".into(),
            control: "shadows".into(),
            value,
            confidence: 0.78,
            reason: format!(
                "Dark tones occupy {:.0}% of the frame while black clipping remains limited.",
                stats.shadow_fraction * 100.0
            ),
        });
    }

    if stats.white_clip_fraction >= 0.01 || stats.highlight_fraction >= 0.22 {
        let value = -((stats.white_clip_fraction * 900.0) + stats.highlight_fraction * 55.0)
            .clamp(10.0, 45.0);
        output.push(Suggestion {
            id: "recover-highlights".into(),
            control: "highlights".into(),
            value,
            confidence: 0.82,
            reason: format!(
                "Bright tones are concentrated and {:.1}% of samples are near white clipping.",
                stats.white_clip_fraction * 100.0
            ),
        });
    }

    if stats.median_luminance < 0.12 && stats.white_clip_fraction < 0.005 {
        output.push(Suggestion {
            id: "raise-exposure".into(),
            control: "exposure".into(),
            value: 0.25,
            confidence: 0.64,
            reason: "Median luminance is low and the image has headroom before white clipping."
                .into(),
        });
    }

    if stats.estimated_warmth_bias.abs() >= 0.18 {
        output.push(Suggestion {
            id: "neutralize-cast".into(),
            control: "temperature".into(),
            value: (-stats.estimated_warmth_bias * 35.0).clamp(-20.0, 20.0),
            confidence: 0.55,
            reason: "A broad warm/cool cast was detected; this is a relative encoded-image correction, not a physical Kelvin estimate."
                .into(),
        });
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyzes_dark_samples_and_median_without_nan() {
        let samples = [
            [0.02, 0.02, 0.02],
            [0.04, 0.03, 0.02],
            [0.08, 0.07, 0.06],
            [0.40, 0.35, 0.30],
        ];
        let stats = analyze_linear_rgb(&samples);
        assert!(stats.shadow_fraction >= 0.5);
        assert!(stats.median_luminance.is_finite());
        assert!(stats.white_clip_fraction == 0.0);
    }

    #[test]
    fn detects_white_clip_and_warm_bias() {
        let samples = [[1.0, 0.90, 0.45], [0.95, 0.80, 0.30], [0.70, 0.55, 0.20]];
        let stats = analyze_linear_rgb(&samples);
        assert!(stats.white_clip_fraction > 0.0);
        assert!(stats.estimated_warmth_bias > 0.18);
    }

    #[test]
    fn suggests_shadow_lift_only_when_black_clipping_is_not_dominant() {
        let suggestions = advise(AnalysisStats {
            shadow_fraction: 0.45,
            black_clip_fraction: 0.004,
            ..Default::default()
        });
        assert!(
            suggestions
                .iter()
                .any(|item| item.control == "shadows" && item.value > 0.0)
        );

        let clipped = advise(AnalysisStats {
            shadow_fraction: 0.45,
            black_clip_fraction: 0.08,
            ..Default::default()
        });
        assert!(!clipped.iter().any(|item| item.control == "shadows"));
    }

    #[test]
    fn highlight_rule_is_explainable_and_bounded() {
        let suggestions = advise(AnalysisStats {
            highlight_fraction: 0.30,
            white_clip_fraction: 0.025,
            ..Default::default()
        });
        let item = suggestions
            .iter()
            .find(|item| item.control == "highlights")
            .expect("highlight suggestion");
        assert!(item.value <= -10.0 && item.value >= -45.0);
        assert!(!item.reason.is_empty());
    }

    #[test]
    fn m19_detailed_analysis_has_ordered_percentiles_and_finite_statistics() {
        let result = analyze_detailed(&[
            [0.0, 0.0, 0.0],
            [0.03, 0.03, 0.03],
            [0.2, 0.18, 0.15],
            [0.7, 0.5, 0.2],
            [1.2, 1.1, 1.0],
        ]);
        assert!(result.p01 <= result.p50 && result.p50 <= result.p99);
        assert!(result.white_clip_fraction > 0.0 && result.black_clip_fraction > 0.0);
        assert!(
            [
                result.luminance_mean,
                result.global_contrast,
                result.mean_chroma,
                result.warmth_bias,
                result.green_magenta_bias
            ]
            .into_iter()
            .all(f32::is_finite)
        );
    }

    #[test]
    fn m19_rule_engine_is_bounded_explainable_and_conflict_aware() {
        let suggestions = advise_detailed(DetailedAnalysis {
            p25: 0.03,
            p50: 0.08,
            p95: 0.91,
            white_clip_fraction: 0.04,
            black_clip_fraction: 0.05,
            warmth_bias: 0.22,
            green_magenta_bias: -0.1,
            global_contrast: 0.55,
            ..Default::default()
        });
        assert!(
            suggestions
                .iter()
                .any(|item| item.control == "highlights" && item.amount < 0.0)
        );
        assert!(suggestions.iter().any(|item| item.control == "temperature"));
        assert!(
            !suggestions.iter().any(|item| item.control == "exposure"),
            "high clipping prevents the dark-frame exposure rule"
        );
        assert!(
            suggestions
                .iter()
                .all(|item| !item.what.is_empty() && !item.why.is_empty())
        );
    }
}
