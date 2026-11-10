//! Controlled synthetic DNG through real LibRaw; never camera-IQ or photographic acceptance.
#[path = "../tests/support/controlled_dng.rs"]
mod fixture;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture::controlled_fixture()?;
    let start = std::time::Instant::now();
    let decoded = starroom_raw::decode_raw(&fixture.0)?;
    println!(
        "RAW_HEADROOM_PROBE scope=owned-synthetic-CFA-real-LibRaw-not-camera-IQ width={} height={} as_shot={:?} headroom_scale={} decode_ms={:.3}",
        decoded.width,
        decoded.height,
        decoded.metadata.as_shot_multipliers,
        decoded.metadata.wb_headroom_scale,
        start.elapsed().as_secs_f64() * 1000.0
    );
    for column in 0..8 {
        let x = column * 8 + 4;
        let index = ((decoded.height as usize / 2) * decoded.width as usize + x) * 3;
        println!(
            "sensor={} working_rgb={:?}",
            [1024, 2048, 4096, 6144, 8192, 10240, 12288, 14336][column],
            &decoded.rgb[index..index + 3]
        );
    }
    Ok(())
}
