//! Color-management boundaries for Starroom.
//! ICC parsing/execution is provided by LittleCMS. Published chromatic adaptation math lives here
//! so the render graph can keep file, working and display transforms explicit.

use lcms2::{
    CIEXYZ, CIEXYZExt, CIExyY, CIExyYTRIPLE, DisallowCache, Flags, GlobalContext, Intent,
    PixelFormat, Profile, ToneCurve, Transform,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use starroom_color::LinearRgb;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, OnceLock},
};
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
    pub const IDENTITY: Self = Self([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
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

/// Build a scene-linear Rec.2020 chromatic-adaptation matrix with the already linked LittleCMS
/// provider. White points are chromaticities (Y is normalized); image luminance is not clipped.
/// Three basis vectors are adapted once, never one FFI call per image pixel.
pub fn chromatic_adaptation_rec2020(
    source_white: Xyz,
    destination_white: Xyz,
) -> Result<Matrix3, ColorManagementError> {
    let normalize = |white: Xyz| -> Result<CIEXYZ, ColorManagementError> {
        if ![white.x, white.y, white.z]
            .into_iter()
            .all(|v| v.is_finite() && v > 1.0e-8)
        {
            return Err(ColorManagementError::InvalidWhitePoint);
        }
        let normalized = CIEXYZ {
            X: f64::from(white.x) / f64::from(white.y),
            Y: 1.0,
            Z: f64::from(white.z) / f64::from(white.y),
        };
        if ![normalized.X, normalized.Z]
            .into_iter()
            .all(|v| v.is_finite() && v < 1.0e6)
        {
            return Err(ColorManagementError::InvalidWhitePoint);
        }
        Ok(normalized)
    };
    let source = normalize(source_white)?;
    let destination = normalize(destination_white)?;
    if source == destination {
        return Ok(Matrix3([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]));
    }
    let mut xyz_matrix = [[0.0; 3]; 3];
    let basis = [
        CIEXYZ {
            X: 1.0,
            Y: 0.0,
            Z: 0.0,
        },
        CIEXYZ {
            X: 0.0,
            Y: 1.0,
            Z: 0.0,
        },
        CIEXYZ {
            X: 0.0,
            Y: 0.0,
            Z: 1.0,
        },
    ];
    for (column, value) in basis.into_iter().enumerate() {
        let adapted = value
            .adapt_to_illuminant(&source, &destination)
            .ok_or(ColorManagementError::InvalidWhitePoint)?;
        for (row, value) in [adapted.X, adapted.Y, adapted.Z].into_iter().enumerate() {
            if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
                return Err(ColorManagementError::InvalidWhitePoint);
            }
            xyz_matrix[row][column] = value as f32;
        }
    }
    Ok(XYZ_TO_REC2020_D65
        .multiply(Matrix3(xyz_matrix))
        .multiply(REC2020_TO_XYZ_D65))
}

/// A measured neutral in working RGB defines an illuminant; adapt it to the exact D65 white
/// represented by the working matrices, preserving the sample's luminance instead of using
/// a green-channel anchor. Invalid/black measurements remain explicit errors.
pub fn measured_neutral_adaptation(sample: LinearRgb) -> Result<Matrix3, ColorManagementError> {
    if ![sample.r, sample.g, sample.b]
        .into_iter()
        .all(|v| v.is_finite() && v > 1.0e-8)
    {
        return Err(ColorManagementError::InvalidWhitePoint);
    }
    chromatic_adaptation_rec2020(
        rec2020_linear_to_xyz_d65(sample),
        rec2020_linear_to_xyz_d65(LinearRgb {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        }),
    )
}

/// Relative encoded-image warm/cool and green/magenta intent. LittleCMS supplies the daylight
/// locus and Bradford adapter; Starroom maps its relative UI units, never claims source Kelvin.
/// Temperature follows reciprocal-temperature displacement around D65. Tint is perpendicular
/// to that locus in CIE 1960 u/v, not an OKLab a/b offset added to every pixel.
pub fn relative_white_balance_adaptation(
    temperature: f32,
    tint: f32,
) -> Result<Matrix3, ColorManagementError> {
    if !temperature.is_finite() || !tint.is_finite() {
        return Err(ColorManagementError::InvalidWhitePoint);
    }
    let temperature = f64::from(temperature.clamp(-1.0, 1.0));
    let tint = f64::from(tint.clamp(-1.0, 1.0));
    if temperature == 0.0 && tint == 0.0 {
        return Ok(Matrix3::IDENTITY);
    }
    // Both endpoints and the finite-difference locus samples stay in the mature LCMS
    // daylight provider's documented 4000..25000K domain. These are mapping anchors,
    // not a measurement of the source camera or encoded photograph's color temperature.
    const BASE_K: f64 = 6504.0;
    let kelvin = 1.0e6 / (1.0e6 / BASE_K + temperature * 65.0);
    let uv = |kelvin: f64| -> Result<[f64; 2], ColorManagementError> {
        let white =
            lcms2::white_point_from_temp(kelvin).ok_or(ColorManagementError::InvalidWhitePoint)?;
        let denominator = -2.0 * white.x + 12.0 * white.y + 3.0;
        Ok([4.0 * white.x / denominator, 6.0 * white.y / denominator])
    };
    let base = uv(BASE_K)?;
    let target = uv(kelvin)?;
    let warm = uv(kelvin - 10.0)?;
    let cool = uv(kelvin + 10.0)?;
    let tangent = [cool[0] - warm[0], cool[1] - warm[1]];
    let length = tangent[0].hypot(tangent[1]);
    if !length.is_finite() || length < 1.0e-12 {
        return Err(ColorManagementError::InvalidWhitePoint);
    }
    let source = rec2020_linear_to_xyz_d65(LinearRgb {
        r: 1.0,
        g: 1.0,
        b: 1.0,
    });
    let sum = f64::from(source.x + source.y + source.z);
    let x = f64::from(source.x) / sum;
    let y = f64::from(source.y) / sum;
    let denominator = -2.0 * x + 12.0 * y + 3.0;
    let u = 4.0 * x / denominator + target[0] - base[0] - tangent[1] / length * tint * 0.01;
    let v = 6.0 * y / denominator + target[1] - base[1] + tangent[0] / length * tint * 0.01;
    let denominator = 2.0 * u - 8.0 * v + 4.0;
    let x = 3.0 * u / denominator;
    let y = 2.0 * v / denominator;
    chromatic_adaptation_rec2020(
        source,
        Xyz {
            x: (x / y) as f32,
            y: 1.0,
            z: ((1.0 - x - y) / y) as f32,
        },
    )
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
    #[error("LittleCMS transform cache is unavailable")]
    TransformCacheUnavailable,
    #[error("chromatic adaptation requires a finite, positive, nonsingular measured white point")]
    InvalidWhitePoint,
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
    #[error("LittleCMS generated an invalid {role} ICC header")]
    GeneratedProfileHeader { role: ProfileRole },
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

struct ParallelLcmsTransform {
    transform: Transform<[f32; 3], [f32; 3], GlobalContext, DisallowCache>,
}

const TRANSFORM_CACHE_ENTRIES: usize = 8;
const TRANSFORM_CACHE_PROFILE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TransformDirection {
    Input,
    Output,
}

impl TransformDirection {
    fn role(self) -> ProfileRole {
        match self {
            Self::Input => ProfileRole::Input,
            Self::Output => ProfileRole::Output,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Input => "input-to-working",
            Self::Output => "working-to-output",
        }
    }
}

struct CachedTransform {
    direction: TransformDirection,
    profile: Option<Vec<u8>>,
    intent: RenderingIntent,
    black_point_compensation: bool,
    transform: Arc<ParallelLcmsTransform>,
}

#[derive(Default, Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IccTransformCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub builds: u64,
    pub evictions: u64,
    pub entries: usize,
    /// Retained exact profile keys only, not native transform/CLUT or physical process memory.
    pub profile_bytes: usize,
}

#[derive(Default)]
struct IccTransformCache {
    entries: VecDeque<CachedTransform>,
    stats: IccTransformCacheStats,
}

impl IccTransformCache {
    fn snapshot(&self) -> IccTransformCacheStats {
        IccTransformCacheStats {
            entries: self.entries.len(),
            profile_bytes: self
                .entries
                .iter()
                .map(|entry| entry.profile.as_ref().map_or(0, Vec::len))
                .sum(),
            ..self.stats
        }
    }

    fn lookup(
        &mut self,
        direction: TransformDirection,
        profile: Option<&[u8]>,
        intent: RenderingIntent,
        black_point_compensation: bool,
    ) -> Result<Option<Arc<ParallelLcmsTransform>>, ColorManagementError> {
        if let Some(index) = self.entries.iter().position(|entry| {
            entry.direction == direction
                && entry.profile.as_deref() == profile
                && entry.intent == intent
                && entry.black_point_compensation == black_point_compensation
        }) {
            let entry = self
                .entries
                .remove(index)
                .ok_or(ColorManagementError::TransformCacheUnavailable)?;
            let transform = Arc::clone(&entry.transform);
            self.entries.push_back(entry);
            self.stats.hits = self.stats.hits.saturating_add(1);
            return Ok(Some(transform));
        }
        self.stats.misses = self.stats.misses.saturating_add(1);
        Ok(None)
    }

    fn retain(
        &mut self,
        direction: TransformDirection,
        profile: Option<&[u8]>,
        intent: RenderingIntent,
        black_point_compensation: bool,
        transform: Arc<ParallelLcmsTransform>,
    ) -> Arc<ParallelLcmsTransform> {
        self.stats.builds = self.stats.builds.saturating_add(1);
        // Another worker may have populated this key while we built outside the lock. Do not
        // retain duplicate native objects or duplicate profile bytes. Count the actual build.
        if let Some(entry) = self.entries.iter().find(|entry| {
            entry.direction == direction
                && entry.profile.as_deref() == profile
                && entry.intent == intent
                && entry.black_point_compensation == black_point_compensation
        }) {
            return Arc::clone(&entry.transform);
        }
        let bytes = profile.map_or(0, <[u8]>::len);
        // Large valid ICCs still execute through LittleCMS, but are not retained. Never replace
        // their profile or flags to meet a cache budget. Invalid profiles are never cached.
        if bytes <= TRANSFORM_CACHE_PROFILE_BYTES {
            while self.entries.len() >= TRANSFORM_CACHE_ENTRIES
                || self.snapshot().profile_bytes + bytes > TRANSFORM_CACHE_PROFILE_BYTES
            {
                self.entries.pop_front();
                self.stats.evictions = self.stats.evictions.saturating_add(1);
            }
            self.entries.push_back(CachedTransform {
                direction,
                profile: profile.map(<[u8]>::to_vec),
                intent,
                black_point_compensation,
                transform: Arc::clone(&transform),
            });
        }
        transform
    }

    #[cfg(test)]
    fn get(
        &mut self,
        direction: TransformDirection,
        profile: Option<&[u8]>,
        intent: RenderingIntent,
        black_point_compensation: bool,
    ) -> Result<Arc<ParallelLcmsTransform>, ColorManagementError> {
        if let Some(transform) =
            self.lookup(direction, profile, intent, black_point_compensation)?
        {
            return Ok(transform);
        }
        let transform =
            create_parallel_transform(direction, profile, intent, black_point_compensation)?;
        Ok(self.retain(
            direction,
            profile,
            intent,
            black_point_compensation,
            transform,
        ))
    }
}

fn create_parallel_transform(
    direction: TransformDirection,
    profile: Option<&[u8]>,
    intent: RenderingIntent,
    black_point_compensation: bool,
) -> Result<Arc<ParallelLcmsTransform>, ColorManagementError> {
    let named = match profile {
        Some(bytes) => {
            Profile::new_icc(bytes).map_err(|source| ColorManagementError::InvalidProfile {
                role: direction.role(),
                source,
            })?
        }
        None => Profile::new_srgb(),
    };
    let working = rec2020_linear_d65_profile()?;
    let (input, output) = match direction {
        TransformDirection::Input => (&named, &working),
        TransformDirection::Output => (&working, &named),
    };
    build_parallel_lcms_transform(
        input,
        output,
        intent,
        black_point_compensation,
        direction.name(),
    )
    .map(Arc::new)
}

fn transform_cache() -> &'static Mutex<IccTransformCache> {
    static CACHE: OnceLock<Mutex<IccTransformCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(IccTransformCache::default()))
}

pub fn icc_transform_cache_stats() -> Result<IccTransformCacheStats, ColorManagementError> {
    transform_cache()
        .lock()
        .map(|cache| cache.snapshot())
        .map_err(|_| ColorManagementError::TransformCacheUnavailable)
}

fn cached_parallel_transform(
    direction: TransformDirection,
    profile: Option<&[u8]>,
    intent: RenderingIntent,
    black_point_compensation: bool,
) -> Result<Arc<ParallelLcmsTransform>, ColorManagementError> {
    // Neither ICC parsing/building nor pixel execution holds the shared cache lock. An export
    // constructing a new profile must not block an interactive preview's warm transform lookup.
    if let Some(transform) = transform_cache()
        .lock()
        .map_err(|_| ColorManagementError::TransformCacheUnavailable)?
        .lookup(direction, profile, intent, black_point_compensation)?
    {
        return Ok(transform);
    }
    let transform =
        create_parallel_transform(direction, profile, intent, black_point_compensation)?;
    Ok(transform_cache()
        .lock()
        .map_err(|_| ColorManagementError::TransformCacheUnavailable)?
        .retain(
            direction,
            profile,
            intent,
            black_point_compensation,
            transform,
        ))
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

/// LittleCMS initializes bytes 24..36 of a newly generated ICC profile from the wall clock.
/// Built-in Starroom profiles are release resources, not user-authored profiles, so their
/// creation metadata must not make otherwise identical exports differ across seconds.
fn canonicalize_generated_profile(
    mut bytes: Vec<u8>,
    role: ProfileRole,
) -> Result<Vec<u8>, ColorManagementError> {
    const ICC_SIGNATURE_OFFSET: usize = 36;
    const ICC_CREATED_OFFSET: usize = 24;
    const ICC_CREATED_END: usize = 36;
    // 2026-01-01 00:00:00, encoded as six big-endian ICC uInt16Number fields.
    const STARROOM_PROFILE_EPOCH: [u8; 12] = [
        0x07, 0xea, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    if bytes.len() < 128
        || bytes.get(ICC_SIGNATURE_OFFSET..ICC_SIGNATURE_OFFSET + 4) != Some(b"acsp")
    {
        return Err(ColorManagementError::GeneratedProfileHeader { role });
    }
    bytes[ICC_CREATED_OFFSET..ICC_CREATED_END].copy_from_slice(&STARROOM_PROFILE_EPOCH);
    Ok(bytes)
}

impl LittleCmsProvider {
    #[must_use]
    pub fn engine_version(&self) -> u32 {
        lcms2::version()
    }

    pub fn srgb_profile_bytes(&self) -> Result<Vec<u8>, ColorManagementError> {
        let bytes =
            Profile::new_srgb()
                .icc()
                .map_err(|source| ColorManagementError::InvalidProfile {
                    role: ProfileRole::Output,
                    source,
                })?;
        canonicalize_generated_profile(bytes, ProfileRole::Output)
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
        // Display P3 is not cinema DCI-P3 or a generic gamma-2.2 RGB space: its named
        // transfer is IEC sRGB. Use mature LCMS type-4 analytic curves, not a sampled JS/CPU
        // approximation. Adobe RGB retains its exact 563/256 photographic TRC. Rec.2020
        // here remains the existing explicitly SDR gamma-2.2 photographic ICC policy.
        let curve = match profile {
            BuiltinOutputProfile::DisplayP3 => ToneCurve::new_parametric(
                4,
                &[2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045],
            )
            .map_err(|source| ColorManagementError::InvalidProfile {
                role: ProfileRole::Output,
                source,
            })?,
            BuiltinOutputProfile::AdobeRgb => ToneCurve::new(563.0 / 256.0),
            _ => ToneCurve::new(2.2),
        };
        let bytes = Profile::new_rgb(&white, &primaries, &[&curve, &curve, &curve])
            .and_then(|profile| profile.icc())
            .map_err(|source| ColorManagementError::InvalidProfile {
                role: ProfileRole::Output,
                source,
            })?;
        canonicalize_generated_profile(bytes, ProfileRole::Output)
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
        let transform = cached_parallel_transform(
            TransformDirection::Input,
            embedded_icc,
            intent,
            black_point_compensation,
        )?;
        transform_pixels_parallel(&transform, pixels);
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
        let transform = cached_parallel_transform(
            TransformDirection::Output,
            output_icc,
            intent,
            black_point_compensation,
        )?;
        transform_pixels_parallel(&transform, pixels);
        ensure_finite("encoded output", pixels)?;
        Ok(output_kind)
    }
}

fn transform_pixels_parallel(transform: &ParallelLcmsTransform, pixels: &mut [[f32; 3]]) {
    const PIXELS_PER_CHUNK: usize = 16_384;
    pixels
        .par_chunks_mut(PIXELS_PER_CHUNK)
        .for_each(|chunk| transform.transform.transform_in_place(chunk));
}

fn build_parallel_lcms_transform(
    input: &Profile,
    output: &Profile,
    intent: RenderingIntent,
    black_point_compensation: bool,
    direction: &'static str,
) -> Result<ParallelLcmsTransform, ColorManagementError> {
    let flags = if black_point_compensation {
        Flags::NO_CACHE | Flags::BLACKPOINT_COMPENSATION
    } else {
        Flags::NO_CACHE
    };
    Transform::<[f32; 3], [f32; 3], GlobalContext, DisallowCache>::new_flags_context(
        GlobalContext::new(),
        input,
        PixelFormat::RGB_FLT,
        output,
        PixelFormat::RGB_FLT,
        lcms_intent(intent),
        flags,
    )
    .map(|transform| ParallelLcmsTransform { transform })
    .map_err(|source| ColorManagementError::TransformCreation { direction, source })
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

    #[test]
    fn icc_cache_reuses_only_exact_profile_direction_intent_and_bpc() {
        let mut cache = IccTransformCache::default();
        let intent = RenderingIntent::RelativeColorimetric;
        let first = cache
            .get(TransformDirection::Input, None, intent, true)
            .unwrap();
        let same = cache
            .get(TransformDirection::Input, None, intent, true)
            .unwrap();
        assert!(Arc::ptr_eq(&first, &same));
        let output = cache
            .get(TransformDirection::Output, None, intent, true)
            .unwrap();
        let perceptual = cache
            .get(
                TransformDirection::Input,
                None,
                RenderingIntent::Perceptual,
                true,
            )
            .unwrap();
        let no_bpc = cache
            .get(TransformDirection::Input, None, intent, false)
            .unwrap();
        for other in [&output, &perceptual, &no_bpc] {
            assert!(!Arc::ptr_eq(&first, other));
        }
        let explicit_srgb = LittleCmsProvider.srgb_profile_bytes().unwrap();
        let embedded = cache
            .get(
                TransformDirection::Input,
                Some(&explicit_srgb),
                intent,
                true,
            )
            .unwrap();
        let p3 = LittleCmsProvider
            .builtin_output_profile_bytes(BuiltinOutputProfile::DisplayP3)
            .unwrap();
        let different = cache
            .get(TransformDirection::Input, Some(&p3), intent, true)
            .unwrap();
        assert!(!Arc::ptr_eq(&embedded, &different));
        assert_eq!(cache.snapshot().hits, 1);
        assert_eq!(cache.snapshot().builds, 6);
        assert_eq!(cache.snapshot().entries, 6);
    }

    #[test]
    fn icc_cache_bounds_keys_evicts_lru_and_does_not_retain_invalid_or_oversized_profiles() {
        let mut cache = IccTransformCache::default();
        let intent = RenderingIntent::RelativeColorimetric;
        let profile = LittleCmsProvider.srgb_profile_bytes().unwrap();
        let first = cache
            .get(TransformDirection::Input, Some(&profile), intent, true)
            .unwrap();
        for index in 1..=TRANSFORM_CACHE_ENTRIES {
            let mut key = profile.clone();
            key.resize(key.len() + index, 0); // Legal unused trailing bytes, distinct exact identity.
            cache
                .get(TransformDirection::Input, Some(&key), intent, true)
                .unwrap();
        }
        assert_eq!(cache.snapshot().entries, TRANSFORM_CACHE_ENTRIES);
        assert_eq!(cache.snapshot().evictions, 1);
        let rebuilt = cache
            .get(TransformDirection::Input, Some(&profile), intent, true)
            .unwrap();
        assert!(!Arc::ptr_eq(&first, &rebuilt));
        let before = cache.snapshot();
        for _ in 0..2 {
            assert!(matches!(
                cache.get(TransformDirection::Input, Some(b"invalid"), intent, true),
                Err(ColorManagementError::InvalidProfile {
                    role: ProfileRole::Input,
                    ..
                })
            ));
        }
        assert_eq!(cache.snapshot().entries, before.entries);
        assert_eq!(cache.snapshot().builds, before.builds);
        let mut large = profile;
        large.resize(TRANSFORM_CACHE_PROFILE_BYTES + 1, 0);
        cache
            .get(TransformDirection::Output, Some(&large), intent, true)
            .unwrap();
        assert_eq!(cache.snapshot().entries, before.entries);
        assert!(cache.snapshot().profile_bytes <= TRANSFORM_CACHE_PROFILE_BYTES);
        let mut bounded = IccTransformCache::default();
        large.truncate(TRANSFORM_CACHE_PROFILE_BYTES / 2 + 1);
        bounded
            .get(TransformDirection::Input, Some(&large), intent, true)
            .unwrap();
        bounded
            .get(TransformDirection::Output, Some(&large), intent, true)
            .unwrap();
        assert_eq!(bounded.snapshot().entries, 1);
        assert_eq!(bounded.snapshot().evictions, 1);
    }

    #[test]
    fn shared_cached_lcms_transform_is_exact_against_uncached_parallel_reference() {
        let mut cache = IccTransformCache::default();
        let intent = RenderingIntent::RelativeColorimetric;
        for direction in [TransformDirection::Input, TransformDirection::Output] {
            let cached = cache.get(direction, None, intent, true).unwrap();
            let srgb = Profile::new_srgb();
            let working = rec2020_linear_d65_profile().unwrap();
            let (input, output) = match direction {
                TransformDirection::Input => (&srgb, &working),
                TransformDirection::Output => (&working, &srgb),
            };
            let reference =
                build_parallel_lcms_transform(input, output, intent, true, direction.name())
                    .unwrap();
            let source = vec![[0.18, 0.6, 0.93], [-0.05, 1.5, 0.3], [0.0, 0.0, 0.0]];
            let mut expected = source.clone();
            transform_pixels_parallel(&reference, &mut expected);
            std::thread::scope(|scope| {
                let jobs = (0..4)
                    .map(|_| {
                        let cached = Arc::clone(&cached);
                        let mut pixels = source.clone();
                        scope.spawn(move || {
                            transform_pixels_parallel(&cached, &mut pixels);
                            pixels
                        })
                    })
                    .collect::<Vec<_>>();
                for job in jobs {
                    assert_eq!(job.join().unwrap(), expected);
                }
            });
        }
    }

    #[test]
    fn duplicate_cold_builds_retain_one_exact_transform_key() {
        let mut cache = IccTransformCache::default();
        let direction = TransformDirection::Input;
        let intent = RenderingIntent::RelativeColorimetric;
        assert!(
            cache
                .lookup(direction, None, intent, true)
                .unwrap()
                .is_none()
        );
        assert!(
            cache
                .lookup(direction, None, intent, true)
                .unwrap()
                .is_none()
        );
        let first = create_parallel_transform(direction, None, intent, true).unwrap();
        let second = create_parallel_transform(direction, None, intent, true).unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        let retained = cache.retain(direction, None, intent, true, first);
        let raced = cache.retain(direction, None, intent, true, second);
        assert!(Arc::ptr_eq(&retained, &raced));
        assert_eq!(cache.snapshot().entries, 1);
        assert_eq!(cache.snapshot().builds, 2);
        assert_eq!(cache.snapshot().misses, 2);
    }

    #[test]
    fn relative_wb_cat_has_correct_axes_neutral_identity_and_black_anchor() {
        assert_eq!(
            relative_white_balance_adaptation(0.0, 0.0).unwrap(),
            Matrix3::IDENTITY
        );
        let gray = Xyz {
            x: 0.18,
            y: 0.18,
            z: 0.18,
        };
        let warm = relative_white_balance_adaptation(1.0, 0.0)
            .unwrap()
            .multiply_vec(gray);
        let cool = relative_white_balance_adaptation(-1.0, 0.0)
            .unwrap()
            .multiply_vec(gray);
        assert!(warm.x > warm.z && cool.x < cool.z);
        let magenta = relative_white_balance_adaptation(0.0, 1.0)
            .unwrap()
            .multiply_vec(gray);
        let green = relative_white_balance_adaptation(0.0, -1.0)
            .unwrap()
            .multiply_vec(gray);
        assert!(
            magenta.x > magenta.y && magenta.z > magenta.y,
            "{magenta:?}"
        );
        assert!(green.y > green.x && green.y > green.z, "{green:?}");
        for temperature in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            for tint in [-1.0, 0.0, 1.0] {
                let matrix = relative_white_balance_adaptation(temperature, tint).unwrap();
                assert_eq!(
                    matrix.multiply_vec(Xyz {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0
                    }),
                    Xyz {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0
                    }
                );
                let result = matrix.multiply_vec(gray);
                let xyz = rec2020_linear_to_xyz_d65(LinearRgb {
                    r: result.x,
                    g: result.y,
                    b: result.z,
                });
                assert!((xyz.y - 0.18).abs() < 1.0e-6);
            }
        }
    }

    #[test]
    fn relative_wb_is_finite_reversible_and_continuous_without_hdr_clamping() {
        for step in -100..=100 {
            for tint in [-1.0, 0.0, 1.0] {
                let matrix = relative_white_balance_adaptation(step as f32 / 100.0, tint).unwrap();
                let input = Xyz {
                    x: -0.2,
                    y: 1.5,
                    z: 12.0,
                };
                let result = matrix.multiply_vec(input);
                assert!(result.z > 1.0);
                assert!(
                    [result.x, result.y, result.z]
                        .into_iter()
                        .all(f32::is_finite)
                );
                let restored = matrix.inverse().unwrap().multiply_vec(result);
                for (a, b) in [restored.x, restored.y, restored.z]
                    .into_iter()
                    .zip([input.x, input.y, input.z])
                {
                    assert!((a - b).abs() < 2.0e-5);
                }
            }
        }
        let boundary = ((1.0e6 / 7000.0 - 1.0e6 / 6504.0) / 65.0) as f32;
        for tint in [-1.0, 0.0, 1.0] {
            let before = relative_white_balance_adaptation(boundary - 1.0e-5, tint).unwrap();
            let after = relative_white_balance_adaptation(boundary + 1.0e-5, tint).unwrap();
            for (a, b) in before
                .0
                .into_iter()
                .flatten()
                .zip(after.0.into_iter().flatten())
            {
                assert!((a - b).abs() < 5.0e-4);
            }
        }
        assert!(relative_white_balance_adaptation(f32::NAN, 0.0).is_err());
        assert!(relative_white_balance_adaptation(0.0, f32::INFINITY).is_err());
    }

    #[test]
    fn prepared_lcms_adaptation_agrees_with_bradford_reference_and_is_reversible() {
        let forward = chromatic_adaptation_rec2020(D50, D65).unwrap();
        let inverse = chromatic_adaptation_rec2020(D65, D50).unwrap();
        for input in [
            LinearRgb {
                r: 0.18,
                g: 0.18,
                b: 0.18,
            },
            LinearRgb {
                r: 0.8,
                g: 0.35,
                b: 0.2,
            },
            LinearRgb {
                r: -0.1,
                g: 0.2,
                b: 0.9,
            },
            LinearRgb {
                r: 3.0,
                g: 1.5,
                b: 8.0,
            },
        ] {
            let vector = Xyz {
                x: input.r,
                y: input.g,
                z: input.b,
            };
            let actual = forward.multiply_vec(vector);
            let expected =
                xyz_d65_to_rec2020_linear(adapt_xyz(rec2020_linear_to_xyz_d65(input), D50, D65));
            for (a, b) in [actual.x, actual.y, actual.z]
                .into_iter()
                .zip([expected.r, expected.g, expected.b])
            {
                assert!((a - b).abs() < 2.0e-5, "LittleCMS vs Bradford: {a} {b}");
            }
            let restored = inverse.multiply_vec(actual);
            for (a, b) in [restored.x, restored.y, restored.z]
                .into_iter()
                .zip([input.r, input.g, input.b])
            {
                assert!((a - b).abs() < 2.0e-5);
            }
        }
        assert_eq!(
            chromatic_adaptation_rec2020(D65, D65).unwrap(),
            Matrix3([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
        );
    }

    #[test]
    fn measured_neutral_cat_preserves_sample_luminance_without_rgb_diagonal_or_clamp() {
        for sample in [
            LinearRgb {
                r: 0.3,
                g: 0.2,
                b: 0.1,
            },
            LinearRgb {
                r: 0.08,
                g: 0.14,
                b: 0.25,
            },
            LinearRgb {
                r: 3.0,
                g: 2.0,
                b: 1.0,
            },
        ] {
            let matrix = measured_neutral_adaptation(sample).unwrap();
            let result = matrix.multiply_vec(Xyz {
                x: sample.r,
                y: sample.g,
                z: sample.b,
            });
            let luminance = rec2020_linear_to_xyz_d65(sample).y;
            for value in [result.x, result.y, result.z] {
                assert!((value - luminance).abs() < 2.0e-6, "{sample:?} {result:?}");
            }
            assert!(matrix.0.iter().flatten().all(|v| v.is_finite()));
            assert!(
                matrix.0[0][1].abs() + matrix.0[0][2].abs() > 1.0e-3,
                "working RGB must use a full adaptation matrix, not a diagonal gain"
            );
        }
    }

    #[test]
    fn adaptation_rejects_invalid_white_points_without_invented_fallback() {
        for white in [
            Xyz {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            Xyz {
                x: f32::NAN,
                y: 1.0,
                z: 1.0,
            },
            Xyz {
                x: 1.0,
                y: f32::INFINITY,
                z: 1.0,
            },
            Xyz {
                x: -1.0,
                y: 1.0,
                z: 1.0,
            },
        ] {
            assert!(matches!(
                chromatic_adaptation_rec2020(white, D65),
                Err(ColorManagementError::InvalidWhitePoint)
            ));
            assert!(matches!(
                chromatic_adaptation_rec2020(D65, white),
                Err(ColorManagementError::InvalidWhitePoint)
            ));
        }
        assert!(
            measured_neutral_adaptation(LinearRgb {
                r: 0.0,
                g: 0.0,
                b: 0.0
            })
            .is_err()
        );
    }

    #[test]
    fn display_p3_trc_matches_independent_standard_at_dark_to_white_samples() {
        let provider = LittleCmsProvider;
        let profile = provider
            .builtin_output_profile_bytes(BuiltinOutputProfile::DisplayP3)
            .unwrap();
        let parsed = Profile::new_icc(&profile).unwrap();
        let mut white = [[1.0; 3]];
        provider
            .input_to_working(
                &mut white,
                Some(&profile),
                RenderingIntent::RelativeColorimetric,
                true,
            )
            .unwrap();
        for encoded in [
            0.0_f32, 0.0001, 0.01, 0.02, 0.04045, 0.05, 0.25, 0.5, 0.8, 1.0,
        ] {
            let expected = if encoded <= 0.04045 {
                encoded / 12.92
            } else {
                ((encoded + 0.055) / 1.055).powf(2.4)
            };
            // Isolate the TRC from ICC colorant/chad 16.16 quantization. Check each stored
            // channel against the independent IEC formula at the original strict bound.
            for tag in [
                lcms2::TagSignature::RedTRCTag,
                lcms2::TagSignature::GreenTRCTag,
                lcms2::TagSignature::BlueTRCTag,
            ] {
                let lcms2::Tag::ToneCurve(curve) = parsed.read_tag(tag) else {
                    panic!("P3 TRC missing")
                };
                assert!((curve.eval(encoded) - expected).abs() < 2.0e-5);
            }
            let mut working = [[encoded; 3]];
            provider
                .input_to_working(
                    &mut working,
                    Some(&profile),
                    RenderingIntent::RelativeColorimetric,
                    true,
                )
                .unwrap();
            for (index, channel) in working[0].iter().enumerate() {
                assert!(
                    (channel / white[0][index] - expected).abs() < 2.0e-5,
                    "P3 encoded {encoded}: actual {channel}, standard {expected}"
                );
            }
        }
    }

    #[test]
    fn display_p3_output_uses_correct_linear_to_encoded_toe_and_shoulder() {
        let provider = LittleCmsProvider;
        let profile = provider
            .builtin_output_profile_bytes(BuiltinOutputProfile::DisplayP3)
            .unwrap();
        for linear in [0.0_f32, 0.0001, 0.001, 0.0031308, 0.01, 0.18, 0.5, 1.0] {
            let expected = if linear <= 0.0031308 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            };
            let mut output = [[linear; 3]];
            provider
                .working_to_output(
                    &mut output,
                    Some(&profile),
                    RenderingIntent::RelativeColorimetric,
                    true,
                )
                .unwrap();
            for channel in output[0] {
                assert!(
                    (channel - expected).abs() < 2.0e-5,
                    "P3 linear {linear}: actual {channel}, standard {expected}"
                );
            }
        }
    }

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
    fn parallel_lcms_chunks_match_the_single_call_reference() {
        let input = rec2020_linear_d65_profile().expect("working profile");
        let output = Profile::new_srgb();
        let sequential = build_lcms_transform(
            &input,
            &output,
            RenderingIntent::RelativeColorimetric,
            true,
            "test-sequential",
        )
        .expect("sequential transform");
        let parallel = build_parallel_lcms_transform(
            &input,
            &output,
            RenderingIntent::RelativeColorimetric,
            true,
            "test-parallel",
        )
        .expect("parallel transform");
        let source = (0..32_777)
            .map(|index| {
                let value = index as f32 / 32_776.0;
                [value * 1.2, (1.0 - value) * 0.8, value * value]
            })
            .collect::<Vec<_>>();
        let mut expected = source.clone();
        let mut actual = source;
        sequential.transform.transform_in_place(&mut expected);
        transform_pixels_parallel(&parallel, &mut actual);
        for (expected, actual) in expected.iter().zip(actual) {
            for channel in 0..3 {
                assert!(
                    (expected[channel] - actual[channel]).abs() <= 1.0e-6,
                    "parallel LittleCMS output diverged: expected={}, actual={}",
                    expected[channel],
                    actual[channel]
                );
            }
        }
    }

    #[test]
    fn generated_builtin_profiles_have_deterministic_valid_headers() {
        let provider = LittleCmsProvider;
        for builtin in [
            BuiltinOutputProfile::Srgb,
            BuiltinOutputProfile::DisplayP3,
            BuiltinOutputProfile::AdobeRgb,
            BuiltinOutputProfile::Rec2020,
        ] {
            let first = provider
                .builtin_output_profile_bytes(builtin)
                .expect("first built-in profile");
            let second = provider
                .builtin_output_profile_bytes(builtin)
                .expect("second built-in profile");
            assert_eq!(first, second, "{builtin:?} profile bytes are not stable");
            assert_eq!(
                &first[24..36],
                &[0x07, 0xea, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0],
                "{builtin:?} profile creation metadata is not canonical"
            );
            Profile::new_icc(&first).expect("canonical profile remains parseable by LittleCMS");
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
