use starroom_color::{CurvePoint, ToneParameters};
use starroom_color_management::InputProfileSource;
use starroom_geometry::{CropRect, GeometryParameters};
use starroom_imageio::{DecodedSourceImage, decode_source_preview};
use starroom_pipeline::{
    RenderSettings, ToneCurveSet, WhiteBalanceMode, WhiteBalanceSample, WhiteBalanceSettings,
    prepare_source_for_ai_denoise, render_source_export_to_srgb8, render_source_preview_to_srgb8,
    sample_source_color_band,
};
use std::{path::PathBuf, time::Instant};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/raw/sources/nikon-d1.nef")
}

#[test]
fn raw_native_sampling_and_neutral_picker_follow_visible_geometry_with_camera_profile() {
    let source = fixture();
    let source_before = std::fs::read(&source).unwrap();
    let decoded = decode_source_preview(&source, 256).expect("real LibRaw sensor decode");
    for geometry in [
        GeometryParameters {
            rotation_degrees: 90.0,
            ..Default::default()
        },
        GeometryParameters {
            flip_horizontal: true,
            ..Default::default()
        },
        GeometryParameters {
            crop: CropRect {
                left: 0.1,
                top: 0.1,
                right: 0.9,
                bottom: 0.9,
            },
            flip_vertical: true,
            ..Default::default()
        },
    ] {
        let settings = RenderSettings {
            geometry,
            white_balance: WhiteBalanceSettings {
                mode: WhiteBalanceMode::NeutralPicker,
                sample: Some(WhiteBalanceSample {
                    x: 0.49,
                    y: 0.49,
                    width: 0.02,
                    height: 0.02,
                }),
            },
            ..Default::default()
        };
        let working = prepare_source_for_ai_denoise(&decoded, &settings).unwrap();
        assert!(working.data.iter().all(|value| value.is_finite()));
        // The actually visible sampled patch is neutral in the high-precision working stage.
        let left = (0.49 * working.width as f32).floor() as usize;
        let right = (0.51 * working.width as f32).ceil() as usize;
        let top = (0.49 * working.height as f32).floor() as usize;
        let bottom = (0.51 * working.height as f32).ceil() as usize;
        let mut sum = [0.0_f32; 3];
        for y in top..bottom {
            for x in left..right {
                for (channel, sum) in sum.iter_mut().enumerate() {
                    *sum += working.data[(y * working.width + x) * 3 + channel];
                }
            }
        }
        assert!((sum[0] - sum[1]).abs() < 1.0e-4);
        assert!((sum[1] - sum[2]).abs() < 1.0e-4);
        sample_source_color_band(&decoded, &settings, 0.5, 0.5).expect("native RAW target picker");
        let preview = render_source_preview_to_srgb8(&decoded, &settings).unwrap();
        assert_eq!(preview.color.input, InputProfileSource::RawCameraMatrix);
        assert!(
            preview
                .color
                .camera_profile_id
                .as_deref()
                .is_some_and(|id| id.contains("nikon"))
        );
        assert_eq!(
            preview,
            render_source_export_to_srgb8(&decoded, &settings).unwrap()
        );
    }
    assert_eq!(
        source_before,
        std::fs::read(source).unwrap(),
        "RAW sensor file must remain immutable"
    );
}

#[test]
fn raw_curve_preview_and_export_share_the_four_channel_stage() {
    let decoded = decode_source_preview(fixture(), 512).expect("LibRaw preview decode");
    let settings = RenderSettings {
        curves: ToneCurveSet {
            master: vec![
                CurvePoint { x: 0.0, y: 0.04 },
                CurvePoint { x: 0.5, y: 0.55 },
                CurvePoint { x: 1.0, y: 1.0 },
            ],
            blue: vec![CurvePoint { x: 0.0, y: 0.0 }, CurvePoint { x: 1.0, y: 0.9 }],
            ..Default::default()
        },
        ..Default::default()
    };
    let preview = render_source_preview_to_srgb8(&decoded, &settings).expect("RAW curve preview");
    let export = render_source_export_to_srgb8(&decoded, &settings).expect("RAW curve export");
    assert_eq!(preview, export);
}

#[test]
fn raw_preview_and_export_use_the_same_native_graph() {
    let decode_start = Instant::now();
    let decoded = decode_source_preview(fixture(), 1024).expect("LibRaw preview decode");
    let decode_time = decode_start.elapsed();
    let DecodedSourceImage::Raw(raw) = &decoded else {
        panic!("NEF must enter the RAW path");
    };
    assert!(raw.preview_half_size);

    let first_preview_start = Instant::now();
    let preview = render_source_preview_to_srgb8(&decoded, &RenderSettings::default())
        .expect("first native preview");
    let first_preview_time = first_preview_start.elapsed();
    let export = render_source_export_to_srgb8(&decoded, &RenderSettings::default())
        .expect("same shared graph");
    assert_eq!(preview, export);
    assert_eq!(preview.color.input, InputProfileSource::RawCameraMatrix);
    assert!(
        preview
            .color
            .camera_profile_id
            .as_deref()
            .is_some_and(|id| id.contains("nikon"))
    );
    assert_eq!(
        preview.color.camera_profile_hash.as_deref().map(str::len),
        Some(64)
    );

    let slider_settings = RenderSettings {
        tone: ToneParameters {
            exposure_ev: 0.75,
            shadows: 0.25,
            highlights: -0.2,
            ..Default::default()
        },
        ..Default::default()
    };
    let slider_start = Instant::now();
    let adjusted =
        render_source_preview_to_srgb8(&decoded, &slider_settings).expect("slider rerender");
    let slider_time = slider_start.elapsed();
    assert_ne!(preview.data, adjusted.data);
    assert!(decode_time.as_secs_f64() < 120.0);
    assert!(first_preview_time.as_secs_f64() < 30.0);
    assert!(slider_time.as_secs_f64() < 30.0);
    eprintln!(
        "RAW_PREVIEW_METRIC decode_ms={:.2} first_preview_ms={:.2} slider_rerender_ms={:.2}",
        decode_time.as_secs_f64() * 1_000.0,
        first_preview_time.as_secs_f64() * 1_000.0,
        slider_time.as_secs_f64() * 1_000.0,
    );
}
