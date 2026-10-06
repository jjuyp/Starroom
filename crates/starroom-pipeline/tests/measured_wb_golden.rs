//! Licensed real images through the shared measured-white/CAT path; no browser image math.
use starroom_imageio::decode_source_preview;
use starroom_pipeline::{
    RenderSettings, WhiteBalanceMode, WhiteBalanceSample, WhiteBalanceSettings,
    render_source_export_to_srgb8, render_source_preview_to_srgb8,
    render_source_preview_with_gpu_to_srgb8,
};
use starroom_render::gpu::GpuRenderer;
use std::{fs, path::Path};

#[test]
fn portrait_mixed_light_and_neon_measured_wb_share_native_preview_export_and_gpu() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden/sources");
    let gpu = GpuRenderer::try_new().ok();
    for name in [
        "astronaut-eileen-collins.png",
        "fire-performer-low-key.jpg",
        "neon-streets-nikon-d7100.jpg",
    ] {
        let path = directory.join(name);
        let original = fs::read(&path).unwrap();
        let source = decode_source_preview(&path, 256).unwrap();
        for mode in [WhiteBalanceMode::Auto, WhiteBalanceMode::NeutralPicker] {
            let settings = RenderSettings {
                white_balance: WhiteBalanceSettings {
                    mode,
                    sample: (mode == WhiteBalanceMode::NeutralPicker).then_some(
                        WhiteBalanceSample {
                            x: 0.45,
                            y: 0.45,
                            width: 0.1,
                            height: 0.1,
                        },
                    ),
                },
                ..Default::default()
            };
            let preview = render_source_preview_to_srgb8(&source, &settings).unwrap();
            let exported = render_source_export_to_srgb8(&source, &settings).unwrap();
            assert_eq!(preview, exported, "{name} {mode:?}");
            assert_eq!(
                exported,
                render_source_export_to_srgb8(&source, &settings).unwrap()
            );
            if let Some(gpu) = &gpu {
                let accelerated =
                    render_source_preview_with_gpu_to_srgb8(&source, &settings, gpu).unwrap();
                let max = preview
                    .data
                    .iter()
                    .zip(&accelerated.data)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(max <= 1, "{name} {mode:?} CPU/GPU max {max}");
            }
        }
        assert_eq!(
            original,
            fs::read(path).unwrap(),
            "source must not be modified"
        );
    }
}
