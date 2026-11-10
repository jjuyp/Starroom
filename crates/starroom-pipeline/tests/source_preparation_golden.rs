//! Real licensed sources through cached preparation and the unchanged complete graph.
use starroom_imageio::decode_source_preview;
use starroom_optics::{LensIdentity, LensMatchMode, OpticsParameters, OpticsSettings};
use starroom_pipeline::{
    RenderSettings, SourcePreparationCache, WhiteBalanceMode, WhiteBalanceSample,
    render_source_export_to_srgb8, render_source_preview_with_preparation_cache_to_srgb8,
};
use starroom_render::{
    gpu::GpuRenderer,
    profiling::{ProfileStage, capture},
};
use std::{fs, path::Path, sync::Arc};

#[test]
fn prepared_real_portrait_and_raw_match_uncached_cpu_gpu_export_and_keep_source_immutable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let gpu = GpuRenderer::try_new().ok();
    for name in [
        "golden/sources/astronaut-eileen-collins.png",
        "raw/sources/nikon-d1.nef",
    ] {
        let path = root.join(name);
        let original = fs::read(&path).unwrap();
        let source = Arc::new(decode_source_preview(&path, 256).unwrap());
        let cache = SourcePreparationCache::default();
        let mut settings = RenderSettings::default();
        settings.geometry.rotation_degrees = 90.0;
        for amount in [0.0, 0.2, -0.5, 0.8] {
            settings.tone.exposure_ev = amount;
            settings.tone.shadows = amount;
            settings.relative_color.tint = amount;
            let expected = render_source_export_to_srgb8(&source, &settings).unwrap();
            assert_eq!(
                render_source_preview_with_preparation_cache_to_srgb8(
                    &source, &settings, None, &cache
                )
                .unwrap(),
                expected
            );
            if let Some(gpu) = &gpu {
                let result = render_source_preview_with_preparation_cache_to_srgb8(
                    &source,
                    &settings,
                    Some(gpu),
                    &cache,
                )
                .unwrap();
                assert_eq!(result.color, expected.color);
                assert_eq!(
                    (result.width, result.height),
                    (expected.width, expected.height)
                );
                assert!(
                    result
                        .data
                        .iter()
                        .zip(&expected.data)
                        .all(|(a, b)| a.abs_diff(*b) <= 1)
                );
            }
        }
        assert_eq!(cache.stats().unwrap().builds, 1);
        assert!(cache.stats().unwrap().hits >= 3);
        settings.white_balance.mode = WhiteBalanceMode::NeutralPicker;
        for x in [0.4, 0.5] {
            settings.white_balance.sample = Some(WhiteBalanceSample {
                x,
                y: 0.45,
                width: 0.05,
                height: 0.05,
            });
            let expected = render_source_export_to_srgb8(&source, &settings).unwrap();
            assert_eq!(
                render_source_preview_with_preparation_cache_to_srgb8(
                    &source, &settings, None, &cache
                )
                .unwrap(),
                expected
            );
            if let Some(gpu) = &gpu {
                let actual = render_source_preview_with_preparation_cache_to_srgb8(
                    &source,
                    &settings,
                    Some(gpu),
                    &cache,
                )
                .unwrap();
                assert!(
                    actual
                        .data
                        .iter()
                        .zip(&expected.data)
                        .all(|(a, b)| a.abs_diff(*b) <= 1)
                );
            }
        }
        assert_eq!(
            cache.stats().unwrap().builds,
            1,
            "picker must reuse source/geometry pixels"
        );
        assert_eq!(fs::read(path).unwrap(), original);
    }
}

#[test]
fn prepared_lensfun_pixels_are_reused_until_actual_optics_changes() {
    let source = Arc::new(
        decode_source_preview(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/golden/sources/astronaut-eileen-collins.png"),
            256,
        )
        .unwrap(),
    );
    let cache = SourcePreparationCache::default();
    let mut settings = RenderSettings {
        optics: OpticsSettings {
            parameters: OpticsParameters {
                enabled: true,
                ..Default::default()
            },
            match_mode: LensMatchMode::Manual,
            manual_identity: Some(LensIdentity {
                camera_make: "Nikon".into(),
                camera_model: "Nikon D750".into(),
                lens_make: "Nikon".into(),
                lens_model: "Nikon AF-S Nikkor 16-35mm f/4G ED VR".into(),
                focal_length_mm: 24.0,
                aperture: 5.6,
                focus_distance_m: Some(10.0),
            }),
        },
        ..Default::default()
    };
    render_source_preview_with_preparation_cache_to_srgb8(&source, &settings, None, &cache)
        .unwrap();
    settings.tone.exposure_ev = 0.5;
    let (rendered, profile) = capture(|| {
        render_source_preview_with_preparation_cache_to_srgb8(&source, &settings, None, &cache)
    });
    assert_eq!(
        rendered.unwrap(),
        render_source_export_to_srgb8(&source, &settings).unwrap()
    );
    assert_eq!(profile.stages[&ProfileStage::Lens].executions, 0);
    assert_eq!(profile.stages[&ProfileStage::Lens].cache_hits, 1);
    settings.optics.parameters.vignette = false;
    assert_eq!(
        render_source_preview_with_preparation_cache_to_srgb8(&source, &settings, None, &cache)
            .unwrap(),
        render_source_export_to_srgb8(&source, &settings).unwrap()
    );
    assert_eq!(cache.stats().unwrap().builds, 2);
}
