// Frozen serial detail math from d14c979 parent, before scheduling optimization.
// Test-only oracle. Keep arithmetic, neighbor order, accumulation precision and edge semantics.
use crate::{DenoiseParameters, LinearImage, LocalDetailParameters, SharpenParameters};

fn gaussian_kernel(radius: f32) -> Vec<f32> {
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
    for y in 0..image.height {
        for x in 0..image.width {
            for channel in 0..3 {
                let mut value = 0.0;
                for (index, weight) in kernel.iter().enumerate() {
                    let offset = index as isize - half;
                    value += image.sample(x as isize + offset, y as isize, channel) * weight;
                }
                horizontal[(y * image.width + x) * 3 + channel] = value;
            }
        }
    }

    let horizontal_image = LinearImage {
        width: image.width,
        height: image.height,
        data: horizontal,
    };
    let mut output = vec![0.0; image.data.len()];
    for y in 0..image.height {
        for x in 0..image.width {
            for channel in 0..3 {
                let mut value = 0.0;
                for (index, weight) in kernel.iter().enumerate() {
                    let offset = index as isize - half;
                    value +=
                        horizontal_image.sample(x as isize, y as isize + offset, channel) * weight;
                }
                output[(y * image.width + x) * 3 + channel] = value;
            }
        }
    }
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
        for y in 0..image.height {
            for x in 0..image.width {
                let x0 = x.saturating_sub(radius);
                let y0 = y.saturating_sub(radius);
                let x1 = (x + radius + 1).min(image.width);
                let y1 = (y + radius + 1).min(image.height);
                let sum = integral[y1 * stride + x1]
                    - integral[y0 * stride + x1]
                    - integral[y1 * stride + x0]
                    + integral[y0 * stride + x0];
                output[(y * image.width + x) * 3 + channel] =
                    (sum / ((x1 - x0) * (y1 - y0)) as f64) as f32;
            }
        }
    }
    LinearImage {
        width: image.width,
        height: image.height,
        data: output,
    }
}

pub fn sharpen(image: &LinearImage, parameters: SharpenParameters) -> LinearImage {
    let amount = parameters.amount.clamp(0.0, 2.0);
    if amount <= f32::EPSILON {
        return image.clone();
    }
    let fine = gaussian_blur(image, parameters.radius.clamp(0.3, 4.0));
    let coarse = gaussian_blur(image, (parameters.radius * 2.2).clamp(0.8, 8.0));
    let threshold = parameters.threshold.max(0.0);
    let mut output = image.clone();
    let detail_mix = parameters.detail.clamp(0.0, 1.0);
    let masking = parameters.masking.clamp(0.0, 1.0);
    let halo = parameters.halo_protection.clamp(0.0, 1.0);
    for pixel in 0..image.width.saturating_mul(image.height) {
        let base = pixel * 3;
        let (source_y, _, _) =
            rgb_to_ycbcr(image.data[base], image.data[base + 1], image.data[base + 2]);
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
        for channel in 0..3 {
            output.data[base + channel] = image.data[base + channel] * scale;
        }
    }
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
    let iso_boost = parameters.high_iso.clamp(0.0, 1.0);
    let luminance_strength = (parameters.luminance + iso_boost * 0.35).clamp(0.0, 1.0);
    let chroma_strength = (parameters.chroma + iso_boost * 0.55).clamp(0.0, 1.0);
    if luminance_strength <= f32::EPSILON && chroma_strength <= f32::EPSILON {
        return image.clone();
    }

    let mut components = Vec::with_capacity(image.data.len());
    for pixel in image.data.as_chunks::<3>().0 {
        let (y, cb, cr) = rgb_to_ycbcr(pixel[0], pixel[1], pixel[2]);
        components.extend_from_slice(&[y, cb, cr]);
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
    let mut output = image.clone();
    for y_index in 0..image.height {
        for x_index in 0..image.width {
            let pixel_index = y_index * image.width + x_index;
            let base = pixel_index * 3;
            let original_y = component_image.data[base];
            let original_cb = component_image.data[base + 1];
            let original_cr = component_image.data[base + 2];
            let range_sigma = 0.018 + (1.0 - detail_protection) * 0.12 + iso_boost * 0.08;
            let mut sum = [0.0_f32; 3];
            let mut weight_sum = 0.0;
            for dy in -half..=half {
                for dx in -half..=half {
                    let sample_y =
                        component_image.sample(x_index as isize + dx, y_index as isize + dy, 0);
                    let spatial = (-0.5 * ((dx * dx + dy * dy) as f32)
                        / (spatial_radius * spatial_radius))
                        .exp();
                    let range = (-0.5 * ((sample_y - original_y) / range_sigma).powi(2)).exp();
                    let weight = spatial * range;
                    for (channel, accumulated) in sum.iter_mut().enumerate() {
                        *accumulated += component_image.sample(
                            x_index as isize + dx,
                            y_index as isize + dy,
                            channel,
                        ) * weight;
                    }
                    weight_sum += weight;
                }
            }
            let filtered = sum.map(|value| value / weight_sum.max(f32::EPSILON));
            let y = original_y
                + (filtered[0] - original_y) * luminance_strength * (1.0 - detail_protection * 0.7);
            let cb = original_cb + (filtered[1] - original_cb) * chroma_strength;
            let cr = original_cr + (filtered[2] - original_cr) * chroma_strength;
            let (r, g, b) = ycbcr_to_rgb(y, cb, cr);
            output.data[base] = r;
            output.data[base + 1] = g;
            output.data[base + 2] = b;
        }
    }
    output
}

/// Three genuinely separate spatial-frequency controls: Texture uses the fine residual,
/// Clarity the mid-frequency residual, and Dehaze a broad luminance veil estimate.
pub fn local_detail(image: &LinearImage, parameters: LocalDetailParameters) -> LinearImage {
    let texture = parameters.texture.clamp(-1.0, 1.0);
    let clarity = parameters.clarity.clamp(-1.0, 1.0);
    let dehaze = parameters.dehaze.clamp(-1.0, 1.0);
    if texture.abs() <= f32::EPSILON
        && clarity.abs() <= f32::EPSILON
        && dehaze.abs() <= f32::EPSILON
    {
        return image.clone();
    }
    let fine = gaussian_blur(image, 0.8);
    let medium = gaussian_blur(image, 3.0);
    let broad = box_blur(image, 12);
    let mut output = image.clone();
    for pixel in 0..image.width.saturating_mul(image.height) {
        let base = pixel * 3;
        let y = rgb_to_ycbcr(image.data[base], image.data[base + 1], image.data[base + 2]).0;
        let fine_y = rgb_to_ycbcr(fine.data[base], fine.data[base + 1], fine.data[base + 2]).0;
        let medium_y = rgb_to_ycbcr(
            medium.data[base],
            medium.data[base + 1],
            medium.data[base + 2],
        )
        .0;
        let broad_y = rgb_to_ycbcr(broad.data[base], broad.data[base + 1], broad.data[base + 2]).0;
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
        for channel in 0..3 {
            output.data[base + channel] = image.data[base + channel] * scale;
        }
    }
    output
}
