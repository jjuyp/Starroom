//! Color-management boundaries for Starroom.
//! ICC parsing/execution is provided by LittleCMS. Published chromatic adaptation math lives here
//! so the render graph can keep file, working and display transforms explicit.

use lcms2::{CIExyY, CIExyYTRIPLE, Flags, Intent, PixelFormat, Profile, ToneCurve, Transform};
use serde::{Deserialize, Serialize};
use starroom_color::LinearRgb;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Xyz {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const D50: Xyz = Xyz {
    x: 0.96422,
    y: 1.0,
    z: 0.82521,
};
pub const D65: Xyz = Xyz {
    x: 0.95047,
    y: 1.0,
    z: 1.08883,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix3(pub [[f32; 3]; 3]);

impl Matrix3 {
    pub fn multiply_vec(self, value: Xyz) -> Xyz {
        Xyz {
            x: self.0[0][0] * value.x + self.0[0][1] * value.y + self.0[0][2] * value.z,
            y: self.0[1][0] * value.x + self.0[1][1] * value.y + self.0[1][2] * value.z,
            z: self.0[2][0] * value.x + self.0[2][1] * value.y + self.0[2][2] * value.z,
        }
    }

    pub fn multiply(self, other: Self) -> Self {
        let mut out = [[0.0; 3]; 3];
        for (row, values) in out.iter_mut().enumerate() {
            for (column, value) in values.iter_mut().enumerate() {
                *value = (0..3)
                    .map(|index| self.0[row][index] * other.0[index][column])
                    .sum();
            }
        }
        Self(out)
    }

    pub fn inverse(self) -> Option<Self> {
        let m = self.0;
        let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
        if det.abs() < 1.0e-8 || !det.is_finite() {
            return None;
        }
        let d = 1.0 / det;
        Some(Self([
            [
                (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * d,
                (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * d,
                (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * d,
            ],
            [
                (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * d,
                (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * d,
                (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * d,
            ],
            [
                (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * d,
                (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * d,
                (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * d,
            ],
        ]))
    }
}

const BRADFORD: Matrix3 = Matrix3([
    [0.8951, 0.2664, -0.1614],
    [-0.7502, 1.7135, 0.0367],
    [0.0389, -0.0685, 1.0296],
]);

const SRGB_TO_XYZ_D65: Matrix3 = Matrix3([
    [0.412_456_4, 0.357_576_1, 0.180_437_5],
    [0.212_672_9, 0.715_152_2, 0.072_175_0],
    [0.019_333_9, 0.119_192, 0.950_304_1],
]);

const XYZ_TO_SRGB_D65: Matrix3 = Matrix3([
    [3.240_454_2, -1.537_138_5, -0.498_531_4],
    [-0.969_266, 1.876_010_8, 0.041_556_0],
    [0.055_643_4, -0.204_025_9, 1.057_225_2],
]);

const REC2020_TO_XYZ_D65: Matrix3 = Matrix3([
    [0.636_958_06, 0.144_616_9, 0.168_880_98],
    [0.262_700_2, 0.677_998_07, 0.059_301_72],
    [0.0, 0.028_072_693, 1.060_985_1],
]);

const XYZ_TO_REC2020_D65: Matrix3 = Matrix3([
    [1.716_651_2, -0.355_670_78, -0.253_366_3],
    [-0.666_684_3, 1.616_481_2, 0.015_768_546],
    [0.017_639_857, -0.042_770_613, 0.942_103_1],
]);

pub fn bradford_adaptation(source_white: Xyz, destination_white: Xyz) -> Matrix3 {
    let source_lms = BRADFORD.multiply_vec(source_white);
    let destination_lms = BRADFORD.multiply_vec(destination_white);
    let scale = Matrix3([
        [destination_lms.x / source_lms.x, 0.0, 0.0],
        [0.0, destination_lms.y / source_lms.y, 0.0],
        [0.0, 0.0, destination_lms.z / source_lms.z],
    ]);
    let inverse = BRADFORD.inverse().expect("Bradford matrix is invertible");
    inverse.multiply(scale).multiply(BRADFORD)
}

pub fn adapt_xyz(value: Xyz, source_white: Xyz, destination_white: Xyz) -> Xyz {
    bradford_adaptation(source_white, destination_white).multiply_vec(value)
}

pub fn xyz_d65_to_rec2020_linear(value: Xyz) -> LinearRgb {
    let rec = XYZ_TO_REC2020_D65.multiply_vec(value);
    LinearRgb {
        r: rec.x,
        g: rec.y,
        b: rec.z,
    }
}

pub fn rec2020_linear_to_xyz_d65(value: LinearRgb) -> Xyz {
    REC2020_TO_XYZ_D65.multiply_vec(Xyz {
        x: value.r,
        y: value.g,
        z: value.b,
    })
}

fn srgb_eotf(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb_oetf(value: f32) -> f32 {
    let value = value.max(0.0);
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// Fallback input transform for rendered images without an embedded ICC profile.
/// Such files are treated as encoded sRGB, converted to Starroom's linear Rec.2020/D65 space.
pub fn srgb_encoded_to_rec2020_linear(rgb: [f32; 3]) -> LinearRgb {
    let linear_srgb = Xyz {
        x: srgb_eotf(rgb[0]),
        y: srgb_eotf(rgb[1]),
        z: srgb_eotf(rgb[2]),
    };
    let xyz = SRGB_TO_XYZ_D65.multiply_vec(linear_srgb);
    let rec = XYZ_TO_REC2020_D65.multiply_vec(xyz);
    LinearRgb {
        r: rec.x,
        g: rec.y,
        b: rec.z,
    }
}

/// Output fallback transform from Starroom linear Rec.2020/D65 to encoded sRGB. Gamut mapping
/// should occur before this function when the working RGB value is outside the target gamut.
pub fn rec2020_linear_to_srgb_encoded(rgb: LinearRgb) -> [f32; 3] {
    let xyz = REC2020_TO_XYZ_D65.multiply_vec(Xyz {
        x: rgb.r,
        y: rgb.g,
        z: rgb.b,
    });
    let linear_srgb = XYZ_TO_SRGB_D65.multiply_vec(xyz);
    [
        srgb_oetf(linear_srgb.x),
        srgb_oetf(linear_srgb.y),
        srgb_oetf(linear_srgb.z),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RenderingIntent {
    Perceptual,
    RelativeColorimetric,
    Saturation,
    AbsoluteColorimetric,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IccProfileDescriptor {
    pub name: String,
    pub fingerprint: String,
    pub embedded: bool,
}

pub trait IccTransformProvider {
    type Error;
    type Transform;

    fn build_transform(
        &self,
        input_profile: &[u8],
        output_profile: &[u8],
        intent: RenderingIntent,
        black_point_compensation: bool,
    ) -> Result<Self::Transform, Self::Error>;

    fn apply_rgb_f32(
        &self,
        transform: &Self::Transform,
        pixels: &mut [[f32; 3]],
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileRole {
    Input,
    Working,
    Output,
}

impl std::fmt::Display for ProfileRole {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Input => "input",
            Self::Working => "working",
            Self::Output => "output",
        })
    }
}

#[derive(Debug, Error)]
pub enum ColorManagementError {
    #[error("invalid {role} ICC profile: {source}")]
    InvalidProfile {
        role: ProfileRole,
        #[source]
        source: lcms2::Error,
    },
    #[error("LittleCMS could not create the {direction} transform: {source}")]
    TransformCreation {
        direction: &'static str,
        #[source]
        source: lcms2::Error,
    },
    #[error("{stage} contained NaN or infinity at pixel {pixel}, channel {channel}")]
    NonFinitePixel {
        stage: &'static str,
        pixel: usize,
        channel: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InputProfileSource {
    EmbeddedIcc,
    AssumedSrgb,
    /// RAW sensor data was decoded, white-balanced, demosaiced and converted by the pinned
    /// camera-matrix provider before entering the linear Rec.2020/D65 working graph.
    RawCameraMatrix,
    /// RAW camera identity was not recognized and no valid embedded DNG matrix was present.
    /// The explicitly reported Generic Profile is used; this is never a silent substitution.
    RawGenericProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputProfileSource {
    SuppliedIcc,
    Srgb,
}

pub struct LcmsTransform {
    transform: Transform<[f32; 3], [f32; 3]>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LittleCmsProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BuiltinOutputProfile {
    Srgb,
    DisplayP3,
    AdobeRgb,
    Rec2020,
}

fn lcms_intent(intent: RenderingIntent) -> Intent {
    match intent {
        RenderingIntent::Perceptual => Intent::Perceptual,
        RenderingIntent::RelativeColorimetric => Intent::RelativeColorimetric,
        RenderingIntent::Saturation => Intent::Saturation,
        RenderingIntent::AbsoluteColorimetric => Intent::AbsoluteColorimetric,
    }
}

fn rec2020_linear_d65_profile() -> Result<Profile, ColorManagementError> {
    let white = CIExyY {
        x: 0.3127,
        y: 0.3290,
        Y: 1.0,
    };
    let primaries = CIExyYTRIPLE {
        Red: CIExyY {
            x: 0.708,
            y: 0.292,
            Y: 1.0,
        },
        Green: CIExyY {
            x: 0.170,
            y: 0.797,
            Y: 1.0,
        },
        Blue: CIExyY {
            x: 0.131,
            y: 0.046,
            Y: 1.0,
        },
    };
    let red = ToneCurve::new(1.0);
    let green = ToneCurve::new(1.0);
    let blue = ToneCurve::new(1.0);
    Profile::new_rgb(&white, &primaries, &[&red, &green, &blue]).map_err(|source| {
        ColorManagementError::InvalidProfile {
            role: ProfileRole::Working,
            source,
        }
    })
}

fn ensure_finite(stage: &'static str, pixels: &[[f32; 3]]) -> Result<(), ColorManagementError> {
    for (pixel, rgb) in pixels.iter().enumerate() {
        for (channel, value) in rgb.iter().enumerate() {
            if !value.is_finite() {
                return Err(ColorManagementError::NonFinitePixel {
                    stage,
                    pixel,
                    channel,
                });
            }
        }
    }
    Ok(())
}

impl LittleCmsProvider {
    #[must_use]
    pub fn engine_version(&self) -> u32 {
        lcms2::version()
    }

    pub fn srgb_profile_bytes(&self) -> Result<Vec<u8>, ColorManagementError> {
        Profile::new_srgb()
            .icc()
            .map_err(|source| ColorManagementError::InvalidProfile {
                role: ProfileRole::Output,
                source,
            })
    }

    pub fn builtin_output_profile_bytes(
        &self,
        profile: BuiltinOutputProfile,
    ) -> Result<Vec<u8>, ColorManagementError> {
        if profile == BuiltinOutputProfile::Srgb {
            return self.srgb_profile_bytes();
        }
        let white = CIExyY {
            x: 0.3127,
            y: 0.3290,
            Y: 1.0,
        };
        let primaries = match profile {
            BuiltinOutputProfile::Srgb => unreachable!(),
            BuiltinOutputProfile::DisplayP3 => CIExyYTRIPLE {
                Red: CIExyY {
                    x: 0.680,
                    y: 0.320,
                    Y: 1.0,
                },
                Green: CIExyY {
                    x: 0.265,
                    y: 0.690,
                    Y: 1.0,
                },
                Blue: CIExyY {
                    x: 0.150,
                    y: 0.060,
                    Y: 1.0,
                },
            },
            BuiltinOutputProfile::AdobeRgb => CIExyYTRIPLE {
                Red: CIExyY {
                    x: 0.640,
                    y: 0.330,
                    Y: 1.0,
                },
                Green: CIExyY {
                    x: 0.210,
                    y: 0.710,
                    Y: 1.0,
                },
                Blue: CIExyY {
                    x: 0.150,
                    y: 0.060,
                    Y: 1.0,
                },
            },
            BuiltinOutputProfile::Rec2020 => CIExyYTRIPLE {
                Red: CIExyY {
                    x: 0.708,
                    y: 0.292,
                    Y: 1.0,
                },
                Green: CIExyY {
                    x: 0.170,
                    y: 0.797,
                    Y: 1.0,
                },
                Blue: CIExyY {
                    x: 0.131,
                    y: 0.046,
                    Y: 1.0,
                },
            },
        };
        let gamma = match profile {
            BuiltinOutputProfile::AdobeRgb => 2.199_218_8,
            _ => 2.2,
        };
        let red = ToneCurve::new(gamma);
        let green = ToneCurve::new(gamma);
        let blue = ToneCurve::new(gamma);
        Profile::new_rgb(&white, &primaries, &[&red, &green, &blue])
            .and_then(|profile| profile.icc())
            .map_err(|source| ColorManagementError::InvalidProfile {
                role: ProfileRole::Output,
                source,
            })
    }

    pub fn input_to_working(
        &self,
        pixels: &mut [[f32; 3]],
        embedded_icc: Option<&[u8]>,
        intent: RenderingIntent,
        black_point_compensation: bool,
    ) -> Result<InputProfileSource, ColorManagementError> {
        ensure_finite("encoded input", pixels)?;
        let source_kind = if embedded_icc.is_some() {
            InputProfileSource::EmbeddedIcc
        } else {
            InputProfileSource::AssumedSrgb
        };
        let input = match embedded_icc {
            Some(bytes) => {
                Profile::new_icc(bytes).map_err(|source| ColorManagementError::InvalidProfile {
                    role: ProfileRole::Input,
                    source,
                })?
            }
            None => Profile::new_srgb(),
        };
        let working = rec2020_linear_d65_profile()?;
        let transform = build_lcms_transform(
            &input,
            &working,
            intent,
            black_point_compensation,
            "input-to-working",
        )?;
        transform.transform.transform_in_place(pixels);
        ensure_finite("linear Rec.2020 working output", pixels)?;
        Ok(source_kind)
    }

    pub fn working_to_output(
        &self,
        pixels: &mut [[f32; 3]],
        output_icc: Option<&[u8]>,
        intent: RenderingIntent,
        black_point_compensation: bool,
    ) -> Result<OutputProfileSource, ColorManagementError> {
        ensure_finite("linear Rec.2020 working input", pixels)?;
        let output_kind = if output_icc.is_some() {
            OutputProfileSource::SuppliedIcc
        } else {
            OutputProfileSource::Srgb
        };
        let working = rec2020_linear_d65_profile()?;
        let output = match output_icc {
            Some(bytes) => {
                Profile::new_icc(bytes).map_err(|source| ColorManagementError::InvalidProfile {
                    role: ProfileRole::Output,
                    source,
                })?
            }
            None => Profile::new_srgb(),
        };
        let transform = build_lcms_transform(
            &working,
            &output,
            intent,
            black_point_compensation,
            "working-to-output",
        )?;
        transform.transform.transform_in_place(pixels);
        ensure_finite("encoded output", pixels)?;
        Ok(output_kind)
    }
}

fn build_lcms_transform(
    input: &Profile,
    output: &Profile,
    intent: RenderingIntent,
    black_point_compensation: bool,
    direction: &'static str,
) -> Result<LcmsTransform, ColorManagementError> {
    let flags = if black_point_compensation {
        Flags::BLACKPOINT_COMPENSATION
    } else {
        Flags::default()
    };
    Transform::new_flags(
        input,
        PixelFormat::RGB_FLT,
        output,
        PixelFormat::RGB_FLT,
        lcms_intent(intent),
        flags,
    )
    .map(|transform| LcmsTransform { transform })
    .map_err(|source| ColorManagementError::TransformCreation { direction, source })
}

impl IccTransformProvider for LittleCmsProvider {
    type Error = ColorManagementError;
    type Transform = LcmsTransform;

    fn build_transform(
        &self,
        input_profile: &[u8],
        output_profile: &[u8],
        intent: RenderingIntent,
        black_point_compensation: bool,
    ) -> Result<Self::Transform, Self::Error> {
        let input = Profile::new_icc(input_profile).map_err(|source| {
            ColorManagementError::InvalidProfile {
                role: ProfileRole::Input,
                source,
            }
        })?;
        let output = Profile::new_icc(output_profile).map_err(|source| {
            ColorManagementError::InvalidProfile {
                role: ProfileRole::Output,
                source,
            }
        })?;
        build_lcms_transform(
            &input,
            &output,
            intent,
            black_point_compensation,
            "profile-to-profile",
        )
    }

    fn apply_rgb_f32(
        &self,
        transform: &Self::Transform,
        pixels: &mut [[f32; 3]],
    ) -> Result<(), Self::Error> {
        ensure_finite("ICC transform input", pixels)?;
        transform.transform.transform_in_place(pixels);
        ensure_finite("ICC transform output", pixels)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum WorkingSpace {
    #[default]
    Rec2020LinearD65,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 2.0e-4
    }

    #[test]
    fn adapting_white_maps_source_white_to_destination_white() {
        let adapted = adapt_xyz(D65, D65, D50);
        assert!(close(adapted.x, D50.x));
        assert!(close(adapted.y, D50.y));
        assert!(close(adapted.z, D50.z));
    }

    #[test]
    fn same_white_adaptation_is_identity_for_sample() {
        let sample = Xyz {
            x: 0.3,
            y: 0.4,
            z: 0.2,
        };
        let adapted = adapt_xyz(sample, D65, D65);
        assert!(close(adapted.x, sample.x));
        assert!(close(adapted.y, sample.y));
        assert!(close(adapted.z, sample.z));
    }

    #[test]
    fn srgb_working_space_round_trip_is_close() {
        let samples = [
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.8, 0.3, 0.12],
            [0.1, 0.5, 0.9],
        ];
        for sample in samples {
            let working = srgb_encoded_to_rec2020_linear(sample);
            let restored = rec2020_linear_to_srgb_encoded(working);
            assert!(close(restored[0], sample[0]));
            assert!(close(restored[1], sample[1]));
            assert!(close(restored[2], sample[2]));
        }
    }

    #[test]
    fn neutral_gray_stays_neutral_through_working_space() {
        let working = srgb_encoded_to_rec2020_linear([0.5, 0.5, 0.5]);
        let restored = rec2020_linear_to_srgb_encoded(working);
        assert!(close(restored[0], restored[1]));
        assert!(close(restored[1], restored[2]));
    }

    #[test]
    fn d50_d65_adaptation_round_trip_is_stable() {
        let sample = Xyz {
            x: 0.21,
            y: 0.35,
            z: 0.18,
        };
        let d50 = adapt_xyz(sample, D65, D50);
        let restored = adapt_xyz(d50, D50, D65);
        assert!(close(restored.x, sample.x));
        assert!(close(restored.y, sample.y));
        assert!(close(restored.z, sample.z));
    }

    #[test]
    fn embedded_srgb_and_missing_profile_fallback_match() {
        let provider = LittleCmsProvider;
        let explicit = provider.srgb_profile_bytes().expect("serialize sRGB");
        let mut embedded_pixels = [[0.12, 0.5, 0.91], [0.8, 0.3, 0.1]];
        let mut fallback_pixels = embedded_pixels;
        let embedded_source = provider
            .input_to_working(
                &mut embedded_pixels,
                Some(&explicit),
                RenderingIntent::RelativeColorimetric,
                true,
            )
            .expect("embedded transform");
        let fallback_source = provider
            .input_to_working(
                &mut fallback_pixels,
                None,
                RenderingIntent::RelativeColorimetric,
                true,
            )
            .expect("fallback transform");
        assert_eq!(embedded_source, InputProfileSource::EmbeddedIcc);
        assert_eq!(fallback_source, InputProfileSource::AssumedSrgb);
        provider
            .working_to_output(
                &mut embedded_pixels,
                None,
                RenderingIntent::RelativeColorimetric,
                true,
            )
            .expect("embedded output transform");
        provider
            .working_to_output(
                &mut fallback_pixels,
                None,
                RenderingIntent::RelativeColorimetric,
                true,
            )
            .expect("fallback output transform");
        for (embedded, fallback) in embedded_pixels.iter().zip(fallback_pixels) {
            for channel in 0..3 {
                assert!(
                    (embedded[channel] - fallback[channel]).abs() <= 1.0 / 255.0,
                    "embedded/fallback sRGB identity exceeded one 8-bit code value: embedded={}, fallback={}",
                    embedded[channel],
                    fallback[channel]
                );
            }
        }
    }

    #[test]
    fn invalid_embedded_profile_is_an_error_not_a_fallback() {
        let provider = LittleCmsProvider;
        let mut pixels = [[0.2, 0.3, 0.4]];
        let result = provider.input_to_working(
            &mut pixels,
            Some(b"not an ICC profile"),
            RenderingIntent::RelativeColorimetric,
            true,
        );
        assert!(matches!(
            result,
            Err(ColorManagementError::InvalidProfile {
                role: ProfileRole::Input,
                ..
            })
        ));
    }

    #[test]
    fn non_finite_pixels_are_rejected_before_lcms() {
        let provider = LittleCmsProvider;
        let mut pixels = [[f32::NAN, 0.3, 0.4]];
        let result = provider.input_to_working(
            &mut pixels,
            None,
            RenderingIntent::RelativeColorimetric,
            true,
        );
        assert!(matches!(
            result,
            Err(ColorManagementError::NonFinitePixel { .. })
        ));
    }

    #[test]
    fn all_four_icc_rendering_intents_build_and_remain_finite() {
        let provider = LittleCmsProvider;
        let intents = [
            RenderingIntent::Perceptual,
            RenderingIntent::RelativeColorimetric,
            RenderingIntent::Saturation,
            RenderingIntent::AbsoluteColorimetric,
        ];
        for intent in intents {
            let mut pixels = [[0.18, 0.5, 0.82]];
            provider
                .input_to_working(&mut pixels, None, intent, true)
                .expect("input transform");
            provider
                .working_to_output(&mut pixels, None, intent, true)
                .expect("output transform");
            assert!(pixels[0].iter().all(|channel| channel.is_finite()));
        }
    }

    #[test]
    fn bundled_littlecms_version_matches_provenance_inventory() {
        assert_eq!(LittleCmsProvider.engine_version(), 2190);
    }
}
