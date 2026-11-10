//! Diagnostic only: run once per process, with an immutable licensed RAW fixture.
//! Exact f32 bits/metadata distinguish buffer-ownership optimizations from image changes.
use sha2::{Digest, Sha256};
use starroom_raw::{decode_raw, decode_raw_preview};
use std::{error::Error, fs, path::Path, time::Instant};

#[cfg(windows)]
fn peak_working_set() -> Result<Option<u64>, Box<dyn Error>> {
    use std::os::windows::process::CommandExt;
    // Read this process's OS-maintained high-water mark, not the diagnostic child's.
    // The numeric PID is generated locally; no filename or user text enters the shell.
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("(Get-Process -Id {}).PeakWorkingSet64", std::process::id()),
        ])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW: diagnostic console only.
        .output()?;
    if !output.status.success() {
        return Err("Windows process peak query failed".into());
    }
    Ok(Some(String::from_utf8(output.stdout)?.trim().parse()?))
}

#[cfg(not(windows))]
fn peak_working_set() -> Result<Option<u64>, Box<dyn Error>> {
    Ok(None)
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 2 {
        return Err("expected immutable RAW fixture path and full|half".into());
    }
    let path = Path::new(&arguments[0]);
    let half = match arguments[1].to_str() {
        Some("full") => false,
        Some("half") => true,
        _ => return Err("decode mode must be full or half".into()),
    };
    let input_hash = format!("{:x}", Sha256::digest(fs::read(path)?));
    let started = Instant::now();
    let image = if half {
        decode_raw_preview(path)?
    } else {
        decode_raw(path)?
    };
    let decode_ms = started.elapsed().as_secs_f64() * 1_000.0;
    // Query before hashing/validating output so probe validation allocations are excluded.
    let peak_bytes = peak_working_set()?;
    let mut hash = Sha256::new();
    let mut block = [0_u8; 4096];
    for samples in image.rgb.chunks(1024) {
        for (sample, bytes) in samples.iter().zip(block.as_chunks_mut::<4>().0) {
            if !sample.is_finite() {
                return Err("non-finite RAW output".into());
            }
            bytes.copy_from_slice(&sample.to_bits().to_le_bytes());
        }
        hash.update(&block[..samples.len() * 4]);
    }
    let metadata_hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&image.metadata)?));
    if input_hash != format!("{:x}", Sha256::digest(fs::read(path)?)) {
        return Err("RAW source changed during read-only probe".into());
    }
    println!(
        "{}",
        serde_json::json!({
            "schemaVersion": 1, "mode": if half { "half" } else { "full" },
            "sourceSha256": input_hash, "width": image.width, "height": image.height,
            "sampleCount": image.rgb.len(), "rgbF32BitsSha256": format!("{:x}", hash.finalize()),
            "metadataSha256": metadata_hash, "decodeWallMs": decode_ms,
            "peakWorkingSetBeforeProbeValidationBytes": peak_bytes,
            "timings": image.timings, "sourceImmutable": true,
            "scope": "LibRaw sensor decode + camera transform; not photographic quality or UI latency"
        })
    );
    Ok(())
}
