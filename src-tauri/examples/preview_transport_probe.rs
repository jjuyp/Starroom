//! Actual shared-graph pixels; isolate transport cost, not input-to-present acceptance.
#[path = "../src/preview_transport.rs"]
mod preview_transport;
use preview_transport::{
    PreviewFrameHeader, PreviewPayloadFormat, preview_frame, rgba_preview_frame,
};
use std::{
    error::Error,
    fs::OpenOptions,
    hint::black_box,
    io::Write,
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

fn main() -> Result<(), Box<dyn Error>> {
    let raw = std::env::args().any(|argument| argument == "--raw");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let source = root.join(if raw {
        "fixtures/raw/sources/fujifilm-x-pro1.raf"
    } else {
        "fixtures/golden/sources/astronaut-eileen-collins.png"
    });
    let original = std::fs::read(&source)?;
    let decoded = starroom_imageio::decode_source_preview(&source, 1024)?;
    let mut settings = starroom_pipeline::RenderSettings::default();
    settings.tone.exposure_ev = 0.35;
    let renderer = starroom_render::gpu::GpuRenderer::try_new()?;
    let rendered =
        starroom_pipeline::render_source_preview_with_gpu_to_srgb8(&decoded, &settings, &renderer)?;
    let header = PreviewFrameHeader {
        width: rendered.width,
        height: rendered.height,
        source_width: rendered.width,
        source_height: rendered.height,
        tile_x: 0,
        tile_y: 0,
        flags: match renderer.status().backend {
            starroom_render::gpu::GpuBackendKind::CpuFallback => 0x10,
            _ => 8,
        } | match rendered.color.input {
            starroom_color_management::InputProfileSource::EmbeddedIcc => 1,
            starroom_color_management::InputProfileSource::RawCameraMatrix => 2,
            starroom_color_management::InputProfileSource::RawGenericProfile => 4,
            _ => 0,
        },
    };
    let profile = rendered.color.camera_profile_id.as_deref().unwrap_or("");
    PreviewPayloadFormat::Rgba8.validate(true)?;
    assert_ne!(
        PreviewPayloadFormat::Rgba8.cache_identity(true),
        PreviewPayloadFormat::Jpeg.cache_identity(true)
    );
    let jpeg = || -> Result<Vec<u8>, Box<dyn Error>> {
        Ok(preview_frame(
            header,
            profile,
            starroom_imageio::encode_jpeg_rgb8(
                black_box(&rendered.data),
                header.width,
                header.height,
                88,
                None,
            )?,
        )?)
    };
    let rgba = || -> Result<Vec<u8>, Box<dyn Error>> {
        Ok(rgba_preview_frame(
            header,
            profile,
            black_box(&rendered.data),
        )?)
    };
    let mut jpeg_ms = vec![];
    let mut rgba_ms = vec![];
    for round in 0..22 {
        for kind in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
            let start = Instant::now();
            let bytes = if kind == 0 { jpeg()? } else { rgba()? };
            black_box(&bytes);
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if round >= 2 {
                if kind == 0 {
                    jpeg_ms.push(elapsed)
                } else {
                    rgba_ms.push(elapsed)
                }
            }
        }
    }
    jpeg_ms.sort_by(f64::total_cmp);
    rgba_ms.sort_by(f64::total_cmp);
    let jpeg_frame = jpeg()?;
    let rgba_frame = rgba()?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let output = root.join("output/playwright");
    std::fs::create_dir_all(&output)?;
    let write_new = |path: PathBuf, bytes: &[u8]| -> Result<(), Box<dyn Error>> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?
            .write_all(bytes)?;
        Ok(())
    };
    let prefix = format!("preview-transport-{timestamp}");
    write_new(output.join(format!("{prefix}-jpeg.bin")), &jpeg_frame)?;
    write_new(output.join(format!("{prefix}-rgba.bin")), &rgba_frame)?;
    assert_eq!(std::fs::read(source)?, original);
    let report = serde_json::json!({ "scope": "MSVC release Native payload encoding/packing only; excludes IPC/browser/presentation",
        "raw": raw, "width": header.width, "height": header.height,
        "gpu": format!("{:?}", renderer.status()), "measuredSamples": 20,
        "jpeg88": {"medianMs": (jpeg_ms[9] + jpeg_ms[10]) / 2.0, "p95Ms": jpeg_ms[18], "bytes": jpeg_frame.len(), "samplesMs": jpeg_ms},
        "rgba8": {"medianMs": (rgba_ms[9] + rgba_ms[10]) / 2.0, "p95Ms": rgba_ms[18], "bytes": rgba_frame.len(), "samplesMs": rgba_ms},
        "browserFramePrefix": prefix, "sourceUnchanged": true });
    let bytes = serde_json::to_vec_pretty(&report)?;
    write_new(
        root.join(format!(".starroom-reports/{prefix}.json")),
        &bytes,
    )?;
    println!("{}", String::from_utf8(bytes)?);
    Ok(())
}
