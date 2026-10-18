//! RGB(A)/f32 adaptation of image0.25.10 src/imageops/sample.rs (sinc/lanczos3,
//! vertical_sample/horizontal_sample), commit76e57184f22772dad1138e96954e57945406b15e.
//! Original project: image-rs contributors. MIT OR Apache-2.0; MIT selected below.
//! Changes: unbounded RGB/f32 and bounded opaque RGBA/f32, checked/fallible buffers,
//! shared precomputed axis weights and independent parallel output rows. Each sample's
//! weight/accumulation order and vertical-then-horizontal pass order are unchanged.
//!
//! MIT License
//! Permission is hereby granted, free of charge, to any person obtaining a copy of
//! this software and associated documentation files (the "Software"), to deal in
//! the Software without restriction, including without limitation the rights to
//! use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
//! the Software, and to permit persons to whom the Software is furnished to do so,
//! subject to the following conditions:
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

use crate::ImageIoError;
use rayon::prelude::*;

struct AxisWeights {
    start: usize,
    weights: Vec<f32>,
}

fn allocate(samples: usize) -> Result<Vec<f32>, ImageIoError> {
    let mut data = Vec::new();
    data.try_reserve_exact(samples)
        .map_err(|_| ImageIoError::ResizeAllocation { samples })?;
    data.resize(samples, 0.0);
    Ok(data)
}

fn samples<const CHANNELS: usize>(width: u32, height: u32) -> Result<usize, ImageIoError> {
    if width == 0 || height == 0 {
        return Err(ImageIoError::InvalidResizeDimensions);
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(CHANNELS))
        .ok_or(ImageIoError::InvalidResizeDimensions)
}

fn sinc(t: f32) -> f32 {
    let a = t * std::f32::consts::PI;
    if t == 0.0 { 1.0 } else { a.sin() / a }
}

fn lanczos3(x: f32) -> f32 {
    if x.abs() < 3.0 {
        sinc(x) * sinc(x / 3.0)
    } else {
        0.0
    }
}

fn axis_weights(source: u32, destination: u32) -> Result<Vec<AxisWeights>, ImageIoError> {
    let ratio = source as f32 / destination as f32;
    let sratio = if ratio < 1.0 { 1.0 } else { ratio };
    let support = 3.0 * sratio;
    let mut axis = Vec::new();
    axis.try_reserve_exact(destination as usize)
        .map_err(|_| ImageIoError::ResizeAllocation {
            samples: destination as usize,
        })?;
    for output in 0..destination {
        let center = (output as f32 + 0.5) * ratio;
        let left = ((center - support).floor() as i64).clamp(0, i64::from(source) - 1) as u32;
        let right =
            ((center + support).ceil() as i64).clamp(i64::from(left) + 1, i64::from(source)) as u32;
        let center = center - 0.5;
        let mut weights = allocate((right - left) as usize)?;
        let mut sum = 0.0;
        for (index, weight) in weights.iter_mut().enumerate() {
            *weight = lanczos3(((left + index as u32) as f32 - center) / sratio);
            sum += *weight;
        }
        if !sum.is_finite() || sum == 0.0 {
            return Err(ImageIoError::NonFiniteResize);
        }
        for weight in &mut weights {
            *weight /= sum;
        }
        axis.push(AxisWeights {
            start: left as usize,
            weights,
        });
    }
    Ok(axis)
}

/// Opaque scene-linear RGB only. Encoded RGBA/premultiplication are separate policies.
pub(crate) fn resize_rgb(
    source: &[f32],
    width: u32,
    height: u32,
    new_width: u32,
    new_height: u32,
) -> Result<Vec<f32>, ImageIoError> {
    resize::<3>(source, width, height, new_width, new_height, false)
}

/// The bounded upstream policy is retained for opaque encoded RGB(A) proxies only.
/// Transparent/premultiplied inputs must not enter this adapter.
pub(crate) fn resize_opaque_rgba(
    source: &[f32],
    width: u32,
    height: u32,
    new_width: u32,
    new_height: u32,
) -> Result<Vec<f32>, ImageIoError> {
    resize::<4>(source, width, height, new_width, new_height, true)
}

fn resize<const CHANNELS: usize>(
    source: &[f32],
    width: u32,
    height: u32,
    new_width: u32,
    new_height: u32,
    bounded: bool,
) -> Result<Vec<f32>, ImageIoError> {
    let input_samples = samples::<CHANNELS>(width, height)?;
    let output_samples = samples::<CHANNELS>(new_width, new_height)?;
    if source.len() != input_samples {
        return Err(ImageIoError::InvalidBufferLength);
    }
    if source.iter().any(|value| !value.is_finite()) {
        return Err(ImageIoError::NonFiniteResize);
    }
    if (width, height) == (new_width, new_height) {
        let mut output = allocate(output_samples)?;
        output.copy_from_slice(source);
        return Ok(output);
    }
    let vertical = axis_weights(height, new_height)?;
    let horizontal = axis_weights(width, new_width)?;
    let mut intermediate = allocate(samples::<CHANNELS>(width, new_height)?)?;
    intermediate
        .par_chunks_mut(width as usize * CHANNELS)
        .zip(vertical.par_iter())
        .for_each(|(row, axis)| {
            for (x, pixel) in row.as_chunks_mut::<CHANNELS>().0.iter_mut().enumerate() {
                let mut sum = [0.0; CHANNELS];
                for (index, weight) in axis.weights.iter().enumerate() {
                    let base = ((axis.start + index) * width as usize + x) * CHANNELS;
                    for channel in 0..CHANNELS {
                        sum[channel] += source[base + channel] * weight;
                    }
                }
                pixel.copy_from_slice(&sum);
            }
        });
    let mut output = allocate(output_samples)?;
    output
        .par_chunks_mut(new_width as usize * CHANNELS)
        .enumerate()
        .for_each(|(y, row)| {
            for (pixel, axis) in row
                .as_chunks_mut::<CHANNELS>()
                .0
                .iter_mut()
                .zip(&horizontal)
            {
                let mut sum = [0.0; CHANNELS];
                for (index, weight) in axis.weights.iter().enumerate() {
                    let base = (y * width as usize + axis.start + index) * CHANNELS;
                    for channel in 0..CHANNELS {
                        sum[channel] += intermediate[base + channel] * weight;
                    }
                }
                if bounded {
                    for value in &mut sum {
                        *value = value.clamp(0.0, 1.0);
                    }
                }
                pixel.copy_from_slice(&sum);
            }
        });
    if output.iter().any(|value| !value.is_finite()) {
        return Err(ImageIoError::NonFiniteResize);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb, Rgba, imageops};

    #[test]
    fn opaque_encoded_proxy_matches_upstream_float_lanczos_bits_including_clipping() {
        for (w, h, dw, dh) in [(17, 11, 8, 5), (13, 1, 5, 1), (1, 13, 1, 5), (5, 7, 11, 13)] {
            let source: Vec<f32> = (0..w * h)
                .flat_map(|i| {
                    [
                        if i % 2 == 0 { 0.0 } else { 1.0 },
                        (i * 31 % 97) as f32 / 96.0,
                        0.25,
                        1.0,
                    ]
                })
                .collect();
            let reference = ImageBuffer::<Rgba<f32>, _>::from_raw(w, h, source.as_slice()).unwrap();
            let expected =
                imageops::resize(&reference, dw, dh, imageops::FilterType::Lanczos3).into_raw();
            let actual = resize_opaque_rgba(&source, w, h, dw, dh).unwrap();
            assert!(
                actual
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| a.to_bits() == b.to_bits())
            );
        }
    }

    #[test]
    fn bounded_samples_match_the_pinned_upstream_bits_when_no_clip_is_required() {
        for (width, height, target_width, target_height) in [
            (7, 5, 3, 2),
            (3, 9, 1, 4),
            (9, 1, 5, 1),
            (1, 9, 1, 4),
            (7, 5, 13, 11),
            (7, 5, 7, 5),
        ] {
            let source: Vec<f32> = (0..width * height * 3)
                .map(|i| 0.2 + (i * 31 % 97) as f32 / 200.0)
                .collect();
            let image =
                ImageBuffer::<Rgb<f32>, _>::from_raw(width, height, source.as_slice()).unwrap();
            let expected = imageops::resize(
                &image,
                target_width,
                target_height,
                imageops::FilterType::Lanczos3,
            )
            .into_raw();
            let actual = resize_rgb(&source, width, height, target_width, target_height).unwrap();
            assert_eq!(
                actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn unbounded_linear_headroom_and_negative_values_are_not_clamped() {
        let source = [-0.25, 1.5, 4.0].repeat(64);
        let output = resize_rgb(&source, 8, 8, 2, 2).unwrap();
        for pixel in output.as_chunks::<3>().0 {
            for (actual, expected) in pixel.iter().zip([-0.25, 1.5, 4.0]) {
                assert!((actual - expected).abs() < 2e-6, "{actual} vs {expected}");
            }
        }
        let scaled: Vec<f32> = source.iter().map(|v| v * 4.0).collect();
        let scaled_output = resize_rgb(&scaled, 8, 8, 2, 2).unwrap();
        for (actual, original) in scaled_output.iter().zip(&output) {
            assert_eq!(actual.to_bits(), (original * 4.0).to_bits());
        }
        assert_eq!(source, [-0.25, 1.5, 4.0].repeat(64));
    }

    #[test]
    fn resize_shape_finite_and_identity_errors_are_explicit() {
        for shape in [
            (0, 1, 1, 1),
            (1, 0, 1, 1),
            (1, 1, 0, 1),
            (u32::MAX, u32::MAX, 1, 1),
        ] {
            assert!(resize_rgb(&[], shape.0, shape.1, shape.2, shape.3).is_err());
        }
        assert!(matches!(
            resize_rgb(&[f32::NAN; 3], 1, 1, 1, 1),
            Err(ImageIoError::NonFiniteResize)
        ));
        let source = [-0.0, 1.5, -0.2];
        let output = resize_rgb(&source, 1, 1, 1, 1).unwrap();
        assert!(
            output
                .iter()
                .zip(source)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
    }
}
