use starroom_color::{CurvePoint, ToneParameters};
use starroom_color_management::InputProfileSource;
use starroom_imageio::{DecodedSourceImage, decode_source_preview};
use starroom_pipeline::{
    RenderSettings, ToneCurveSet, render_source_export_to_srgb8, render_source_preview_to_srgb8,
};
use std::{path::PathBuf, time::Instant};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/raw/sources/nikon-d1.nef")
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
