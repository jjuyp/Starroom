#[path = "../../starroom-imageio/tests/support/controlled_dng.rs"]
mod fixture;

use starroom_color::ToneParameters;
use starroom_imageio::decode_source;
use starroom_pipeline::{
    RenderSettings, render_source_export_to_srgb8, render_source_preview_to_srgb8,
};
use tiff::encoder::Rational;

#[test]
fn physical_raw_white_balance_renders_real_sensor_and_invalidates_preparation_cache() {
    use starroom_pipeline::{
        CameraWhiteBalanceTarget, SourcePreparationCache,
        render_source_preview_with_preparation_cache_to_srgb8,
    };
    use std::sync::Arc;
    let file = fixture::controlled_fixture().unwrap();
    let original_bytes = std::fs::read(&file.0).unwrap();
    let source = Arc::new(decode_source(&file.0).unwrap());
    let cache = SourcePreparationCache::default();
    let mut settings = RenderSettings::default();
    let original = render_source_preview_to_srgb8(&source, &settings).unwrap();
    let mut hashes = Vec::new();
    let mut outputs = Vec::new();
    for (kelvin, delta_uv) in [(2856.0, -0.01), (4200.0, 0.0), (6504.0, 0.01)] {
        settings.raw_white_balance = Some(CameraWhiteBalanceTarget { kelvin, delta_uv });
        let restored: RenderSettings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(restored.raw_white_balance, settings.raw_white_balance);
        let preview =
            render_source_preview_with_preparation_cache_to_srgb8(&source, &restored, None, &cache)
                .unwrap();
        let export = render_source_export_to_srgb8(&source, &restored).unwrap();
        assert_eq!(preview, export);
        assert_ne!(preview.data, original.data);
        hashes.push(preview.color.camera_profile_hash);
        outputs.push(preview.data);
    }
    assert_ne!(outputs[0], outputs[1]);
    assert_ne!(outputs[1], outputs[2]);
    assert_ne!(hashes[0], hashes[1]);
    assert_eq!(cache.stats().unwrap().builds, 3);
    settings.raw_white_balance = Some(CameraWhiteBalanceTarget {
        kelvin: 0.0,
        delta_uv: 0.0,
    });
    assert!(render_source_preview_to_srgb8(&source, &settings).is_err());
    assert_eq!(std::fs::read(&file.0).unwrap(), original_bytes);
    let mut legacy = (*source).clone();
    if let starroom_imageio::DecodedSourceImage::Raw(raw) = &mut legacy {
        raw.metadata.camera_profile_input = None;
    }
    settings.raw_white_balance = Some(CameraWhiteBalanceTarget {
        kelvin: 4200.0,
        delta_uv: 0.0,
    });
    assert!(render_source_preview_to_srgb8(&legacy, &settings).is_err());
    settings.raw_white_balance = None;
    assert_eq!(
        render_source_preview_to_srgb8(&legacy, &settings).unwrap(),
        original
    );
}

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
