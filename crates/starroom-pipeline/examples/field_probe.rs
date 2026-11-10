//! Local-only field diagnostic. Source photographs are never modified.
use std::{error::Error, path::PathBuf, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let source = PathBuf::from(args.next().ok_or("expected source path")?);
    let output = PathBuf::from(args.next().ok_or("expected new output directory")?);
    let edge: u32 = args
        .next()
        .map(|v| v.to_string_lossy().parse())
        .transpose()?
        .unwrap_or(2048);
    // Refuse existing destinations, including the source directory.
    std::fs::create_dir(&output)?;
    let start = Instant::now();
    let decoded = starroom_imageio::decode_source_preview(&source, edge)?;
    println!(
        "decode_ms={} dimensions={}x{}",
        start.elapsed().as_millis(),
        decoded.width(),
        decoded.height()
    );
    for (name, exposure, shadows) in [
        ("neutral", 0.0, 0.0),
        ("exposure", 1.0, 0.0),
        ("shadows", 0.0, 0.5),
        ("combined", 1.0, 0.5),
    ] {
        let mut settings = starroom_pipeline::RenderSettings::default();
        settings.tone.exposure_ev = exposure;
        settings.tone.shadows = shadows;
        let start = Instant::now();
        let (rendered, profile) =
            starroom_pipeline::profile_source_preview_to_srgb8(&decoded, &settings);
        let rendered = rendered?;
        println!(
            "{name}: render_ms={} profile={profile:?}",
            start.elapsed().as_millis()
        );
        let bytes = starroom_imageio::encode_jpeg_rgb8(
            &rendered.data,
            rendered.width,
            rendered.height,
            95,
            None,
        )?;
        std::fs::write(output.join(format!("{name}.jpg")), bytes)?;
    }
    Ok(())
}
