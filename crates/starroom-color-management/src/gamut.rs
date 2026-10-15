//! Destination-aware output boundary. LCMS owns profile conversion and gamut proofing;
//! the existing Native OKLab coordinates define a constant-lightness/hue chroma search.
//! Chroma-reduction reference: W3C CSS Color 4 section 14.1.3 (not CSS local-MINDE conformance).
use super::*;
use lcms2::{Tag, TagSignature, ThreadContext};
use starroom_color::{Oklab, oklab_to_rec2020, rec2020_to_oklab};

type ProofTransform = Transform<[f32; 3], [f32; 3], ThreadContext, DisallowCache>;

#[derive(Clone, Copy)]
struct DestinationAffine([[f32; 4]; 3]);
impl DestinationAffine {
    fn apply(self, p: [f32; 3]) -> [f32; 3] {
        self.0
            .map(|r| r[0] * p[0] + r[1] * p[1] + r[2] * p[2] + r[3])
    }
}
fn exact_destination_affine(
    destination: &[u8],
    intent: RenderingIntent,
    bpc: bool,
) -> Result<Option<DestinationAffine>, ColorManagementError> {
    let mut named =
        Profile::new_icc(destination).map_err(|source| ColorManagementError::InvalidProfile {
            role: ProfileRole::Output,
            source,
        })?;
    if !named.is_matrix_shaper()
        || [
            Intent::Perceptual,
            Intent::RelativeColorimetric,
            Intent::AbsoluteColorimetric,
            Intent::Saturation,
        ]
        .into_iter()
        .any(|i| named.is_clut(i, 0) || named.is_clut(i, 1))
    {
        return Ok(None);
    }
    for tag in [
        TagSignature::RedTRCTag,
        TagSignature::GreenTRCTag,
        TagSignature::BlueTRCTag,
    ] {
        let Tag::ToneCurve(curve) = named.read_tag(tag) else {
            return Ok(None);
        };
        if !curve.is_monotonic()
            || curve.is_descending()
            || curve.eval(0.0_f32).abs() > 1e-6
            || (curve.eval(1.0_f32) - 1.0).abs() > 1e-6
        {
            return Ok(None);
        }
    }
    let linear = ToneCurve::new(1.0);
    for tag in [
        TagSignature::RedTRCTag,
        TagSignature::GreenTRCTag,
        TagSignature::BlueTRCTag,
    ] {
        if !named.write_tag(tag, Tag::ToneCurve(&linear)) {
            return Ok(None);
        }
    }
    let working = rec2020_linear_d65_profile()?;
    let transform =
        build_parallel_lcms_transform(&working, &named, intent, bpc, "linear destination proof")?;
    let mut basis = [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    transform.transform.transform_in_place(&mut basis);
    ensure_finite("linear destination basis", &basis)?;
    let affine = DestinationAffine(std::array::from_fn(|row| {
        [
            basis[1][row] - basis[0][row],
            basis[2][row] - basis[0][row],
            basis[3][row] - basis[0][row],
            basis[0][row],
        ]
    }));
    // This is an exact affine extraction, not a sampled gamut approximation. Validate actual
    // LCMS at independent mixed, negative and HDR values before enabling the fast path.
    let check = [
        [0.1, 0.8, 0.3],
        [-0.1, 0.5, 0.2],
        [2.0, 0.1, 0.0],
        [0.31, 0.72, 0.93],
    ];
    let mut actual = check;
    transform.transform.transform_in_place(&mut actual);
    for (p, q) in check.into_iter().zip(actual) {
        if affine
            .apply(p)
            .into_iter()
            .zip(q)
            .any(|(a, b)| !b.is_finite() || (a - b).abs() > 2e-5 * b.abs().max(1.0))
        {
            return Ok(None);
        }
    }
    Ok(Some(affine))
}

struct Proof {
    linear: Option<DestinationAffine>,
    // Transforms drop before their context owners. Contexts are never mutated after creation;
    // Mutex provides safe Send/Sync ownership without unsafe trait declarations.
    first: ProofTransform,
    second: ProofTransform,
    output: Arc<ParallelLcmsTransform>,
    inverse: Arc<ParallelLcmsTransform>,
    _first_context: Mutex<ThreadContext>,
    _second_context: Mutex<ThreadContext>,
}

fn build_proof(
    profile: Option<&[u8]>,
    intent: RenderingIntent,
    bpc: bool,
) -> Result<Proof, ColorManagementError> {
    let working_bytes = rec2020_linear_d65_profile()?.icc().map_err(|source| {
        ColorManagementError::InvalidProfile {
            role: ProfileRole::Working,
            source,
        }
    })?;
    let destination =
        match profile {
            Some(bytes) => bytes.to_vec(),
            None => Profile::new_srgb().icc().map_err(|source| {
                ColorManagementError::InvalidProfile {
                    role: ProfileRole::Output,
                    source,
                }
            })?,
        };
    let build = |alarm| {
        let mut context = ThreadContext::new();
        context.set_alarm_codes(alarm);
        let working = Profile::new_icc_context(&context, &working_bytes).map_err(|source| {
            ColorManagementError::InvalidProfile {
                role: ProfileRole::Working,
                source,
            }
        })?;
        let named = Profile::new_icc_context(&context, &destination).map_err(|source| {
            ColorManagementError::InvalidProfile {
                role: ProfileRole::Output,
                source,
            }
        })?;
        let mut flags = Flags::NO_CACHE | Flags::GAMUT_CHECK | Flags::SOFT_PROOFING;
        if bpc {
            flags = flags | Flags::BLACKPOINT_COMPENSATION;
        }
        let transform = ProofTransform::new_proofing_context(
            &context,
            &working,
            PixelFormat::RGB_FLT,
            &working,
            PixelFormat::RGB_FLT,
            &named,
            lcms_intent(intent),
            Intent::RelativeColorimetric,
            flags,
        )
        .map_err(|source| ColorManagementError::TransformCreation {
            direction: "destination gamut proof",
            source,
        })?;
        Ok::<_, ColorManagementError>((transform, context))
    };
    let (first, first_context) = build([65535; 16])?;
    let (second, second_context) = build([0; 16])?;
    Ok(Proof {
        linear: exact_destination_affine(&destination, intent, bpc)?,
        first,
        second,
        output: cached_parallel_transform(TransformDirection::Output, profile, intent, bpc)?,
        inverse: cached_parallel_transform(TransformDirection::Input, profile, intent, bpc)?,
        _first_context: Mutex::new(first_context),
        _second_context: Mutex::new(second_context),
    })
}

struct Entry {
    profile: Option<Vec<u8>>,
    intent: RenderingIntent,
    bpc: bool,
    proof: Arc<Proof>,
}
#[derive(Default)]
struct ProofCache {
    entries: VecDeque<Entry>,
}
impl ProofCache {
    fn lookup(
        &mut self,
        profile: Option<&[u8]>,
        intent: RenderingIntent,
        bpc: bool,
    ) -> Result<Option<Arc<Proof>>, ColorManagementError> {
        let Some(index) = self
            .entries
            .iter()
            .position(|e| e.profile.as_deref() == profile && e.intent == intent && e.bpc == bpc)
        else {
            return Ok(None);
        };
        let entry = self
            .entries
            .remove(index)
            .ok_or(ColorManagementError::TransformCacheUnavailable)?;
        let result = Arc::clone(&entry.proof);
        self.entries.push_back(entry);
        Ok(Some(result))
    }
    fn retain(
        &mut self,
        profile: Option<&[u8]>,
        intent: RenderingIntent,
        bpc: bool,
        proof: Arc<Proof>,
    ) -> Result<Arc<Proof>, ColorManagementError> {
        if let Some(existing) = self.lookup(profile, intent, bpc)? {
            return Ok(existing);
        }
        let bytes = profile.map_or(0, <[u8]>::len);
        if bytes <= TRANSFORM_CACHE_PROFILE_BYTES {
            while self.entries.len() >= 4
                || self
                    .entries
                    .iter()
                    .map(|e| e.profile.as_ref().map_or(0, Vec::len))
                    .sum::<usize>()
                    + bytes
                    > TRANSFORM_CACHE_PROFILE_BYTES
            {
                self.entries.pop_front();
            }
            self.entries.push_back(Entry {
                profile: profile.map(<[u8]>::to_vec),
                intent,
                bpc,
                proof: Arc::clone(&proof),
            });
        }
        Ok(proof)
    }
}
fn proof_cache() -> &'static Mutex<ProofCache> {
    static CACHE: OnceLock<Mutex<ProofCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ProofCache::default()))
}
fn cached_proof(
    profile: Option<&[u8]>,
    intent: RenderingIntent,
    bpc: bool,
) -> Result<Arc<Proof>, ColorManagementError> {
    {
        if let Some(proof) = proof_cache()
            .lock()
            .map_err(|_| ColorManagementError::TransformCacheUnavailable)?
            .lookup(profile, intent, bpc)?
        {
            return Ok(proof);
        }
    }
    let proof = Arc::new(build_proof(profile, intent, bpc)?);
    proof_cache()
        .lock()
        .map_err(|_| ColorManagementError::TransformCacheUnavailable)?
        .retain(profile, intent, bpc, proof)
}

impl Proof {
    fn search_membership(&self, input: &[[f32; 3]]) -> Result<Vec<bool>, ColorManagementError> {
        if let Some(linear) = self.linear {
            // Search candidates may be moved conservatively inside the true linear device
            // boundary. Original pixels still use full ICC membership/identity above.
            // This small inward numerical guard prevents gamma-amplified rounding at black.
            return Ok(input
                .iter()
                .map(|p| {
                    linear
                        .apply(*p)
                        .iter()
                        .all(|v| v.is_finite() && (2e-6..=0.999998).contains(v))
                })
                .collect());
        }
        self.general_membership(input)
    }
    fn membership(&self, input: &[[f32; 3]]) -> Result<Vec<bool>, ColorManagementError> {
        if let Some(linear) = self.linear {
            let mut result = vec![false; input.len()];
            let mut indices = Vec::new();
            let mut boundary = Vec::new();
            for (index, p) in input.iter().enumerate() {
                let device = linear.apply(*p);
                if device
                    .iter()
                    .all(|v| v.is_finite() && (2e-6..=0.99999).contains(v))
                {
                    result[index] = true;
                } else if device
                    .iter()
                    .all(|v| v.is_finite() && (-1e-4..=1.0001).contains(v))
                {
                    // Near a device boundary, gamma can amplify tiny linear errors. Preserve
                    // the real profile's encoded/round-trip judgement, not a relaxed linear clip.
                    indices.push(index);
                    boundary.push(*p);
                }
            }
            if !boundary.is_empty() {
                for (index, fits) in indices.into_iter().zip(self.general_membership(&boundary)?) {
                    result[index] = fits;
                }
            }
            return Ok(result);
        }
        self.general_membership(input)
    }
    fn general_membership(&self, input: &[[f32; 3]]) -> Result<Vec<bool>, ColorManagementError> {
        let mut a = input.to_vec();
        let mut b = input.to_vec();
        let mut encoded = input.to_vec();
        self.first.transform_in_place(&mut a);
        self.second.transform_in_place(&mut b);
        self.output.transform.transform_in_place(&mut encoded);
        let restored = if input.iter().flatten().any(|v| !(0.0..=1.0).contains(v))
            || a.iter().zip(&b).any(|(a, b)| a != b)
        {
            let mut restored = encoded.clone();
            self.inverse.transform.transform_in_place(&mut restored);
            ensure_finite("destination gamut inverse", &restored)?;
            Some(restored)
        } else {
            None
        };
        ensure_finite("destination gamut proof", &a)?;
        ensure_finite("destination gamut proof", &b)?;
        ensure_finite("destination gamut output", &encoded)?;
        Ok(input
            .iter()
            .enumerate()
            .zip(a.iter().zip(b.iter().zip(encoded)))
            .map(|((index, source), (a, (b, encoded)))| {
                // The proof LUT is bounded. Outside that domain use the mature ICC inverse
                // to validate actual colorimetric reproduction, never clamp working RGB.
                let round_trip = restored.as_ref().is_some_and(|r| {
                    source
                        .iter()
                        .zip(r[index])
                        .all(|(s, d)| (*s - d).abs() <= 2e-7 + 2e-5 * s.abs())
                });
                let reproduced =
                    source.iter().all(|v| (0.0..=1.0).contains(v)) && a == b || round_trip;
                reproduced && encoded.iter().all(|v| (-1e-5..=1.00001).contains(v))
            })
            .collect())
    }
}

/// Runs on the final bounded-output boundary only. Creative/RAW stages stay unbounded.
/// Identity in destination gamut is exact; only out-of-gamut chroma is changed.
pub fn map_output_gamut(
    pixels: &mut [[f32; 3]],
    output_icc: Option<&[u8]>,
    intent: RenderingIntent,
    black_point_compensation: bool,
) -> Result<(), ColorManagementError> {
    ensure_finite("output gamut input", pixels)?;
    let proof = cached_proof(output_icc, intent, black_point_compensation)?;
    pixels
        .par_chunks_mut(2048)
        .enumerate()
        .try_for_each(|(chunk_index, chunk)| {
            let inside = proof.membership(chunk)?;
            let mut indices = Vec::new();
            let mut lab = Vec::new();
            for (index, in_gamut) in inside.into_iter().enumerate() {
                if in_gamut {
                    continue;
                }
                let p = chunk[index];
                let value = rec2020_to_oklab(LinearRgb {
                    r: p[0],
                    g: p[1],
                    b: p[2],
                });
                if ![value.l, value.a, value.b].iter().all(|v| v.is_finite()) {
                    return Err(ColorManagementError::NonFinitePixel {
                        stage: "output gamut coordinates",
                        pixel: chunk_index * 2048 + index,
                        channel: 0,
                    });
                }
                if value.l >= 1.0 {
                    chunk[index] = [1.0; 3];
                    continue;
                }
                if value.l <= 0.0 {
                    chunk[index] = [0.0; 3];
                    continue;
                }
                indices.push(index);
                lab.push(value);
            }
            if indices.is_empty() {
                return Ok(());
            }
            let candidate = |value: Oklab, ratio: f32| {
                let rgb = oklab_to_rec2020(Oklab {
                    l: value.l,
                    a: value.a * ratio,
                    b: value.b * ratio,
                });
                [rgb.r, rgb.g, rgb.b]
            };
            let mut low = vec![0.0_f32; indices.len()];
            let mut high = vec![1.0_f32; indices.len()];
            let mut best: Vec<_> = lab.iter().map(|v| candidate(*v, 0.0)).collect();
            if proof.membership(&best)?.iter().any(|v| !v) {
                return Err(ColorManagementError::GamutMappingUnavailable);
            }
            for _ in 0..16 {
                let trial: Vec<_> = lab
                    .iter()
                    .enumerate()
                    .map(|(i, v)| candidate(*v, (low[i] + high[i]) * 0.5))
                    .collect();
                for (i, fits) in proof.search_membership(&trial)?.into_iter().enumerate() {
                    let middle = (low[i] + high[i]) * 0.5;
                    if fits {
                        low[i] = middle;
                        best[i] = trial[i];
                    } else {
                        high[i] = middle;
                    }
                }
            }
            for (index, mapped) in indices.into_iter().zip(best) {
                chunk[index] = mapped;
            }
            Ok(())
        })?;
    ensure_finite("output gamut mapped result", pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_proof_cache_has_exact_keys_lru_bounds_and_duplicate_build_ownership() {
        let mut cache = ProofCache::default();
        let intent = RenderingIntent::RelativeColorimetric;
        let bytes = LittleCmsProvider.srgb_profile_bytes().unwrap();
        let first = Arc::new(build_proof(Some(&bytes), intent, true).unwrap());
        cache
            .retain(Some(&bytes), intent, true, Arc::clone(&first))
            .unwrap();
        assert!(Arc::ptr_eq(
            &first,
            &cache.lookup(Some(&bytes), intent, true).unwrap().unwrap()
        ));
        assert!(cache.lookup(None, intent, true).unwrap().is_none());
        assert!(cache.lookup(Some(&bytes), intent, false).unwrap().is_none());
        assert!(
            cache
                .lookup(Some(&bytes), RenderingIntent::Perceptual, true)
                .unwrap()
                .is_none()
        );
        let duplicate = Arc::new(build_proof(Some(&bytes), intent, true).unwrap());
        assert!(!Arc::ptr_eq(&first, &duplicate));
        assert!(Arc::ptr_eq(
            &first,
            &cache.retain(Some(&bytes), intent, true, duplicate).unwrap()
        ));
        for index in 1..=4 {
            let mut key = bytes.clone();
            key.resize(bytes.len() + index, 0); // Unused ICC suffix is still exact key identity.
            let proof = Arc::new(build_proof(Some(&key), intent, true).unwrap());
            cache.retain(Some(&key), intent, true, proof).unwrap();
        }
        assert_eq!(cache.entries.len(), 4);
        assert!(cache.lookup(Some(&bytes), intent, true).unwrap().is_none());
        let mut oversized = bytes.clone();
        oversized.resize(TRANSFORM_CACHE_PROFILE_BYTES + 1, 0);
        let large = Arc::new(build_proof(Some(&oversized), intent, true).unwrap());
        cache.retain(Some(&oversized), intent, true, large).unwrap();
        assert!(
            cache
                .lookup(Some(&oversized), intent, true)
                .unwrap()
                .is_none()
        );
        assert!(build_proof(Some(b"invalid"), intent, true).is_err());
        let mut byte_bounded = ProofCache::default();
        oversized.truncate(TRANSFORM_CACHE_PROFILE_BYTES / 2 + 1);
        for bpc in [true, false] {
            let proof = Arc::new(build_proof(Some(&oversized), intent, bpc).unwrap());
            byte_bounded
                .retain(Some(&oversized), intent, bpc, proof)
                .unwrap();
        }
        assert_eq!(byte_bounded.entries.len(), 1);
        assert!(
            byte_bounded
                .lookup(Some(&oversized), intent, true)
                .unwrap()
                .is_none()
        );
        assert!(
            byte_bounded
                .lookup(Some(&oversized), intent, false)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn mapped_gradients_preserve_neutral_precision_and_perceptual_continuity() {
        use crate::quality::compare_xyz_d65;
        let xyz = |p: [f32; 3]| {
            rec2020_linear_to_xyz_d65(LinearRgb {
                r: p[0],
                g: p[1],
                b: p[2],
            })
        };
        for name in [
            BuiltinOutputProfile::Srgb,
            BuiltinOutputProfile::DisplayP3,
            BuiltinOutputProfile::AdobeRgb,
            BuiltinOutputProfile::Rec2020,
        ] {
            let bytes = LittleCmsProvider
                .builtin_output_profile_bytes(name)
                .unwrap();
            let original: Vec<_> = (0..8193).map(|i| [i as f32 / 8192.0; 3]).collect();
            let mut gray = original.clone();
            map_output_gamut(
                &mut gray,
                Some(&bytes),
                RenderingIntent::RelativeColorimetric,
                true,
            )
            .unwrap();
            assert_eq!(
                gray, original,
                "{name:?}: gray gradient must be exact before encoding"
            );
            LittleCmsProvider
                .working_to_output(
                    &mut gray,
                    Some(&bytes),
                    RenderingIntent::RelativeColorimetric,
                    true,
                )
                .unwrap();
            assert!(gray.windows(2).all(|w| w[0][0] <= w[1][0]));
            assert!(
                gray.windows(2).filter(|w| w[0][0] != w[1][0]).count() > 8000,
                "{name:?}: output cannot collapse to 8-bit precision"
            );
            for hue in [0.0_f32, 60.0, 120.0, 180.0, 240.0, 300.0] {
                let mut ramp: Vec<_> = (0..2049)
                    .map(|i| {
                        let c = i as f32 / 2048.0 * 0.4;
                        let rgb = oklab_to_rec2020(Oklab {
                            l: 0.65,
                            a: c * hue.to_radians().cos(),
                            b: c * hue.to_radians().sin(),
                        });
                        [rgb.r, rgb.g, rgb.b]
                    })
                    .collect();
                map_output_gamut(
                    &mut ramp,
                    Some(&bytes),
                    RenderingIntent::RelativeColorimetric,
                    true,
                )
                .unwrap();
                for pair in ramp.windows(2) {
                    let delta = compare_xyz_d65(xyz(pair[0]), xyz(pair[1])).unwrap();
                    assert!(
                        delta.delta_e_2000 < 0.15,
                        "{name:?} hue {hue}: discontinuity {delta:?}"
                    );
                }
                LittleCmsProvider
                    .working_to_output(
                        &mut ramp,
                        Some(&bytes),
                        RenderingIntent::RelativeColorimetric,
                        true,
                    )
                    .unwrap();
                assert!(
                    ramp.iter().flatten().all(|v| (-1e-5..=1.00001).contains(v)),
                    "{name:?} hue {hue}"
                );
            }
        }
    }

    #[test]
    fn shared_proof_is_thread_safe_and_parallel_chunk_errors_keep_global_pixel_index() {
        let source = vec![[0.8, 0.1, 0.2]; 4200];
        let mut expected = source.clone();
        map_output_gamut(
            &mut expected,
            None,
            RenderingIntent::RelativeColorimetric,
            true,
        )
        .unwrap();
        std::thread::scope(|scope| {
            let jobs: Vec<_> = (0..4)
                .map(|_| {
                    let mut pixels = source.clone();
                    scope.spawn(move || {
                        map_output_gamut(
                            &mut pixels,
                            None,
                            RenderingIntent::RelativeColorimetric,
                            true,
                        )
                        .unwrap();
                        pixels
                    })
                })
                .collect();
            for job in jobs {
                assert_eq!(job.join().unwrap(), expected);
            }
        });
        let mut invalid = source;
        invalid[4097][2] = f32::INFINITY;
        assert!(matches!(
            map_output_gamut(
                &mut invalid,
                None,
                RenderingIntent::RelativeColorimetric,
                true
            ),
            Err(ColorManagementError::NonFinitePixel {
                pixel: 4097,
                channel: 2,
                ..
            })
        ));
    }

    #[test]
    fn noncanonical_profile_uses_real_general_provider_without_profile_substitution() {
        let mut profile = Profile::new_srgb();
        // Deliberately authored restricted-range TRC, not a named production/printer fixture.
        let curve = ToneCurve::new_tabulated_float(&[0.02, 0.26, 0.5, 0.74, 0.98]);
        for tag in [
            TagSignature::RedTRCTag,
            TagSignature::GreenTRCTag,
            TagSignature::BlueTRCTag,
        ] {
            assert!(profile.write_tag(tag, Tag::ToneCurve(&curve)));
        }
        let bytes = profile.icc().unwrap();
        let proof = build_proof(Some(&bytes), RenderingIntent::RelativeColorimetric, true).unwrap();
        assert!(proof.linear.is_none());
        let mut neutral = [[0.3; 3]];
        let original = neutral;
        map_output_gamut(
            &mut neutral,
            Some(&bytes),
            RenderingIntent::RelativeColorimetric,
            true,
        )
        .unwrap();
        assert_eq!(neutral, original);
        LittleCmsProvider
            .working_to_output(
                &mut neutral,
                Some(&bytes),
                RenderingIntent::RelativeColorimetric,
                true,
            )
            .unwrap();
        let mut srgb = original;
        LittleCmsProvider
            .working_to_output(&mut srgb, None, RenderingIntent::RelativeColorimetric, true)
            .unwrap();
        assert!((neutral[0][0] - srgb[0][0]).abs() > 0.05);
    }

    #[test]
    fn matrix_fast_membership_matches_mature_general_oracle_on_independent_grid() {
        let samples: Vec<_> = (0..10)
            .flat_map(|r| {
                (0..10).flat_map(move |g| {
                    (0..10).map(move |b| {
                        [
                            0.05 + r as f32 * 0.1,
                            0.05 + g as f32 * 0.1,
                            0.05 + b as f32 * 0.1,
                        ]
                    })
                })
            })
            .collect();
        for name in [
            BuiltinOutputProfile::Srgb,
            BuiltinOutputProfile::DisplayP3,
            BuiltinOutputProfile::AdobeRgb,
            BuiltinOutputProfile::Rec2020,
        ] {
            let bytes = LittleCmsProvider
                .builtin_output_profile_bytes(name)
                .unwrap();
            let proof =
                cached_proof(Some(&bytes), RenderingIntent::RelativeColorimetric, true).unwrap();
            assert!(proof.linear.is_some());
            assert_eq!(
                proof.membership(&samples).unwrap(),
                proof.general_membership(&samples).unwrap(),
                "{name:?}"
            );
        }
    }
    #[test]
    fn destination_mapping_intents_flags_and_repeated_output_are_explicit_and_finite() {
        for intent in [
            RenderingIntent::RelativeColorimetric,
            RenderingIntent::Perceptual,
            RenderingIntent::Saturation,
            RenderingIntent::AbsoluteColorimetric,
        ] {
            for bpc in [false, true] {
                let original = [[0.0; 3], [1.0; 3], [0.25; 3], [0.8, 0.1, 0.2]];
                let mut first = original;
                let mut second = original;
                map_output_gamut(&mut first, None, intent, bpc).unwrap();
                map_output_gamut(&mut second, None, intent, bpc).unwrap();
                assert_eq!(first, second);
                assert_eq!(first[..3], original[..3]);
            }
        }
    }
    #[test]
    fn neutral_anchor_sweep_does_not_depend_on_coarse_proof_lut_black_cells() {
        let proof = cached_proof(None, RenderingIntent::RelativeColorimetric, true).unwrap();
        let samples: Vec<_> = (1..1000)
            .map(|i| {
                let rgb = oklab_to_rec2020(Oklab {
                    l: i as f32 / 1000.0,
                    a: 0.0,
                    b: 0.0,
                });
                [rgb.r, rgb.g, rgb.b]
            })
            .collect();
        let valid = proof.membership(&samples).unwrap();
        for (i, fits) in valid.into_iter().enumerate() {
            assert!(
                fits,
                "neutral L={} failed: {:?}",
                (i + 1) as f32 / 1000.0,
                samples[i]
            );
        }
        let bright: Vec<_> = (1..1000)
            .map(|i| {
                let rgb = oklab_to_rec2020(Oklab {
                    l: 1.0 - i as f32 * 1e-6,
                    a: 0.0,
                    b: 0.0,
                });
                [rgb.r, rgb.g, rgb.b]
            })
            .collect();
        for (i, fits) in proof.membership(&bright).unwrap().into_iter().enumerate() {
            assert!(
                fits,
                "near-white L={} failed: {:?}",
                1.0 - (i + 1) as f32 * 1e-6,
                bright[i]
            );
        }
    }
    #[test]
    fn named_destination_mapping_is_bounded_preserves_identity_and_perceptual_path() {
        let source = [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.8, 0.1, 0.2],
            [0.2, 0.2, 0.2],
            [0.0; 3],
            [1.0; 3],
        ];
        for name in [
            BuiltinOutputProfile::Srgb,
            BuiltinOutputProfile::DisplayP3,
            BuiltinOutputProfile::AdobeRgb,
            BuiltinOutputProfile::Rec2020,
        ] {
            let bytes = LittleCmsProvider
                .builtin_output_profile_bytes(name)
                .unwrap();
            let mut mapped = source;
            map_output_gamut(
                &mut mapped,
                Some(&bytes),
                RenderingIntent::RelativeColorimetric,
                true,
            )
            .unwrap();
            assert_eq!(mapped[5..], source[5..]);
            if name == BuiltinOutputProfile::Rec2020 {
                assert_eq!(mapped, source);
            }
            for (before, after) in source.iter().zip(mapped) {
                let lab = |p: [f32; 3]| {
                    rec2020_to_oklab(LinearRgb {
                        r: p[0],
                        g: p[1],
                        b: p[2],
                    })
                };
                let a = lab(*before);
                let b = lab(after);
                assert!(
                    (a.l - b.l).abs() < 2e-5,
                    "{name:?}: lightness changed {a:?} {b:?}"
                );
                assert!(
                    (a.a * b.b - a.b * b.a).abs() < 2e-5,
                    "{name:?}: hue changed {a:?} {b:?}"
                );
            }
            LittleCmsProvider
                .working_to_output(
                    &mut mapped,
                    Some(&bytes),
                    RenderingIntent::RelativeColorimetric,
                    true,
                )
                .unwrap();
            assert!(
                mapped
                    .iter()
                    .flatten()
                    .all(|v| (-1e-5..=1.00001).contains(v)),
                "{name:?}: {mapped:?}"
            );
        }
    }
    #[test]
    fn wider_rgb_profile_preserves_valid_colours_outside_working_unit_cube() {
        // Explicitly authored synthetic wide RGB profile, not a named photographic profile.
        let white = CIExyY {
            x: 0.3127,
            y: 0.3290,
            Y: 1.0,
        };
        let primaries = CIExyYTRIPLE {
            Red: CIExyY {
                x: 0.75,
                y: 0.25,
                Y: 1.0,
            },
            Green: CIExyY {
                x: 0.1,
                y: 0.85,
                Y: 1.0,
            },
            Blue: CIExyY {
                x: 0.02,
                y: 0.02,
                Y: 1.0,
            },
        };
        let curve = ToneCurve::new(1.8);
        let bytes = Profile::new_rgb(&white, &primaries, &[&curve, &curve, &curve])
            .unwrap()
            .icc()
            .unwrap();
        let mut source = [[0.95, 0.01, 0.01], [0.01, 0.95, 0.01], [0.01, 0.01, 0.95]];
        LittleCmsProvider
            .input_to_working(
                &mut source,
                Some(&bytes),
                RenderingIntent::RelativeColorimetric,
                true,
            )
            .unwrap();
        assert!(source.iter().flatten().any(|v| !(0.0..=1.0).contains(v)));
        let before = source;
        map_output_gamut(
            &mut source,
            Some(&bytes),
            RenderingIntent::RelativeColorimetric,
            true,
        )
        .unwrap();
        assert_eq!(
            source, before,
            "valid wide colours must not be silently forced into Rec.2020 unit gamut"
        );
    }
    #[test]
    fn output_mapping_rejects_invalid_inputs_and_keeps_hdr_policy_at_output_only() {
        let mut hdr = [[4.0; 3], [-0.2; 3], [0.0; 3]];
        map_output_gamut(&mut hdr, None, RenderingIntent::RelativeColorimetric, true).unwrap();
        assert_eq!(hdr, [[1.0; 3], [0.0; 3], [0.0; 3]]);
        assert!(
            map_output_gamut(
                &mut [[f32::NAN, 0.0, 0.0]],
                None,
                RenderingIntent::RelativeColorimetric,
                true
            )
            .is_err()
        );
        assert!(
            map_output_gamut(
                &mut [[0.2; 3]],
                Some(&[0, 1, 2]),
                RenderingIntent::RelativeColorimetric,
                true
            )
            .is_err()
        );
    }
}
