//! Preserve every former Browser pixel-test intent against the production Native graph.
//! The frozen migration corpus remains separate and unchanged; no Browser math runs here.
use starroom_color::{CurvePoint, PreparedCurve};
use starroom_imageio::{DecodedRenderedImage, RenderedFormat};
use starroom_pipeline::{RenderSettings, render_export_to_srgb8, render_preview_to_srgb8};

fn source(rgb: &[[u8; 3]], width: u32, height: u32) -> DecodedRenderedImage {
    DecodedRenderedImage {
        width,
        height,
        format: RenderedFormat::Png,
        rgba: rgb
            .iter()
            .flat_map(|pixel| {
                [
                    f32::from(pixel[0]) / 255.0,
                    f32::from(pixel[1]) / 255.0,
                    f32::from(pixel[2]) / 255.0,
                    1.0,
                ]
            })
            .collect(),
        embedded_icc: None,
        exif: None,
    }
}

fn render(source: &DecodedRenderedImage, settings: &RenderSettings) -> Vec<u8> {
    let preview = render_preview_to_srgb8(source, settings).unwrap();
    assert_eq!(preview, render_export_to_srgb8(source, settings).unwrap());
    preview.data
}

#[test]
fn legacy_neutral_adjustments_remain_pixel_exact_in_native_graph() {
    assert_eq!(
        render(&source(&[[40, 80, 120]], 1, 1), &RenderSettings::default()),
        [40, 80, 120]
    );
}

#[test]
fn legacy_exposure_changes_all_rendered_channels_in_native_graph() {
    let mut settings = RenderSettings::default();
    settings.tone.exposure_ev = 1.0;
    let output = render(&source(&[[40, 80, 120]], 1, 1), &settings);
    assert!(
        output
            .iter()
            .zip([40, 80, 120])
            .all(|(actual, original)| *actual > original)
    );
}

#[test]
fn legacy_curve_controls_change_actual_native_shadow_and_highlight_pixels() {
    let settings = RenderSettings {
        curve: vec![
            CurvePoint { x: 0.0, y: 0.0 },
            CurvePoint { x: 0.25, y: 0.5 },
            CurvePoint { x: 0.75, y: 0.5 },
            CurvePoint { x: 1.0, y: 1.0 },
        ],
        ..Default::default()
    };
    let output = render(&source(&[[35; 3], [128; 3], [220; 3]], 3, 1), &settings);
    assert!(output[0] > 35);
    assert!(output[6] < 220);
}

#[test]
fn legacy_monotone_curve_intent_is_preserved_by_shared_native_spline() {
    let curve = PreparedCurve::new(&[
        CurvePoint { x: 0.0, y: 0.0 },
        CurvePoint { x: 0.25, y: 0.18 },
        CurvePoint { x: 0.5, y: 0.58 },
        CurvePoint { x: 1.0, y: 1.0 },
    ]);
    let mut previous = curve.map(0.0);
    for index in 1..=100 {
        let output = curve.map(index as f32 / 100.0);
        assert!(output >= previous - 1e-5);
        assert!((0.0..=1.0).contains(&output));
        previous = output;
    }
}

#[test]
fn legacy_shadow_lift_targets_dark_pixels_more_than_twice_midtone_gain() {
    let mut settings = RenderSettings::default();
    settings.tone.shadows = 0.5;
    let output = render(&source(&[[35, 30, 25], [130, 120, 110]], 2, 1), &settings);
    let dark_gain = i32::from(output[0]) - 35;
    let mid_gain = i32::from(output[3]) - 130;
    assert!(dark_gain > 0);
    assert!(
        dark_gain > mid_gain * 2,
        "dark gain {dark_gain}, mid gain {mid_gain}"
    );
}

#[test]
fn legacy_black_anchor_survives_native_shadows_at_maximum() {
    let mut settings = RenderSettings::default();
    settings.tone.shadows = 1.0;
    assert_eq!(render(&source(&[[0; 3]], 1, 1), &settings), [0; 3]);
}

#[test]
fn legacy_sharpness_produces_a_visible_native_edge_change() {
    let mut pixels = [[80; 3]; 9];
    pixels[4] = [160; 3];
    let mut settings = RenderSettings::default();
    settings.sharpen.amount = 1.0;
    assert!(render(&source(&pixels, 3, 3), &settings)[12] > 160);
}

#[test]
fn legacy_relative_temperature_warms_encoded_native_gray() {
    let mut settings = RenderSettings::default();
    settings.relative_color.temperature = 0.7;
    let output = render(&source(&[[100; 3]], 1, 1), &settings);
    assert!(output[0] > output[2]);
}
