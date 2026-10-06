//! Native engine timing only, not pointer-to-present or a desktop responsiveness acceptance.
//! Uses the existing licensed portrait without modifying it; emits measurements, no image files.
use starroom_imageio::decode_source_preview;
use starroom_pipeline::{
    RenderSettings, WhiteBalanceMode, WhiteBalanceSample, WhiteBalanceSettings,
    render_source_export_to_srgb8, render_source_preview_with_gpu_to_srgb8,
};
use starroom_render::gpu::GpuRenderer;
use std::{error::Error, path::Path, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/golden/sources/astronaut-eileen-collins.png");
    let source = decode_source_preview(path, 512)?;
    let gpu = GpuRenderer::try_new()?;
    let mut report = Vec::new();
    for control in [
        "Exposure",
        "Temperature",
        "Tint",
        "Saturation",
        "Vibrance",
        "AutoWhiteBalance",
        "NeutralPicker",
    ] {
        let mut samples = Vec::new();
        let mut max_delta = 0;
        for index in 0..6 {
            let mut settings = RenderSettings::default();
            let value = (index as f32 + 1.0) / 10.0;
            match control {
                "Exposure" => settings.tone.exposure_ev = value,
                "Temperature" => settings.relative_color.temperature = value,
                "Tint" => settings.relative_color.tint = value,
                "Saturation" => settings.relative_color.saturation = value,
                "Vibrance" => settings.relative_color.vibrance = value,
                "AutoWhiteBalance" => settings.white_balance.mode = WhiteBalanceMode::Auto,
                "NeutralPicker" => {
                    settings.white_balance = WhiteBalanceSettings {
                        mode: WhiteBalanceMode::NeutralPicker,
                        sample: Some(WhiteBalanceSample {
                            x: 0.4 + value * 0.1,
                            y: 0.45,
                            width: 0.05,
                            height: 0.05,
                        }),
                    }
                }
                _ => unreachable!("static control list"),
            }
            let start = Instant::now();
            let accelerated = render_source_preview_with_gpu_to_srgb8(&source, &settings, &gpu)?;
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            // Warm-up omitted; CPU reference is deliberately outside the measured GPU interval.
            if index > 0 {
                samples.push(elapsed);
            }
            let reference = render_source_export_to_srgb8(&source, &settings)?;
            let delta = accelerated
                .data
                .iter()
                .zip(&reference.data)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .ok_or("empty output")?;
            max_delta = max_delta.max(delta);
            if delta > 1 {
                return Err(format!("{control}: CPU/GPU delta {delta}").into());
            }
        }
        samples.sort_by(f64::total_cmp);
        report.push(serde_json::json!({
            "control": control, "samples": 5, "medianMs": samples[2], "p95Ms": samples[4],
            "maxRgb8Delta": max_delta,
            "changedParameters": control != "AutoWhiteBalance",
        }));
    }
    println!(
        "{}",
        serde_json::json!({
            "scope": "512px real portrait, Native engine only; no UI/encode presentation timing",
            "colorPolicy": starroom_pipeline::COLOR_POLICY_VERSION,
            "adapter": gpu.status(), "measurements": report,
            "gpuResources": gpu.resource_stats(),
        })
    );
    Ok(())
}
