//! Actual LCMS output-boundary evidence, not a new gamut mapper or an IQ acceptance.
use lcms2::{DisallowCache, Flags, Intent, PixelFormat, Profile, ThreadContext, Transform};
use starroom_color_management::{BuiltinOutputProfile, LittleCmsProvider, RenderingIntent};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 1.0, 1.0],
        [0.8, 0.1, 0.2],
        [1.0, 1.0, 1.0],
    ];
    let working_bytes =
        LittleCmsProvider.builtin_output_profile_bytes(BuiltinOutputProfile::Rec2020)?;
    let mut encoded_source = source;
    LittleCmsProvider.working_to_output(
        &mut encoded_source,
        Some(&working_bytes),
        RenderingIntent::RelativeColorimetric,
        true,
    )?;
    for name in [
        BuiltinOutputProfile::Srgb,
        BuiltinOutputProfile::DisplayP3,
        BuiltinOutputProfile::AdobeRgb,
        BuiltinOutputProfile::Rec2020,
    ] {
        let profile = LittleCmsProvider.builtin_output_profile_bytes(name)?;
        let mut pixels = source;
        LittleCmsProvider.working_to_output(
            &mut pixels,
            Some(&profile),
            RenderingIntent::RelativeColorimetric,
            true,
        )?;
        println!("{name:?}: {pixels:?}");
        // Use a private mature LCMS context: never change process-global alarm state.
        let check = |alarm| {
            let mut context = ThreadContext::new();
            context.set_alarm_codes(alarm);
            let input = Profile::new_icc_context(&context, &working_bytes)?;
            let proof = Profile::new_icc_context(&context, &profile)?;
            let transform =
            Transform::<[f32; 3], [f32; 3], ThreadContext, DisallowCache>::new_proofing_context(
                &context,
                &input,
                PixelFormat::RGB_FLT,
                &input,
                PixelFormat::RGB_FLT,
                &proof,
                Intent::RelativeColorimetric,
                Intent::RelativeColorimetric,
                Flags::NO_CACHE | Flags::GAMUT_CHECK | Flags::SOFT_PROOFING | Flags::BLACKPOINT_COMPENSATION,
            )?;
            let mut checked = encoded_source;
            transform.transform_in_place(&mut checked);
            Ok::<_, lcms2::Error>(checked)
        };
        let a = check([65535; 16])?;
        let b = check([0; 16])?;
        let outside: Vec<_> = a.iter().zip(b).map(|(a, b)| *a != b).collect();
        println!("{name:?} mature dual-alarm gamut flags: {outside:?}");
    }
    Ok(())
}
