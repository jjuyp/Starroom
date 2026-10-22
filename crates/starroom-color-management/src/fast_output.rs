//! Official LCMS Fast Float, restricted to a verified bounded sRGB output domain.
//! Never installed globally, on RAW/input transforms, or on arbitrary output ICCs.
use super::*;
use lcms2::ThreadContext;

unsafe extern "C" {
    fn cmsFastFloatExtensions() -> *mut std::ffi::c_void;
}

pub(super) struct FastOutput {
    transform: Transform<[f32; 3], [f32; 3], ThreadContext, DisallowCache>,
    // The plugin context outlives its transform; no mutation after construction.
    _context: Mutex<ThreadContext>,
    destination: gamut::DestinationAffine,
}

impl FastOutput {
    pub(super) fn for_exact_srgb(
        supplied: Option<&[u8]>,
        intent: RenderingIntent,
        bpc: bool,
    ) -> Result<Option<Self>, ColorManagementError> {
        let srgb = LittleCmsProvider.srgb_profile_bytes()?;
        if supplied.is_some_and(|bytes| bytes != srgb) {
            // Adobe RGB sampled inverse TRC failed the independent 16-bit dark ramp.
            // Other profiles retain the canonical engine, not a substituted profile.
            return Ok(None);
        }
        let Some(destination) = gamut::exact_destination_affine(&srgb, intent, bpc)? else {
            return Ok(None);
        };
        let mut context = ThreadContext::new();
        // SAFETY: the pinned upstream function returns a static LCMS plugin descriptor;
        // matching 2.19 headers/library, private context, and immutable lifetime ownership.
        if !unsafe { context.install_plugin(cmsFastFloatExtensions()) } {
            return Err(ColorManagementError::OutputAccelerationInitialization);
        }
        let convert = |source| ColorManagementError::TransformCreation {
            direction: "bounded sRGB output accelerator",
            source,
        };
        // Preserve the canonical in-memory primaries: an ICC serialization round-trip
        // quantizes matrix tags and is not equivalent to constructing the working profile.
        let (white, primaries) = rec2020_linear_d65_parameters();
        let linear = ToneCurve::new(1.0);
        let input =
            Profile::new_rgb_context(&context, &white, &primaries, &[&linear, &linear, &linear])
                .map_err(convert)?;
        let output = match supplied {
            Some(bytes) => Profile::new_icc_context(&context, bytes).map_err(convert)?,
            None => Profile::new_srgb_context(&context),
        };
        let flags = if bpc {
            Flags::NO_CACHE | Flags::BLACKPOINT_COMPENSATION
        } else {
            Flags::NO_CACHE
        };
        let transform = Transform::new_flags_context(
            &context,
            &input,
            PixelFormat::RGB_FLT,
            &output,
            PixelFormat::RGB_FLT,
            lcms_intent(intent),
            flags,
        )
        .map_err(convert)?;
        drop(input);
        drop(output);
        Ok(Some(Self {
            transform,
            _context: Mutex::new(context),
            destination,
        }))
    }

    fn safe(&self, pixel: [f32; 3]) -> bool {
        // The plugin's sampled curves have different out-of-domain behavior. Keep both
        // shaper inputs strictly in range, with a margin above validated matrix rounding.
        pixel.into_iter().all(|v| (0.0..=1.0).contains(&v))
            && self.destination.apply(pixel).into_iter().all(|v| {
                // Serialized sRGB has a piecewise TRC knee. A sampled LUT cell
                // crossing it is not the exact parametric curve; retain LCMS's
                // canonical evaluator within two upstream 32768-step cells.
                (0.0001..=0.9999).contains(&v) && (v - 0.0031308).abs() > 2.0 / 32768.0
            })
    }

    pub(super) fn apply(
        &self,
        pixels: &mut [[f32; 3]],
        canonical: &Transform<[f32; 3], [f32; 3], GlobalContext, DisallowCache>,
    ) -> usize {
        let mut start = 0;
        let mut accelerated = 0;
        while start < pixels.len() {
            let fast = self.safe(pixels[start]);
            let end = start
                + pixels[start..]
                    .iter()
                    .take_while(|p| self.safe(**p) == fast)
                    .count();
            if fast {
                self.transform.transform_in_place(&mut pixels[start..end]);
                accelerated += end - start;
            } else {
                canonical.transform_in_place(&mut pixels[start..end]);
            }
            start = end;
        }
        accelerated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_output_preserves_unbounded_oracle_and_16bit_dark_ramps() {
        let working = rec2020_linear_d65_profile().unwrap();
        let bytes = LittleCmsProvider.srgb_profile_bytes().unwrap();
        for supplied in [None, Some(bytes.as_slice())] {
            let output =
                supplied.map_or_else(Profile::new_srgb, |icc| Profile::new_icc(icc).unwrap());
            for intent in [
                RenderingIntent::Perceptual,
                RenderingIntent::RelativeColorimetric,
                RenderingIntent::AbsoluteColorimetric,
                RenderingIntent::Saturation,
            ] {
                for bpc in [false, true] {
                    let canonical =
                        build_parallel_lcms_transform(&working, &output, intent, bpc, "oracle")
                            .unwrap();
                    let fast = FastOutput::for_exact_srgb(supplied, intent, bpc)
                        .unwrap()
                        .unwrap();
                    let mut source =
                        vec![[0.0; 3], [1.0; 3], [-0.1, 0.3, 4.0], [4.0; 3], [-0.0; 3]];
                    for i in 0..4096 {
                        let g = 10.0_f32.powf(-9.0 + 9.0 * i as f32 / 4095.0);
                        source.push([g; 3]);
                        source.push(std::array::from_fn(|c| {
                            ((i * (97 + c * 16) + c * 431) % 4096) as f32 / 4095.0
                        }));
                    }
                    let mut expected = source.clone();
                    canonical.transform.transform_in_place(&mut expected);
                    let mut actual = source.clone();
                    assert!(fast.apply(&mut actual, &canonical.transform) > 1000);
                    for (i, (a, b)) in actual.iter().zip(&expected).enumerate() {
                        for c in 0..3 {
                            assert!(a[c].is_finite());
                            if !fast.safe(source[i]) {
                                assert_eq!(a[c].to_bits(), b[c].to_bits(), "unsafe domain {i}/{c}");
                            } else {
                                assert!(
                                    (a[c] - b[c]).abs() <= 2.0e-6,
                                    "safe {i}/{c}: {} vs {}",
                                    a[c],
                                    b[c]
                                );
                            }
                            let quant = |v: f32| (v.clamp(0.0, 1.0) * 65535.0).round() as i32;
                            assert!((quant(a[c]) - quant(b[c])).abs() <= 1);
                        }
                    }
                    let global_after = build_parallel_lcms_transform(
                        &working,
                        &output,
                        intent,
                        bpc,
                        "global isolation",
                    )
                    .unwrap();
                    let mut after = source;
                    global_after.transform.transform_in_place(&mut after);
                    assert_eq!(after, expected, "no global plugin registration");
                }
            }
        }
    }

    #[test]
    fn serialized_srgb_dense_cube_and_toe_match_canonical_in_parallel() {
        let bytes = LittleCmsProvider.srgb_profile_bytes().unwrap();
        let working = rec2020_linear_d65_profile().unwrap();
        let output = Profile::new_icc(&bytes).unwrap();
        let intent = RenderingIntent::RelativeColorimetric;
        let canonical =
            build_parallel_lcms_transform(&working, &output, intent, true, "oracle").unwrap();
        let fast = FastOutput::for_exact_srgb(Some(&bytes), intent, true)
            .unwrap()
            .unwrap();
        let mut source = Vec::new();
        for r in 0..33 {
            for g in 0..33 {
                for b in 0..33 {
                    source.push([r as f32 / 32.0, g as f32 / 32.0, b as f32 / 32.0]);
                }
            }
        }
        for i in 0..16384 {
            source.push([0.0025 + i as f32 * 0.0013 / 16383.0; 3]);
        }
        let mut expected = source.clone();
        canonical.transform.transform_in_place(&mut expected);
        let mut actual = source.clone();
        let accelerated: usize = actual
            .par_chunks_mut(16384)
            .map(|chunk| fast.apply(chunk, &canonical.transform))
            .sum();
        assert!(accelerated > 10000);
        for (i, (a, b)) in actual.iter().zip(&expected).enumerate() {
            for c in 0..3 {
                if fast.safe(source[i]) {
                    assert!(
                        (a[c] - b[c]).abs() <= 2.0e-6,
                        "dense {i}/{c}: {} vs {}",
                        a[c],
                        b[c]
                    );
                } else {
                    assert_eq!(a[c].to_bits(), b[c].to_bits());
                }
                let quant = |v: f32| (v.clamp(0.0, 1.0) * 65535.0).round() as i32;
                assert!((quant(a[c]) - quant(b[c])).abs() <= 1);
            }
        }
    }

    #[test]
    fn real_cached_output_selects_only_exact_srgb_and_never_accelerates_input() {
        for profile in [
            BuiltinOutputProfile::Srgb,
            BuiltinOutputProfile::DisplayP3,
            BuiltinOutputProfile::AdobeRgb,
            BuiltinOutputProfile::Rec2020,
        ] {
            let bytes = LittleCmsProvider
                .builtin_output_profile_bytes(profile)
                .unwrap();
            for direction in [TransformDirection::Input, TransformDirection::Output] {
                let transform = create_parallel_transform(
                    direction,
                    Some(&bytes),
                    RenderingIntent::RelativeColorimetric,
                    true,
                )
                .unwrap();
                assert_eq!(
                    transform.fast_output.is_some(),
                    matches!(direction, TransformDirection::Output)
                        && profile == BuiltinOutputProfile::Srgb
                );
            }
        }
    }
}
