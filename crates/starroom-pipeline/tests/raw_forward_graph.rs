#[path = "../../starroom-imageio/tests/support/controlled_dng.rs"]
mod fixture;

use starroom_color::ToneParameters;
use starroom_imageio::decode_source;
use starroom_pipeline::{
    RenderSettings, render_source_export_to_srgb8, render_source_preview_to_srgb8,
};
use tiff::encoder::Rational;

#[test]
fn real_primary_profile_signature_policy_reaches_native_preview_and_export_pixels() {
    let matching = fixture::controlled_fixture_with_signatures(
        None,
        true,
        Some([Some(b"camera"), Some(b"camera")]),
    )
    .unwrap();
    let mismatching = fixture::controlled_fixture_with_signatures(
        None,
        true,
        Some([Some(b"camera"), Some(b"different-profile")]),
    )
    .unwrap();
    let matched = decode_source(&matching.0).unwrap();
    let mismatched = decode_source(&mismatching.0).unwrap();
    for exposure in [-2.0, 0.0, 1.0] {
        let settings = RenderSettings {
            tone: ToneParameters {
                exposure_ev: exposure,
                ..Default::default()
            },
            ..Default::default()
        };
        let matched_preview = render_source_preview_to_srgb8(&matched, &settings).unwrap();
        let mismatched_preview = render_source_preview_to_srgb8(&mismatched, &settings).unwrap();
        assert_eq!(
            matched_preview,
            render_source_export_to_srgb8(&matched, &settings).unwrap()
        );
        assert_eq!(
            mismatched_preview,
            render_source_export_to_srgb8(&mismatched, &settings).unwrap()
        );
        assert_ne!(matched_preview.data, mismatched_preview.data);
        assert_ne!(
            matched_preview.color.camera_profile_hash,
            mismatched_preview.color.camera_profile_hash
        );
    }
}

#[test]
fn actual_dual_neutral_white_flows_through_shared_preview_export_graph() {
    use starroom_color_management::temperature::{CorrelatedTemperature, temperature_to_xyz};
    let kelvin = 4200.0;
    let white = temperature_to_xyz(CorrelatedTemperature {
        kelvin,
        delta_uv: 0.004,
    })
    .unwrap();
    let weight =
        ((1e6 / kelvin as f32 - 1e6 / 2856.0) / (1e6 / 6504.0 - 1e6 / 2856.0)).clamp(0.0, 1.0);
    let neutral = [
        white.x * (1.0 + 0.01 * weight),
        1.0,
        white.z * (1.0 - 0.01 * weight),
    ]
    .map(|value| Rational {
        n: (value * 10000000.0).round() as u32,
        d: 10000000,
    });
    let source = fixture::controlled_fixture_with_dual_neutral(Some(neutral)).unwrap();
    let original = std::fs::read(&source.0).unwrap();
    let decoded = decode_source(&source.0).unwrap();
    for exposure in [-2.0, 0.0, 1.0] {
        let settings = RenderSettings {
            tone: ToneParameters {
                exposure_ev: exposure,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            render_source_preview_to_srgb8(&decoded, &settings).unwrap(),
            render_source_export_to_srgb8(&decoded, &settings).unwrap()
        );
    }
    assert_eq!(original, std::fs::read(&source.0).unwrap());
}

#[test]
fn real_forward_dng_sensor_uses_shared_native_preview_export_graph() {
    let baseline = fixture::controlled_fixture().unwrap();
    let baseline = decode_source(&baseline.0).unwrap();
    let source = fixture::controlled_fixture_with_dng_profile(
        Some([
            Rational { n: 2, d: 1 },
            Rational { n: 1, d: 1 },
            Rational { n: 1, d: 2 },
        ]),
        true,
    )
    .unwrap();
    let original = std::fs::read(&source.0).unwrap();
    let decoded = decode_source(&source.0).unwrap();
    for exposure in [0.0, -2.0, 1.0] {
        let settings = RenderSettings {
            tone: ToneParameters {
                exposure_ev: exposure,
                ..Default::default()
            },
            ..Default::default()
        };
        let preview = render_source_preview_to_srgb8(&decoded, &settings).unwrap();
        let export = render_source_export_to_srgb8(&decoded, &settings).unwrap();
        assert_eq!(preview, export);
        assert!(
            preview
                .color
                .camera_profile_id
                .as_deref()
                .unwrap()
                .starts_with("dng-forward-matrix:")
        );
        assert_ne!(
            preview.data,
            render_source_preview_to_srgb8(&baseline, &settings)
                .unwrap()
                .data
        );
    }
    assert_eq!(original, std::fs::read(&source.0).unwrap());
}
