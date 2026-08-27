//! Portrait-retouch contracts and CPU references for Starroom v0.2.
//! Face/skin detection is provider-based so MediaPipe can be integrated without coupling the core.

use ndarray::Array4;
use ort::{
    execution_providers::{CPUExecutionProvider, directml::DirectMLExecutionProvider},
    session::Session,
    value::TensorRef,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use starroom_color::{
    LinearRgb, oklab_to_oklch, oklab_to_rec2020, oklch_to_oklab, rec2020_to_oklab,
};
use starroom_detail::{LinearImage, gaussian_blur};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use thiserror::Error;

/// M16's fixed, local-only model identities.  The binary weights intentionally do not live in
/// the public repository; `MODEL_PROVENANCE.md` records their exact public source and SHA-256.
pub const YUNET_MODEL_ID: &str = "opencv-zoo/face_detection_yunet_2026may";
pub const YUNET_MODEL_VERSION: &str = "47534e27c9851bb1128ccc0102f1145e27f23f98";
pub const YUNET_MODEL_SHA256: &str =
    "ebafce4e3c118d6554634be5c27ab333b4c047a9a8c3faf1d7cf93101c22f0f0";
pub const BISENET_MODEL_ID: &str = "yakhyo/face-parsing-bisenet-resnet18";
pub const BISENET_MODEL_VERSION: &str = "8a4729d95118d0e97c44185f9bdef3d6bfeaaf99";
pub const BISENET_MODEL_SHA256: &str =
    "0d9bd318e46987c3bdbfacae9e2c0f461cae1c6ac6ea6d43bbe541a91727e33f";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PortraitError {
    #[error("YuNet detector model is missing: {path}")]
    DetectorModelMissing { path: PathBuf },
    #[error("BiSeNet parser model is missing: {path}")]
    ParserModelMissing { path: PathBuf },
    #[error("model SHA-256 mismatch for {model_id}: expected {expected}, got {actual}")]
    ModelHashMismatch {
        model_id: String,
        expected: String,
        actual: String,
    },
    #[error("ONNX Runtime is unavailable: {0}")]
    RuntimeUnavailable(String),
    #[error("YuNet initialization failed: {0}")]
    DetectorInitializationFailed(String),
    #[error("BiSeNet initialization failed: {0}")]
    ParserInitializationFailed(String),
    #[error("YuNet detection failed: {0}")]
    DetectionFailed(String),
    #[error("BiSeNet parsing failed: {0}")]
    ParsingFailed(String),
    #[error("YuNet returned an invalid output: {0}")]
    InvalidDetectionOutput(String),
    #[error("BiSeNet returned an invalid output: {0}")]
    InvalidParsingOutput(String),
    #[error("no face was detected")]
    NoFaceDetected,
    #[error("invalid face crop transform: {0}")]
    InvalidTransform(String),
    #[error("execution provider is not supported: {0}")]
    UnsupportedExecutionProvider(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionProvider {
    #[default]
    Cpu,
    DirectMl,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelDescriptor {
    pub id: String,
    pub version: String,
    pub sha256: String,
    pub path: PathBuf,
}

impl ModelDescriptor {
    pub fn yunet(path: impl Into<PathBuf>) -> Self {
        Self {
            id: YUNET_MODEL_ID.into(),
            version: YUNET_MODEL_VERSION.into(),
            sha256: YUNET_MODEL_SHA256.into(),
            path: path.into(),
        }
    }
    pub fn bisenet_resnet18(path: impl Into<PathBuf>) -> Self {
        Self {
            id: BISENET_MODEL_ID.into(),
            version: BISENET_MODEL_VERSION.into(),
            sha256: BISENET_MODEL_SHA256.into(),
            path: path.into(),
        }
    }
    pub fn verify(&self, parser: bool) -> Result<(), PortraitError> {
        if !self.path.is_file() {
            return Err(if parser {
                PortraitError::ParserModelMissing {
                    path: self.path.clone(),
                }
            } else {
                PortraitError::DetectorModelMissing {
                    path: self.path.clone(),
                }
            });
        }
        let bytes = fs::read(&self.path)
            .map_err(|error| PortraitError::RuntimeUnavailable(error.to_string()))?;
        let actual = format!("{:x}", Sha256::digest(bytes));
        if actual != self.sha256.to_ascii_lowercase() {
            return Err(PortraitError::ModelHashMismatch {
                model_id: self.id.clone(),
                expected: self.sha256.clone(),
                actual,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortraitModelRegistry {
    pub detector: ModelDescriptor,
    pub parser: ModelDescriptor,
    pub execution_provider: ExecutionProvider,
}

impl PortraitModelRegistry {
    pub fn local_default(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        Self {
            detector: ModelDescriptor::yunet(root.join("face_detection_yunet_2026may.onnx")),
            parser: ModelDescriptor::bisenet_resnet18(root.join("bisenet_resnet18.onnx")),
            execution_provider: ExecutionProvider::Cpu,
        }
    }
    pub fn verify(&self) -> Result<(), PortraitError> {
        self.detector.verify(false)?;
        self.parser.verify(true)
    }
}

/// Source-image semantic names. Their serialized form is stable so M15 layers remain editable
/// after reloading a project, without serializing model weights or raw probability rasters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum PortraitRegion {
    Face,
    Skin,
    Eyes,
    LeftEye,
    RightEye,
    Brows,
    LeftBrow,
    RightBrow,
    Lips,
    Mouth,
    Hair,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FaceCropTransform {
    /// Source pixel center of the square crop.
    pub center_x: f32,
    pub center_y: f32,
    pub side: f32,
    /// Eye-line alignment angle in degrees. The inverse is used when projecting parser masks.
    pub rotation_degrees: f32,
}

impl FaceCropTransform {
    pub fn from_face(
        bounds: FaceBounds,
        width: u32,
        height: u32,
        scale: f32,
        left_eye: Landmark,
        right_eye: Landmark,
    ) -> Result<Self, PortraitError> {
        if width == 0 || height == 0 || !scale.is_finite() || scale < 1.0 {
            return Err(PortraitError::InvalidTransform(
                "image dimensions and crop scale must be valid".into(),
            ));
        }
        let width_f = width as f32;
        let height_f = height as f32;
        let cx = ((bounds.left + bounds.right) * 0.5 * width_f).clamp(0.0, width_f);
        let cy = ((bounds.top + bounds.bottom) * 0.5 * height_f).clamp(0.0, height_f);
        let side = ((bounds.right - bounds.left) * width_f)
            .max((bounds.bottom - bounds.top) * height_f)
            * scale;
        if !side.is_finite() || side <= 1.0 {
            return Err(PortraitError::InvalidTransform(
                "face bounds are too small or non-finite".into(),
            ));
        }
        let dy = (right_eye.y - left_eye.y) * height_f;
        let dx = (right_eye.x - left_eye.x) * width_f;
        let rotation_degrees = (-dy.atan2(dx).to_degrees()).clamp(-90.0, 90.0);
        Ok(Self {
            center_x: cx,
            center_y: cy,
            side,
            rotation_degrees,
        })
    }
    pub fn source_point(
        self,
        crop_x: f32,
        crop_y: f32,
        crop_edge: f32,
    ) -> Result<(f32, f32), PortraitError> {
        if ![
            crop_x,
            crop_y,
            crop_edge,
            self.center_x,
            self.center_y,
            self.side,
            self.rotation_degrees,
        ]
        .into_iter()
        .all(f32::is_finite)
            || crop_edge <= 0.0
            || self.side <= 0.0
        {
            return Err(PortraitError::InvalidTransform(
                "non-finite crop coordinate".into(),
            ));
        }
        let local_x = (crop_x / crop_edge - 0.5) * self.side;
        let local_y = (crop_y / crop_edge - 0.5) * self.side;
        let radians = -self.rotation_degrees.to_radians();
        Ok((
            self.center_x + local_x * radians.cos() - local_y * radians.sin(),
            self.center_y + local_x * radians.sin() + local_y * radians.cos(),
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DetectedFace {
    pub id: String,
    pub confidence: f32,
    pub bounds: FaceBounds,
    /// left eye, right eye, nose, left mouth corner, right mouth corner in source-normalized space
    pub landmarks: [Landmark; 5],
    pub crop: FaceCropTransform,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DetectionCacheKey {
    pub source_identity: String,
    pub detector_model_hash: String,
    pub parameters_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ParsingCacheKey {
    pub source_identity: String,
    pub face_id: String,
    pub parser_model_hash: String,
    pub crop_transform_hash: String,
}

pub fn stable_face_id(
    source_identity: &str,
    bounds: FaceBounds,
    landmarks: &[Landmark; 5],
) -> String {
    // Deliberately uses only per-image geometry; this is not facial recognition and cannot link
    // people across photographs.
    let mut input = format!(
        "{source_identity}:{:.5}:{:.5}:{:.5}:{:.5}",
        bounds.left, bounds.top, bounds.right, bounds.bottom
    );
    for point in landmarks {
        input.push_str(&format!(":{:.5}:{:.5}", point.x, point.y));
    }
    format!(
        "face-{}",
        &format!("{:x}", Sha256::digest(input.as_bytes()))[..16]
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SoftMask {
    pub width: u32,
    pub height: u32,
    /// R16Float-compatible semantic coverage in source-image coordinates, stored internally as
    /// finite f32 until the renderer allocates its R16Float resource.
    pub values: Vec<f32>,
}

impl SoftMask {
    pub fn new(width: u32, height: u32, values: Vec<f32>) -> Result<Self, PortraitError> {
        if width == 0
            || height == 0
            || values.len() != width as usize * height as usize
            || values.iter().any(|v| !v.is_finite())
        {
            return Err(PortraitError::InvalidParsingOutput(
                "mask dimensions or values are invalid".into(),
            ));
        }
        Ok(Self {
            width,
            height,
            values: values.into_iter().map(|v| v.clamp(0.0, 1.0)).collect(),
        })
    }
    pub fn weight_at(&self, x: f32, y: f32) -> f32 {
        if !x.is_finite() || !y.is_finite() {
            return 0.0;
        }
        let px = (x.clamp(0.0, 1.0) * (self.width.saturating_sub(1)) as f32).round() as usize;
        let py = (y.clamp(0.0, 1.0) * (self.height.saturating_sub(1)) as f32).round() as usize;
        self.values
            .get(py * self.width as usize + px)
            .copied()
            .unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PortraitParseResult {
    pub face_id: String,
    pub cache_key: ParsingCacheKey,
    pub regions: BTreeMap<PortraitRegion, SoftMask>,
}

/// A real, local ONNX Runtime session pair. No network transport, telemetry, browser math or
/// hidden fallback is involved. DirectML is attempted only when requested; an explicit CPU
/// session is then created as the documented local fallback.
pub struct PortraitOnnxProvider {
    detector: Session,
    parser: Session,
    pub registry: PortraitModelRegistry,
    /// The provider actually used by both sessions. A requested DirectML
    /// session deliberately falls back as a pair to CPU, never per-model.
    pub execution_provider: ExecutionProvider,
}

impl PortraitOnnxProvider {
    pub fn initialize(registry: PortraitModelRegistry) -> Result<Self, PortraitError> {
        registry.verify()?;
        let (detector, parser, execution_provider) = match registry.execution_provider {
            ExecutionProvider::Cpu => (
                Self::open_session(&registry.detector, ExecutionProvider::Cpu, false)?,
                Self::open_session(&registry.parser, ExecutionProvider::Cpu, true)?,
                ExecutionProvider::Cpu,
            ),
            ExecutionProvider::DirectMl => {
                let direct = (|| {
                    Ok::<_, PortraitError>((
                        Self::open_session(&registry.detector, ExecutionProvider::DirectMl, false)?,
                        Self::open_session(&registry.parser, ExecutionProvider::DirectMl, true)?,
                    ))
                })();
                match direct {
                    Ok((detector, parser)) => (detector, parser, ExecutionProvider::DirectMl),
                    // A partial DirectML initialization is intentionally discarded so
                    // detection and parsing cannot silently run on different devices.
                    Err(_) => (
                        Self::open_session(&registry.detector, ExecutionProvider::Cpu, false)?,
                        Self::open_session(&registry.parser, ExecutionProvider::Cpu, true)?,
                        ExecutionProvider::Cpu,
                    ),
                }
            }
        };
        Ok(Self {
            detector,
            parser,
            registry,
            execution_provider,
        })
    }
    fn open_session(
        descriptor: &ModelDescriptor,
        requested: ExecutionProvider,
        parser: bool,
    ) -> Result<Session, PortraitError> {
        let failure = |detail: String| {
            if parser {
                PortraitError::ParserInitializationFailed(detail)
            } else {
                PortraitError::DetectorInitializationFailed(detail)
            }
        };
        match requested {
            ExecutionProvider::Cpu => Session::builder()
                .map_err(|e| failure(e.to_string()))?
                .with_execution_providers([CPUExecutionProvider::default().build()])
                .map_err(|e| failure(e.to_string()))?
                .commit_from_file(&descriptor.path)
                .map_err(|e| failure(e.to_string())),
            ExecutionProvider::DirectMl => Session::builder()
                .map_err(|e| failure(e.to_string()))?
                .with_execution_providers([DirectMLExecutionProvider::default().build()])
                .map_err(|e| failure(e.to_string()))?
                .commit_from_file(&descriptor.path)
                .map_err(|e| failure(e.to_string())),
        }
    }

    pub fn detect(
        &mut self,
        width: u32,
        height: u32,
        rgba: &[u8],
        crop_scale: f32,
        source_identity: &str,
    ) -> Result<Vec<DetectedFace>, PortraitError> {
        if rgba.len() != width as usize * height as usize * 4 {
            return Err(PortraitError::DetectionFailed(
                "RGBA buffer length does not match source dimensions".into(),
            ));
        }
        let input = resize_rgba_rgb_chw(width, height, rgba, ModelInputFormat::yunet())?;
        let tensor = Array4::from_shape_vec((1, 3, 320, 320), input)
            .map_err(|e| PortraitError::DetectionFailed(e.to_string()))?;
        let output = self
            .detector
            .run(ort::inputs![TensorRef::from_array_view(&tensor).map_err(
                |e| PortraitError::DetectionFailed(e.to_string())
            )?])
            .map_err(|e| PortraitError::DetectionFailed(e.to_string()))?;
        let candidates = decode_yunet_outputs(&output, width, height)?;
        let mut faces = Vec::new();
        for candidate in non_maximum_suppression(candidates, 0.3) {
            if candidate.confidence < 0.65 {
                continue;
            }
            let landmarks = candidate.landmarks;
            let crop = FaceCropTransform::from_face(
                candidate.bounds,
                width,
                height,
                crop_scale,
                landmarks[0],
                landmarks[1],
            )?;
            let id = stable_face_id(source_identity, candidate.bounds, &landmarks);
            faces.push(DetectedFace {
                id,
                confidence: candidate.confidence,
                bounds: candidate.bounds,
                landmarks,
                crop,
            });
        }
        if faces.is_empty() {
            return Err(PortraitError::NoFaceDetected);
        }
        Ok(faces)
    }

    pub fn parse(
        &mut self,
        width: u32,
        height: u32,
        rgba: &[u8],
        face: &DetectedFace,
        source_identity: &str,
    ) -> Result<PortraitParseResult, PortraitError> {
        let crop = aligned_crop_rgba(width, height, rgba, face.crop, 512)?;
        let input = resize_rgba_rgb_chw(512, 512, &crop, ModelInputFormat::bisenet())?;
        let tensor = Array4::from_shape_vec((1, 3, 512, 512), input)
            .map_err(|e| PortraitError::ParsingFailed(e.to_string()))?;
        let output = self
            .parser
            .run(ort::inputs![
                TensorRef::from_array_view(&tensor)
                    .map_err(|e| PortraitError::ParsingFailed(e.to_string()))?
            ])
            .map_err(|e| PortraitError::ParsingFailed(e.to_string()))?;
        let probabilities = parser_probabilities(&output)?;
        let regions = project_semantic_regions(&probabilities, width, height, face.crop)?;
        let transform_hash = format!(
            "{:x}",
            Sha256::digest(format!(
                "{:.5}:{:.5}:{:.5}:{:.5}",
                face.crop.center_x, face.crop.center_y, face.crop.side, face.crop.rotation_degrees
            ))
        );
        Ok(PortraitParseResult {
            face_id: face.id.clone(),
            cache_key: ParsingCacheKey {
                source_identity: source_identity.into(),
                face_id: face.id.clone(),
                parser_model_hash: self.registry.parser.sha256.clone(),
                crop_transform_hash: transform_hash,
            },
            regions,
        })
    }
}

#[derive(Clone)]
struct DetectionCandidate {
    confidence: f32,
    bounds: FaceBounds,
    landmarks: [Landmark; 5],
}

#[derive(Clone, Copy)]
struct ModelInputFormat {
    target_width: u32,
    target_height: u32,
    mean: [f32; 3],
    std: [f32; 3],
    divide_255: bool,
}

impl ModelInputFormat {
    const fn yunet() -> Self {
        Self {
            target_width: 320,
            target_height: 320,
            mean: [0.0, 0.0, 0.0],
            std: [1.0, 1.0, 1.0],
            divide_255: false,
        }
    }

    const fn bisenet() -> Self {
        Self {
            target_width: 512,
            target_height: 512,
            mean: [0.485, 0.456, 0.406],
            std: [0.229, 0.224, 0.225],
            divide_255: true,
        }
    }

    const fn birefnet() -> Self {
        Self {
            target_width: 1024,
            target_height: 1024,
            mean: [0.485, 0.456, 0.406],
            std: [0.229, 0.224, 0.225],
            divide_255: true,
        }
    }

    const fn segformer() -> Self {
        Self {
            target_width: 512,
            target_height: 512,
            mean: [0.485, 0.456, 0.406],
            std: [0.229, 0.224, 0.225],
            divide_255: true,
        }
    }
}

fn resize_rgba_rgb_chw(
    width: u32,
    height: u32,
    rgba: &[u8],
    format: ModelInputFormat,
) -> Result<Vec<f32>, PortraitError> {
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        return Err(PortraitError::InvalidTransform(
            "invalid image for model pre-processing".into(),
        ));
    }
    let mut out = vec![0.0; (format.target_width * format.target_height * 3) as usize];
    for y in 0..format.target_height {
        for x in 0..format.target_width {
            let sx = ((x as f32 + 0.5) * width as f32 / format.target_width as f32 - 0.5)
                .round()
                .clamp(0.0, width.saturating_sub(1) as f32) as usize;
            let sy = ((y as f32 + 0.5) * height as f32 / format.target_height as f32 - 0.5)
                .round()
                .clamp(0.0, height.saturating_sub(1) as f32) as usize;
            let base = (sy * width as usize + sx) * 4;
            for c in 0..3 {
                let raw = rgba[base + c] as f32 / if format.divide_255 { 255.0 } else { 1.0 };
                out[(c * format.target_height as usize + y as usize)
                    * format.target_width as usize
                    + x as usize] = (raw - format.mean[c]) / format.std[c];
            }
        }
    }
    Ok(out)
}

fn aligned_crop_rgba(
    width: u32,
    height: u32,
    rgba: &[u8],
    transform: FaceCropTransform,
    edge: u32,
) -> Result<Vec<u8>, PortraitError> {
    let mut out = vec![0; edge as usize * edge as usize * 4];
    for y in 0..edge {
        for x in 0..edge {
            let (sx, sy) = transform.source_point(x as f32 + 0.5, y as f32 + 0.5, edge as f32)?;
            let dx = sx.round() as i32;
            let dy = sy.round() as i32;
            let destination = (y as usize * edge as usize + x as usize) * 4;
            if dx >= 0 && dy >= 0 && dx < width as i32 && dy < height as i32 {
                let source = (dy as usize * width as usize + dx as usize) * 4;
                out[destination..destination + 4].copy_from_slice(&rgba[source..source + 4]);
            } else {
                out[destination + 3] = 255;
            }
        }
    }
    Ok(out)
}

fn decode_yunet_outputs(
    output: &ort::session::SessionOutputs<'_>,
    source_width: u32,
    source_height: u32,
) -> Result<Vec<DetectionCandidate>, PortraitError> {
    // YuNet has three feature strides (8, 16, 32), each with class/objectness/bbox/keypoint
    // tensors. We identify them by validated tensor channel shape rather than relying on an
    // upstream output ordering.
    let mut grouped: BTreeMap<usize, BTreeMap<String, Vec<f32>>> = BTreeMap::new();
    for (name, value) in output {
        let (shape, data) = value
            .try_extract_tensor::<f32>()
            .map_err(|error| PortraitError::InvalidDetectionOutput(error.to_string()))?;
        if shape.len() != 3
            || shape[0] != 1
            || !(shape[2] == 1 || shape[2] == 4 || shape[2] == 10)
            || data.iter().any(|value| !value.is_finite())
        {
            return Err(PortraitError::InvalidDetectionOutput(format!(
                "unexpected YuNet output shape {shape:?}"
            )));
        }
        grouped
            .entry(shape[1] as usize)
            .or_default()
            .insert(name.to_ascii_lowercase(), data.to_vec());
    }
    let mut candidates = Vec::new();
    for (anchors, tensors) in grouped {
        let cls = tensors
            .iter()
            .find(|(name, _)| name.starts_with("cls_"))
            .map(|(_, value)| value)
            .ok_or_else(|| PortraitError::InvalidDetectionOutput("missing class tensor".into()))?;
        let objectness = tensors
            .iter()
            .find(|(name, _)| name.starts_with("obj_"))
            .map(|(_, value)| value)
            .ok_or_else(|| {
                PortraitError::InvalidDetectionOutput("missing objectness tensor".into())
            })?;
        let bbox = tensors
            .iter()
            .find(|(name, _)| name.starts_with("bbox_"))
            .map(|(_, value)| value)
            .ok_or_else(|| PortraitError::InvalidDetectionOutput("missing bbox tensor".into()))?;
        let kps = tensors
            .iter()
            .find(|(name, _)| name.starts_with("kps_"))
            .map(|(_, value)| value)
            .ok_or_else(|| {
                PortraitError::InvalidDetectionOutput("missing keypoint tensor".into())
            })?;
        let stride = match anchors {
            1600 => 8.0,
            400 => 16.0,
            100 => 32.0,
            _ => {
                return Err(PortraitError::InvalidDetectionOutput(format!(
                    "unsupported YuNet anchor count {anchors}"
                )));
            }
        };
        let grid_width = (320.0 / stride) as usize;
        for index in 0..anchors {
            let confidence =
                (cls[index].clamp(0.0, 1.0) * objectness[index].clamp(0.0, 1.0)).sqrt();
            if confidence < 0.65 {
                continue;
            }
            let gx = (index % grid_width) as f32;
            let gy = (index / grid_width) as f32;
            let center_x = (gx + bbox[index * 4]) * stride;
            let center_y = (gy + bbox[index * 4 + 1]) * stride;
            let box_w = bbox[index * 4 + 2].exp() * stride;
            let box_h = bbox[index * 4 + 3].exp() * stride;
            if ![center_x, center_y, box_w, box_h]
                .into_iter()
                .all(f32::is_finite)
                || box_w <= 1.0
                || box_h <= 1.0
            {
                continue;
            }
            let scale_x = source_width as f32 / 320.0;
            let scale_y = source_height as f32 / 320.0;
            let bounds = FaceBounds {
                left: ((center_x - box_w * 0.5) * scale_x / source_width as f32).clamp(0.0, 1.0),
                top: ((center_y - box_h * 0.5) * scale_y / source_height as f32).clamp(0.0, 1.0),
                right: ((center_x + box_w * 0.5) * scale_x / source_width as f32).clamp(0.0, 1.0),
                bottom: ((center_y + box_h * 0.5) * scale_y / source_height as f32).clamp(0.0, 1.0),
            };
            if bounds.right <= bounds.left || bounds.bottom <= bounds.top {
                continue;
            }
            let mut landmarks = [Landmark {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            }; 5];
            for point in 0..5 {
                landmarks[point] = Landmark {
                    x: ((gx + kps[index * 10 + point * 2]) * stride * scale_x
                        / source_width as f32)
                        .clamp(0.0, 1.0),
                    y: ((gy + kps[index * 10 + point * 2 + 1]) * stride * scale_y
                        / source_height as f32)
                        .clamp(0.0, 1.0),
                    z: 0.0,
                };
            }
            candidates.push(DetectionCandidate {
                confidence,
                bounds,
                landmarks,
            });
        }
    }
    Ok(candidates)
}

fn non_maximum_suppression(
    mut candidates: Vec<DetectionCandidate>,
    threshold: f32,
) -> Vec<DetectionCandidate> {
    candidates.sort_by(|left, right| right.confidence.total_cmp(&left.confidence));
    let mut accepted: Vec<DetectionCandidate> = Vec::new();
    for candidate in candidates {
        let overlaps = accepted
            .iter()
            .any(|existing| iou(existing.bounds, candidate.bounds) >= threshold);
        if !overlaps {
            accepted.push(candidate);
        }
    }
    accepted
}

fn iou(left: FaceBounds, right: FaceBounds) -> f32 {
    let x0 = left.left.max(right.left);
    let y0 = left.top.max(right.top);
    let x1 = left.right.min(right.right);
    let y1 = left.bottom.min(right.bottom);
    let intersection = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
    let left_area = (left.right - left.left).max(0.0) * (left.bottom - left.top).max(0.0);
    let right_area = (right.right - right.left).max(0.0) * (right.bottom - right.top).max(0.0);
    intersection / (left_area + right_area - intersection).max(1.0e-6)
}

/// Holds soft parser probabilities in CHW order; this deliberately preserves logits-derived
/// coverage until semantic masks have been refined and projected to source-image space.
struct ParserProbabilities {
    classes: usize,
    width: usize,
    height: usize,
    values: Vec<f32>,
}

fn parser_probabilities(
    output: &ort::session::SessionOutputs<'_>,
) -> Result<ParserProbabilities, PortraitError> {
    if output.len() == 0 {
        return Err(PortraitError::InvalidParsingOutput(
            "parser emitted no outputs".into(),
        ));
    }
    let value = &output[0];
    let (shape, logits) = value
        .try_extract_tensor::<f32>()
        .map_err(|error| PortraitError::InvalidParsingOutput(error.to_string()))?;
    if shape.len() != 4
        || shape[0] != 1
        || shape[1] < 19
        || shape[2] == 0
        || shape[3] == 0
        || logits.iter().any(|value| !value.is_finite())
    {
        return Err(PortraitError::InvalidParsingOutput(format!(
            "expected [1,>=19,H,W], got {shape:?}"
        )));
    }
    let classes = shape[1] as usize;
    let height = shape[2] as usize;
    let width = shape[3] as usize;
    let mut values = vec![0.0; logits.len()];
    for pixel in 0..width * height {
        let max = (0..classes)
            .map(|class| logits[class * width * height + pixel])
            .fold(f32::NEG_INFINITY, f32::max);
        let sum = (0..classes)
            .map(|class| (logits[class * width * height + pixel] - max).exp())
            .sum::<f32>();
        for class in 0..classes {
            values[class * width * height + pixel] =
                ((logits[class * width * height + pixel] - max).exp() / sum).clamp(0.0, 1.0);
        }
    }
    Ok(ParserProbabilities {
        classes,
        width,
        height,
        values,
    })
}

impl ParserProbabilities {
    fn class(&self, class: usize, x: usize, y: usize) -> f32 {
        if class >= self.classes {
            0.0
        } else {
            self.values[class * self.width * self.height + y * self.width + x]
        }
    }
}

fn project_semantic_regions(
    probabilities: &ParserProbabilities,
    source_width: u32,
    source_height: u32,
    transform: FaceCropTransform,
) -> Result<BTreeMap<PortraitRegion, SoftMask>, PortraitError> {
    let mut output: BTreeMap<PortraitRegion, Vec<f32>> = PortraitRegion::iter()
        .map(|region| {
            (
                region,
                vec![0.0; source_width as usize * source_height as usize],
            )
        })
        .collect();
    for y in 0..probabilities.height {
        for x in 0..probabilities.width {
            let (source_x, source_y) = transform.source_point(
                x as f32 + 0.5,
                y as f32 + 0.5,
                probabilities.width as f32,
            )?;
            if source_x < 0.0
                || source_y < 0.0
                || source_x >= source_width as f32
                || source_y >= source_height as f32
            {
                continue;
            }
            let index = source_y
                .round()
                .clamp(0.0, source_height.saturating_sub(1) as f32)
                as usize
                * source_width as usize
                + source_x
                    .round()
                    .clamp(0.0, source_width.saturating_sub(1) as f32) as usize;
            // CelebAMask-HQ / yakhyo class indices: skin=1, brows=2/3, eyes=4/5, eyeglass=6,
            // mouth=11, upper/lower lip=12/13, hair=17. Skin explicitly removes every protected
            // semantic probability rather than depending on an argmax label map.
            let skin = probabilities.class(1, x, y);
            let left_brow = probabilities.class(2, x, y);
            let right_brow = probabilities.class(3, x, y);
            let left_eye = probabilities.class(4, x, y);
            let right_eye = probabilities.class(5, x, y);
            let glasses = probabilities.class(6, x, y);
            let mouth = probabilities.class(11, x, y);
            let lips =
                (probabilities.class(12, x, y) + probabilities.class(13, x, y)).clamp(0.0, 1.0);
            let hair = probabilities.class(17, x, y);
            let brows = (left_brow + right_brow).clamp(0.0, 1.0);
            let eyes = (left_eye + right_eye).clamp(0.0, 1.0);
            let protected = (eyes + brows + lips + mouth + hair + glasses).clamp(0.0, 1.0);
            let face = (skin + eyes + brows + lips + mouth + probabilities.class(10, x, y))
                .clamp(0.0, 1.0);
            let values = [
                (PortraitRegion::Face, face),
                (PortraitRegion::Skin, skin * (1.0 - protected)),
                (PortraitRegion::Eyes, eyes),
                (PortraitRegion::LeftEye, left_eye),
                (PortraitRegion::RightEye, right_eye),
                (PortraitRegion::Brows, brows),
                (PortraitRegion::LeftBrow, left_brow),
                (PortraitRegion::RightBrow, right_brow),
                (PortraitRegion::Lips, lips),
                (PortraitRegion::Mouth, mouth),
                (PortraitRegion::Hair, hair),
            ];
            for (region, value) in values {
                let destination = output.get_mut(&region).expect("fixed region map");
                destination[index] = destination[index].max(value.clamp(0.0, 1.0));
            }
        }
    }
    output
        .into_iter()
        .map(|(region, values)| {
            Ok((
                region,
                SoftMask::new(
                    source_width,
                    source_height,
                    feather_mask(values, source_width, source_height, 0.75),
                )?,
            ))
        })
        .collect()
}

fn feather_mask(values: Vec<f32>, width: u32, height: u32, radius: f32) -> Vec<f32> {
    if radius <= 0.0 {
        return values;
    }
    let mut output = values.clone();
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let mut sum = 0.0;
            let mut count = 0.0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let sx = (x + dx).clamp(0, width as i32 - 1) as usize;
                    let sy = (y + dy).clamp(0, height as i32 - 1) as usize;
                    sum += values[sy * width as usize + sx];
                    count += 1.0;
                }
            }
            output[y as usize * width as usize + x as usize] =
                (values[y as usize * width as usize + x as usize] * (1.0 - radius)
                    + sum / count * radius)
                    .clamp(0.0, 1.0);
        }
    }
    output
}

impl PortraitRegion {
    fn iter() -> impl Iterator<Item = Self> {
        [
            Self::Face,
            Self::Skin,
            Self::Eyes,
            Self::LeftEye,
            Self::RightEye,
            Self::Brows,
            Self::LeftBrow,
            Self::RightBrow,
            Self::Lips,
            Self::Mouth,
            Self::Hair,
        ]
        .into_iter()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Landmark {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub trait FaceLandmarkProvider {
    type Error;

    fn detect(
        &self,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<Vec<Vec<Landmark>>, Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FaceBounds {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl FaceBounds {
    pub fn from_landmarks(landmarks: &[Landmark]) -> Option<Self> {
        let first = landmarks.first()?;
        let mut left = first.x;
        let mut right = first.x;
        let mut top = first.y;
        let mut bottom = first.y;
        for point in &landmarks[1..] {
            if !point.x.is_finite() || !point.y.is_finite() {
                continue;
            }
            left = left.min(point.x);
            right = right.max(point.x);
            top = top.min(point.y);
            bottom = bottom.max(point.y);
        }
        Some(Self {
            left: left.clamp(0.0, 1.0),
            top: top.clamp(0.0, 1.0),
            right: right.clamp(0.0, 1.0),
            bottom: bottom.clamp(0.0, 1.0),
        })
    }

    pub fn expanded(self, fraction: f32) -> Self {
        let expansion_x = (self.right - self.left) * fraction.max(0.0);
        let expansion_y = (self.bottom - self.top) * fraction.max(0.0);
        Self {
            left: (self.left - expansion_x).clamp(0.0, 1.0),
            top: (self.top - expansion_y).clamp(0.0, 1.0),
            right: (self.right + expansion_x).clamp(0.0, 1.0),
            bottom: (self.bottom + expansion_y).clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrequencySplitParams {
    /// Full-resolution Gaussian radius in pixels.
    pub radius: f32,
    /// 0..1 high-frequency attenuation during recombination.
    pub smooth_strength: f32,
}

impl Default for FrequencySplitParams {
    fn default() -> Self {
        Self {
            radius: 4.0,
            smooth_strength: 0.35,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrequencyLayers {
    pub low: LinearImage,
    pub high: LinearImage,
}

/// M17's non-destructive skin retouch controls. All fields live in the native shared graph;
/// callers supply only soft semantic masks, never pixels over the UI boundary.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinRetouchParameters {
    /// 0..1 high-frequency attenuation, deliberately capped below full texture removal.
    pub smooth: f32,
    /// 0..1 retained natural texture, default 0.70.
    pub texture: f32,
    /// 0..1 low-frequency skin-tone equalisation.
    pub tone_evenness: f32,
    /// -30..30 degree OKLCh hue adjustment, applied inside protected skin only.
    pub hue_degrees: f32,
    /// -0.5..0.5 relative OKLCh chroma adjustment.
    pub chroma: f32,
    /// -2..2 scene-linear exposure EV.
    pub exposure_ev: f32,
}

impl Default for SkinRetouchParameters {
    fn default() -> Self {
        Self {
            smooth: 0.0,
            texture: 0.70,
            tone_evenness: 0.0,
            hue_degrees: 0.0,
            chroma: 0.0,
            exposure_ev: 0.0,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum SkinRetouchError {
    #[error("skin retouch controls are outside supported finite ranges")]
    InvalidParameters,
    #[error("skin/protection masks do not match the input image")]
    InvalidMask,
}

impl SkinRetouchParameters {
    pub fn validated(self) -> Result<Self, SkinRetouchError> {
        if ![
            self.smooth,
            self.texture,
            self.tone_evenness,
            self.hue_degrees,
            self.chroma,
            self.exposure_ev,
        ]
        .into_iter()
        .all(f32::is_finite)
            || !(0.0..=1.0).contains(&self.smooth)
            || !(0.0..=1.0).contains(&self.texture)
            || !(0.0..=1.0).contains(&self.tone_evenness)
            || !(-30.0..=30.0).contains(&self.hue_degrees)
            || !(-0.5..=0.5).contains(&self.chroma)
            || !(-2.0..=2.0).contains(&self.exposure_ev)
        {
            return Err(SkinRetouchError::InvalidParameters);
        }
        Ok(self)
    }
}

/// Frequency-aware, edge-protected skin retouch. `skin_mask` is the semantic skin probability;
/// `protected_mask` is the union of eyes/brows/lips/hair (and any provider-specific protected
/// regions). No stage clamps scene-linear values, so HDR headroom remains intact.
pub fn apply_skin_retouch(
    image: &LinearImage,
    parameters: SkinRetouchParameters,
    skin_mask: &[f32],
    protected_mask: &[f32],
) -> Result<LinearImage, SkinRetouchError> {
    let parameters = parameters.validated()?;
    let count = image.width.saturating_mul(image.height);
    if skin_mask.len() != count
        || protected_mask.len() != count
        || skin_mask
            .iter()
            .chain(protected_mask)
            .any(|value| !value.is_finite())
    {
        return Err(SkinRetouchError::InvalidMask);
    }
    if parameters == SkinRetouchParameters::default() {
        return Ok(image.clone());
    }

    // The split radius grows gently with smoothing. High frequency is never removed completely:
    // even at slider maximum, retain at least 25% and honour the texture preservation control.
    let layers = split_frequency_image(
        image,
        FrequencySplitParams {
            radius: 2.5 + parameters.smooth * 5.5,
            smooth_strength: parameters.smooth,
        },
    );
    let even_low = gaussian_blur(&layers.low, 5.0 + parameters.tone_evenness * 11.0);
    let mut output = image.clone();
    for pixel in 0..count {
        let coverage = (skin_mask[pixel].clamp(0.0, 1.0)
            * (1.0 - protected_mask[pixel].clamp(0.0, 1.0)))
        .clamp(0.0, 1.0);
        if coverage <= 0.0 {
            continue;
        }
        // Texture=0 is still intentionally safe: maximum attenuation stays below 75%.
        let attenuation = parameters.smooth * (0.75 - parameters.texture * 0.35).clamp(0.25, 0.75);
        for channel in 0..3 {
            let index = pixel * 3 + channel;
            let low = layers.low.data[index];
            let equalized_low = low + (even_low.data[index] - low) * parameters.tone_evenness;
            let retouched = equalized_low + layers.high.data[index] * (1.0 - attenuation);
            output.data[index] = image.data[index] + (retouched - image.data[index]) * coverage;
        }
        let index = pixel * 3;
        let mut lch = oklab_to_oklch(rec2020_to_oklab(LinearRgb {
            r: output.data[index],
            g: output.data[index + 1],
            b: output.data[index + 2],
        }));
        lch.h_deg += parameters.hue_degrees * coverage;
        lch.c = (lch.c * (1.0 + parameters.chroma * coverage)).max(0.0);
        let mut rgb = oklab_to_rec2020(oklch_to_oklab(lch));
        let exposure = 2.0_f32.powf(parameters.exposure_ev * coverage);
        rgb.r *= exposure;
        rgb.g *= exposure;
        rgb.b *= exposure;
        if ![rgb.r, rgb.g, rgb.b].into_iter().all(f32::is_finite) {
            return Err(SkinRetouchError::InvalidParameters);
        }
        output.data[index] = rgb.r;
        output.data[index + 1] = rgb.g;
        output.data[index + 2] = rgb.b;
    }
    Ok(output)
}

pub fn split_frequency_image(
    image: &LinearImage,
    parameters: FrequencySplitParams,
) -> FrequencyLayers {
    let low = gaussian_blur(image, parameters.radius.max(0.25));
    let mut high = image.clone();
    for ((destination, source), base) in high.data.iter_mut().zip(&image.data).zip(&low.data) {
        *destination = source - base;
    }
    FrequencyLayers { low, high }
}

/// Recombines frequency layers while attenuating only the high-frequency component.
/// A production skin tool can additionally modify the low layer inside a skin mask before this
/// step; this reference deliberately does not blur the entire face indiscriminately.
pub fn recombine_frequency(
    layers: &FrequencyLayers,
    smooth_strength: f32,
    mask: Option<&[f32]>,
) -> LinearImage {
    let keep = 1.0 - smooth_strength.clamp(0.0, 1.0);
    let pixel_count = layers.low.width.saturating_mul(layers.low.height);
    let mut output = layers.low.clone();
    for pixel in 0..pixel_count {
        let mask_weight = mask
            .and_then(|values| values.get(pixel))
            .copied()
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let local_keep = 1.0 - (1.0 - keep) * mask_weight;
        for channel in 0..3 {
            let index = pixel * 3 + channel;
            output.data[index] = layers.low.data[index] + layers.high.data[index] * local_keep;
        }
    }
    output
}

/// Legacy 1D reference retained for project/test compatibility. New portrait rendering uses the
/// 2D RGB functions above.
pub fn split_frequency(signal: &[f32], params: FrequencySplitParams) -> (Vec<f32>, Vec<f32>) {
    if signal.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let radius = params.radius.round().max(1.0) as usize;
    let mut low = vec![0.0; signal.len()];
    for (index, output) in low.iter_mut().enumerate() {
        let start = index.saturating_sub(radius);
        let end = (index + radius + 1).min(signal.len());
        let sum: f32 = signal[start..end].iter().copied().sum();
        *output = sum / (end - start) as f32;
    }
    let high = signal
        .iter()
        .zip(&low)
        .map(|(source, base)| source - base)
        .collect();
    (low, high)
}

pub fn recombine_with_smoothing(low: &[f32], high: &[f32], strength: f32) -> Vec<f32> {
    let keep = 1.0 - strength.clamp(0.0, 1.0);
    low.iter()
        .zip(high)
        .map(|(low_value, high_value)| low_value + high_value * keep)
        .collect()
}

pub const BIREFNET_MODEL_ID: &str = "birefnet-subject";
pub const BIREFNET_MODEL_VERSION: &str = "v1/BiRefNet-general-bb_swin_v1_tiny-epoch_232";
pub const BIREFNET_MODEL_SHA256: &str =
    "5600024376f572a557870a5eb0afb1e5961636bef4e1e22132025467d0f03333";
pub const SEGFORMER_MODEL_ID: &str = "segformer-b0-ade20k-sky";
pub const SEGFORMER_MODEL_VERSION: &str = "489d5cd81a0b59fab9b7ea758d3548ebe99677da";
pub const SEGFORMER_MODEL_SHA256: &str =
    "56d255beface9e9f82ab68a1292b8b03881aa45161dffe914b7fb9657133dc58";
pub const SEGFORMER_SKY_CLASS_ID: usize = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum AiMaskSemantic {
    Subject,
    Background,
    Person,
    Sky,
    Skin,
    Hair,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AiMaskError {
    #[error("AI mask model is missing: {path}")]
    ModelMissing { path: PathBuf },
    #[error("AI mask model hash mismatch for {model_id}: expected {expected}, got {actual}")]
    ModelHashMismatch {
        model_id: String,
        expected: String,
        actual: String,
    },
    #[error("ONNX Runtime is unavailable: {0}")]
    RuntimeUnavailable(String),
    #[error("AI mask provider initialization failed: {0}")]
    ProviderInitializationFailed(String),
    #[error("DirectML is unavailable: {0}")]
    DirectMlUnavailable(String),
    #[error("AI mask inference failed: {0}")]
    InferenceFailed(String),
    #[error("AI mask input tensor is invalid: {0}")]
    InvalidTensor(String),
    #[error("AI mask output is invalid: {0}")]
    InvalidOutput(String),
    #[error("AI mask inference ran out of memory")]
    OutOfMemory,
    #[error("AI mask generation was cancelled")]
    Cancelled,
    #[error("semantic {0:?} is provided by the M16 portrait provider")]
    PortraitProviderRequired(AiMaskSemantic),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiMaskModelDescriptor {
    pub id: String,
    pub version: String,
    pub sha256: String,
    pub path: PathBuf,
}

impl AiMaskModelDescriptor {
    fn verify(&self) -> Result<(), AiMaskError> {
        if !self.path.is_file() {
            return Err(AiMaskError::ModelMissing {
                path: self.path.clone(),
            });
        }
        let bytes = fs::read(&self.path)
            .map_err(|error| AiMaskError::RuntimeUnavailable(error.to_string()))?;
        let actual = format!("{:x}", Sha256::digest(bytes));
        if actual != self.sha256 {
            return Err(AiMaskError::ModelHashMismatch {
                model_id: self.id.clone(),
                expected: self.sha256.clone(),
                actual,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiMaskModelRegistry {
    pub foreground: AiMaskModelDescriptor,
    pub scene: AiMaskModelDescriptor,
    pub execution_provider: ExecutionProvider,
}

impl AiMaskModelRegistry {
    pub fn local_default(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        Self {
            foreground: AiMaskModelDescriptor {
                id: BIREFNET_MODEL_ID.into(),
                version: BIREFNET_MODEL_VERSION.into(),
                sha256: BIREFNET_MODEL_SHA256.into(),
                path: root.join("BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx"),
            },
            scene: AiMaskModelDescriptor {
                id: SEGFORMER_MODEL_ID.into(),
                version: SEGFORMER_MODEL_VERSION.into(),
                sha256: SEGFORMER_MODEL_SHA256.into(),
                path: root.join("segformer-b0-ade20k-489d5cd.onnx"),
            },
            execution_provider: ExecutionProvider::DirectMl,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedAiMask {
    pub semantic: AiMaskSemantic,
    pub provider_id: String,
    pub model_id: String,
    pub model_version: String,
    pub model_hash: String,
    pub cache_identity: String,
    pub execution_provider: ExecutionProvider,
    pub mask: SoftMask,
}

pub trait AiMaskProvider {
    fn generate(
        &mut self,
        width: u32,
        height: u32,
        rgba: &[u8],
        source_hash: &str,
        semantic: AiMaskSemantic,
        cancellation: &AtomicBool,
    ) -> Result<GeneratedAiMask, AiMaskError>;
}

pub struct AiMaskOnnxProvider {
    foreground: Session,
    scene: Session,
    pub registry: AiMaskModelRegistry,
    pub execution_provider: ExecutionProvider,
}

impl AiMaskOnnxProvider {
    pub fn initialize(registry: AiMaskModelRegistry) -> Result<Self, AiMaskError> {
        registry.foreground.verify()?;
        registry.scene.verify()?;
        let open = |descriptor: &AiMaskModelDescriptor, provider| {
            let ep = match provider {
                ExecutionProvider::Cpu => CPUExecutionProvider::default().build(),
                ExecutionProvider::DirectMl => DirectMLExecutionProvider::default().build(),
            };
            Session::builder()
                .map_err(|error| AiMaskError::ProviderInitializationFailed(error.to_string()))?
                .with_execution_providers([ep])
                .map_err(|error| AiMaskError::ProviderInitializationFailed(error.to_string()))?
                .commit_from_file(&descriptor.path)
                .map_err(|error| AiMaskError::ProviderInitializationFailed(error.to_string()))
        };
        let requested = registry.execution_provider;
        let sessions = match requested {
            ExecutionProvider::Cpu => (
                open(&registry.foreground, requested)?,
                open(&registry.scene, requested)?,
                requested,
            ),
            ExecutionProvider::DirectMl => match (
                open(&registry.foreground, requested),
                open(&registry.scene, requested),
            ) {
                (Ok(foreground), Ok(scene)) => (foreground, scene, requested),
                _ => (
                    open(&registry.foreground, ExecutionProvider::Cpu)?,
                    open(&registry.scene, ExecutionProvider::Cpu)?,
                    ExecutionProvider::Cpu,
                ),
            },
        };
        Ok(Self {
            foreground: sessions.0,
            scene: sessions.1,
            registry,
            execution_provider: sessions.2,
        })
    }

    fn cache_identity(source_hash: &str, semantic: AiMaskSemantic, model_hash: &str) -> String {
        format!(
            "{:x}",
            Sha256::digest(
                format!("{source_hash}:ai-mask-v1:{semantic:?}:{model_hash}").as_bytes()
            )
        )
    }
}

impl AiMaskProvider for AiMaskOnnxProvider {
    fn generate(
        &mut self,
        width: u32,
        height: u32,
        rgba: &[u8],
        source_hash: &str,
        semantic: AiMaskSemantic,
        cancellation: &AtomicBool,
    ) -> Result<GeneratedAiMask, AiMaskError> {
        if cancellation.load(Ordering::Relaxed) {
            return Err(AiMaskError::Cancelled);
        }
        if matches!(
            semantic,
            AiMaskSemantic::Person | AiMaskSemantic::Skin | AiMaskSemantic::Hair
        ) {
            return Err(AiMaskError::PortraitProviderRequired(semantic));
        }
        let (descriptor, format, classes, class_id, invert, provider_id): (
            &AiMaskModelDescriptor,
            ModelInputFormat,
            usize,
            usize,
            bool,
            &str,
        ) = match semantic {
            AiMaskSemantic::Subject => (
                &self.registry.foreground,
                ModelInputFormat::birefnet(),
                1,
                0,
                false,
                "foreground",
            ),
            AiMaskSemantic::Background => (
                &self.registry.foreground,
                ModelInputFormat::birefnet(),
                1,
                0,
                true,
                "foreground",
            ),
            AiMaskSemantic::Sky => (
                &self.registry.scene,
                ModelInputFormat::segformer(),
                150,
                SEGFORMER_SKY_CLASS_ID,
                false,
                "semantic-scene",
            ),
            _ => unreachable!(),
        };
        let input = resize_rgba_rgb_chw(width, height, rgba, format)
            .map_err(|error| AiMaskError::InvalidTensor(error.to_string()))?;
        let tensor = Array4::from_shape_vec(
            (
                1,
                3,
                format.target_height as usize,
                format.target_width as usize,
            ),
            input,
        )
        .map_err(|error| AiMaskError::InvalidTensor(error.to_string()))?;
        let outputs = if provider_id == "foreground" {
            self.foreground
                .run(ort::inputs![TensorRef::from_array_view(&tensor).map_err(
                    |error| AiMaskError::InvalidTensor(error.to_string())
                )?])
        } else {
            self.scene
                .run(ort::inputs![TensorRef::from_array_view(&tensor).map_err(
                    |error| AiMaskError::InvalidTensor(error.to_string())
                )?])
        }
        .map_err(|error| {
            let text = error.to_string();
            if text.to_ascii_lowercase().contains("memory") {
                AiMaskError::OutOfMemory
            } else {
                AiMaskError::InferenceFailed(text)
            }
        })?;
        if cancellation.load(Ordering::Relaxed) {
            return Err(AiMaskError::Cancelled);
        }
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|error| AiMaskError::InvalidOutput(error.to_string()))?;
        if shape.len() != 4
            || shape[0] != 1
            || shape[1] as usize != classes
            || data.iter().any(|value| !value.is_finite())
        {
            return Err(AiMaskError::InvalidOutput(format!(
                "expected [1,{classes},H,W], got {shape:?}"
            )));
        }
        let out_h = shape[2] as usize;
        let out_w = shape[3] as usize;
        let pixels = out_w * out_h;
        let mut values = Vec::with_capacity(pixels);
        for pixel in 0..pixels {
            let probability = if classes == 1 {
                1.0 / (1.0 + (-data[pixel]).exp())
            } else {
                let max = (0..classes)
                    .map(|class| data[class * pixels + pixel])
                    .fold(f32::NEG_INFINITY, f32::max);
                let sum = (0..classes)
                    .map(|class| (data[class * pixels + pixel] - max).exp())
                    .sum::<f32>();
                (data[class_id * pixels + pixel] - max).exp() / sum.max(1.0e-12)
            };
            values.push(if invert {
                1.0 - probability
            } else {
                probability
            });
        }
        let mask = SoftMask::new(out_w as u32, out_h as u32, values)
            .map_err(|error| AiMaskError::InvalidOutput(error.to_string()))?;
        Ok(GeneratedAiMask {
            semantic,
            provider_id: provider_id.into(),
            model_id: descriptor.id.clone(),
            model_version: descriptor.version.clone(),
            model_hash: descriptor.sha256.clone(),
            cache_identity: Self::cache_identity(source_hash, semantic, &descriptor.sha256),
            execution_provider: self.execution_provider,
            mask,
        })
    }
}

pub fn cancellation_token() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_split_recombines_exactly_at_zero_smoothing() {
        let source = [0.1, 0.2, 0.8, 0.3, 0.2];
        let (low, high) = split_frequency(&source, FrequencySplitParams::default());
        let rebuilt = recombine_with_smoothing(&low, &high, 0.0);
        for (actual, expected) in rebuilt.iter().zip(source) {
            assert!((actual - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn smoothing_reduces_high_frequency_energy() {
        let source = [0.1, 0.2, 0.8, 0.3, 0.2];
        let (low, high) = split_frequency(&source, FrequencySplitParams::default());
        let rebuilt = recombine_with_smoothing(&low, &high, 0.6);
        assert!(rebuilt[2] < source[2]);
    }

    #[test]
    fn two_dimensional_frequency_layers_recombine_at_zero_strength() {
        let image = LinearImage::new(3, 1, vec![0.1, 0.1, 0.1, 0.8, 0.7, 0.6, 0.1, 0.1, 0.1])
            .expect("fixture");
        let layers = split_frequency_image(&image, FrequencySplitParams::default());
        let rebuilt = recombine_frequency(&layers, 0.0, None);
        for (actual, expected) in rebuilt.data.iter().zip(&image.data) {
            assert!((actual - expected).abs() < 1e-5);
        }
    }

    #[test]
    fn mask_limits_smoothing_to_selected_pixels() {
        let image = LinearImage::new(3, 1, vec![0.1, 0.1, 0.1, 0.8, 0.7, 0.6, 0.1, 0.1, 0.1])
            .expect("fixture");
        let layers = split_frequency_image(&image, FrequencySplitParams::default());
        let rebuilt = recombine_frequency(&layers, 1.0, Some(&[0.0, 1.0, 0.0]));
        assert!((rebuilt.data[0] - image.data[0]).abs() < 1e-5);
        assert!((rebuilt.data[6] - image.data[6]).abs() < 1e-5);
        assert!((rebuilt.data[3] - image.data[3]).abs() > 1e-5);
    }

    #[test]
    fn m17_skin_retouch_is_identity_at_neutral_controls() {
        let image =
            LinearImage::new(2, 1, vec![0.11, 0.08, 0.06, 0.72, 0.51, 0.38]).expect("fixture");
        let output = apply_skin_retouch(
            &image,
            SkinRetouchParameters::default(),
            &[1.0, 1.0],
            &[0.0, 0.0],
        )
        .expect("identity");
        assert_eq!(output, image);
    }

    #[test]
    fn m17_skin_retouch_preserves_protected_features_and_stays_finite_at_extremes() {
        let image = LinearImage::new(
            5,
            1,
            vec![
                0.2, 0.14, 0.1, 0.8, 0.55, 0.4, 0.15, 0.1, 0.08, 1.4, 0.9, 0.5, 0.3, 0.2, 0.16,
            ],
        )
        .expect("fixture");
        let output = apply_skin_retouch(
            &image,
            SkinRetouchParameters {
                smooth: 1.0,
                texture: 0.0,
                tone_evenness: 1.0,
                hue_degrees: 30.0,
                chroma: -0.5,
                exposure_ev: 2.0,
            },
            &[1.0; 5],
            &[0.0, 0.0, 1.0, 0.0, 0.0],
        )
        .expect("retouch");
        assert_eq!(
            &output.data[6..9],
            &image.data[6..9],
            "eye/lip protection must be exact"
        );
        assert!(output.data.iter().all(|value| value.is_finite()));
        assert!(
            output
                .data
                .iter()
                .zip(&image.data)
                .any(|(left, right)| (left - right).abs() > 1.0e-4)
        );
    }

    #[test]
    fn m17_skin_retouch_rejects_non_finite_or_wrong_masks() {
        let image = LinearImage::new(1, 1, vec![0.2, 0.2, 0.2]).expect("fixture");
        assert!(matches!(
            apply_skin_retouch(
                &image,
                SkinRetouchParameters {
                    smooth: f32::NAN,
                    ..Default::default()
                },
                &[1.0],
                &[0.0]
            ),
            Err(SkinRetouchError::InvalidParameters)
        ));
        assert!(matches!(
            apply_skin_retouch(&image, SkinRetouchParameters::default(), &[], &[0.0]),
            Err(SkinRetouchError::InvalidMask)
        ));
    }

    #[test]
    fn face_bounds_are_normalized_and_expandable() {
        let bounds = FaceBounds::from_landmarks(&[
            Landmark {
                x: 0.3,
                y: 0.2,
                z: 0.0,
            },
            Landmark {
                x: 0.7,
                y: 0.8,
                z: 0.0,
            },
        ])
        .expect("bounds");
        let expanded = bounds.expanded(0.1);
        assert!(expanded.left < bounds.left);
        assert!(expanded.right > bounds.right);
        assert!(expanded.top < bounds.top);
        assert!(expanded.bottom > bounds.bottom);
    }

    #[test]
    fn m16_face_id_is_stable_per_source_geometry_without_biometric_state() {
        let bounds = FaceBounds {
            left: 0.2,
            top: 0.1,
            right: 0.7,
            bottom: 0.8,
        };
        let landmarks = [Landmark {
            x: 0.3,
            y: 0.3,
            z: 0.0,
        }; 5];
        assert_eq!(
            stable_face_id("photo-a", bounds, &landmarks),
            stable_face_id("photo-a", bounds, &landmarks)
        );
        assert_ne!(
            stable_face_id("photo-a", bounds, &landmarks),
            stable_face_id("photo-b", bounds, &landmarks)
        );
    }

    #[test]
    fn m16_crop_transform_is_finite_and_keeps_eye_line_rotation() {
        let bounds = FaceBounds {
            left: 0.25,
            top: 0.2,
            right: 0.75,
            bottom: 0.8,
        };
        let crop = FaceCropTransform::from_face(
            bounds,
            1000,
            800,
            1.4,
            Landmark {
                x: 0.35,
                y: 0.35,
                z: 0.0,
            },
            Landmark {
                x: 0.65,
                y: 0.4,
                z: 0.0,
            },
        )
        .expect("crop");
        // The largest normalized face dimension is .5 * 1000 = 500 px; the
        // required 1.4x portrait crop is therefore exactly 700 px.  This
        // protects the contract rather than requiring arbitrary extra margin.
        assert!((crop.side - 700.0).abs() < 0.01 && crop.rotation_degrees.is_finite());
        let source = crop
            .source_point(256.0, 256.0, 512.0)
            .expect("inverse transform");
        assert!((source.0 - crop.center_x).abs() < 3.0 && (source.1 - crop.center_y).abs() < 3.0);
    }

    #[test]
    fn m16_skin_probability_excludes_protected_semantics() {
        let mut values = vec![0.0; 19];
        values[1] = 0.9; // skin
        values[4] = 0.8; // left eye
        values[17] = 0.7; // hair
        let probabilities = ParserProbabilities {
            classes: 19,
            width: 1,
            height: 1,
            values,
        };
        let transform = FaceCropTransform {
            center_x: 1.0,
            center_y: 1.0,
            side: 2.0,
            rotation_degrees: 0.0,
        };
        let regions =
            project_semantic_regions(&probabilities, 3, 3, transform).expect("semantic projection");
        let skin = regions[&PortraitRegion::Skin]
            .values
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        let eye = regions[&PortraitRegion::LeftEye]
            .values
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        assert!(skin < 0.2, "skin must exclude eye/hair probability");
        assert!(eye > 0.1);
    }

    #[test]
    fn m16_missing_or_bad_model_is_a_typed_error() {
        let missing = ModelDescriptor::yunet("not-a-real-yunet.onnx");
        assert!(matches!(
            missing.verify(false),
            Err(PortraitError::DetectorModelMissing { .. })
        ));
        let mask = SoftMask::new(1, 1, vec![f32::NAN]);
        assert!(matches!(mask, Err(PortraitError::InvalidParsingOutput(_))));
    }

    #[test]
    fn m20_registry_is_pinned_and_missing_models_are_typed() {
        let registry = AiMaskModelRegistry::local_default("definitely-missing-model-root");
        assert_eq!(registry.foreground.sha256, BIREFNET_MODEL_SHA256);
        assert_eq!(registry.scene.version, SEGFORMER_MODEL_VERSION);
        assert_eq!(SEGFORMER_SKY_CLASS_ID, 2);
        assert!(matches!(
            registry.foreground.verify(),
            Err(AiMaskError::ModelMissing { .. })
        ));
    }

    #[test]
    fn m20_hash_mismatch_and_cache_identity_are_deterministic() {
        let path = std::env::temp_dir().join(format!(
            "starroom-m20-bad-model-{}.onnx",
            std::process::id()
        ));
        fs::write(&path, b"not an ONNX model").expect("write isolated test model");
        let descriptor = AiMaskModelDescriptor {
            id: "bad-model".into(),
            version: "test".into(),
            sha256: "00".repeat(32),
            path: path.clone(),
        };
        assert!(matches!(
            descriptor.verify(),
            Err(AiMaskError::ModelHashMismatch { .. })
        ));
        fs::remove_file(path).expect("remove isolated test model");

        let first = AiMaskOnnxProvider::cache_identity(
            "source-hash",
            AiMaskSemantic::Subject,
            BIREFNET_MODEL_SHA256,
        );
        let repeated = AiMaskOnnxProvider::cache_identity(
            "source-hash",
            AiMaskSemantic::Subject,
            BIREFNET_MODEL_SHA256,
        );
        let background = AiMaskOnnxProvider::cache_identity(
            "source-hash",
            AiMaskSemantic::Background,
            BIREFNET_MODEL_SHA256,
        );
        assert_eq!(first, repeated);
        assert_ne!(first, background);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn m20_soft_masks_preserve_probability_and_reject_invalid_values() {
        let mask = SoftMask::new(2, 1, vec![0.15, 0.85]).expect("soft mask");
        assert!((mask.weight_at(0.0, 0.0) - 0.15).abs() < 1.0e-6);
        assert!((mask.weight_at(1.0, 0.0) - 0.85).abs() < 1.0e-6);
        assert!(SoftMask::new(1, 1, vec![f32::INFINITY]).is_err());
        let token = cancellation_token();
        token.store(true, Ordering::Relaxed);
        assert!(token.load(Ordering::Relaxed));
    }
}
