// Frozen test-only pre-ownership kernel from commit37387e3, starroom-look/src/lib.rs.
// No runtime alternate renderer. Do not change this oracle to match production.
use super::{GrainSettings, LinearImage, LookError, VignetteSettings};
use sha2::{Digest, Sha256};

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn random_unit(seed: u64, x: u64, y: u64, c: u64) -> f32 {
    let mut z = seed
        ^ x.wrapping_mul(0x9E3779B185EBCA87)
        ^ y.wrapping_mul(0xC2B2AE3D27D4EB4F)
        ^ c.wrapping_mul(0x165667B19E3779F9);
    z ^= z >> 30;
    z = z.wrapping_mul(0xBF58476D1CE4E5B9);
    z ^= z >> 27;
    z = z.wrapping_mul(0x94D049BB133111EB);
    ((z ^ (z >> 31)) >> 40) as f32 / 16_777_215.0
}

pub(super) fn finishing(
    image: &LinearImage,
    grain: GrainSettings,
    vignette: VignetteSettings,
    image_identity: &str,
) -> Result<LinearImage, LookError> {
    if image.data.len() != image.width * image.height * 3 {
        return Err(LookError::InvalidImage);
    }
    if grain.amount == 0.0 && vignette.amount == 0.0 {
        return LinearImage::new(image.width, image.height, image.data.clone())
            .map_err(|_| LookError::InvalidImage);
    }
    let identity = Sha256::digest(image_identity.as_bytes());
    let seed = grain.seed ^ u64::from_le_bytes(identity[..8].try_into().unwrap());
    let mut data = image.data.clone();
    let cx = (image.width.saturating_sub(1)) as f32 * 0.5;
    let cy = (image.height.saturating_sub(1)) as f32 * 0.5;
    let aspect = image.width as f32 / image.height.max(1) as f32;
    for y in 0..image.height {
        for x in 0..image.width {
            let i = (y * image.width + x) * 3;
            let dx = (x as f32 - cx) / cx.max(1.0);
            let dy = (y as f32 - cy) / cy.max(1.0);
            let roundness = (vignette.roundness.clamp(-1.0, 1.0) + 1.0) * 0.5;
            let aspect_correction = 1.0 + (aspect.max(1.0) - 1.0) * (1.0 - roundness);
            let radius = ((dx * aspect_correction).powi(2) + dy.powi(2)).sqrt();
            let edge = ((radius - vignette.midpoint.clamp(0.0, 1.0))
                / (vignette.feather.abs().max(0.02)))
            .clamp(0.0, 1.0);
            let smooth = edge * edge * (3.0 - 2.0 * edge);
            let luminance =
                (0.2627 * data[i] + 0.6780 * data[i + 1] + 0.0593 * data[i + 2]).max(0.0);
            let highlight_weight = ((luminance - 0.55) / 1.45).clamp(0.0, 1.0);
            let protected = 1.0
                - vignette.highlight_protect.clamp(0.0, 1.0)
                    * highlight_weight
                    * highlight_weight
                    * (3.0 - 2.0 * highlight_weight);
            let vig_ev = -vignette.amount.clamp(-1.0, 1.0) * 2.0 * smooth * protected;
            let scale = 2.0f32.powf(vig_ev);
            let grain_gain = grain.amount.clamp(0.0, 1.0)
                * (0.003 + 0.025 * grain.size.clamp(0.1, 1.0))
                * (0.35 + 0.65 * luminance.sqrt());
            let cell = 1 + (grain.size.clamp(0.1, 1.0) * 4.0).round() as u64;
            let coarse_x = x as u64 / cell;
            let coarse_y = y as u64 / cell;
            let coarse = (random_unit(seed, coarse_x, coarse_y, 0) - 0.5) * 2.0;
            let fine = (random_unit(seed, x as u64, y as u64, 0) - 0.5) * 2.0;
            let mono = lerp(coarse, fine, grain.roughness.clamp(0.0, 1.0));
            for c in 0..3 {
                let colored = (random_unit(seed, x as u64, y as u64, c as u64 + 1) - 0.5) * 2.0;
                let noise = lerp(mono, colored, grain.color.clamp(0.0, 1.0));
                data[i + c] = data[i + c] * scale + noise * grain_gain;
            }
        }
    }
    LinearImage::new(image.width, image.height, data).map_err(|_| LookError::InvalidImage)
}
