//! Actual decoded RAW crop benchmark; not whole-editor latency or a complete 100MP gate.
use starroom_imageio::{DecodedSourceImage, decode_source};
use std::{error::Error, path::PathBuf, time::Instant};

#[cfg(windows)]
fn private_bytes() -> Option<u64> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "(Get-Process -Id {}).PrivateMemorySize64",
                std::process::id()
            ),
        ])
        .creation_flags(0x08000000)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().parse().ok())
        .flatten()
}
#[cfg(not(windows))]
fn private_bytes() -> Option<u64> {
    None
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/raw/sources/fujifilm-x-pro1.raf")
        });
    let original_file = std::fs::read(&path)?;
    let source = decode_source(&path)?;
    let DecodedSourceImage::Raw(image) = &source else {
        return Err("RAW fixture required".into());
    };
    let width = image.width.min(512);
    let height = image.height.min(512);
    let x = (image.width - width) / 2;
    let y = (image.height - height) / 2;
    let mut baseline_ms = Vec::new();
    let mut optimized_ms = Vec::new();
    let mut baseline_memory = None;
    let mut optimized_memory = None;
    for iteration in 0..8 {
        let start = Instant::now();
        let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
        for row in y..y + height {
            let index = (row as usize * image.width as usize + x as usize) * 3;
            rgb.extend_from_slice(&image.rgb[index..index + width as usize * 3]);
        }
        // Exact previous production operation order, retained only in this diagnostic.
        let mut baseline = (**image).clone();
        let pause = Instant::now();
        if iteration == 0 {
            baseline_memory = private_bytes();
        }
        let excluded_measurement = pause.elapsed();
        baseline.width = width;
        baseline.height = height;
        baseline.rgb = rgb;
        baseline_ms.push(
            start
                .elapsed()
                .saturating_sub(excluded_measurement)
                .as_secs_f64()
                * 1000.0,
        );
        let start = Instant::now();
        let optimized = source.crop(x, y, width, height)?;
        optimized_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        let DecodedSourceImage::Raw(optimized) = optimized else {
            return Err("RAW crop required".into());
        };
        assert_eq!(baseline, *optimized);
        if iteration == 0 {
            optimized_memory = private_bytes();
        }
    }
    assert_eq!(original_file, std::fs::read(&path)?);
    let percentile = |samples: &mut Vec<f64>, fraction: f64| {
        samples.sort_by(f64::total_cmp);
        samples[((samples.len() as f64 * fraction).ceil() as usize).saturating_sub(1)]
    };
    println!(
        "{}",
        serde_json::json!({
            "scope":"actual LibRaw RAW float crop only; not end-to-end/100MP acceptance",
            "sourceDimensions":[image.width,image.height],"cropDimensions":[width,height],"samples":8,
            "originalFullFrameCloneBytes":image.rgb.len() as u64*4,"optimizedFullFrameCloneBytes":0,
            "baselineMedianMs":percentile(&mut baseline_ms,0.5),"baselineP95Ms":percentile(&mut baseline_ms,0.95),
            "optimizedMedianMs":percentile(&mut optimized_ms,0.5),"optimizedP95Ms":percentile(&mut optimized_ms,0.95),
            "baselinePrivateBytesWithClone":baseline_memory,"optimizedPrivateBytesWithCrop":optimized_memory,
            "outputAndMetadataExact":true,"sourceFileUnchanged":true
        })
    );
    Ok(())
}
