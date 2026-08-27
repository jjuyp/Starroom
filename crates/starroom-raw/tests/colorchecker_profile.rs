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
