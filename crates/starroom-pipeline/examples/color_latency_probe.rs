//! Native engine timing only, not pointer-to-present or a desktop responsiveness acceptance.
//! Uses the existing licensed portrait without modifying it; emits measurements, no image files.
use starroom_imageio::decode_source_preview;
use starroom_pipeline::{
    CameraWhiteBalanceTarget, RenderSettings, SourcePreparationCache, WhiteBalanceMode,
    WhiteBalanceSample, WhiteBalanceSettings, render_source_export_to_srgb8,
    render_source_preview_with_gpu_to_srgb8, render_source_preview_with_preparation_cache_to_srgb8,
};
use starroom_render::gpu::GpuRenderer;
use std::{
    error::Error,
    io::Write,
    path::Path,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    let raw = arguments.iter().any(|argument| argument == "--raw");
    let edge = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--edge="))
        .map(str::parse::<u32>)
        .transpose()?
        .unwrap_or(512);
    if !(256..=2048).contains(&edge) {
        return Err("edge must stay inside 256..2048".into());
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(if raw {
        "../../fixtures/raw/sources/fujifilm-x-pro1.raf"
    } else {
        "../../fixtures/golden/sources/astronaut-eileen-collins.png"
    });
    let original_bytes = std::fs::read(&path)?;
    let decode_start = Instant::now();
    let source = Arc::new(decode_source_preview(&path, edge)?);
    let decode_ms = decode_start.elapsed().as_secs_f64() * 1000.0;
    let use_prepared = arguments.iter().any(|argument| argument == "--prepared");
    let preparation = SourcePreparationCache::default();
    let gpu = GpuRenderer::try_new()?;
    let mut report = Vec::new();
    let mut controls = vec![
        "Exposure",
        "Temperature",
        "Tint",
        "Saturation",
        "Vibrance",
        "AutoWhiteBalance",
        "NeutralPicker",
    ];
    if raw {
        controls.extend(["PhysicalTemperature", "PhysicalTint"]);
    }
    for control in controls {
        let mut samples = Vec::new();
        let mut max_delta = 0;
        let mut last_profile = None;
        for index in 0..6 {
            let mut settings = RenderSettings::default();
            let value = (index as f32 + 1.0) / 10.0;
            match control {
                "Exposure" => settings.tone.exposure_ev = value,
                "Temperature" => settings.relative_color.temperature = value,
                "Tint" => settings.relative_color.tint = value,
                "Saturation" => settings.relative_color.saturation = value,
                "Vibrance" => settings.relative_color.vibrance = value,
                "PhysicalTemperature" => {
                    settings.raw_white_balance = Some(CameraWhiteBalanceTarget {
                        kelvin: 3000.0 + f64::from(value) * 5000.0,
                        delta_uv: 0.0,
                    })
                }
                "PhysicalTint" => {
                    settings.raw_white_balance = Some(CameraWhiteBalanceTarget {
                        kelvin: 6504.0,
                        delta_uv: f64::from(value) * 0.025,
                    })
                }
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
            let (accelerated, profile) = starroom_render::profiling::capture(|| {
                if use_prepared {
                    render_source_preview_with_preparation_cache_to_srgb8(
                        &source,
                        &settings,
                        Some(&gpu),
                        &preparation,
                    )
                } else {
                    render_source_preview_with_gpu_to_srgb8(&source, &settings, &gpu)
                }
            });
            let accelerated = accelerated?;
            last_profile = Some(profile);
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
            "lastProfile": last_profile,
        }));
    }
    if std::fs::read(&path)? != original_bytes {
        return Err("source bytes changed".into());
    }
    let result = serde_json::json!({
        "scope": "real immutable fixture, Native engine only; no UI/encode presentation timing",
        "sourceKind": if raw { "RAW sensor / X-Trans" } else { "encoded portrait" },
        "previewDimensions": [source.width(), source.height()], "decodeMs": decode_ms,
        "colorPolicy": starroom_pipeline::COLOR_POLICY_VERSION,
        "kernelHead": std::env::var("STARROOM_BENCHMARK_HEAD").ok(),
        "adapter": gpu.status(), "measurements": report,
        "gpuResources": gpu.resource_stats(),
        "iccTransformCache": starroom_color_management::icc_transform_cache_stats()?,
        "sourcePreparation": preparation.stats()?, "usesPreparationCache": use_prepared,
    });
    std::fs::create_dir_all(".starroom-reports")?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let report_path = format!(
        ".starroom-reports/color-latency-{stamp}-{}.json",
        std::process::id()
    );
    let mut report = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&report_path)?;
    report.write_all(serde_json::to_string_pretty(&result)?.as_bytes())?;
    report.sync_all()?;
    println!("{result}");
    eprintln!("Measurement report: {report_path}");
    Ok(())
}
