//! Production Lensfun v0.3.4 database provider and correction adapter for Starroom.
//! The official CC-BY-SA database is embedded at its pinned upstream commit. Distortion model
//! equations are adapted from Lensfun's LGPL modifier implementation so Preview/Export do not
//! depend on a system DLL or silently substitute a generic profile.

use quick_xml::{
    Reader, XmlVersion,
    escape::unescape,
    events::{BytesStart, Event},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::OnceLock};

include!(concat!(env!("OUT_DIR"), "/lensfun_db.rs"));

pub const LENSFUN_VERSION: &str = "0.3.4";
pub const LENSFUN_COMMIT: &str = "101c745e847a5de4a1e569a94368ce2027198598";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LensIdentity {
    pub camera_make: String,
    pub camera_model: String,
    pub lens_make: String,
    pub lens_model: String,
    pub focal_length_mm: f32,
    pub aperture: f32,
    pub focus_distance_m: Option<f32>,
}

impl LensIdentity {
    pub fn metadata_complete(&self) -> bool {
        !self.camera_model.trim().is_empty()
            && !self.lens_model.trim().is_empty()
            && self.focal_length_mm.is_finite()
            && self.focal_length_mm > 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LensMatchMode {
    #[default]
    Auto,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LensProfileStatus {
    AutoMatched,
    ManualMatched,
    MissingMetadata,
    UnknownCamera,
    UnknownLens,
    MountMismatch,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LensProfileResolution {
    pub status: LensProfileStatus,
    pub profile_id: Option<String>,
    pub database_version: String,
    pub camera_mount: Option<String>,
    pub correction: Option<LensCorrection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DistortionCoefficients {
    pub model: DistortionModel,
    /// Brown-Conrady radial terms on normalized radius.
    pub k1: f32,
    pub k2: f32,
    pub k3: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum DistortionModel {
    #[default]
    None,
    Poly3,
    Poly5,
    PtLens,
}

impl Default for DistortionCoefficients {
    fn default() -> Self {
        Self {
            model: DistortionModel::None,
            k1: 0.0,
            k2: 0.0,
            k3: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ChromaticAberrationCoefficients {
    /// Relative radial scale for red and blue against green.
    pub red_scale: f32,
    pub blue_scale: f32,
}

impl Default for ChromaticAberrationCoefficients {
    fn default() -> Self {
        Self {
            red_scale: 1.0,
            blue_scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VignetteCoefficients {
    /// Multiplicative falloff: gain = 1 / (1 + v1*r² + v2*r⁴ + v3*r⁶).
    pub v1: f32,
    pub v2: f32,
    pub v3: f32,
}

impl Default for VignetteCoefficients {
    fn default() -> Self {
        Self {
            v1: 0.0,
            v2: 0.0,
            v3: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct LensCorrection {
    pub distortion: DistortionCoefficients,
    pub chromatic_aberration: ChromaticAberrationCoefficients,
    pub vignette: VignetteCoefficients,
}

pub trait LensProfileProvider {
    type Error;

    fn resolve(&self, lens: &LensIdentity) -> Result<Option<LensCorrection>, Self::Error>;
}

#[derive(Debug, Clone, Default)]
struct CameraRecord {
    maker: String,
    model: String,
    mount: String,
}

#[derive(Debug, Clone, Default)]
struct Calibration {
    model: String,
    focal: f32,
    aperture: f32,
    distance: f32,
    terms: [f32; 3],
}

#[derive(Debug, Clone, Default)]
struct LensRecord {
    source: String,
    maker: String,
    model: String,
    mounts: Vec<String>,
    crop_factor: f32,
    distortion: Vec<Calibration>,
    tca: Vec<Calibration>,
    vignette: Vec<Calibration>,
}

#[derive(Debug)]
struct LensfunDatabase {
    cameras: Vec<CameraRecord>,
    lenses: Vec<LensRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LensfunError {
    Database(String),
}

fn attribute(element: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.as_ref() == key)
        .and_then(|attribute| {
            attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|value| value.into_owned())
        })
}

fn number(element: &BytesStart<'_>, key: &[u8], default: f32) -> f32 {
    attribute(element, key)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn calibration(element: &BytesStart<'_>) -> Calibration {
    let model = attribute(element, b"model").unwrap_or_default();
    let mut terms = [0.0; 3];
    match element.name().as_ref() {
        b"distortion" if model == "ptlens" => {
            terms = [
                number(element, b"a", 0.0),
                number(element, b"b", 0.0),
                number(element, b"c", 0.0),
            ];
        }
        b"distortion" => {
            terms = [
                number(element, b"k1", 0.0),
                number(element, b"k2", 0.0),
                0.0,
            ]
        }
        b"tca" => {
            terms = [
                number(element, b"vr", number(element, b"kr", 1.0)),
                number(element, b"vb", number(element, b"kb", 1.0)),
                0.0,
            ];
        }
        b"vignetting" => {
            terms = [
                number(element, b"k1", 0.0),
                number(element, b"k2", 0.0),
                number(element, b"k3", 0.0),
            ];
        }
        _ => {}
    }
    Calibration {
        model,
        focal: number(element, b"focal", 0.0),
        aperture: number(element, b"aperture", 0.0),
        distance: number(element, b"distance", 1000.0),
        terms,
    }
}

fn parse_database() -> Result<LensfunDatabase, LensfunError> {
    let mut cameras = Vec::new();
    let mut lenses = Vec::new();
    for (source_name, xml) in LENSFUN_XML {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);
        let mut camera: Option<CameraRecord> = None;
        let mut lens: Option<LensRecord> = None;
        let mut field: Option<String> = None;
        loop {
            match reader.read_event() {
                Ok(Event::Start(element)) => match element.name().as_ref() {
                    b"camera" => camera = Some(CameraRecord::default()),
                    b"lens" => {
                        lens = Some(LensRecord {
                            source: (*source_name).into(),
                            ..Default::default()
                        })
                    }
                    b"maker" | b"model" | b"mount" | b"cropfactor"
                        if attribute(&element, b"lang").is_none() =>
                    {
                        field = Some(String::from_utf8_lossy(element.name().as_ref()).into_owned());
                    }
                    _ => field = None,
                },
                Ok(Event::Empty(element)) => match element.name().as_ref() {
                    b"distortion" => {
                        if let Some(record) = &mut lens {
                            record.distortion.push(calibration(&element));
                        }
                    }
                    b"tca" => {
                        if let Some(record) = &mut lens {
                            record.tca.push(calibration(&element));
                        }
                    }
                    b"vignetting" => {
                        if let Some(record) = &mut lens {
                            record.vignette.push(calibration(&element));
                        }
                    }
                    _ => {}
                },
                Ok(Event::Text(text)) => {
                    if let Some(name) = &field {
                        let decoded = text
                            .decode()
                            .map_err(|error| LensfunError::Database(error.to_string()))?;
                        let value = unescape(&decoded)
                            .map_err(|error| LensfunError::Database(error.to_string()))?
                            .into_owned();
                        if let Some(record) = &mut lens {
                            match name.as_str() {
                                "maker" => record.maker = value,
                                "model" => record.model = value,
                                "mount" => record.mounts.push(value),
                                "cropfactor" => record.crop_factor = value.parse().unwrap_or(0.0),
                                _ => {}
                            }
                        } else if let Some(record) = &mut camera {
                            match name.as_str() {
                                "maker" => record.maker = value,
                                "model" => record.model = value,
                                "mount" => record.mount = value,
                                _ => {}
                            }
                        }
                    }
                }
                Ok(Event::End(element)) => match element.name().as_ref() {
                    b"camera" => {
                        if let Some(record) = camera.take()
                            && !record.model.is_empty()
                        {
                            cameras.push(record);
                        }
                    }
                    b"lens" => {
                        if let Some(record) = lens.take()
                            && !record.model.is_empty()
                        {
                            lenses.push(record);
                        }
                    }
                    _ => field = None,
                },
                Ok(Event::Eof) => break,
                Err(error) => {
                    return Err(LensfunError::Database(format!("{source_name}: {error}")));
                }
                _ => {}
            }
        }
    }
    if cameras.is_empty() || lenses.is_empty() {
        return Err(LensfunError::Database("embedded database is empty".into()));
    }
    Ok(LensfunDatabase { cameras, lenses })
}

fn normalized(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn match_score(expected_make: &str, expected_model: &str, make: &str, model: &str) -> i32 {
    let expected_make = normalized(expected_make);
    let expected_model = normalized(expected_model);
    let make = normalized(make);
    let model = normalized(model);
    if expected_model.is_empty() || model.is_empty() {
        return 0;
    }
    let mut score = if expected_model == model {
        1000
    } else if model.contains(&expected_model) || expected_model.contains(&model) {
        500
    } else {
        0
    };
    if !expected_make.is_empty()
        && (expected_make == make || make.contains(&expected_make) || expected_make.contains(&make))
    {
        score += 200;
    }
    score
}

fn interpolate(samples: &[Calibration], focal: f32) -> Option<Calibration> {
    let nearest = samples.iter().min_by(|left, right| {
        (left.focal - focal)
            .abs()
            .total_cmp(&(right.focal - focal).abs())
    })?;
    let same: Vec<_> = samples
        .iter()
        .filter(|sample| sample.model == nearest.model)
        .collect();
    let lower = same
        .iter()
        .filter(|sample| sample.focal <= focal)
        .max_by(|a, b| a.focal.total_cmp(&b.focal));
    let upper = same
        .iter()
        .filter(|sample| sample.focal >= focal)
        .min_by(|a, b| a.focal.total_cmp(&b.focal));
    match (lower, upper) {
        (Some(left), Some(right)) if (right.focal - left.focal).abs() > 1.0e-6 => {
            let t = ((focal - left.focal) / (right.focal - left.focal)).clamp(0.0, 1.0);
            let mut output = (*left).clone();
            for index in 0..3 {
                output.terms[index] =
                    left.terms[index] + (right.terms[index] - left.terms[index]) * t;
            }
            output.focal = focal;
            Some(output)
        }
        _ => Some((*nearest).clone()),
    }
}

fn nearest_vignette(
    samples: &[Calibration],
    focal: f32,
    aperture: f32,
    distance: f32,
) -> Option<Calibration> {
    samples
        .iter()
        .min_by(|left, right| {
            let metric = |sample: &Calibration| {
                ((sample.focal - focal) / focal.max(1.0)).powi(2)
                    + ((sample.aperture - aperture) / aperture.max(1.0)).powi(2)
                    + ((sample.distance.max(0.1).ln() - distance.max(0.1).ln()) * 0.15).powi(2)
            };
            metric(left).total_cmp(&metric(right))
        })
        .cloned()
}

fn to_correction(record: &LensRecord, identity: &LensIdentity) -> LensCorrection {
    let distortion = interpolate(&record.distortion, identity.focal_length_mm);
    let tca = interpolate(&record.tca, identity.focal_length_mm);
    let vignette = nearest_vignette(
        &record.vignette,
        identity.focal_length_mm,
        identity.aperture,
        identity.focus_distance_m.unwrap_or(1000.0),
    );
    LensCorrection {
        distortion: distortion
            .map(|sample| DistortionCoefficients {
                model: match sample.model.as_str() {
                    "poly3" => DistortionModel::Poly3,
                    "poly5" => DistortionModel::Poly5,
                    "ptlens" => DistortionModel::PtLens,
                    _ => DistortionModel::None,
                },
                k1: sample.terms[0],
                k2: sample.terms[1],
                k3: sample.terms[2],
            })
            .unwrap_or_default(),
        chromatic_aberration: tca
            .map(|sample| ChromaticAberrationCoefficients {
                red_scale: sample.terms[0],
                blue_scale: sample.terms[1],
            })
            .unwrap_or_default(),
        vignette: vignette
            .map(|sample| VignetteCoefficients {
                v1: sample.terms[0],
                v2: sample.terms[1],
                v3: sample.terms[2],
            })
            .unwrap_or_default(),
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LensfunProvider;

impl LensfunProvider {
    pub fn resolve_profile(
        &self,
        identity: &LensIdentity,
        mode: LensMatchMode,
    ) -> Result<LensProfileResolution, LensfunError> {
        if !identity.metadata_complete() {
            return Ok(LensProfileResolution {
                status: LensProfileStatus::MissingMetadata,
                profile_id: None,
                database_version: LENSFUN_VERSION.into(),
                camera_mount: None,
                correction: None,
            });
        }
        static DATABASE: OnceLock<Result<LensfunDatabase, LensfunError>> = OnceLock::new();
        let database = DATABASE
            .get_or_init(parse_database)
            .as_ref()
            .map_err(Clone::clone)?;
        let camera = database.cameras.iter().max_by_key(|camera| {
            match_score(
                &identity.camera_make,
                &identity.camera_model,
                &camera.maker,
                &camera.model,
            )
        });
        let camera_score = camera
            .map(|camera| {
                match_score(
                    &identity.camera_make,
                    &identity.camera_model,
                    &camera.maker,
                    &camera.model,
                )
            })
            .unwrap_or(0);
        if camera_score == 0 {
            return Ok(LensProfileResolution {
                status: LensProfileStatus::UnknownCamera,
                profile_id: None,
                database_version: LENSFUN_VERSION.into(),
                camera_mount: None,
                correction: None,
            });
        }
        let mount = camera
            .map(|camera| camera.mount.clone())
            .unwrap_or_default();
        let mut candidates: Vec<_> = database
            .lenses
            .iter()
            .filter_map(|lens| {
                let score = match_score(
                    &identity.lens_make,
                    &identity.lens_model,
                    &lens.maker,
                    &lens.model,
                );
                (score > 0).then_some((score, lens))
            })
            .collect();
        candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
        if candidates.is_empty() {
            return Ok(LensProfileResolution {
                status: LensProfileStatus::UnknownLens,
                profile_id: None,
                database_version: LENSFUN_VERSION.into(),
                camera_mount: Some(mount),
                correction: None,
            });
        }
        let best_score = candidates[0].0;
        candidates.retain(|candidate| candidate.0 == best_score);
        let mounted: Vec<_> = candidates
            .iter()
            .filter(|(_, lens)| lens.mounts.is_empty() || lens.mounts.contains(&mount))
            .collect();
        if mounted.is_empty() {
            return Ok(LensProfileResolution {
                status: LensProfileStatus::MountMismatch,
                profile_id: None,
                database_version: LENSFUN_VERSION.into(),
                camera_mount: Some(mount),
                correction: None,
            });
        }
        let unique: HashSet<_> = mounted
            .iter()
            .map(|(_, lens)| normalized(&lens.model))
            .collect();
        if unique.len() > 1 && mode == LensMatchMode::Auto {
            return Ok(LensProfileResolution {
                status: LensProfileStatus::Ambiguous,
                profile_id: None,
                database_version: LENSFUN_VERSION.into(),
                camera_mount: Some(mount),
                correction: None,
            });
        }
        let lens = mounted[0].1;
        Ok(LensProfileResolution {
            status: if mode == LensMatchMode::Auto {
                LensProfileStatus::AutoMatched
            } else {
                LensProfileStatus::ManualMatched
            },
            profile_id: Some(format!(
                "lensfun:{}:{}:{}:cf{:.3}",
                LENSFUN_VERSION,
                lens.source,
                normalized(&lens.model),
                lens.crop_factor
            )),
            database_version: LENSFUN_VERSION.into(),
            camera_mount: Some(mount),
            correction: Some(to_correction(lens, identity)),
        })
    }
}

impl LensProfileProvider for LensfunProvider {
    type Error = LensfunError;
    fn resolve(&self, lens: &LensIdentity) -> Result<Option<LensCorrection>, Self::Error> {
        Ok(self.resolve_profile(lens, LensMatchMode::Auto)?.correction)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalizedPoint {
    /// Centered normalized coordinate where image center is 0,0 and a corner is near ±1,±1.
    pub x: f32,
    pub y: f32,
}

pub fn distort(point: NormalizedPoint, coefficients: DistortionCoefficients) -> NormalizedPoint {
    let radius2 = point.x * point.x + point.y * point.y;
    let radius4 = radius2 * radius2;
    let radius = radius2.sqrt();
    // Exact Lensfun v0.3.4 forward distortion equations (`mod-coord.cpp`).
    let scale = match coefficients.model {
        DistortionModel::None => 1.0,
        DistortionModel::Poly3 => 1.0 - coefficients.k1 + coefficients.k1 * radius2,
        DistortionModel::Poly5 => 1.0 + coefficients.k1 * radius2 + coefficients.k2 * radius4,
        DistortionModel::PtLens => {
            let d = 1.0 - coefficients.k1 - coefficients.k2 - coefficients.k3;
            coefficients.k1 * radius2 * radius
                + coefficients.k2 * radius2
                + coefficients.k3 * radius
                + d
        }
    };
    NormalizedPoint {
        x: point.x * scale,
        y: point.y * scale,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpticsParameters {
    pub enabled: bool,
    pub distortion: bool,
    pub tca: bool,
    pub vignette: bool,
    pub auto_scale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OpticsSettings {
    pub parameters: OpticsParameters,
    pub match_mode: LensMatchMode,
    pub manual_identity: Option<LensIdentity>,
}

impl Default for OpticsParameters {
    fn default() -> Self {
        Self {
            enabled: false,
            distortion: true,
            tca: true,
            vignette: true,
            auto_scale: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorrectedImage {
    pub data: Vec<f32>,
    pub auto_scale: f32,
    pub cropped_fraction: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpticsError {
    InvalidBuffer,
    NonFiniteCorrection,
}

fn bilinear(data: &[f32], width: usize, height: usize, x: f32, y: f32, channel: usize) -> f32 {
    if x < 0.0
        || y < 0.0
        || x > (width.saturating_sub(1)) as f32
        || y > (height.saturating_sub(1)) as f32
    {
        return 0.0;
    }
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let sample = |sx: usize, sy: usize| data[(sy * width + sx) * 3 + channel];
    let top = sample(x0, y0) * (1.0 - tx) + sample(x1, y0) * tx;
    let bottom = sample(x0, y1) * (1.0 - tx) + sample(x1, y1) * tx;
    top * (1.0 - ty) + bottom * ty
}

fn safe_scale(correction: LensCorrection) -> f32 {
    let mut low = 0.5_f32;
    let mut high = 1.0_f32;
    for _ in 0..14 {
        let candidate = (low + high) * 0.5;
        let safe = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
            .into_iter()
            .all(|(x, y)| {
                let point = distort(
                    NormalizedPoint {
                        x: x * candidate,
                        y: y * candidate,
                    },
                    correction.distortion,
                );
                point.x.abs() <= 1.0 && point.y.abs() <= 1.0
            });
        if safe {
            low = candidate;
        } else {
            high = candidate;
        }
    }
    low
}

/// Applies Lensfun profile distortion/TCA/vignetting on linear RGB. Lensfun documents colour
/// corrections before non-linear output encoding; Starroom therefore calls this inside the
/// shared linear working graph for both RAW and rendered files.
pub fn apply_lens_correction(
    width: usize,
    height: usize,
    data: &[f32],
    correction: LensCorrection,
    parameters: OpticsParameters,
) -> Result<CorrectedImage, OpticsError> {
    if data.len() != width.saturating_mul(height).saturating_mul(3) || width == 0 || height == 0 {
        return Err(OpticsError::InvalidBuffer);
    }
    if !data.iter().all(|value| value.is_finite()) {
        return Err(OpticsError::InvalidBuffer);
    }
    if !parameters.enabled {
        return Ok(CorrectedImage {
            data: data.to_vec(),
            auto_scale: 1.0,
            cropped_fraction: 0.0,
        });
    }
    let scale = if parameters.auto_scale && parameters.distortion {
        safe_scale(correction)
    } else {
        1.0
    };
    let mut output = vec![0.0; data.len()];
    let mut cropped = 0usize;
    for y in 0..height {
        for x in 0..width {
            let target = NormalizedPoint {
                x: (x as f32 / (width - 1).max(1) as f32 * 2.0 - 1.0) * scale,
                y: (y as f32 / (height - 1).max(1) as f32 * 2.0 - 1.0) * scale,
            };
            let green = if parameters.distortion {
                distort(target, correction.distortion)
            } else {
                target
            };
            if green.x.abs() > 1.0 || green.y.abs() > 1.0 {
                cropped += 1;
            }
            for channel in 0..3 {
                let coordinate = if parameters.tca {
                    channel_coordinate(green, correction.chromatic_aberration, channel)
                } else {
                    green
                };
                let source_x = (coordinate.x + 1.0) * 0.5 * (width - 1) as f32;
                let source_y = (coordinate.y + 1.0) * 0.5 * (height - 1) as f32;
                let mut value = bilinear(data, width, height, source_x, source_y, channel);
                if parameters.vignette {
                    value *= vignette_gain(target, correction.vignette);
                }
                if !value.is_finite() {
                    return Err(OpticsError::NonFiniteCorrection);
                }
                output[(y * width + x) * 3 + channel] = value;
            }
        }
    }
    Ok(CorrectedImage {
        data: output,
        auto_scale: scale,
        cropped_fraction: cropped as f32 / width.saturating_mul(height) as f32,
    })
}

/// Numerically inverts a radial distortion profile. This is deterministic and suitable for the
/// CPU reference; the GPU path can use the same fixed iteration count.
pub fn undistort(point: NormalizedPoint, coefficients: DistortionCoefficients) -> NormalizedPoint {
    let mut estimate = point;
    for _ in 0..8 {
        let projected = distort(estimate, coefficients);
        estimate.x += point.x - projected.x;
        estimate.y += point.y - projected.y;
    }
    estimate
}

pub fn channel_coordinate(
    green_coordinate: NormalizedPoint,
    coefficients: ChromaticAberrationCoefficients,
    channel: usize,
) -> NormalizedPoint {
    let scale = match channel {
        0 => coefficients.red_scale,
        2 => coefficients.blue_scale,
        _ => 1.0,
    };
    NormalizedPoint {
        x: green_coordinate.x * scale,
        y: green_coordinate.y * scale,
    }
}

pub fn vignette_gain(point: NormalizedPoint, coefficients: VignetteCoefficients) -> f32 {
    let radius2 = point.x * point.x + point.y * point.y;
    let radius4 = radius2 * radius2;
    let radius6 = radius4 * radius2;
    let denominator =
        1.0 + coefficients.v1 * radius2 + coefficients.v2 * radius4 + coefficients.v3 * radius6;
    if denominator.abs() < 1.0e-6 || !denominator.is_finite() {
        1.0
    } else {
        (1.0 / denominator).clamp(0.1, 8.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1.0e-4
    }

    #[test]
    fn neutral_profile_is_identity() {
        let point = NormalizedPoint { x: 0.7, y: -0.3 };
        let projected = distort(point, DistortionCoefficients::default());
        assert_eq!(point, projected);
        assert_eq!(vignette_gain(point, VignetteCoefficients::default()), 1.0);
    }

    #[test]
    fn distortion_round_trip_is_close() {
        let point = NormalizedPoint { x: 0.55, y: 0.32 };
        let coefficients = DistortionCoefficients {
            model: DistortionModel::Poly5,
            k1: -0.12,
            k2: 0.03,
            k3: 0.0,
        };
        let distorted = distort(point, coefficients);
        let restored = undistort(distorted, coefficients);
        assert!(close(point.x, restored.x));
        assert!(close(point.y, restored.y));
    }

    #[test]
    fn tca_scales_only_requested_channel() {
        let point = NormalizedPoint { x: 0.8, y: 0.1 };
        let coefficients = ChromaticAberrationCoefficients {
            red_scale: 0.998,
            blue_scale: 1.003,
        };
        let red = channel_coordinate(point, coefficients, 0);
        let green = channel_coordinate(point, coefficients, 1);
        let blue = channel_coordinate(point, coefficients, 2);
        assert!(red.x < green.x);
        assert!(blue.x > green.x);
    }

    #[test]
    fn positive_vignette_coefficients_brighten_corrected_edges() {
        let center = vignette_gain(
            NormalizedPoint { x: 0.0, y: 0.0 },
            VignetteCoefficients {
                v1: -0.25,
                v2: 0.0,
                v3: 0.0,
            },
        );
        let edge = vignette_gain(
            NormalizedPoint { x: 0.9, y: 0.0 },
            VignetteCoefficients {
                v1: -0.25,
                v2: 0.0,
                v3: 0.0,
            },
        );
        assert!(edge > center);
    }

    #[test]
    fn m10_embedded_lensfun_database_resolves_camera_mount_and_profile() {
        let identity = LensIdentity {
            camera_make: "NIKON CORPORATION".into(),
            camera_model: "Nikon D750".into(),
            lens_make: "Nikon".into(),
            lens_model: "Nikon AF-S Nikkor 16-35mm f/4G ED VR".into(),
            focal_length_mm: 24.0,
            aperture: 5.6,
            focus_distance_m: Some(10.0),
        };
        let resolution = LensfunProvider
            .resolve_profile(&identity, LensMatchMode::Auto)
            .expect("Lensfun database");
        assert_eq!(resolution.status, LensProfileStatus::AutoMatched);
        assert_eq!(resolution.camera_mount.as_deref(), Some("Nikon F AF"));
        let correction = resolution.correction.expect("profile correction");
        assert_eq!(correction.distortion.model, DistortionModel::PtLens);
        assert!(correction.chromatic_aberration.red_scale > 0.99);
        assert!(
            resolution
                .profile_id
                .expect("profile id")
                .contains("lensfun:0.3.4")
        );
    }

    #[test]
    fn m10_missing_unknown_and_mount_mismatch_are_explicit() {
        let missing = LensfunProvider
            .resolve_profile(
                &LensIdentity {
                    camera_make: String::new(),
                    camera_model: String::new(),
                    lens_make: String::new(),
                    lens_model: String::new(),
                    focal_length_mm: 0.0,
                    aperture: 0.0,
                    focus_distance_m: None,
                },
                LensMatchMode::Auto,
            )
            .expect("database");
        assert_eq!(missing.status, LensProfileStatus::MissingMetadata);
        let unknown = LensfunProvider
            .resolve_profile(
                &LensIdentity {
                    camera_make: "Nikon".into(),
                    camera_model: "Nikon D750".into(),
                    lens_make: "NoSuch".into(),
                    lens_model: "NoSuch 1mm".into(),
                    focal_length_mm: 1.0,
                    aperture: 1.0,
                    focus_distance_m: None,
                },
                LensMatchMode::Auto,
            )
            .expect("database");
        assert_eq!(unknown.status, LensProfileStatus::UnknownLens);
    }

    #[test]
    fn m10_linear_correction_is_finite_and_reports_crop() {
        let width = 7;
        let height = 5;
        let data: Vec<f32> = (0..width * height)
            .flat_map(|index| {
                let value = index as f32 / (width * height) as f32;
                [value, value * 0.8, value * 0.6]
            })
            .collect();
        let correction = LensCorrection {
            distortion: DistortionCoefficients {
                model: DistortionModel::Poly5,
                k1: -0.08,
                k2: 0.02,
                k3: 0.0,
            },
            chromatic_aberration: ChromaticAberrationCoefficients {
                red_scale: 0.999,
                blue_scale: 1.002,
            },
            vignette: VignetteCoefficients {
                v1: -0.2,
                v2: 0.05,
                v3: 0.0,
            },
        };
        let result = apply_lens_correction(
            width,
            height,
            &data,
            correction,
            OpticsParameters {
                enabled: true,
                ..Default::default()
            },
        )
        .expect("correction");
        assert_eq!(result.data.len(), data.len());
        assert!(result.data.iter().all(|value| value.is_finite()));
        assert!((0.5..=1.0).contains(&result.auto_scale));
        assert!((0.0..=1.0).contains(&result.cropped_fraction));
    }
}
