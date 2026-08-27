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

pub const CAMERA_PROFILE_RESOLVER_VERSION: &str = "starroom-camera-profile-v1";

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
    /// LibRaw camera-to-XYZ rows are stored by camera channel, then XYZ.
    pub libraw_cam_xyz: [[f32; 3]; 4],
    pub camera_neutral: [f32; 4],
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
                let (matrix, weight) = interpolate_dng_candidates(&dng_candidates, input);
                let all_forward = dng_candidates.iter().all(|item| item.used_forward);
                let none_forward = dng_candidates.iter().all(|item| !item.used_forward);
                let source = if all_forward {
                    CameraProfileSource::DngForwardMatrix
                } else if none_forward {
                    CameraProfileSource::DngColorMatrix
                } else {
                    CameraProfileSource::DngForwardAndColorMatrix
                };
                (
                    CameraProfileStatus::Resolved,
                    source,
                    adapt_matrix(matrix, D50, D65),
                    dng_candidates.iter().map(|item| item.illuminant).collect(),
                    weight,
                    CameraFamily::EmbeddedDng,
                )
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
            color = calibration.multiply(color);
        }
    }
    color
        .inverse()
        .filter(|matrix| valid_matrix(*matrix))
        .map(|matrix| CandidateMatrix {
            matrix,
            used_forward: false,
            illuminant,
        })
}

fn interpolate_dng_candidates(
    candidates: &[CandidateMatrix],
    input: &CameraProfileInput,
) -> (Matrix3, Option<f32>) {
    if candidates.len() == 1 {
        return (candidates[0].matrix, None);
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
    (
        lerp_matrix(first.matrix, second.matrix, weight),
        Some(weight),
    )
}

fn estimated_as_shot_kelvin(input: &CameraProfileInput) -> Option<f32> {
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
    let denominator = 0.1858 - y;
    if denominator.abs() < 1.0e-6 {
        return None;
    }
    let n = (x - 0.3320) / denominator;
    let kelvin = -449.0 * n.powi(3) + 3525.0 * n.powi(2) - 6823.3 * n + 5520.33;
    (kelvin.is_finite() && (1_500.0..=25_000.0).contains(&kelvin)).then_some(kelvin)
}

fn libraw_camera_to_xyz(cam_xyz: [[f32; 3]; 4]) -> Option<Matrix3> {
    let matrix = Matrix3([
        [cam_xyz[0][0], cam_xyz[1][0], cam_xyz[2][0]],
        [cam_xyz[0][1], cam_xyz[1][1], cam_xyz[2][1]],
        [cam_xyz[0][2], cam_xyz[1][2], cam_xyz[2][2]],
    ]);
    valid_matrix(matrix).then_some(matrix)
}

fn adapt_matrix(matrix: Matrix3, source_white: Xyz, destination_white: Xyz) -> Matrix3 {
    bradford_adaptation(source_white, destination_white).multiply(matrix)
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
            dng: [DngMatrixSet::default(), DngMatrixSet::default()],
        }
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
            x: 2.0,
            y: 4.0,
            z: 5.0,
        });
        let expected = bradford_adaptation(D50, D65).multiply_vec(Xyz {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        });
        assert!((d50_xyz.x - expected.x).abs() < 1.0e-4);
        assert!((d50_xyz.y - expected.y).abs() < 1.0e-4);
        assert!((d50_xyz.z - expected.z).abs() < 1.0e-4);
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
