#[path = "../../starroom-imageio/tests/support/controlled_dng.rs"]
mod fixture;

use starroom_color::ToneParameters;
use starroom_imageio::decode_source;
use starroom_pipeline::{
    RenderSettings, render_source_export_to_srgb8, render_source_preview_to_srgb8,
};
use tiff::encoder::Rational;

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
