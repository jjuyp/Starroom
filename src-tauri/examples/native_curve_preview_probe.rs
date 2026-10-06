//! Small Native presentation-contract probe, not a photo-render or desktop-latency benchmark.
use serde::Deserialize;
use starroom_color::CurvePoint;
use std::{
    error::Error,
    io::{self, Read},
    time::Instant,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    points: Vec<CurvePoint>,
    sample_count: u16,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut json = String::new();
    io::stdin().take(256 * 1024).read_to_string(&mut json)?;
    let request: Request = serde_json::from_str(&json)?;
    let started = Instant::now();
    let samples =
        starroom_desktop_lib::sample_native_curve_preview(request.points, request.sample_count)?;
    println!(
        "{}",
        serde_json::json!({ "samples": samples,
        "nativeCommandMs": started.elapsed().as_secs_f64() * 1000.0,
        "scope": "actual Native curve command, no photo pixels or UI transport timing" })
    );
    Ok(())
}
