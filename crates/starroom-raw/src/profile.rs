use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use starroom_color_management::{D50, D65, Matrix3, Xyz, bradford_adaptation};

const DNG_FORWARD_MATRIX: u32 = 1;
const DNG_ILLUMINANT: u32 = 1 << 1;
const DNG_COLOR_MATRIX: u32 = 1 << 2;
const DNG_CALIBRATION: u32 = 1 << 3;

const GENERIC_SRGB_TO_XYZ_D65: Matrix3 = Matrix3([
    [0.412_456_4, 0.357_576_1, 0.180_437_5],
    [0.212_672_9, 0.715_152_2, 0.072_175_0],
    [0.019_333_9, 0.119_192, 0.950_304_1],
]);

pub const CAMERA_PROFILE_RESOLVER_VERSION: &str = "starroom-camera-profile-v5-analog-balance";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CameraFamily {
    Nikon,
    Canon,
    Sony,
    Fujifilm,
    EmbeddedDng,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CameraProfileStatus {
    Resolved,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CameraProfileSource {
    DngForwardMatrix,
    DngColorMatrix,
    DngForwardAndColorMatrix,
    LibRawCameraMatrix,
    GenericLinearSrgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationIlluminant {
    pub code: u16,
    pub kelvin: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DngMatrixSet {
    pub parsed_fields: u32,
    pub illuminant: u16,
    /// DNG ColorMatrix is camera channels by XYZ (up to 4x3).
    pub color_matrix: [[f32; 3]; 4],
    pub calibration: [[f32; 4]; 4],
    /// DNG ForwardMatrix is XYZ by camera channels (3x4).
    pub forward_matrix: [[f32; 4]; 3],
}

impl Default for DngMatrixSet {
    fn default() -> Self {
        Self {
            parsed_fields: 0,
            illuminant: 0,
            color_matrix: [[0.0; 3]; 4],
            calibration: [[0.0; 4]; 4],
            forward_matrix: [[0.0; 4]; 3],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CameraProfileInput {
    pub make: String,
    pub model: String,
    pub dng_version: u32,
    /// LibRaw XYZ-to-camera coefficients, camera channel rows and XYZ columns.
    pub libraw_cam_xyz: [[f32; 3]; 4],
    pub camera_neutral: [f32; 4],
    /// Original DNG AnalogBalance; LibRaw cam_xyz already includes it, raw DNG CM/CC do not.
    pub analog_balance: [f32; 4],
    pub dng: [DngMatrixSet; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraProfileDescriptor {
    pub id: String,
    pub version: String,
    pub hash: String,
    pub make: String,
    pub model: String,
    pub family: CameraFamily,
    pub status: CameraProfileStatus,
    pub source: CameraProfileSource,
    /// Explicit camera RGB -> XYZ D65 transform used before working-space conversion.
    pub camera_to_xyz_d65: [[f32; 3]; 3],
    pub calibration_illuminants: Vec<CalibrationIlluminant>,
    pub dual_illuminant_weight: Option<f32>,
}

impl CameraProfileDescriptor {
    pub fn camera_rgb_to_xyz_d65(&self, camera_rgb: [f32; 3]) -> [f32; 3] {
        let xyz = Matrix3(self.camera_to_xyz_d65).multiply_vec(Xyz {
            x: camera_rgb[0],
            y: camera_rgb[1],
            z: camera_rgb[2],
        });
        [xyz.x, xyz.y, xyz.z]
    }
}

#[derive(Debug, Clone, Copy)]
struct CandidateMatrix {
    matrix: Matrix3,
    color_matrix: Option<Matrix3>,
    calibration: Matrix3,
    used_forward: bool,
    illuminant: CalibrationIlluminant,
}

pub struct CameraProfileResolver;

impl CameraProfileResolver {
    pub fn resolve(input: &CameraProfileInput) -> CameraProfileDescriptor {
        let family = camera_family(&input.make);
        let dng_candidates: Vec<CandidateMatrix> =
            input.dng.iter().filter_map(dng_candidate).collect();

        let (status, source, matrix, illuminants, weight, resolved_family) =
            if input.dng_version != 0 && !dng_candidates.is_empty() {
                let all_forward = dng_candidates.iter().all(|item| item.used_forward);
                let none_forward = dng_candidates.iter().all(|item| !item.used_forward);
                let source = if all_forward {
                    CameraProfileSource::DngForwardMatrix
                } else if none_forward {
                    CameraProfileSource::DngColorMatrix
                } else {
                    CameraProfileSource::DngForwardAndColorMatrix
                };
                let transformed = interpolate_dng_candidates(&dng_candidates, input).and_then(
                    |(matrix, weight)| {
                        if none_forward {
                            let matrix = unbalanced_camera_matrix(matrix, input.analog_balance)?;
                            balanced_color_matrix_to_xyz_d65(matrix, input.camera_neutral)
                                .map(|matrix| (matrix, weight))
                        } else {
                            Some((adapt_matrix(matrix, D50, D65), weight))
                        }
                    },
                );
                if let Some((matrix, weight)) = transformed {
                    (
                        CameraProfileStatus::Resolved,
                        source,
                        matrix,
                        dng_candidates.iter().map(|item| item.illuminant).collect(),
                        weight,
                        CameraFamily::EmbeddedDng,
                    )
                } else {
                    generic_profile_tuple()
                }
            } else if family != CameraFamily::Unknown {
                if let Some(matrix) = libraw_camera_to_xyz(input.libraw_cam_xyz) {
                    (
                        CameraProfileStatus::Resolved,
                        CameraProfileSource::LibRawCameraMatrix,
                        matrix,
                        Vec::new(),
                        None,
                        family,
                    )
                } else {
                    generic_profile_tuple()
                }
            } else {
                generic_profile_tuple()
            };

        let source_name = match source {
            CameraProfileSource::DngForwardMatrix => "dng-forward-matrix",
            CameraProfileSource::DngColorMatrix => "dng-color-matrix",
            CameraProfileSource::DngForwardAndColorMatrix => "dng-mixed-matrix",
            CameraProfileSource::LibRawCameraMatrix => "libraw-camera-matrix",
            CameraProfileSource::GenericLinearSrgb => "generic-linear-srgb",
        };
        let id = format!(
            "{}:{}:{}",
            source_name,
            slug(&input.make),
            slug(&input.model)
        );
        let mut descriptor = CameraProfileDescriptor {
            id,
            version: CAMERA_PROFILE_RESOLVER_VERSION.to_owned(),
            hash: String::new(),
            make: input.make.clone(),
            model: input.model.clone(),
            family: resolved_family,
            status,
            source,
            camera_to_xyz_d65: matrix.0,
            calibration_illuminants: illuminants,
            dual_illuminant_weight: weight,
        };
        descriptor.hash = profile_hash(&descriptor);
        descriptor
    }
}

fn generic_profile_tuple() -> (
    CameraProfileStatus,
    CameraProfileSource,
    Matrix3,
    Vec<CalibrationIlluminant>,
    Option<f32>,
    CameraFamily,
) {
    (
        CameraProfileStatus::Generic,
        CameraProfileSource::GenericLinearSrgb,
        GENERIC_SRGB_TO_XYZ_D65,
        Vec::new(),
        None,
        CameraFamily::Unknown,
    )
}

fn camera_family(make: &str) -> CameraFamily {
    let normalized = make.trim().to_ascii_lowercase();
    if normalized.starts_with("nikon") {
        CameraFamily::Nikon
    } else if normalized.starts_with("canon") {
        CameraFamily::Canon
    } else if normalized.starts_with("sony") {
        CameraFamily::Sony
    } else if normalized.starts_with("fujifilm") || normalized.starts_with("fuji") {
        CameraFamily::Fujifilm
    } else {
        CameraFamily::Unknown
    }
}

fn dng_candidate(set: &DngMatrixSet) -> Option<CandidateMatrix> {
    let illuminant_code = if set.parsed_fields & DNG_ILLUMINANT != 0 {
        set.illuminant
    } else {
        0
    };
    let illuminant = CalibrationIlluminant {
        code: illuminant_code,
        kelvin: illuminant_kelvin(illuminant_code),
    };
    if set.parsed_fields & DNG_FORWARD_MATRIX != 0 {
        let matrix = Matrix3([
            [
                set.forward_matrix[0][0],
                set.forward_matrix[0][1],
                set.forward_matrix[0][2],
            ],
            [
                set.forward_matrix[1][0],
                set.forward_matrix[1][1],
                set.forward_matrix[1][2],
            ],
            [
                set.forward_matrix[2][0],
                set.forward_matrix[2][1],
                set.forward_matrix[2][2],
            ],
        ]);
        if valid_matrix(matrix) {
            return Some(CandidateMatrix {
                matrix,
                color_matrix: None,
                calibration: Matrix3::IDENTITY,
                used_forward: true,
                illuminant,
            });
        }
    }
    if set.parsed_fields & DNG_COLOR_MATRIX == 0 {
        return None;
    }
    let mut color = Matrix3([
        set.color_matrix[0],
        set.color_matrix[1],
        set.color_matrix[2],
    ]);
    let original_color = color;
    let mut camera_calibration = Matrix3::IDENTITY;
    if set.parsed_fields & DNG_CALIBRATION != 0 {
        let calibration = Matrix3([
            [
                set.calibration[0][0],
                set.calibration[0][1],
                set.calibration[0][2],
            ],
            [
                set.calibration[1][0],
                set.calibration[1][1],
                set.calibration[1][2],
            ],
            [
                set.calibration[2][0],
                set.calibration[2][1],
                set.calibration[2][2],
            ],
        ]);
        if valid_matrix(calibration) {
            camera_calibration = calibration;
            color = calibration.multiply(color);
        }
    }
    color
        .inverse()
        .filter(|matrix| valid_matrix(*matrix))
        .map(|matrix| CandidateMatrix {
            matrix,
            color_matrix: Some(original_color),
            calibration: camera_calibration,
            used_forward: false,
            illuminant,
        })
}

fn interpolate_dng_candidates(
    candidates: &[CandidateMatrix],
    input: &CameraProfileInput,
) -> Option<(Matrix3, Option<f32>)> {
    if candidates.len() == 1 {
        return Some((candidates[0].matrix, None));
    }
    let first = candidates[0];
    let second = candidates[1];
    let target_kelvin = estimated_as_shot_kelvin(input).or_else(|| {
        match (first.illuminant.kelvin, second.illuminant.kelvin) {
            (Some(a), Some(b)) => Some(2.0 / (1.0 / a + 1.0 / b)),
            _ => None,
        }
    });
    let weight = match (
        target_kelvin,
        first.illuminant.kelvin,
        second.illuminant.kelvin,
    ) {
        (Some(target), Some(a), Some(b)) if (a - b).abs() > 1.0 => {
            let target_mired = 1_000_000.0 / target;
            let a_mired = 1_000_000.0 / a;
            let b_mired = 1_000_000.0 / b;
            ((target_mired - a_mired) / (b_mired - a_mired)).clamp(0.0, 1.0)
        }
        _ => 0.5,
    };
    let matrix = if !first.used_forward && !second.used_forward {
        // DNG chapter 6 defines interpolated CC and CM separately. Interpolating endpoint
        // products introduces an extra cross-term and is not equivalent to CC * CM.
        let color = lerp_matrix(first.color_matrix?, second.color_matrix?, weight);
        let calibration = lerp_matrix(first.calibration, second.calibration, weight);
        calibration.multiply(color).inverse()?
    } else {
        lerp_matrix(first.matrix, second.matrix, weight)
    };
    valid_matrix(matrix).then_some((matrix, Some(weight)))
}

/// Estimates correlated color temperature from real RAW camera-neutral metadata.
pub fn estimated_as_shot_kelvin(input: &CameraProfileInput) -> Option<f32> {
    let matrix = libraw_camera_to_xyz(input.libraw_cam_xyz)?;
    let neutral = Xyz {
        x: input.camera_neutral[0],
        y: input.camera_neutral[1],
        z: input.camera_neutral[2],
    };
    if neutral.x <= 0.0 || neutral.y <= 0.0 || neutral.z <= 0.0 {
        return None;
    }
    let xyz = matrix.multiply_vec(neutral);
    let sum = xyz.x + xyz.y + xyz.z;
    if !sum.is_finite() || sum <= 1.0e-8 {
        return None;
    }
    let x = xyz.x / sum;
    let y = xyz.y / sum;
    // McCamy's approximation uses n = (x - 0.3320) / (y - 0.1858).
    // Reversing the denominator sign shifts a D65 neutral toward roughly 4600 K.
    let denominator = y - 0.1858;
    if denominator.abs() < 1.0e-6 {
        return None;
    }
    let n = (x - 0.3320) / denominator;
    let kelvin = -449.0 * n.powi(3) + 3525.0 * n.powi(2) - 6823.3 * n + 5520.33;
    (kelvin.is_finite() && (1_500.0..=25_000.0).contains(&kelvin)).then_some(kelvin)
}

fn libraw_camera_to_xyz(cam_xyz: [[f32; 3]; 4]) -> Option<Matrix3> {
    // Adapted from LibRaw 0.22.2 cam_xyz_coeff (utils_dcraw.cpp, LGPL-2.1-or-later).
    // The demosaiced buffer is already WB-scaled. Normalize XYZ->camera rows
    // against D65 before inversion, just as LibRaw normalizes cam_rgb before
    // deriving rgb_cam. Transposition is not an inverse for a camera matrix.
    let mut forward = Matrix3([cam_xyz[0], cam_xyz[1], cam_xyz[2]]);
    for row in &mut forward.0 {
        let white = row[0] * D65.x + row[1] * D65.y + row[2] * D65.z;
        if !white.is_finite() || white <= 1.0e-5 {
            return None;
        }
        for value in row {
            *value /= white;
        }
    }
    let matrix = forward.inverse()?;
    valid_matrix(matrix).then_some(matrix)
}

fn adapt_matrix(matrix: Matrix3, source_white: Xyz, destination_white: Xyz) -> Matrix3 {
    bradford_adaptation(source_white, destination_white).multiply(matrix)
}

/// DNG 1.7.1 chapter 6: XYZtoCamera = AB * CC * CM. The input matrix is
/// inverse(CC * CM), so append inverse(AB) on the right, never gain pixels a second time.
fn unbalanced_camera_matrix(matrix: Matrix3, analog: [f32; 4]) -> Option<Matrix3> {
    if !analog[..3].iter().all(|v| v.is_finite() && *v > 0.0) {
        return None;
    }
    let inverse_analog = Matrix3([
        [1.0 / analog[0], 0.0, 0.0],
        [0.0, 1.0 / analog[1], 0.0],
        [0.0, 0.0, 1.0 / analog[2]],
    ]);
    let output = matrix.multiply(inverse_analog);
    valid_matrix(output).then_some(output)
}

/// Adobe DNG 1.7.1 chapter 6: ColorMatrix inverts unbalanced camera coordinates, unlike
/// ForwardMatrix's already white-balanced D50 coordinates. The LibRaw boundary has baked WB;
/// undo that diagonal, find the measured neutral illuminant and adapt it, never assume D50.
fn balanced_color_matrix_to_xyz_d65(matrix: Matrix3, neutral: [f32; 4]) -> Option<Matrix3> {
    if !neutral[..3].iter().all(|v| v.is_finite() && *v > 0.0) {
        return None;
    }
    let white = matrix.multiply_vec(Xyz {
        x: neutral[0],
        y: neutral[1],
        z: neutral[2],
    });
    if ![white.x, white.y, white.z]
        .into_iter()
        .all(|v| v.is_finite() && v > 1.0e-8)
    {
        return None;
    }
    let undo_wb = Matrix3([
        [neutral[0] / white.y, 0.0, 0.0],
        [0.0, neutral[1] / white.y, 0.0],
        [0.0, 0.0, neutral[2] / white.y],
    ]);
    let white = Xyz {
        x: white.x / white.y,
        y: 1.0,
        z: white.z / white.y,
    };
    let output = bradford_adaptation(white, D65)
        .multiply(matrix)
        .multiply(undo_wb);
    valid_matrix(output).then_some(output)
}

fn lerp_matrix(first: Matrix3, second: Matrix3, weight: f32) -> Matrix3 {
    let mut result = [[0.0; 3]; 3];
    for (row, values) in result.iter_mut().enumerate() {
        for (column, value) in values.iter_mut().enumerate() {
            *value = first.0[row][column] * (1.0 - weight) + second.0[row][column] * weight;
        }
    }
    Matrix3(result)
}

fn valid_matrix(matrix: Matrix3) -> bool {
    matrix.0.iter().flatten().all(|value| value.is_finite()) && matrix.inverse().is_some()
}

fn illuminant_kelvin(code: u16) -> Option<f32> {
    match code {
        1 => Some(5_500.0),
        3 | 17 => Some(2_856.0),
        4 => Some(5_500.0),
        9 => Some(5_500.0),
        10 => Some(6_500.0),
        11 => Some(7_500.0),
        20 => Some(5_500.0),
        21 => Some(6_504.0),
        22 => Some(7_500.0),
        23 => Some(5_003.0),
        24 => Some(3_200.0),
        _ => None,
    }
}

fn profile_hash(descriptor: &CameraProfileDescriptor) -> String {
    let bytes = serde_json::to_vec(&(
        &descriptor.id,
        &descriptor.version,
        descriptor.family,
        descriptor.status,
        descriptor.source,
        descriptor.camera_to_xyz_d65,
        &descriptor.calibration_illuminants,
        descriptor.dual_illuminant_weight,
    ))
    .expect("camera profile fingerprint fields are serializable");
    format!("{:x}", Sha256::digest(bytes))
}

fn slug(value: &str) -> String {
    let normalized: String = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect();
    normalized.trim_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> CameraProfileInput {
        CameraProfileInput {
            make: "Unknown Maker".into(),
            model: "Test Camera".into(),
            dng_version: 0,
            libraw_cam_xyz: [[0.0; 3]; 4],
            camera_neutral: [0.5, 1.0, 0.7, 1.0],
            analog_balance: [1.0; 4],
            dng: [DngMatrixSet::default(), DngMatrixSet::default()],
        }
    }

    #[test]
    fn analog_balance_is_left_of_noncommuting_calibration_not_a_second_pixel_gain() {
        let color = Matrix3([[2.0, 0.1, 0.0], [0.2, 3.0, 0.1], [0.0, 0.3, 4.0]]);
        let calibration = Matrix3([[1.0, 0.2, 0.0], [0.1, 1.0, 0.0], [0.0, 0.1, 1.0]]);
        let analog = Matrix3([[2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.5]]);
        let expected = analog
            .multiply(calibration)
            .multiply(color)
            .inverse()
            .unwrap();
        let actual = unbalanced_camera_matrix(
            calibration.multiply(color).inverse().unwrap(),
            [2.0, 1.0, 0.5, 1.0],
        )
        .unwrap();
        for (actual, expected) in actual.0.iter().flatten().zip(expected.0.iter().flatten()) {
            assert!((actual - expected).abs() < 1e-6);
        }
        let wrong = calibration
            .multiply(analog)
            .multiply(color)
            .inverse()
            .unwrap();
        assert!((actual.0[0][1] - wrong.0[0][1]).abs() > 0.01);
    }

    #[test]
    fn invalid_dng_analog_balance_is_explicit_generic_not_identity_substitution() {
        let mut value = input();
        value.dng_version = 1;
        value.dng[0].parsed_fields = DNG_COLOR_MATRIX;
        for row in 0..3 {
            value.dng[0].color_matrix[row][row] = 1.0;
        }
        for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            value.analog_balance[0] = invalid;
            let profile = CameraProfileResolver::resolve(&value);
            assert_eq!(profile.status, CameraProfileStatus::Generic);
            assert_eq!(profile.source, CameraProfileSource::GenericLinearSrgb);
        }
    }

    #[test]
    fn libraw_camera_coefficients_do_not_receive_analog_balance_again() {
        let mut value = input();
        value.make = "Nikon".into();
        value.libraw_cam_xyz = [[0.6, 0.2, 0.0], [0.2, 0.7, 0.1], [0.1, 0.1, 0.8], [0.0; 3]];
        let original = CameraProfileResolver::resolve(&value);
        value.analog_balance = [2.0, 1.0, 0.5, 1.0];
        assert_eq!(original, CameraProfileResolver::resolve(&value));
    }

    #[test]
    fn unknown_camera_is_explicit_generic_profile() {
        let profile = CameraProfileResolver::resolve(&input());
        assert_eq!(profile.status, CameraProfileStatus::Generic);
        assert_eq!(profile.source, CameraProfileSource::GenericLinearSrgb);
        assert_eq!(profile.family, CameraFamily::Unknown);
        assert_eq!(profile.hash.len(), 64);
    }

    #[test]
    fn known_camera_uses_libraw_matrix() {
        let mut value = input();
        value.make = "NIKON CORPORATION".into();
        value.libraw_cam_xyz = [[0.6, 0.2, 0.0], [0.2, 0.7, 0.1], [0.1, 0.1, 0.8], [0.0; 3]];
        let profile = CameraProfileResolver::resolve(&value);
        assert_eq!(profile.status, CameraProfileStatus::Resolved);
        assert_eq!(profile.family, CameraFamily::Nikon);
        assert_eq!(profile.source, CameraProfileSource::LibRawCameraMatrix);
        let white = profile.camera_rgb_to_xyz_d65([1.0; 3]);
        for (actual, expected) in white.into_iter().zip([D65.x, D65.y, D65.z]) {
            assert!((actual - expected).abs() < 1.0e-5);
        }
    }

    #[test]
    fn as_shot_kelvin_comes_from_camera_neutral_and_profile_matrix() {
        let mut value = input();
        value.camera_neutral = [1.0; 4];
        value.libraw_cam_xyz = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]];
        let kelvin = estimated_as_shot_kelvin(&value).expect("valid RAW WB metadata");
        assert!((6_000.0..=7_000.0).contains(&kelvin));

        value.libraw_cam_xyz = [[0.0; 3]; 4];
        assert_eq!(estimated_as_shot_kelvin(&value), None);
    }

    #[test]
    fn known_camera_without_a_valid_matrix_is_explicit_generic() {
        let mut value = input();
        value.make = "Canon".into();
        let profile = CameraProfileResolver::resolve(&value);
        assert_eq!(profile.status, CameraProfileStatus::Generic);
        assert_eq!(profile.source, CameraProfileSource::GenericLinearSrgb);
        assert!(profile.id.starts_with("generic-linear-srgb:"));
    }

    #[test]
    fn dng_forward_matrix_is_adapted_from_d50_to_d65() {
        let mut value = input();
        value.dng_version = 1;
        value.dng[0].parsed_fields = DNG_FORWARD_MATRIX | DNG_ILLUMINANT;
        value.dng[0].illuminant = 23;
        value.dng[0].forward_matrix = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ];
        let profile = CameraProfileResolver::resolve(&value);
        assert_eq!(profile.source, CameraProfileSource::DngForwardMatrix);
        let white = profile.camera_rgb_to_xyz_d65([D50.x, D50.y, D50.z]);
        assert!((white[0] - D65.x).abs() < 1.0e-4);
        assert!((white[1] - D65.y).abs() < 1.0e-4);
        assert!((white[2] - D65.z).abs() < 1.0e-4);
    }

    #[test]
    fn dng_color_matrix_is_inverted() {
        let mut value = input();
        value.dng_version = 1;
        value.dng[0].parsed_fields = DNG_COLOR_MATRIX | DNG_ILLUMINANT;
        value.dng[0].illuminant = 23;
        value.dng[0].color_matrix[0] = [2.0, 0.0, 0.0];
        value.dng[0].color_matrix[1] = [0.0, 4.0, 0.0];
        value.dng[0].color_matrix[2] = [0.0, 0.0, 5.0];
        let profile = CameraProfileResolver::resolve(&value);
        assert_eq!(profile.source, CameraProfileSource::DngColorMatrix);
        let d50_xyz = Matrix3(profile.camera_to_xyz_d65).multiply_vec(Xyz {
            // The public resolver receives camera RGB after LibRaw WB. Convert this test's
            // unbalanced [2,4,5] to that explicit boundary instead of assuming XYZ is D50.
            x: 2.0 / value.camera_neutral[0],
            y: 4.0,
            z: 5.0 / value.camera_neutral[2],
        });
        let expected = bradford_adaptation(
            Xyz {
                x: 1.0,
                y: 1.0,
                z: 0.56,
            },
            D65,
        )
        .multiply_vec(Xyz {
            x: 4.0,
            y: 4.0,
            z: 4.0,
        });
        assert!((d50_xyz.x - expected.x).abs() < 1.0e-4);
        assert!((d50_xyz.y - expected.y).abs() < 1.0e-4);
        assert!((d50_xyz.z - expected.z).abs() < 1.0e-4);
    }

    #[test]
    fn dng_color_matrix_neutral_maps_to_d65_not_an_assumed_d50() {
        let mut value = input();
        value.dng_version = 1;
        value.camera_neutral = [1.0; 4];
        value.dng[0].parsed_fields = DNG_COLOR_MATRIX | DNG_ILLUMINANT;
        value.dng[0].illuminant = 21;
        // XYZ->camera matrix for a Rec.2020/D65 reference camera, independent published values.
        value.dng[0].color_matrix = [
            [1.716_651_2, -0.355_670_78, -0.253_366_3],
            [-0.666_684_3, 1.616_481_2, 0.015_768_546],
            [0.017_639_857, -0.042_770_613, 0.942_103_1],
            [0.0; 3],
        ];
        let profile = CameraProfileResolver::resolve(&value);
        assert_eq!(profile.status, CameraProfileStatus::Resolved);
        let actual = profile.camera_rgb_to_xyz_d65([1.0; 3]);
        for (a, b) in actual.into_iter().zip([D65.x, D65.y, D65.z]) {
            assert!((a - b).abs() < 2.0e-5);
        }
        value.camera_neutral[0] = 0.0;
        let invalid = CameraProfileResolver::resolve(&value);
        assert_eq!(invalid.status, CameraProfileStatus::Generic);
        assert_eq!(invalid.source, CameraProfileSource::GenericLinearSrgb);
    }

    #[test]
    fn dual_color_matrix_interpolation_happens_before_inversion() {
        let mut value = input();
        value.dng_version = 1;
        for (index, diagonal) in [[2.0, 4.0, 5.0], [4.0, 6.0, 7.0]].into_iter().enumerate() {
            value.dng[index].parsed_fields = DNG_COLOR_MATRIX | DNG_ILLUMINANT;
            value.dng[index].illuminant = if index == 0 { 17 } else { 21 };
            for (row, entry) in diagonal.into_iter().enumerate() {
                value.dng[index].color_matrix[row][row] = entry;
            }
        }
        let candidates: Vec<_> = value.dng.iter().filter_map(dng_candidate).collect();
        let (matrix, weight) = interpolate_dng_candidates(&candidates, &value).unwrap();
        assert!((weight.unwrap() - 0.5).abs() < 1.0e-5);
        for (row, expected) in [1.0 / 3.0, 1.0 / 5.0, 1.0 / 6.0].into_iter().enumerate() {
            assert!((matrix.0[row][row] - expected).abs() < 1.0e-6);
        }
    }

    #[test]
    fn dual_calibration_and_color_matrices_interpolate_independently_before_product() {
        let mut value = input();
        value.dng_version = 1;
        for index in 0..2 {
            let set = &mut value.dng[index];
            set.parsed_fields = DNG_COLOR_MATRIX | DNG_CALIBRATION | DNG_ILLUMINANT;
            set.illuminant = if index == 0 { 17 } else { 21 };
            let colors = if index == 0 {
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
                set.color_matrix[row][row] = colors[row];
                set.calibration[row][row] = calibration[row];
            }
        }
        let candidates: Vec<_> = value.dng.iter().filter_map(dng_candidate).collect();
        let (actual, weight) = interpolate_dng_candidates(&candidates, &value).unwrap();
        assert!((weight.unwrap() - 0.5).abs() < 1e-5);
        // Independent arithmetic oracle: average(CC)=[1,1,1], average(CM)=[3,5,6].
        for (row, expected) in [1.0 / 3.0, 1.0 / 5.0, 1.0 / 6.0].into_iter().enumerate() {
            assert!(
                (actual.0[row][row] - expected).abs() < 1e-6,
                "channel {row}: {} instead of {expected}",
                actual.0[row][row]
            );
        }
    }

    #[test]
    fn dual_noncommuting_calibration_preserves_cc_times_cm_order() {
        let mut value = input();
        value.dng_version = 1;
        let colors = [
            [[2.0, 0.1, 0.0], [0.0, 3.0, 0.1], [0.1, 0.0, 4.0]],
            [[3.0, 0.0, 0.2], [0.2, 4.0, 0.0], [0.0, 0.1, 5.0]],
        ];
        let calibrations = [
            [[1.0, 0.1, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [[1.0, 0.0, 0.0], [0.2, 1.0, 0.0], [0.0, 0.0, 1.0]],
        ];
        for index in 0..2 {
            let set = &mut value.dng[index];
            set.parsed_fields = DNG_COLOR_MATRIX | DNG_CALIBRATION | DNG_ILLUMINANT;
            set.illuminant = if index == 0 { 17 } else { 21 };
            for row in 0..3 {
                for column in 0..3 {
                    set.color_matrix[row][column] = colors[index][row][column];
                    set.calibration[row][column] = calibrations[index][row][column];
                }
            }
        }
        let candidates: Vec<_> = value.dng.iter().filter_map(dng_candidate).collect();
        let actual = interpolate_dng_candidates(&candidates, &value).unwrap().0;
        // Independently multiplied midpoint rows; reversing multiplication gives other values.
        let expected = Matrix3([
            [2.505, 0.225, 0.1025],
            [0.35, 3.505, 0.06],
            [0.05, 0.05, 4.5],
        ])
        .inverse()
        .unwrap();
        for row in 0..3 {
            for column in 0..3 {
                assert!((actual.0[row][column] - expected.0[row][column]).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn singular_interpolated_calibration_has_explicit_generic_profile_status() {
        let mut value = input();
        value.dng_version = 1;
        for index in 0..2 {
            let set = &mut value.dng[index];
            set.parsed_fields = DNG_COLOR_MATRIX | DNG_CALIBRATION | DNG_ILLUMINANT;
            set.illuminant = if index == 0 { 17 } else { 21 };
            for row in 0..3 {
                set.color_matrix[row][row] = 1.0;
                set.calibration[row][row] = if index == 0 { 1.0 } else { -1.0 };
            }
        }
        let profile = CameraProfileResolver::resolve(&value);
        assert_eq!(profile.status, CameraProfileStatus::Generic);
        assert_eq!(profile.source, CameraProfileSource::GenericLinearSrgb);
    }

    #[test]
    fn dual_illuminant_profile_interpolates_in_mired_space() {
        let mut value = input();
        value.dng_version = 1;
        for (index, illuminant) in [17, 21].into_iter().enumerate() {
            value.dng[index].parsed_fields = DNG_FORWARD_MATRIX | DNG_ILLUMINANT;
            value.dng[index].illuminant = illuminant;
            let diagonal = if index == 0 { 1.0 } else { 2.0 };
            value.dng[index].forward_matrix = [
                [diagonal, 0.0, 0.0, 0.0],
                [0.0, diagonal, 0.0, 0.0],
                [0.0, 0.0, diagonal, 0.0],
            ];
        }
        let profile = CameraProfileResolver::resolve(&value);
        let weight = profile.dual_illuminant_weight.expect("dual matrix weight");
        assert!((0.0..=1.0).contains(&weight));
        assert_eq!(profile.calibration_illuminants.len(), 2);
    }

    #[test]
    fn dual_color_matrices_and_camera_calibration_are_supported() {
        let mut value = input();
        value.dng_version = 1;
        for (index, illuminant) in [17, 21].into_iter().enumerate() {
            value.dng[index].parsed_fields = DNG_COLOR_MATRIX | DNG_CALIBRATION | DNG_ILLUMINANT;
            value.dng[index].illuminant = illuminant;
            value.dng[index].color_matrix[0] = [1.0 + index as f32 * 0.1, 0.0, 0.0];
            value.dng[index].color_matrix[1] = [0.0, 1.0, 0.0];
            value.dng[index].color_matrix[2] = [0.0, 0.0, 1.0 - index as f32 * 0.1];
            value.dng[index].calibration[0][0] = 1.01;
            value.dng[index].calibration[1][1] = 1.0;
            value.dng[index].calibration[2][2] = 0.99;
        }
        let profile = CameraProfileResolver::resolve(&value);
        assert_eq!(profile.source, CameraProfileSource::DngColorMatrix);
        assert_eq!(profile.calibration_illuminants.len(), 2);
        assert!(profile.dual_illuminant_weight.is_some());
        assert!(
            profile
                .camera_to_xyz_d65
                .iter()
                .flatten()
                .all(|value| value.is_finite())
        );
    }

    #[test]
    fn resolved_matrix_round_trip_is_finite() {
        let mut value = input();
        value.make = "SONY".into();
        value.libraw_cam_xyz = [
            [0.65, 0.21, 0.02],
            [0.18, 0.70, 0.08],
            [0.09, 0.09, 0.83],
            [0.0; 3],
        ];
        let profile = CameraProfileResolver::resolve(&value);
        let matrix = Matrix3(profile.camera_to_xyz_d65);
        let inverse = matrix.inverse().expect("resolved matrix is invertible");
        let sample = Xyz {
            x: 0.18,
            y: 0.42,
            z: 0.09,
        };
        let round_trip = inverse.multiply_vec(matrix.multiply_vec(sample));
        assert!((round_trip.x - sample.x).abs() < 1.0e-5);
        assert!((round_trip.y - sample.y).abs() < 1.0e-5);
        assert!((round_trip.z - sample.z).abs() < 1.0e-5);
    }
}
