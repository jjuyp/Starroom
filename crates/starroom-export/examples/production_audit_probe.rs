//! Phase-0 counterexamples using production APIs and owned temporary pixels, never user originals.
//! This diagnostic is NOT a completion gate, color oracle or a substitute for real UI acceptance.

use starroom_color_management::{BuiltinOutputProfile, LittleCmsProvider, RenderingIntent};
use starroom_export::{
    CollisionPolicy, ExportFormat, ExportRequest, ExportSettings, NativeSharedGraphRenderer,
    export_one,
};
use starroom_imageio::{DecodedRenderedImage, DecodedSourceImage, RenderedFormat, encode_png_rgb8};
use starroom_pipeline::{
    LayerAdjustments, LayerBlendMode, NativeAdjustmentLayer, RenderSettings,
    render_source_export_to_srgb8, render_source_preview_with_gpu_to_srgb8,
};
use starroom_project::{MaskDefinition, MaskTree};
use starroom_render::gpu::GpuRenderer;
use std::{error::Error, fs, sync::atomic::AtomicBool, time::SystemTime};

fn source() -> DecodedSourceImage {
    let mut rgba = Vec::new();
    for y in 0..24 {
        for x in 0..32 {
            let value = 0.15 + 0.6 * (x + y) as f32 / 54.0;
            rgba.extend_from_slice(&[value, value * 0.8, value * 0.6, 1.0]);
        }
    }
    DecodedSourceImage::Rendered(DecodedRenderedImage {
        width: 32,
        height: 24,
        format: RenderedFormat::Png,
        rgba,
        embedded_icc: None,
        exif: None,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!(
        "starroom-owned-production-probe-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_nanos()
    ));
    fs::create_dir(&root)?;
    let path = root.join("owned-fixture.png");
    let bytes = encode_png_rgb8(&[30, 80, 120, 180, 100, 50], 2, 1, None)?;
    fs::write(&path, &bytes)?;
    let request = ExportRequest {
        asset_id: 1,
        source_path: path.clone(),
        destination_directory: root.clone(),
        original_name: "owned-fixture.png".into(),
        capture_date: None,
        rating: 0,
        keywords: Vec::new(),
        camera: None,
        look: None,
        sequence: 1,
        source_fingerprint: "owned-test-fixture".into(),
        edit_state_identity: "audit-exposure".into(),
        settings: ExportSettings {
            format: ExportFormat::Png,
            filename_template: "owned-fixture".into(),
            collision: CollisionPolicy::Overwrite,
            ..Default::default()
        },
    };
    let mut settings = RenderSettings::default();
    settings.tone.exposure_ev = 1.0;
    let result = export_one(
        &NativeSharedGraphRenderer,
        &request,
        &settings,
        &AtomicBool::new(false),
    );
    let overwritten = fs::read(&path)? != bytes;
    println!(
        "SOURCE_SAFETY {}",
        serde_json::json!({
            "accepted":result.is_ok(),"ownedSourceBytesChanged":overwritten,
            "error":result.err().map(|error|error.to_string())
        })
    );

    let decoded = source();
    let mut cpu_settings = RenderSettings::default();
    cpu_settings.vignette.amount = 0.8;
    cpu_settings.vignette.midpoint = 0.2;
    cpu_settings.vignette.feather = 0.5;
    let mut adjustment = LayerAdjustments::default();
    adjustment.tone.contrast = 0.7;
    adjustment.tone.shadows = 0.5;
    cpu_settings.layers.push(NativeAdjustmentLayer {
        id: "owned-noncommuting-layer".into(),
        name: "Audit".into(),
        enabled: true,
        opacity: 1.0,
        blend_mode: LayerBlendMode::Normal,
        mask: MaskTree::Leaf(MaskDefinition::None),
        adjustments: adjustment,
    });
    match GpuRenderer::try_new() {
        Ok(gpu) => {
            let cpu = render_source_export_to_srgb8(&decoded, &cpu_settings)?;
            let accelerated =
                render_source_preview_with_gpu_to_srgb8(&decoded, &cpu_settings, &gpu)?;
            let max_code_difference = cpu
                .data
                .iter()
                .zip(&accelerated.data)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            println!(
                "VIGNETTE_ORDER {}",
                serde_json::json!({
                    "adapter":gpu.status(),"maxRgb8CodeDifference":max_code_difference,
                    "samples":cpu.data.len()
                })
            );
        }
        Err(error) => println!("VIGNETTE_ORDER unavailable: {error}"),
    }

    let profile =
        LittleCmsProvider.builtin_output_profile_bytes(BuiltinOutputProfile::DisplayP3)?;
    let mut neutral = [[0.02, 0.02, 0.02]];
    LittleCmsProvider.input_to_working(
        &mut neutral,
        Some(&profile),
        RenderingIntent::RelativeColorimetric,
        true,
    )?;
    println!(
        "P3_TRC {}",
        serde_json::json!({
            "encodedNeutral":0.02,"actualLinear":neutral[0],"standardSrgbLinear":0.02/12.92
        })
    );
    let mut transparent = decoded.clone();
    if let DecodedSourceImage::Rendered(image) = &mut transparent {
        image.rgba[..4].copy_from_slice(&[1.0, 0.0, 0.0, 0.0]);
    }
    let shown = render_source_export_to_srgb8(&transparent, &RenderSettings::default())?;
    println!(
        "ALPHA_POLICY {}",
        serde_json::json!({"transparentSourceRgb":[1,0,0],"sourceAlpha":0,"rgbOnlyOutput":&shown.data[..3]})
    );
    // This directory is generated above with an exclusive create, never a provided user path.
    let resolved = fs::canonicalize(&root)?;
    let temporary_root = fs::canonicalize(std::env::temp_dir())?;
    if !resolved.is_absolute() || resolved.parent() != Some(temporary_root.as_path()) {
        return Err("unsafe diagnostic cleanup target".into());
    }
    fs::remove_dir_all(resolved)?;
    Ok(())
}
