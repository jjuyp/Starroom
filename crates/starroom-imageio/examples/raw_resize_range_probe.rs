//! Diagnostic counterexample only, not a replacement resampler or a passing HDR gate.
use image::{ImageBuffer, Rgb, imageops};

fn main() {
    let source = ImageBuffer::from_pixel(8, 8, Rgb([-0.25_f32, 1.5, 4.0]));
    let output = imageops::resize(&source, 2, 2, imageops::FilterType::Lanczos3);
    println!(
        "PINNED_IMAGE_FLOAT_RESIZE source={:?} resized={:?}",
        source.get_pixel(0, 0).0,
        output.get_pixel(0, 0).0
    );
    assert!(
        output.pixels().all(|p| p.0 == [0.0, 1.0, 1.0]),
        "retain the actual upstream counterexample, not an invented alternate implementation"
    );
    assert!(source.pixels().all(|p| p.0 == [-0.25, 1.5, 4.0]));
}
