use serde::Deserialize;
use starroom_color::{CurvePoint, ToneParameters};
use starroom_imageio::{DecodedRenderedImage, RenderedFormat};
use starroom_pipeline::{RelativeColorParameters, RenderSettings, render_preview_to_srgb8};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureFile {
    contract_version: u32,
    cases: Vec<FixtureCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureCase {
    name: String,
    source_rgb8: Vec<u8>,
    browser_rgb8: Vec<u8>,
    adjustments: FixtureAdjustments,
    curve: Vec<CurvePoint>,
    max_channel_delta: u8,
    max_mean_delta: f32,
}

#[derive(Debug, Default, Deserialize)]
struct FixtureAdjustments {
    #[serde(default)]
    exposure: f32,
    #[serde(default)]
    contrast: f32,
    #[serde(default)]
    highlights: f32,
    #[serde(default)]
    shadows: f32,
    #[serde(default)]
    temperature: f32,
    #[serde(default)]
    tint: f32,
}

#[test]
fn native_cpu_stays_within_documented_browser_migration_tolerances() {
    let fixtures: FixtureFile = serde_json::from_str(include_str!(
        "../../../tests/fixtures/m1c/browser-native-reference.json"
    ))
    .expect("valid reference fixture");
    assert_eq!(fixtures.contract_version, 1);

    for fixture in fixtures.cases {
        let rgba = fixture
            .source_rgb8
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|rgb| {
                [
                    rgb[0] as f32 / 255.0,
                    rgb[1] as f32 / 255.0,
                    rgb[2] as f32 / 255.0,
                    1.0,
                ]
            })
            .collect();
        let decoded = DecodedRenderedImage {
            width: (fixture.source_rgb8.len() / 3) as u32,
            height: 1,
            format: RenderedFormat::Png,
            rgba,
            embedded_icc: None,
            exif: None,
        };
        let unit = |value: f32| value / 100.0;
        let settings = RenderSettings {
            tone: ToneParameters {
                exposure_ev: fixture.adjustments.exposure,
                contrast: unit(fixture.adjustments.contrast),
                highlights: unit(fixture.adjustments.highlights),
                shadows: unit(fixture.adjustments.shadows),
                ..Default::default()
            },
            relative_color: RelativeColorParameters {
                temperature: unit(fixture.adjustments.temperature),
                tint: unit(fixture.adjustments.tint),
                ..Default::default()
            },
            curve: fixture.curve,
            ..Default::default()
        };
        let native = render_preview_to_srgb8(&decoded, &settings).expect("native preview");
        assert_eq!(
            native.data.len(),
            fixture.browser_rgb8.len(),
            "{}",
            fixture.name
        );
        let deltas: Vec<u8> = native
            .data
            .iter()
            .zip(&fixture.browser_rgb8)
            .map(|(native, browser)| native.abs_diff(*browser))
            .collect();
        let max_delta = deltas.iter().copied().max().unwrap_or(0);
        let mean_delta =
            deltas.iter().map(|value| f32::from(*value)).sum::<f32>() / deltas.len().max(1) as f32;
        assert!(
            max_delta <= fixture.max_channel_delta,
            "{} max channel delta {} > {}; native={:?}, browser={:?}",
            fixture.name,
            max_delta,
            fixture.max_channel_delta,
            native.data,
            fixture.browser_rgb8
        );
        assert!(
            mean_delta <= fixture.max_mean_delta,
            "{} mean delta {:.2} > {:.2}",
            fixture.name,
            mean_delta,
            fixture.max_mean_delta
        );
    }
}
