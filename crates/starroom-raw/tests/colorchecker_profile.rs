use serde::Deserialize;
use starroom_color_management::quality::compare_xyz_d65;
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

#[derive(Default)]
struct QualitySummary {
    delta_e: f64,
    rmse: f64,
    luminance: f64,
}
impl QualitySummary {
    fn observe(&mut self, actual: [f32; 3], expected: Xyz, name: &str) {
        let metrics = compare_xyz_d65(
            Xyz {
                x: actual[0],
                y: actual[1],
                z: actual[2],
            },
            expected,
        )
        .unwrap();
        assert!(
            metrics.delta_e_2000 < 0.01,
            "{name} perceptual error: {metrics:?}"
        );
        assert!(
            metrics.xyz_rmse < 2e-5 && metrics.luminance_error < 2e-5,
            "{name}: {metrics:?}"
        );
        self.delta_e = self.delta_e.max(metrics.delta_e_2000);
        self.rmse = self.rmse.max(metrics.xyz_rmse);
        self.luminance = self.luminance.max(metrics.luminance_error);
    }
    fn report(&self, id: &str) {
        eprintln!(
            "COLORCHECKER_QUALITY profile={id} patches=24 max_delta_e_2000={} max_xyz_rmse={} max_luminance_error={}",
            self.delta_e, self.rmse, self.luminance
        );
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
        analog_balance: [1.0; 4],
        dng: [dng, DngMatrixSet::default()],
    });
    assert_eq!(profile.status, CameraProfileStatus::Resolved);
    assert_eq!(profile.source, CameraProfileSource::DngColorMatrix);
    let mut quality = QualitySummary::default();
    for patch in fixture().patches {
        let xyz = adapt_xyz(xy_y_to_xyz(patch.xy_y), D50, D65);
        let input = [
            xyz.x * diagonal[0] / neutral[0] / 4.0,
            xyz.y * diagonal[1] / 4.0,
            xyz.z * diagonal[2] / neutral[2] / 4.0,
        ];
        let output = profile.camera_rgb_to_xyz_d65(input);
        quality.observe(output, xyz, &patch.name);
        for (actual, expected) in output.into_iter().zip([xyz.x, xyz.y, xyz.z]) {
            assert!(
                (actual - expected).abs() < 2.0e-5,
                "{}: {actual} {expected}",
                patch.name
            );
        }
    }
    quality.report(&profile.id);
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
        analog_balance: [1.0; 4],
        dng,
    });
    assert_eq!(profile.status, CameraProfileStatus::Resolved);
    assert!((profile.dual_illuminant_weight.unwrap() - 0.5).abs() < 1e-5);
    let mut quality = QualitySummary::default();
    for patch in fixture().patches {
        let xyz = adapt_xyz(xy_y_to_xyz(patch.xy_y), D50, D65);
        let camera = [
            xyz.x * diagonal[0] / neutral[0] / 5.0,
            xyz.y,
            xyz.z * diagonal[2] / neutral[2] / 5.0,
        ];
        quality.observe(profile.camera_rgb_to_xyz_d65(camera), xyz, &patch.name);
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
    quality.report(&profile.id);
}

#[test]
fn colorchecker_analog_balance_preserves_d65_reference_camera_coordinates() {
    let analog = [2.0, 1.0, 0.5, 1.0];
    let neutral = [2.0 * D65.x, 1.0, 0.5 * D65.z, 1.0];
    let profile = CameraProfileResolver::resolve(&CameraProfileInput {
        make: "Starroom ColorChecker Oracle".into(),
        model: "Analog balance camera".into(),
        dng_version: 1,
        libraw_cam_xyz: [[0.0; 3]; 4],
        camera_neutral: neutral,
        analog_balance: analog,
        dng: [
            DngMatrixSet {
                parsed_fields: 1 << 2,
                color_matrix: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]],
                ..Default::default()
            },
            Default::default(),
        ],
    });
    assert_eq!(profile.status, CameraProfileStatus::Resolved);
    let mut quality = QualitySummary::default();
    for patch in fixture().patches {
        let xyz = adapt_xyz(xy_y_to_xyz(patch.xy_y), D50, D65);
        let camera = [xyz.x / D65.x, xyz.y, xyz.z / D65.z];
        quality.observe(profile.camera_rgb_to_xyz_d65(camera), xyz, &patch.name);
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
    quality.report(&profile.id);
}

#[test]
fn colorchecker_forward_profile_applies_calibration_in_baked_wb_domain() {
    let profile = CameraProfileResolver::resolve(&CameraProfileInput {
        make: "Starroom ColorChecker Oracle".into(),
        model: "Forward calibrated camera".into(),
        dng_version: 1,
        libraw_cam_xyz: [[0.0; 3]; 4],
        camera_neutral: [0.5, 1.0, 0.25, 1.0],
        analog_balance: [2.0, 1.0, 0.5, 1.0],
        dng: [
            DngMatrixSet {
                parsed_fields: 1 | (1 << 3),
                calibration: [
                    [1.0, 0.2, 0.0, 0.0],
                    [0.1, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0; 4],
                ],
                forward_matrix: [
                    [D50.x, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, D50.z, 0.0],
                ],
                ..Default::default()
            },
            Default::default(),
        ],
    });
    assert_eq!(profile.status, CameraProfileStatus::Resolved);
    let mut quality = QualitySummary::default();
    for patch in fixture().patches {
        let xyz = xy_y_to_xyz(patch.xy_y);
        // Independent inverse of the expanded camera->XYZ D50 2x2 calibrated block.
        let camera = [
            (40.0 * xyz.x / D50.x + 156.0 * xyz.y) / 196.0,
            (xyz.x / D50.x + 195.0 * xyz.y) / 196.0,
            xyz.z / D50.z,
        ];
        let expected = adapt_xyz(xyz, D50, D65);
        quality.observe(profile.camera_rgb_to_xyz_d65(camera), expected, &patch.name);
        for (actual, expected) in profile
            .camera_rgb_to_xyz_d65(camera)
            .into_iter()
            .zip([expected.x, expected.y, expected.z])
        {
            assert!(
                (actual - expected).abs() < 2e-5,
                "{}: {actual} {expected}",
                patch.name
            );
        }
    }
    quality.report(&profile.id);
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
        analog_balance: [1.0; 4],
        dng: [dng, DngMatrixSet::default()],
    });
    assert_eq!(profile.status, CameraProfileStatus::Resolved);
    assert_eq!(profile.source, CameraProfileSource::DngForwardMatrix);

    let fixture = fixture();
    assert_eq!(fixture.patches.len(), 24);
    let mut quality = QualitySummary::default();
    for patch in fixture.patches {
        let d50 = xy_y_to_xyz(patch.xy_y);
        let expected = adapt_xyz(d50, D50, D65);
        let actual = profile.camera_rgb_to_xyz_d65([d50.x, d50.y, d50.z]);
        quality.observe(actual, expected, &patch.name);
        assert!(
            actual.iter().all(|value| value.is_finite()),
            "{}",
            patch.name
        );
        assert!((actual[0] - expected.x).abs() < 1.0e-5, "{} X", patch.name);
        assert!((actual[1] - expected.y).abs() < 1.0e-5, "{} Y", patch.name);
        assert!((actual[2] - expected.z).abs() < 1.0e-5, "{} Z", patch.name);
    }
    quality.report(&profile.id);
}
