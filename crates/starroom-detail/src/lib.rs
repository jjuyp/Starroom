//! CPU reference detail engine for Starroom.
//! Production preview/export will move these operations to tiled GPU kernels, but the GPU must
//! remain numerically close to these deterministic references.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, PartialEq)]
pub struct LinearImage {
    pub width: usize,
    pub height: usize,
    /// Interleaved RGB linear-light pixels.
    pub data: Vec<f32>,
}

impl LinearImage {
    pub fn new(width: usize, height: usize, data: Vec<f32>) -> Result<Self, DetailError> {
        if data.len() != width.saturating_mul(height).saturating_mul(3) {
            return Err(DetailError::InvalidBufferLength);
        }
        if data.iter().any(|value| !value.is_finite()) {
            return Err(DetailError::NonFiniteInput);
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    // Retained only for the frozen pre-optimization serial oracle.
    #[cfg(test)]
    fn sample(&self, x: isize, y: isize, channel: usize) -> f32 {
        let safe_x = x.clamp(0, self.width.saturating_sub(1) as isize) as usize;
        let safe_y = y.clamp(0, self.height.saturating_sub(1) as isize) as usize;
        self.data[(safe_y * self.width + safe_x) * 3 + channel]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharpenParameters {
    /// 0..1
    pub amount: f32,
    /// Gaussian radius in pixels at full resolution.
    pub radius: f32,
    /// Fine/coarse detail emphasis, 0..1.
    #[serde(default = "default_half")]
    pub detail: f32,
    /// Edge masking, 0 sharpens all frequencies, 1 protects flat regions.
    #[serde(default)]
    pub masking: f32,
    /// Limits overshoot against the local luminance range.
    #[serde(default = "default_halo")]
    pub halo_protection: f32,
    /// Legacy minimum local difference, retained for sidecar compatibility.
    #[serde(default = "default_threshold")]
    pub threshold: f32,
}

const fn default_half() -> f32 {
    0.5
}
const fn default_halo() -> f32 {
    0.75
}
const fn default_threshold() -> f32 {
    0.002
}

impl Default for SharpenParameters {
    fn default() -> Self {
        Self {
            amount: 0.35,
            radius: 1.0,
            detail: default_half(),
            masking: 0.0,
            halo_protection: default_halo(),
            threshold: default_threshold(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DenoiseParameters {
    /// 0..1 luminance smoothing strength.
    pub luminance: f32,
    /// 0..1 chroma smoothing strength.
    pub chroma: f32,
    pub radius: f32,
    /// Preserves high-frequency luminance edges, 0..1.
    #[serde(default = "default_half")]
    pub detail_protection: f32,
    /// Additional sensor-noise strength multiplier, 0..1.
    #[serde(default)]
    pub high_iso: f32,
}

impl Default for DenoiseParameters {
    fn default() -> Self {
        Self {
            luminance: 0.0,
            chroma: 0.0,
            radius: 1.25,
            detail_protection: default_half(),
            high_iso: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalDetailParameters {
    /// Fine spatial-frequency contrast, -1..1.
    pub texture: f32,
    /// Mid-frequency local contrast, -1..1.
    pub clarity: f32,
    /// Broad haze/veil removal, -1..1.
    pub dehaze: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailError {
    InvalidBufferLength,
    NonFiniteInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetailCancelled;

/// Shared normalized f32 coefficients for CPU and parity-checked GPU backends.
pub fn gaussian_kernel(radius: f32) -> Vec<f32> {
    let sigma = radius.max(0.25);
    let half = (sigma * 3.0).ceil().clamp(1.0, 24.0) as isize;
    let mut kernel = Vec::with_capacity((half * 2 + 1) as usize);
    let mut sum = 0.0;
    for offset in -half..=half {
        let x = offset as f32;
        let value = (-0.5 * (x / sigma).powi(2)).exp();
        kernel.push(value);
        sum += value;
    }
    for value in &mut kernel {
        *value /= sum.max(f32::EPSILON);
    }
    kernel
}

pub fn gaussian_blur(image: &LinearImage, radius: f32) -> LinearImage {
    if image.width == 0 || image.height == 0 {
        return image.clone();
    }
    let kernel = gaussian_kernel(radius);
    let half = (kernel.len() / 2) as isize;
    let mut horizontal = vec![0.0; image.data.len()];
    horizontal
        .par_chunks_mut(image.width * 3)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, pixel) in row.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                let mut value = [0.0; 3];
                for (index, weight) in kernel.iter().enumerate() {
                    let offset = index as isize - half;
                    let source_x =
                        (x as isize + offset).clamp(0, image.width as isize - 1) as usize;
                    let base = (y * image.width + source_x) * 3;
                    for (channel, value) in value.iter_mut().enumerate() {
                        *value += image.data[base + channel] * weight;
                    }
                }
                pixel.copy_from_slice(&value);
            }
        });

    let horizontal_image = LinearImage {
        width: image.width,
        height: image.height,
        data: horizontal,
    };
    let mut output = vec![0.0; image.data.len()];
    output
        .par_chunks_mut(image.width * 3)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, pixel) in row.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                let mut value = [0.0; 3];
                for (index, weight) in kernel.iter().enumerate() {
                    let offset = index as isize - half;
                    let source_y =
                        (y as isize + offset).clamp(0, image.height as isize - 1) as usize;
                    let base = (source_y * image.width + x) * 3;
                    for (channel, value) in value.iter_mut().enumerate() {
                        *value += horizontal_image.data[base + channel] * weight;
                    }
                }
                pixel.copy_from_slice(&value);
            }
        });
    LinearImage {
        width: image.width,
        height: image.height,
        data: output,
    }
}

/// O(n) summed-area broad blur used by Dehaze; avoids a very large Gaussian kernel on full-size
/// RAW exports while remaining deterministic across Preview and Export.
fn box_blur(image: &LinearImage, radius: usize) -> LinearImage {
    if image.width == 0 || image.height == 0 || radius == 0 {
        return image.clone();
    }
    let stride = image.width + 1;
    let mut output = vec![0.0; image.data.len()];
    for channel in 0..3 {
        let mut integral = vec![0.0_f64; (image.width + 1) * (image.height + 1)];
        for y in 0..image.height {
            let mut row = 0.0_f64;
            for x in 0..image.width {
                row += image.data[(y * image.width + x) * 3 + channel] as f64;
                integral[(y + 1) * stride + x + 1] = integral[y * stride + x + 1] + row;
            }
        }
        output
            .par_chunks_mut(image.width * 3)
            .enumerate()
            .for_each(|(y, row)| {
                for (x, pixel) in row.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                    let x0 = x.saturating_sub(radius);
                    let y0 = y.saturating_sub(radius);
                    let x1 = (x + radius + 1).min(image.width);
                    let y1 = (y + radius + 1).min(image.height);
                    let sum = integral[y1 * stride + x1]
                        - integral[y0 * stride + x1]
                        - integral[y1 * stride + x0]
                        + integral[y0 * stride + x0];
                    pixel[channel] = (sum / ((x1 - x0) * (y1 - y0)) as f64) as f32;
                }
            });
    }
    LinearImage {
        width: image.width,
        height: image.height,
        data: output,
    }
}

pub fn sharpen(image: &LinearImage, parameters: SharpenParameters) -> LinearImage {
    sharpen_owned(image.clone(), parameters)
}

/// Consume the mutable working frame; inactive stages and pointwise output retain its allocation.
pub fn sharpen_owned(image: LinearImage, parameters: SharpenParameters) -> LinearImage {
    let amount = parameters.amount.clamp(0.0, 2.0);
    if amount <= f32::EPSILON {
        return image;
    }
    let fine = gaussian_blur(&image, parameters.radius.clamp(0.3, 4.0));
    let coarse = gaussian_blur(&image, (parameters.radius * 2.2).clamp(0.8, 8.0));
    let threshold = parameters.threshold.max(0.0);
    let mut output = image;
    let detail_mix = parameters.detail.clamp(0.0, 1.0);
    let masking = parameters.masking.clamp(0.0, 1.0);
    let halo = parameters.halo_protection.clamp(0.0, 1.0);
    output
        .data
        .par_chunks_mut(3)
        .enumerate()
        .for_each(|(pixel, output_pixel)| {
            let base = pixel * 3;
            let (source_y, _, _) = rgb_to_ycbcr(output_pixel[0], output_pixel[1], output_pixel[2]);
            let (fine_y, _, _) =
                rgb_to_ycbcr(fine.data[base], fine.data[base + 1], fine.data[base + 2]);
            let (coarse_y, _, _) = rgb_to_ycbcr(
                coarse.data[base],
                coarse.data[base + 1],
                coarse.data[base + 2],
            );
            let fine_detail = source_y - fine_y;
            let coarse_detail = fine_y - coarse_y;
            let detail_y =
                fine_detail * (0.55 + detail_mix * 0.9) + coarse_detail * (1.0 - detail_mix) * 0.35;
            let edge = (source_y - coarse_y).abs();
            let edge_mask = if masking <= f32::EPSILON {
                1.0
            } else {
                smoothstep(threshold, threshold + 0.03 + masking * 0.08, edge)
            };
            let local_range = (fine_y - coarse_y).abs() + fine_detail.abs() + 0.002;
            let limit = local_range * (2.2 - halo * 1.6);
            let delta_y = (detail_y * amount * edge_mask).clamp(-limit, limit);
            let scale = if source_y.abs() > 1.0e-7 {
                (source_y + delta_y) / source_y
            } else {
                1.0
            };
            for output_channel in output_pixel.iter_mut() {
                *output_channel *= scale;
            }
        });
    output
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0).max(1.0e-7)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn rgb_to_ycbcr(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let y = 0.2627 * r + 0.6780 * g + 0.0593 * b;
    let cb = b - y;
    let cr = r - y;
    (y, cb, cr)
}

fn ycbcr_to_rgb(y: f32, cb: f32, cr: f32) -> (f32, f32, f32) {
    let r = y + cr;
    let b = y + cb;
    let g = (y - 0.2627 * r - 0.0593 * b) / 0.6780;
    (r, g, b)
}

/// Separates luminance from chroma so chroma noise can be smoothed more strongly without
/// smearing luminance detail. This is a classic deterministic NR path, not AI denoise.
pub fn denoise(image: &LinearImage, parameters: DenoiseParameters) -> LinearImage {
    denoise_owned(image.clone(), parameters)
}

/// Immutable neighbour components stay separate; output reuses the consumed RGB allocation.
pub fn denoise_owned(image: LinearImage, parameters: DenoiseParameters) -> LinearImage {
    denoise_owned_impl(image, parameters, &|| false)
}

/// Request cancellation never exposes a partially processed image. Uncancelled math and
/// allocation ownership are identical to `denoise_owned`; no process-global token is used.
pub fn denoise_owned_cancellable(
    image: LinearImage,
    parameters: DenoiseParameters,
    cancelled: &AtomicBool,
) -> Result<LinearImage, DetailCancelled> {
    try_denoise_owned(image, parameters, &|| cancelled.load(Ordering::Acquire))
}

fn try_denoise_owned(
    image: LinearImage,
    parameters: DenoiseParameters,
    requested: &(impl Fn() -> bool + Sync),
) -> Result<LinearImage, DetailCancelled> {
    let observed = AtomicBool::new(false);
    let probe = || {
        if requested() {
            observed.store(true, Ordering::Release);
            true
        } else {
            observed.load(Ordering::Acquire)
        }
    };
    let output = denoise_owned_impl(image, parameters, &probe);
    if probe() {
        Err(DetailCancelled)
    } else {
        Ok(output)
    }
}

fn denoise_owned_impl(
    image: LinearImage,
    parameters: DenoiseParameters,
    cancelled: &(impl Fn() -> bool + Sync),
) -> LinearImage {
    if cancelled() {
        return image;
    }
    let iso_boost = parameters.high_iso.clamp(0.0, 1.0);
    let luminance_strength = (parameters.luminance + iso_boost * 0.35).clamp(0.0, 1.0);
    let chroma_strength = (parameters.chroma + iso_boost * 0.55).clamp(0.0, 1.0);
    if luminance_strength <= f32::EPSILON && chroma_strength <= f32::EPSILON {
        return image;
    }
    if image.width == 0 || image.height == 0 {
        return image;
    }

    let mut components = image.data.clone();
    components.par_chunks_mut(image.width * 3).for_each(|row| {
        if cancelled() {
            return;
        }
        for pixel in row.as_chunks_mut::<3>().0 {
            let (y, cb, cr) = rgb_to_ycbcr(pixel[0], pixel[1], pixel[2]);
            pixel.copy_from_slice(&[y, cb, cr]);
        }
    });
    if cancelled() {
        return image;
    }
    let component_image = LinearImage {
        width: image.width,
        height: image.height,
        data: components,
    };
    // Edge-aware range weighting prevents the simple Gaussian smear of the prototype. Chroma
    // uses the same accepted neighbours but remains independently controllable.
    let spatial_radius = parameters.radius.clamp(0.6, 4.0);
    let half = (spatial_radius * 2.0).ceil() as isize;
    let detail_protection = parameters.detail_protection.clamp(0.0, 1.0);
    let range_sigma = 0.018 + (1.0 - detail_protection) * 0.12 + iso_boost * 0.08;
    // Same f32 weights and dy/dx order as the serial path, evaluated once per frame.
    let mut spatial_kernel = Vec::new();
    for dy in -half..=half {
        for dx in -half..=half {
            let spatial =
                (-0.5 * ((dx * dx + dy * dy) as f32) / (spatial_radius * spatial_radius)).exp();
            spatial_kernel.push((dx, dy, spatial));
        }
    }
    // Clamp each column/row once, not once per accepted neighbour of every pixel.
    // The maximum radius is4 ->17 samples per axis. Only addresses are hoisted;
    // dy/dx accumulation order, all range exponentials and f32 math stay unchanged.
    let diameter = (half * 2 + 1) as usize;
    let columns: Vec<[usize; 17]> = (0..image.width)
        .map(|x| {
            std::array::from_fn(|index| {
                (x as isize + index as isize - half).clamp(0, image.width as isize - 1) as usize * 3
            })
        })
        .collect();
    let mut output = image;
    output
        .data
        .par_chunks_mut(component_image.width.max(1) * 3)
        .enumerate()
        .for_each(|(y_index, output_row)| {
            if cancelled() {
                return;
            }
            let rows: [usize; 17] = std::array::from_fn(|index| {
                (y_index as isize + index as isize - half)
                    .clamp(0, component_image.height as isize - 1) as usize
                    * component_image.width
                    * 3
            });
            for (block_index, block) in output_row.chunks_mut(64 * 3).enumerate() {
                if cancelled() {
                    return;
                }
                for (offset, output_pixel) in block.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                    let x_index = block_index * 64 + offset;
                    let pixel_index = y_index * component_image.width + x_index;
                    let base = pixel_index * 3;
                    let original_y = component_image.data[base];
                    let original_cb = component_image.data[base + 1];
                    let original_cr = component_image.data[base + 2];
                    let mut sum = [0.0_f32; 3];
                    let mut weight_sum = 0.0;
                    for (row, weights) in rows[..diameter]
                        .iter()
                        .zip(spatial_kernel.chunks_exact(diameter))
                    {
                        for (column, &(_, _, spatial)) in
                            columns[x_index][..diameter].iter().zip(weights)
                        {
                            let source_base = row + column;
                            let sample_y = component_image.data[source_base];
                            let range =
                                (-0.5 * ((sample_y - original_y) / range_sigma).powi(2)).exp();
                            let weight = spatial * range;
                            for (channel, accumulated) in sum.iter_mut().enumerate() {
                                *accumulated +=
                                    component_image.data[source_base + channel] * weight;
                            }
                            weight_sum += weight;
                        }
                    }
                    let filtered = sum.map(|value| value / weight_sum.max(f32::EPSILON));
                    let y = original_y
                        + (filtered[0] - original_y)
                            * luminance_strength
                            * (1.0 - detail_protection * 0.7);
                    let cb = original_cb + (filtered[1] - original_cb) * chroma_strength;
                    let cr = original_cr + (filtered[2] - original_cr) * chroma_strength;
                    let (r, g, b) = ycbcr_to_rgb(y, cb, cr);
                    output_pixel.copy_from_slice(&[r, g, b]);
                }
            }
        });
    output
}

/// Three genuinely separate spatial-frequency controls: Texture uses the fine residual,
/// Clarity the mid-frequency residual, and Dehaze a broad luminance veil estimate.
pub fn local_detail(image: &LinearImage, parameters: LocalDetailParameters) -> LinearImage {
    local_detail_owned(image.clone(), parameters)
}

/// Neighbour-filter inputs remain immutable; only each independent final pixel is updated in place.
pub fn local_detail_owned(image: LinearImage, parameters: LocalDetailParameters) -> LinearImage {
    let texture = parameters.texture.clamp(-1.0, 1.0);
    let clarity = parameters.clarity.clamp(-1.0, 1.0);
    let dehaze = parameters.dehaze.clamp(-1.0, 1.0);
    if texture.abs() <= f32::EPSILON
        && clarity.abs() <= f32::EPSILON
        && dehaze.abs() <= f32::EPSILON
    {
        return image;
    }
    let fine = gaussian_blur(&image, 0.8);
    let medium = gaussian_blur(&image, 3.0);
    let broad = box_blur(&image, 12);
    let mut output = image;
    output
        .data
        .par_chunks_mut(3)
        .enumerate()
        .for_each(|(pixel, output_pixel)| {
            let base = pixel * 3;
            let y = rgb_to_ycbcr(output_pixel[0], output_pixel[1], output_pixel[2]).0;
            let fine_y = rgb_to_ycbcr(fine.data[base], fine.data[base + 1], fine.data[base + 2]).0;
            let medium_y = rgb_to_ycbcr(
                medium.data[base],
                medium.data[base + 1],
                medium.data[base + 2],
            )
            .0;
            let broad_y =
                rgb_to_ycbcr(broad.data[base], broad.data[base + 1], broad.data[base + 2]).0;
            let target = y
                + (y - fine_y) * texture * 0.8
                + (fine_y - medium_y) * clarity * 1.15
                + (y - broad_y) * dehaze * 0.7
                + dehaze * (0.18 - broad_y) * 0.10;
            let scale = if y.abs() > 1.0e-7 {
                target.max(0.0) / y
            } else {
                1.0
            };
            for output_channel in output_pixel.iter_mut() {
                *output_channel *= scale;
            }
        });
    output
}

#[cfg(test)]
#[path = "../tests/oracles/serial_detail_v1.rs"]
mod serial_reference;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellable_denoise_retains_allocation_and_frozen_serial_bits() {
        for threads in [1, 2, 4] {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap()
                .install(|| {
                    for (width, height) in
                        [(0, 0), (0, 5), (5, 0), (1, 1), (1, 7), (7, 1), (37, 23)]
                    {
                        let source = LinearImage::new(
                            width,
                            height,
                            (0..width * height * 3)
                                .map(|index| [-0.0, 0.0, -0.25, 0.01, 0.18, 1.0, 4.0][index % 7])
                                .collect(),
                        )
                        .unwrap();
                        for radius in [0.6, 1.25, 4.0] {
                            for (luminance, chroma) in
                                [(0.0, 0.0), (0.5, 0.0), (0.0, 0.5), (1.0, 1.0)]
                            {
                                let parameters = DenoiseParameters {
                                    radius,
                                    luminance,
                                    chroma,
                                    ..Default::default()
                                };
                                let input = source.clone();
                                let pointer = input.data.as_ptr();
                                let capacity = input.data.capacity();
                                let actual = denoise_owned_cancellable(
                                    input,
                                    parameters,
                                    &AtomicBool::new(false),
                                )
                                .unwrap();
                                let expected = serial_reference::denoise(&source, parameters);
                                assert_eq!(actual.data.as_ptr(), pointer);
                                assert_eq!(actual.data.capacity(), capacity);
                                assert_eq!(
                                    actual.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                                    expected
                                        .data
                                        .iter()
                                        .map(|v| v.to_bits())
                                        .collect::<Vec<_>>()
                                );
                            }
                        }
                    }
                });
        }
    }

    #[test]
    fn denoise_cancellation_is_latched_during_conversion_and_filtering_without_partial_success() {
        use std::sync::atomic::AtomicUsize;
        let source = LinearImage::new(
            65,
            9,
            (0..65 * 9 * 3)
                .map(|index| (index % 97) as f32 / 31.0 - 0.2)
                .collect(),
        )
        .unwrap();
        let original = source.clone();
        let parameters = DenoiseParameters {
            luminance: 0.7,
            chroma: 0.8,
            ..Default::default()
        };
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        pool.install(|| {
            // Deterministic checkpoints in entry, conversion and the actual NR loop.
            // Request returns false again afterward: observed cancellation must still latch.
            for trigger in [1, 4, 30] {
                let calls = AtomicUsize::new(0);
                let result = try_denoise_owned(source.clone(), parameters, &|| {
                    calls.fetch_add(1, Ordering::AcqRel) + 1 == trigger
                });
                assert_eq!(result, Err(DetailCancelled));
                assert!(calls.load(Ordering::Acquire) >= trigger);
            }
        });
        assert_eq!(
            denoise_owned_cancellable(source.clone(), parameters, &AtomicBool::new(true)),
            Err(DetailCancelled)
        );
        let subsequent =
            denoise_owned_cancellable(source.clone(), parameters, &AtomicBool::new(false)).unwrap();
        assert_eq!(subsequent, serial_reference::denoise(&source, parameters));
        assert_eq!(source, original);
    }

    #[test]
    fn owned_detail_stages_retain_the_same_allocation_and_exact_serial_pixels() {
        let source = LinearImage::new(
            31,
            19,
            (0..31 * 19 * 3)
                .map(|i| ((i * 97 % 1021) as f32 - 50.0) / 317.0)
                .collect(),
        )
        .unwrap();
        for amount in [0.0, 0.5, 1.0] {
            let mut image = source.clone();
            let pointer = image.data.as_ptr();
            let capacity = image.data.capacity();
            let mut expected = source.clone();
            let noise = DenoiseParameters {
                luminance: amount,
                chroma: amount,
                ..Default::default()
            };
            image = denoise_owned(image, noise);
            expected = serial_reference::denoise(&expected, noise);
            assert_eq!(image.data.as_ptr(), pointer);
            assert_eq!(image.data.capacity(), capacity);
            assert_eq!(
                image.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                expected
                    .data
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>()
            );
            let detail = LocalDetailParameters {
                texture: amount,
                clarity: -amount,
                dehaze: amount,
            };
            image = local_detail_owned(image, detail);
            expected = serial_reference::local_detail(&expected, detail);
            assert_eq!(image.data.as_ptr(), pointer);
            assert_eq!(image.data.capacity(), capacity);
            assert_eq!(
                image.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                expected
                    .data
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>()
            );
            let sharpen = SharpenParameters {
                amount,
                ..Default::default()
            };
            image = sharpen_owned(image, sharpen);
            expected = serial_reference::sharpen(&expected, sharpen);
            assert_eq!(image.data.as_ptr(), pointer);
            assert_eq!(image.data.capacity(), capacity);
            assert_eq!(
                image.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                expected
                    .data
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn denoise_hoisted_addresses_preserve_serial_bits_at_all_borders_and_signed_zero() {
        for (width, height) in [(0, 7), (7, 0), (1, 1), (1, 19), (19, 1), (3, 2), (17, 17)] {
            let values = [-0.0, 0.0, -0.25, 0.18, 1.0, 4.0, -8000.0, 8000.0];
            let image = LinearImage::new(
                width,
                height,
                (0..width * height * 3)
                    .map(|i| values[(i * 13 + i / 3) % values.len()])
                    .collect(),
            )
            .unwrap();
            let original: Vec<_> = image.data.iter().map(|v| v.to_bits()).collect();
            for radius in [0.6, 1.25, 4.0] {
                for (luminance, chroma) in [(0.5, 0.0), (0.0, 0.5), (1.0, 1.0)] {
                    let parameters = DenoiseParameters {
                        radius,
                        luminance,
                        chroma,
                        ..Default::default()
                    };
                    let actual = denoise(&image, parameters);
                    let expected = serial_reference::denoise(&image, parameters);
                    assert!(actual.data.iter().all(|v| v.is_finite()));
                    assert_eq!(
                        actual.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                        expected
                            .data
                            .iter()
                            .map(|v| v.to_bits())
                            .collect::<Vec<_>>(),
                        "{width}x{height} radius{radius} luminance{luminance} chroma{chroma}"
                    );
                }
            }
            assert_eq!(
                image.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                original
            );
        }
    }

    #[test]
    fn parallel_detail_matches_frozen_serial_bits_for_hdr_edges_and_thread_counts() {
        fn same(actual: LinearImage, expected: LinearImage) {
            assert_eq!(
                (actual.width, actual.height),
                (expected.width, expected.height)
            );
            assert_eq!(actual.data.len(), expected.data.len());
            for (index, (a, b)) in actual.data.iter().zip(&expected.data).enumerate() {
                assert_eq!(a.to_bits(), b.to_bits(), "sample {index}: {a} vs {b}");
            }
        }
        for threads in [1, 2, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            pool.install(|| {
                for (width, height) in [(0, 0), (0, 5), (5, 0), (1, 1), (1, 9), (9, 1), (37, 23)] {
                    let data = (0..width * height * 3)
                        .map(|i| ((i * 97 % 1021) as f32 - 50.0) / 317.0)
                        .collect();
                    let image = LinearImage::new(width, height, data).unwrap();
                    let original = image.clone();
                    for radius in [0.0, 0.3, 1.0, 4.0, 8.0] {
                        same(
                            gaussian_blur(&image, radius),
                            serial_reference::gaussian_blur(&image, radius),
                        );
                    }
                    for amount in [0.0, 0.35, 2.0] {
                        let parameters = SharpenParameters {
                            amount,
                            radius: 2.3,
                            detail: 0.7,
                            masking: 0.5,
                            halo_protection: 0.9,
                            ..Default::default()
                        };
                        same(
                            sharpen(&image, parameters),
                            serial_reference::sharpen(&image, parameters),
                        );
                    }
                    for radius in [0.6, 1.7, 4.0] {
                        for (luminance, chroma, high_iso, detail_protection) in [
                            (0.0, 0.0, 0.0, 0.0),
                            (0.4, 0.0, 0.0, 0.8),
                            (0.0, 0.4, 0.0, 0.5),
                            (1.0, 1.0, 1.0, 1.0),
                            (-0.5, 2.0, 0.3, 0.0),
                        ] {
                            let parameters = DenoiseParameters {
                                luminance,
                                chroma,
                                high_iso,
                                radius,
                                detail_protection,
                            };
                            same(
                                denoise(&image, parameters),
                                serial_reference::denoise(&image, parameters),
                            );
                        }
                    }
                    for (texture, clarity, dehaze) in [
                        (0.0, 0.0, 0.0),
                        (1.0, 0.0, 0.0),
                        (0.0, 1.0, 0.0),
                        (0.0, 0.0, 1.0),
                        (-1.0, 1.0, -0.7),
                    ] {
                        let parameters = LocalDetailParameters {
                            texture,
                            clarity,
                            dehaze,
                        };
                        same(
                            local_detail(&image, parameters),
                            serial_reference::local_detail(&image, parameters),
                        );
                    }
                    assert_eq!(image, original);
                }
            });
        }
    }

    fn fixture() -> LinearImage {
        LinearImage::new(
            5,
            1,
            vec![
                0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.8, 0.8, 0.8, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1,
            ],
        )
        .expect("fixture")
    }

    #[test]
    fn zero_sharpen_is_identity() {
        let image = fixture();
        let output = sharpen(
            &image,
            SharpenParameters {
                amount: 0.0,
                ..Default::default()
            },
        );
        assert_eq!(output, image);
    }

    #[test]
    fn sharpen_increases_edge_peak() {
        let image = fixture();
        let output = sharpen(
            &image,
            SharpenParameters {
                amount: 0.8,
                ..Default::default()
            },
        );
        assert!(output.data[6] > image.data[6]);
    }

    #[test]
    fn zero_denoise_is_identity() {
        let image = fixture();
        assert_eq!(denoise(&image, DenoiseParameters::default()), image);
    }

    #[test]
    fn chroma_denoise_reduces_color_spike() {
        let image = LinearImage::new(3, 1, vec![0.2, 0.2, 0.2, 0.8, 0.1, 0.1, 0.2, 0.2, 0.2])
            .expect("fixture");
        let output = denoise(
            &image,
            DenoiseParameters {
                luminance: 0.0,
                chroma: 1.0,
                radius: 1.0,
                ..Default::default()
            },
        );
        let original_chroma = (image.data[3] - image.data[4]).abs();
        let output_chroma = (output.data[3] - output.data[4]).abs();
        assert!(output_chroma < original_chroma);
    }

    #[test]
    fn m9_detail_controls_are_not_aliases_of_one_filter() {
        let image = LinearImage::new(
            9,
            1,
            (0..9)
                .flat_map(|index: usize| {
                    let base = if index < 4 { 0.12 } else { 0.62 };
                    let noise = if index.is_multiple_of(2) {
                        0.025
                    } else {
                        -0.025
                    };
                    [base + noise, base, base - noise]
                })
                .collect(),
        )
        .expect("fixture");
        let texture = local_detail(
            &image,
            LocalDetailParameters {
                texture: 1.0,
                ..Default::default()
            },
        );
        let clarity = local_detail(
            &image,
            LocalDetailParameters {
                clarity: 1.0,
                ..Default::default()
            },
        );
        let haze = local_detail(
            &image,
            LocalDetailParameters {
                dehaze: 1.0,
                ..Default::default()
            },
        );
        assert_ne!(texture.data, clarity.data);
        assert_ne!(clarity.data, haze.data);
        assert_ne!(texture.data, haze.data);
    }

    #[test]
    fn m9_sharpen_masking_and_halo_protection_bound_overshoot() {
        let image = fixture();
        let loose = sharpen(
            &image,
            SharpenParameters {
                amount: 2.0,
                masking: 0.0,
                halo_protection: 0.0,
                ..Default::default()
            },
        );
        let protected = sharpen(
            &image,
            SharpenParameters {
                amount: 2.0,
                masking: 0.8,
                halo_protection: 1.0,
                ..Default::default()
            },
        );
        assert!(protected.data[6] - image.data[6] < loose.data[6] - image.data[6]);
    }

    #[test]
    fn m9_high_iso_denoise_reduces_noise_but_preserves_step_edge() {
        let image = LinearImage::new(
            7,
            1,
            vec![
                0.08, 0.12, 0.07, 0.14, 0.09, 0.13, 0.09, 0.13, 0.08, 0.75, 0.72, 0.78, 0.69, 0.74,
                0.71, 0.77, 0.70, 0.75, 0.72, 0.76, 0.70,
            ],
        )
        .expect("fixture");
        let output = denoise(
            &image,
            DenoiseParameters {
                luminance: 0.7,
                chroma: 0.9,
                radius: 1.5,
                detail_protection: 0.8,
                high_iso: 1.0,
            },
        );
        let left_span = (output.data[0] - output.data[3]).abs();
        assert!(left_span < (image.data[0] - image.data[3]).abs());
        assert!(output.data[9] - output.data[6] > 0.4);
        assert!(output.data.iter().all(|value| value.is_finite()));
    }
}
