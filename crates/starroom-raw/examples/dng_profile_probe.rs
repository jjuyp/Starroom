//! Native camera-profile counterexample, authored matrix data; not a camera-photo IQ fixture.
use starroom_color_management::{D65, Xyz, xyz_d65_to_rec2020_linear};
use starroom_raw::{CameraProfileInput, CameraProfileResolver, DngMatrixSet};

fn main() {
    let basis = [
        Xyz {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        },
        Xyz {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        },
        Xyz {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        },
    ];
    let mut dng = DngMatrixSet {
        parsed_fields: (1 << 1) | (1 << 2),
        illuminant: 21,
        ..Default::default()
    };
    for (column, xyz) in basis.into_iter().enumerate() {
        let rgb = xyz_d65_to_rec2020_linear(xyz);
        for (row, value) in [rgb.r, rgb.g, rgb.b].into_iter().enumerate() {
            dng.color_matrix[row][column] = value;
        }
    }
    let input = CameraProfileInput {
        make: "Starroom".into(),
        model: "Authored D65 camera-space oracle".into(),
        dng_version: 1,
        libraw_cam_xyz: [[0.0; 3]; 4],
        camera_neutral: [1.0; 4],
        analog_balance: [1.0; 4],
        dng: [dng, DngMatrixSet::default()],
    };
    let profile = CameraProfileResolver::resolve(&input);
    let actual = profile.camera_rgb_to_xyz_d65([1.0; 3]);
    let working = xyz_d65_to_rec2020_linear(Xyz {
        x: actual[0],
        y: actual[1],
        z: actual[2],
    });
    println!(
        "{}",
        serde_json::json!({"scope":"authored D65 ColorMatrix/neutral, real Native resolver",
        "profileId":profile.id,"profileVersion":profile.version,"profileHash":profile.hash,
        "expectedXyzD65":[D65.x,D65.y,D65.z],"actualXyzD65":actual,
        "expectedNeutralWorkingRgb":[1,1,1],"actualWorkingRgb":[working.r,working.g,working.b]})
    );
}
