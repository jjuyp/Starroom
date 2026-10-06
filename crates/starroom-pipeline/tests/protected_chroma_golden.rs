//! Real licensed portrait through the production Native pipeline; no browser math/mock renderer.
use starroom_imageio::decode_source_preview;
use starroom_pipeline::{
    RenderSettings, render_source_export_to_srgb8, render_source_preview_to_srgb8,
    render_source_preview_with_gpu_to_srgb8,
};
use starroom_render::gpu::GpuRenderer;
use std::{fs, path::Path};

#[test]
fn real_portrait_desaturation_and_vibrance_are_deterministic_shared_and_source_immutable() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/golden/sources/astronaut-eileen-collins.png");
    let original = fs::read(&path).unwrap();
    let source = decode_source_preview(&path, 512).unwrap();
    let gpu = GpuRenderer::try_new().ok();
    for (saturation, vibrance) in [
        (-1.0, -1.0),
        (-1.0, 1.0),
        (0.0, 1.0),
        (0.0, -1.0),
        (0.5, 0.5),
    ] {
        let mut settings = RenderSettings::default();
        settings.relative_color.saturation = saturation;
        settings.relative_color.vibrance = vibrance;
        let preview = render_source_preview_to_srgb8(&source, &settings).unwrap();
        let exported = render_source_export_to_srgb8(&source, &settings).unwrap();
        assert_eq!(preview.data, exported.data);
        assert_eq!(
            exported.data,
            render_source_export_to_srgb8(&source, &settings)
                .unwrap()
                .data
        );
        if saturation == -1.0 {
            for pixel in exported.data.chunks_exact(3) {
                assert!(pixel[0].abs_diff(pixel[1]) <= 1 && pixel[1].abs_diff(pixel[2]) <= 1);
            }
        }
        if let Some(gpu) = &gpu {
            let accelerated =
                render_source_preview_with_gpu_to_srgb8(&source, &settings, gpu).unwrap();
            let max = exported
                .data
                .iter()
                .zip(&accelerated.data)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(max <= 1, "portrait chroma CPU/GPU difference {max}");
        }
    }
    assert_eq!(fs::read(path).unwrap(), original);
}
