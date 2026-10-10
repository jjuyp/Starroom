use serde::Deserialize;
use starroom_color_management::{D50, D65, Xyz, adapt_xyz};
use starroom_raw::{
    CameraProfileInput, CameraProfileResolver, CameraProfileSource, CameraProfileStatus,
    DngMatrixSet,
};
use std::{fs, path::PathBuf};

#[derive(Deserialize)]
struct Fixture {
    patches: Vec<Patch>,
}

#[derive(Deserialize)]
struct Patch {
    name: String,
    #[serde(rename = "xyY")]
    xy_y: [f32; 3],
}

fn fixture() -> Fixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/colorchecker/babelcolor-average-v0.4.7.json");
    serde_json::from_slice(&fs::read(path).expect("ColorChecker fixture"))
        .expect("valid ColorChecker fixture")
}

fn xy_y_to_xyz(value: [f32; 3]) -> Xyz {
    let [x, y, luminance] = value;
    Xyz {
        x: x * luminance / y,
        y: luminance,
        z: (1.0 - x - y) * luminance / y,
    }
}

#[test]
fn colorchecker_color_matrix_undoes_baked_wb_and_preserves_d65_chart_colors() {
    let diagonal = [2.0, 4.0, 5.0];
    let white = [D65.x, D65.y, D65.z];
    let neutral = [
        diagonal[0] * white[0] / 4.0,
        1.0,
        diagonal[2] * white[2] / 4.0,
        1.0,
    ];
    let mut dng = DngMatrixSet {
        parsed_fields: (1 << 1) | (1 << 2),
        illuminant: 21,
        ..Default::default()
    };
    for (row, value) in diagonal.into_iter().enumerate() {
        dng.color_matrix[row][row] = value;
    }
    let profile = CameraProfileResolver::resolve(&CameraProfileInput {
        make: "Starroom ColorChecker Oracle".into(),
        model: "D65 ColorMatrix camera".into(),
        dng_version: 1,
        libraw_cam_xyz: [[0.0; 3]; 4],
        camera_neutral: neutral,
        dng: [dng, DngMatrixSet::default()],
    });
    assert_eq!(profile.status, CameraProfileStatus::Resolved);
    assert_eq!(profile.source, CameraProfileSource::DngColorMatrix);
    for patch in fixture().patches {
        let xyz = adapt_xyz(xy_y_to_xyz(patch.xy_y), D50, D65);
        let input = [
            xyz.x * diagonal[0] / neutral[0] / 4.0,
            xyz.y * diagonal[1] / 4.0,
            xyz.z * diagonal[2] / neutral[2] / 4.0,
        ];
        let output = profile.camera_rgb_to_xyz_d65(input);
        for (actual, expected) in output.into_iter().zip([xyz.x, xyz.y, xyz.z]) {
            assert!(
                (actual - expected).abs() < 2.0e-5,
                "{}: {actual} {expected}",
                patch.name
            );
        }
    }
}

#[test]
fn colorchecker_dual_calibration_uses_independent_interpolated_matrices() {
    // Independent midpoint oracle: mean(CC) = identity, mean(CM) = diag(3,5,6).
    // Existing resolver uses its explicit midpoint fallback when LibRaw has no CCT oracle.
    let diagonal = [3.0, 5.0, 6.0];
    let neutral = [
        diagonal[0] * D65.x / 5.0,
        1.0,
        diagonal[2] * D65.z / 5.0,
        1.0,
    ];
    let mut dng = [DngMatrixSet::default(), DngMatrixSet::default()];
    for (index, set) in dng.iter_mut().enumerate() {
        set.parsed_fields = (1 << 1) | (1 << 2) | (1 << 3);
        set.illuminant = if index == 0 { 17 } else { 21 };
        let color = if index == 0 {
            [2.0, 4.0, 5.0]
        } else {
            [4.0, 6.0, 7.0]
        };
        let calibration = if index == 0 {
            [0.8, 1.1, 1.0]
        } else {
            [1.2, 0.9, 1.0]
        };
        for row in 0..3 {
            set.color_matrix[row][row] = color[row];
            set.calibration[row][row] = calibration[row];
        }
    }
    let profile = CameraProfileResolver::resolve(&CameraProfileInput {
        make: "Starroom ColorChecker Oracle".into(),
        model: "Dual CC/CM camera".into(),
        dng_version: 1,
        libraw_cam_xyz: [[0.0; 3]; 4],
        camera_neutral: neutral,
        dng,
    });
    assert_eq!(profile.status, CameraProfileStatus::Resolved);
    assert!((profile.dual_illuminant_weight.unwrap() - 0.5).abs() < 1e-5);
    for patch in fixture().patches {
        let xyz = adapt_xyz(xy_y_to_xyz(patch.xy_y), D50, D65);
        let camera = [
            xyz.x * diagonal[0] / neutral[0] / 5.0,
            xyz.y,
            xyz.z * diagonal[2] / neutral[2] / 5.0,
        ];
        for (actual, expected) in profile
            .camera_rgb_to_xyz_d65(camera)
            .into_iter()
            .zip([xyz.x, xyz.y, xyz.z])
        {
            assert!(
                (actual - expected).abs() < 2e-5,
                "{}: {actual} {expected}",
                patch.name
            );
        }
    }
}

#[test]
fn colorchecker_d50_forward_profile_matches_bradford_d65_reference() {
    let mut dng = DngMatrixSet {
        parsed_fields: 1 | (1 << 1),
        illuminant: 23,
        ..DngMatrixSet::default()
    };
    dng.forward_matrix = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ];
    let profile = CameraProfileResolver::resolve(&CameraProfileInput {
        make: "ColorChecker Fixture".into(),
        model: "D50 Forward Matrix".into(),
        dng_version: 1,
        libraw_cam_xyz: [[0.0; 3]; 4],
        camera_neutral: [1.0; 4],
        dng: [dng, DngMatrixSet::default()],
    });
    assert_eq!(profile.status, CameraProfileStatus::Resolved);
    assert_eq!(profile.source, CameraProfileSource::DngForwardMatrix);

    let fixture = fixture();
    assert_eq!(fixture.patches.len(), 24);
    for patch in fixture.patches {
        let d50 = xy_y_to_xyz(patch.xy_y);
        let expected = adapt_xyz(d50, D50, D65);
        let actual = profile.camera_rgb_to_xyz_d65([d50.x, d50.y, d50.z]);
        assert!(
            actual.iter().all(|value| value.is_finite()),
            "{}",
            patch.name
        );
        assert!((actual[0] - expected.x).abs() < 1.0e-5, "{} X", patch.name);
        assert!((actual[1] - expected.y).abs() < 1.0e-5, "{} Y", patch.name);
        assert!((actual[2] - expected.z).abs() < 1.0e-5, "{} Z", patch.name);
    }
}
