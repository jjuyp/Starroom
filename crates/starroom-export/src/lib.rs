//! Full-resolution professional export built on the same Native graph as preview.

use image::{ImageBuffer, Rgb, imageops};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use starroom_color_management::{BuiltinOutputProfile, LittleCmsProvider};
use starroom_imageio::{
    DecodedSourceImage, decode_source, encode_jpeg_rgb8_with_metadata,
    encode_png_rgb8_with_metadata, encode_tiff_rgb8_with_metadata,
};
use starroom_pipeline::{
    RenderSettings, render_source_export_to_icc8, render_source_export_to_srgb8,
};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

pub const EXPORT_PRESET_SCHEMA_VERSION: u32 = 1;
pub const EXPORT_ENGINE_VERSION: &str = "starroom-export-v1";
const MAX_WORKING_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("UnsupportedFormat: {0}")]
    UnsupportedFormat(String),
    #[error("UnsupportedBitDepth: {0}")]
    UnsupportedBitDepth(u8),
    #[error("UnsupportedColorProfile: {0}")]
    UnsupportedColorProfile(String),
    #[error("InvalidResize: {0}")]
    InvalidResize(String),
    #[error("InvalidFilename: {0}")]
    InvalidFilename(String),
    #[error("DestinationUnavailable: {0}")]
    DestinationUnavailable(String),
    #[error("FileAlreadyExists: {0}")]
    FileAlreadyExists(PathBuf),
    #[error("EncodeFailed: {0}")]
    EncodeFailed(String),
    #[error("ColorTransformFailed: {0}")]
    ColorTransformFailed(String),
    #[error("MetadataWriteFailed: {0}")]
    MetadataWriteFailed(String),
    #[error("OutOfMemory: estimated {0} bytes")]
    OutOfMemory(u64),
    #[error("Cancelled")]
    Cancelled,
    #[error("SourceMissing: {0}")]
    SourceMissing(PathBuf),
    #[error("ProjectInvalid: {0}")]
    ProjectInvalid(String),
    #[error("AtomicWriteFailed: {0}")]
    AtomicWriteFailed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Jpeg,
    Png,
    Tiff,
}

impl ExportFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Tiff => "tif",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputColorSpace {
    Srgb,
    DisplayP3,
    AdobeRgb,
    Rec2020,
}

impl From<OutputColorSpace> for BuiltinOutputProfile {
    fn from(value: OutputColorSpace) -> Self {
        match value {
            OutputColorSpace::Srgb => Self::Srgb,
            OutputColorSpace::DisplayP3 => Self::DisplayP3,
            OutputColorSpace::AdobeRgb => Self::AdobeRgb,
            OutputColorSpace::Rec2020 => Self::Rec2020,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum ResizeMode {
    Original,
    Width { pixels: u32 },
    Height { pixels: u32 },
    LongEdge { pixels: u32 },
    ShortEdge { pixels: u32 },
    Percentage { percent: f32 },
    FitWithin { width: u32, height: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputSharpenTarget {
    Off,
    Screen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputSharpenAmount {
    Low,
    Standard,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetadataPolicy {
    AllMetadata,
    CopyrightOnly,
    CameraMetadata,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CollisionPolicy {
    Fail,
    AutoRename,
    Overwrite,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettings {
    pub format: ExportFormat,
    pub bit_depth: u8,
    pub quality: u8,
    pub color_space: OutputColorSpace,
    pub embed_profile: bool,
    pub resize: ResizeMode,
    pub output_sharpen: OutputSharpenTarget,
    pub sharpen_amount: OutputSharpenAmount,
    pub metadata: MetadataPolicy,
    pub include_location: bool,
    pub copyright: Option<String>,
    pub filename_template: String,
    pub collision: CollisionPolicy,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            format: ExportFormat::Jpeg,
            bit_depth: 8,
            quality: 92,
            color_space: OutputColorSpace::Srgb,
            embed_profile: true,
            resize: ResizeMode::Original,
            output_sharpen: OutputSharpenTarget::Off,
            sharpen_amount: OutputSharpenAmount::Standard,
            metadata: MetadataPolicy::AllMetadata,
            include_location: false,
            copyright: None,
            filename_template: "{original_name}-starroom".into(),
            collision: CollisionPolicy::AutoRename,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPreset {
    pub schema_version: u32,
    pub name: String,
    pub settings: ExportSettings,
}

impl ExportPreset {
    pub fn new(name: impl Into<String>, settings: ExportSettings) -> Result<Self, ExportError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(ExportError::ProjectInvalid(
                "preset name is required".into(),
            ));
        }
        validate_settings(&settings)?;
        Ok(Self {
            schema_version: EXPORT_PRESET_SCHEMA_VERSION,
            name: name.trim().into(),
            settings,
        })
    }
    pub fn serialize(&self) -> Result<Vec<u8>, ExportError> {
        serde_json::to_vec_pretty(self)
            .map_err(|error| ExportError::ProjectInvalid(error.to_string()))
    }
    pub fn deserialize(bytes: &[u8]) -> Result<Self, ExportError> {
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|error| ExportError::ProjectInvalid(error.to_string()))?;
        if value.schema_version != EXPORT_PRESET_SCHEMA_VERSION {
            return Err(ExportError::ProjectInvalid(format!(
                "unsupported preset schema {}",
                value.schema_version
            )));
        }
        validate_settings(&value.settings)?;
        Ok(value)
    }
}

#[derive(Debug, Clone)]
pub struct ExportRequest {
    pub asset_id: i64,
    pub source_path: PathBuf,
    pub destination_directory: PathBuf,
    pub original_name: String,
    pub capture_date: Option<String>,
    pub rating: u8,
    pub camera: Option<String>,
    pub look: Option<String>,
    pub sequence: u32,
    pub source_fingerprint: String,
    pub edit_state_identity: String,
    pub settings: ExportSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportItemStatus {
    Pending,
    Rendering,
    Encoding,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportItemResult {
    pub asset_id: i64,
    pub status: ExportItemStatus,
    pub destination: Option<PathBuf>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub recipe_identity: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BatchExportResult {
    pub completed: Vec<ExportItemResult>,
    pub failed: Vec<ExportItemResult>,
    pub cancelled: Vec<ExportItemResult>,
    pub skipped: Vec<ExportItemResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportLogEntry {
    pub asset_id: i64,
    pub timestamp: i64,
    pub destination: PathBuf,
    pub format: ExportFormat,
    pub width: u32,
    pub height: u32,
    pub profile: OutputColorSpace,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RenderedBuffer {
    pub width: u32,
    pub height: u32,
    pub rgb8: Vec<u8>,
    pub source_exif: Option<Vec<u8>>,
}

pub trait FullResolutionRenderer {
    fn render(
        &self,
        source: &Path,
        color: OutputColorSpace,
        settings: &RenderSettings,
    ) -> Result<RenderedBuffer, ExportError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NativeSharedGraphRenderer;

impl FullResolutionRenderer for NativeSharedGraphRenderer {
    fn render(
        &self,
        source: &Path,
        color: OutputColorSpace,
        settings: &RenderSettings,
    ) -> Result<RenderedBuffer, ExportError> {
        if !source.is_file() {
            return Err(ExportError::SourceMissing(source.to_owned()));
        }
        let decoded = decode_source(source)
            .map_err(|error| ExportError::ProjectInvalid(error.to_string()))?;
        let source_exif = match &decoded {
            DecodedSourceImage::Rendered(image) => image.exif.clone(),
            DecodedSourceImage::Raw(_) => None,
        };
        let rendered = if color == OutputColorSpace::Srgb {
            render_source_export_to_srgb8(&decoded, settings)
        } else {
            let profile = LittleCmsProvider
                .builtin_output_profile_bytes(color.into())
                .map_err(|error| ExportError::UnsupportedColorProfile(error.to_string()))?;
            render_source_export_to_icc8(&decoded, settings, &profile)
        }
        .map_err(|error| ExportError::ColorTransformFailed(error.to_string()))?;
        Ok(RenderedBuffer {
            width: rendered.width,
            height: rendered.height,
            rgb8: rendered.data,
            source_exif,
        })
    }
}

pub fn validate_settings(settings: &ExportSettings) -> Result<(), ExportError> {
    if settings.bit_depth != 8 {
        return Err(ExportError::UnsupportedBitDepth(settings.bit_depth));
    }
    if settings.format == ExportFormat::Jpeg && !(1..=100).contains(&settings.quality) {
        return Err(ExportError::EncodeFailed(
            "JPEG quality must be 1..100".into(),
        ));
    }
    if settings.filename_template.trim().is_empty() {
        return Err(ExportError::InvalidFilename("template is empty".into()));
    }
    resize_dimensions(1, 1, settings.resize)?;
    Ok(())
}

pub fn resize_dimensions(
    width: u32,
    height: u32,
    mode: ResizeMode,
) -> Result<(u32, u32), ExportError> {
    if width == 0 || height == 0 {
        return Err(ExportError::InvalidResize(
            "source dimensions are zero".into(),
        ));
    }
    let calculate = |scale: f64| -> Result<(u32, u32), ExportError> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(ExportError::InvalidResize(
                "scale must be finite and positive".into(),
            ));
        }
        let w = (f64::from(width) * scale).round();
        let h = (f64::from(height) * scale).round();
        if w < 1.0 || h < 1.0 || w > f64::from(u32::MAX) || h > f64::from(u32::MAX) {
            return Err(ExportError::InvalidResize(
                "result is outside supported dimensions".into(),
            ));
        }
        Ok((w as u32, h as u32))
    };
    match mode {
        ResizeMode::Original => Ok((width, height)),
        ResizeMode::Width { pixels } => {
            if pixels == 0 {
                return Err(ExportError::InvalidResize("width is zero".into()));
            }
            calculate(f64::from(pixels) / f64::from(width))
        }
        ResizeMode::Height { pixels } => {
            if pixels == 0 {
                return Err(ExportError::InvalidResize("height is zero".into()));
            }
            calculate(f64::from(pixels) / f64::from(height))
        }
        ResizeMode::LongEdge { pixels } => {
            if pixels == 0 {
                return Err(ExportError::InvalidResize("long edge is zero".into()));
            }
            calculate(f64::from(pixels) / f64::from(width.max(height)))
        }
        ResizeMode::ShortEdge { pixels } => {
            if pixels == 0 {
                return Err(ExportError::InvalidResize("short edge is zero".into()));
            }
            calculate(f64::from(pixels) / f64::from(width.min(height)))
        }
        ResizeMode::Percentage { percent } => calculate(f64::from(percent) / 100.0),
        ResizeMode::FitWithin {
            width: bound_w,
            height: bound_h,
        } => {
            if bound_w == 0 || bound_h == 0 {
                return Err(ExportError::InvalidResize("fit bound is zero".into()));
            }
            calculate(
                (f64::from(bound_w) / f64::from(width))
                    .min(f64::from(bound_h) / f64::from(height))
                    .min(1.0),
            )
        }
    }
}

pub fn render_filename(request: &ExportRequest) -> Result<String, ExportError> {
    let original = Path::new(&request.original_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("photo");
    let mut name = request.settings.filename_template.clone();
    for (token, value) in [
        ("{original_name}", original.to_owned()),
        ("{date}", request.capture_date.clone().unwrap_or_default()),
        ("{sequence}", format!("{:04}", request.sequence)),
        ("{rating}", request.rating.to_string()),
        ("{camera}", request.camera.clone().unwrap_or_default()),
        ("{look}", request.look.clone().unwrap_or_default()),
    ] {
        name = name.replace(token, &value);
    }
    if name.contains('{') || name.contains('}') {
        return Err(ExportError::InvalidFilename(
            "unknown template token".into(),
        ));
    }
    sanitize_filename(&name)
}

pub fn sanitize_filename(value: &str) -> Result<String, ExportError> {
    let mut value = value
        .chars()
        .map(|character| {
            if matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            ) || character.is_control()
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    value = value.trim().trim_end_matches(['.', ' ']).to_owned();
    let upper = value.to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or("");
    let reserved = matches!(
        stem,
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if reserved {
        value.insert(0, '_');
    }
    if value.is_empty() {
        Err(ExportError::InvalidFilename(
            "filename is empty after sanitization".into(),
        ))
    } else {
        Ok(value)
    }
}

pub fn export_recipe_identity(request: &ExportRequest) -> Result<String, ExportError> {
    let mut hash = Sha256::new();
    hash.update(b"starroom-export-recipe-v1\0");
    hash.update(request.source_fingerprint.as_bytes());
    hash.update(request.edit_state_identity.as_bytes());
    hash.update(
        serde_json::to_vec(&request.settings)
            .map_err(|error| ExportError::ProjectInvalid(error.to_string()))?,
    );
    hash.update(EXPORT_ENGINE_VERSION.as_bytes());
    Ok(hex(hash.finalize()))
}

pub fn export_one<R: FullResolutionRenderer>(
    renderer: &R,
    request: &ExportRequest,
    render_settings: &RenderSettings,
    cancelled: &AtomicBool,
) -> Result<ExportItemResult, ExportError> {
    validate_settings(&request.settings)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err(ExportError::Cancelled);
    }
    let mut rendered = renderer.render(
        &request.source_path,
        request.settings.color_space,
        render_settings,
    )?;
    if rendered.rgb8.iter().len() != rendered.width as usize * rendered.height as usize * 3 {
        return Err(ExportError::EncodeFailed(
            "render buffer length mismatch".into(),
        ));
    }
    let (width, height) =
        resize_dimensions(rendered.width, rendered.height, request.settings.resize)?;
    memory_preflight(width, height)?;
    if (width, height) != (rendered.width, rendered.height) {
        let image = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(
            rendered.width,
            rendered.height,
            rendered.rgb8,
        )
        .ok_or_else(|| ExportError::EncodeFailed("invalid render buffer".into()))?;
        rendered.rgb8 =
            imageops::resize(&image, width, height, imageops::FilterType::Lanczos3).into_raw();
        rendered.width = width;
        rendered.height = height;
    }
    if request.settings.output_sharpen == OutputSharpenTarget::Screen {
        let (sigma, threshold) = match request.settings.sharpen_amount {
            OutputSharpenAmount::Low => (0.55, 2),
            OutputSharpenAmount::Standard => (0.8, 2),
            OutputSharpenAmount::High => (1.1, 1),
        };
        let image = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(width, height, rendered.rgb8)
            .ok_or_else(|| ExportError::EncodeFailed("invalid resize buffer".into()))?;
        rendered.rgb8 = imageops::unsharpen(&image, sigma, threshold).into_raw();
    }
    if cancelled.load(Ordering::Relaxed) {
        return Err(ExportError::Cancelled);
    }
    let profile = if request.settings.embed_profile {
        Some(
            LittleCmsProvider
                .builtin_output_profile_bytes(request.settings.color_space.into())
                .map_err(|error| ExportError::UnsupportedColorProfile(error.to_string()))?,
        )
    } else {
        None
    };
    let exif = export_exif(request, rendered.source_exif.as_deref())?;
    let bytes = match request.settings.format {
        ExportFormat::Jpeg => encode_jpeg_rgb8_with_metadata(
            &rendered.rgb8,
            width,
            height,
            request.settings.quality,
            profile,
            exif,
        ),
        ExportFormat::Png => {
            encode_png_rgb8_with_metadata(&rendered.rgb8, width, height, profile, exif)
        }
        ExportFormat::Tiff => {
            encode_tiff_rgb8_with_metadata(&rendered.rgb8, width, height, profile, exif)
        }
    }
    .map_err(|error| ExportError::EncodeFailed(error.to_string()))?;
    fs::create_dir_all(&request.destination_directory)
        .map_err(|error| ExportError::DestinationUnavailable(error.to_string()))?;
    let base = render_filename(request)?;
    let destination = resolve_collision(
        &request.destination_directory,
        &base,
        request.settings.format.extension(),
        request.settings.collision,
    )?;
    atomic_write(&destination, &bytes, cancelled)?;
    Ok(ExportItemResult {
        asset_id: request.asset_id,
        status: ExportItemStatus::Completed,
        destination: Some(destination),
        width: Some(width),
        height: Some(height),
        recipe_identity: export_recipe_identity(request)?,
        error: None,
    })
}

fn export_exif(
    request: &ExportRequest,
    source_exif: Option<&[u8]>,
) -> Result<Option<Vec<u8>>, ExportError> {
    match request.settings.metadata {
        MetadataPolicy::None => Ok(None),
        MetadataPolicy::AllMetadata if request.settings.include_location => source_exif
            .map(|value| value.to_vec())
            .map(Some)
            .ok_or_else(|| {
                ExportError::MetadataWriteFailed(
                    "location was requested but the source has no transferable EXIF".into(),
                )
            }),
        MetadataPolicy::CopyrightOnly => Ok(minimal_exif(
            None,
            None,
            request.settings.copyright.as_deref(),
        )),
        MetadataPolicy::CameraMetadata => Ok(minimal_exif(
            request.camera.as_deref(),
            request.capture_date.as_deref(),
            None,
        )),
        MetadataPolicy::AllMetadata => Ok(minimal_exif(
            request.camera.as_deref(),
            request.capture_date.as_deref(),
            request.settings.copyright.as_deref(),
        )),
    }
}

/// Creates a small IFD0 EXIF payload containing only explicitly allowed ASCII fields. It has no
/// GPS IFD pointer, so the default metadata path cannot disclose source location.
fn minimal_exif(
    camera: Option<&str>,
    date: Option<&str>,
    copyright: Option<&str>,
) -> Option<Vec<u8>> {
    let date = date.map(|value| {
        let normalized = value.replace('-', ":");
        if normalized.len() == 10 {
            format!("{normalized} 00:00:00")
        } else {
            normalized
        }
    });
    let mut fields = Vec::new();
    if let Some(value) = camera.filter(|value| !value.trim().is_empty()) {
        fields.push((0x0110_u16, value.trim().to_owned()));
    }
    if let Some(value) = date.as_deref().filter(|value| !value.trim().is_empty()) {
        fields.push((0x0132_u16, value.trim().to_owned()));
    }
    if let Some(value) = copyright.filter(|value| !value.trim().is_empty()) {
        fields.push((0x8298_u16, value.trim().to_owned()));
    }
    if fields.is_empty() {
        return None;
    }
    fields.sort_by_key(|(tag, _)| *tag);
    let count = u16::try_from(fields.len()).ok()?;
    let data_start = 8_u32 + 2 + u32::from(count) * 12 + 4;
    let mut out = vec![b'I', b'I', 42, 0, 8, 0, 0, 0];
    out.extend_from_slice(&count.to_le_bytes());
    let mut extra = Vec::new();
    for (tag, text) in fields {
        let mut value = text.replace('\0', " ").into_bytes();
        value.push(0);
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&2_u16.to_le_bytes());
        out.extend_from_slice(&u32::try_from(value.len()).ok()?.to_le_bytes());
        if value.len() <= 4 {
            value.resize(4, 0);
            out.extend_from_slice(&value);
        } else {
            let offset = data_start.checked_add(u32::try_from(extra.len()).ok()?)?;
            out.extend_from_slice(&offset.to_le_bytes());
            extra.extend_from_slice(&value);
        }
    }
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.extend_from_slice(&extra);
    Some(out)
}

pub fn export_batch<R: FullResolutionRenderer>(
    renderer: &R,
    requests: &[ExportRequest],
    render_settings: &RenderSettings,
    cancelled: &AtomicBool,
    log_path: Option<&Path>,
) -> BatchExportResult {
    let mut batch = BatchExportResult::default();
    for request in requests {
        if cancelled.load(Ordering::Relaxed) {
            batch.cancelled.push(failed_item(
                request,
                ExportItemStatus::Cancelled,
                "Cancelled",
            ));
            continue;
        }
        match export_one(renderer, request, render_settings, cancelled) {
            Ok(item) => {
                if let (Some(path), Some(width), Some(height)) =
                    (item.destination.as_ref(), item.width, item.height)
                {
                    let _ = append_log(
                        log_path,
                        ExportLogEntry {
                            asset_id: item.asset_id,
                            timestamp: now(),
                            destination: path.clone(),
                            format: request.settings.format,
                            width,
                            height,
                            profile: request.settings.color_space,
                            success: true,
                            error: None,
                        },
                    );
                }
                batch.completed.push(item)
            }
            Err(ExportError::Cancelled) => batch.cancelled.push(failed_item(
                request,
                ExportItemStatus::Cancelled,
                "Cancelled",
            )),
            Err(error) => {
                let message = error.to_string();
                let _ = append_log(
                    log_path,
                    ExportLogEntry {
                        asset_id: request.asset_id,
                        timestamp: now(),
                        destination: request.destination_directory.clone(),
                        format: request.settings.format,
                        width: 0,
                        height: 0,
                        profile: request.settings.color_space,
                        success: false,
                        error: Some(message.clone()),
                    },
                );
                batch
                    .failed
                    .push(failed_item(request, ExportItemStatus::Failed, &message));
            }
        }
    }
    batch
}

pub fn memory_preflight(width: u32, height: u32) -> Result<u64, ExportError> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(ExportError::OutOfMemory(u64::MAX))?;
    let bytes = pixels
        .checked_mul(32)
        .ok_or(ExportError::OutOfMemory(u64::MAX))?;
    if bytes > MAX_WORKING_BYTES {
        Err(ExportError::OutOfMemory(bytes))
    } else {
        Ok(bytes)
    }
}
fn resolve_collision(
    directory: &Path,
    base: &str,
    extension: &str,
    policy: CollisionPolicy,
) -> Result<PathBuf, ExportError> {
    let direct = directory.join(format!("{base}.{extension}"));
    if !direct.exists() {
        return Ok(direct);
    }
    match policy {
        CollisionPolicy::Fail => Err(ExportError::FileAlreadyExists(direct)),
        CollisionPolicy::Overwrite => Ok(direct),
        CollisionPolicy::AutoRename => {
            for index in 1..100_000 {
                let candidate = directory.join(format!("{base}-{index}.{extension}"));
                if !candidate.exists() {
                    return Ok(candidate);
                }
            }
            Err(ExportError::DestinationUnavailable(
                "automatic rename space exhausted".into(),
            ))
        }
    }
}
fn atomic_write(path: &Path, bytes: &[u8], cancelled: &AtomicBool) -> Result<(), ExportError> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.starroom-tmp", std::process::id()));
    let temporary = path.with_file_name(name);
    let result = (|| {
        let mut file = File::create(&temporary)
            .map_err(|error| ExportError::AtomicWriteFailed(error.to_string()))?;
        file.write_all(bytes)
            .map_err(|error| ExportError::AtomicWriteFailed(error.to_string()))?;
        file.sync_all()
            .map_err(|error| ExportError::AtomicWriteFailed(error.to_string()))?;
        if cancelled.load(Ordering::Relaxed) {
            return Err(ExportError::Cancelled);
        }
        if path.exists() {
            fs::remove_file(path)
                .map_err(|error| ExportError::AtomicWriteFailed(error.to_string()))?;
        }
        fs::rename(&temporary, path)
            .map_err(|error| ExportError::AtomicWriteFailed(error.to_string()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
fn append_log(path: Option<&Path>, entry: ExportLogEntry) -> Result<(), ExportError> {
    let Some(path) = path else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| ExportError::DestinationUnavailable(error.to_string()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| ExportError::DestinationUnavailable(error.to_string()))?;
    serde_json::to_writer(&mut file, &entry)
        .map_err(|error| ExportError::DestinationUnavailable(error.to_string()))?;
    file.write_all(b"\n")
        .map_err(|error| ExportError::DestinationUnavailable(error.to_string()))
}
fn failed_item(request: &ExportRequest, status: ExportItemStatus, error: &str) -> ExportItemResult {
    ExportItemResult {
        asset_id: request.asset_id,
        status,
        destination: None,
        width: None,
        height: None,
        recipe_identity: export_recipe_identity(request).unwrap_or_default(),
        error: Some(error.into()),
    }
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64
}
fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    struct MockRenderer;
    impl FullResolutionRenderer for MockRenderer {
        fn render(
            &self,
            source: &Path,
            _: OutputColorSpace,
            _: &RenderSettings,
        ) -> Result<RenderedBuffer, ExportError> {
            if source.to_string_lossy().contains("fail") {
                return Err(ExportError::SourceMissing(source.into()));
            }
            Ok(RenderedBuffer {
                width: 4,
                height: 3,
                rgb8: vec![128; 4 * 3 * 3],
                source_exif: None,
            })
        }
    }
    fn root(name: &str) -> PathBuf {
        let value = env::temp_dir().join(format!("starroom-export-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&value);
        fs::create_dir_all(&value).unwrap();
        value
    }
    fn request(root: &Path, id: i64, format: ExportFormat) -> ExportRequest {
        ExportRequest {
            asset_id: id,
            source_path: root.join("source.fake"),
            destination_directory: root.join("out"),
            original_name: "portrait.raw".into(),
            capture_date: Some("2026-08-24".into()),
            rating: 5,
            camera: Some("Nikon D750".into()),
            look: Some("Warm".into()),
            sequence: id as u32,
            source_fingerprint: "source".into(),
            edit_state_identity: "edit".into(),
            settings: ExportSettings {
                format,
                filename_template: "Kyoto_{date}_{sequence}".into(),
                ..Default::default()
            },
        }
    }
    #[test]
    fn jpeg_png_tiff_export_and_profile_embed() {
        let root = root("formats");
        for (format, extension) in [
            (ExportFormat::Jpeg, "jpg"),
            (ExportFormat::Png, "png"),
            (ExportFormat::Tiff, "tif"),
        ] {
            let result = export_one(
                &MockRenderer,
                &request(&root, 1, format),
                &RenderSettings::default(),
                &AtomicBool::new(false),
            )
            .unwrap();
            let path = result.destination.unwrap();
            assert_eq!(path.extension().unwrap(), extension);
            assert!(fs::metadata(path).unwrap().len() > 20);
        }
    }
    #[test]
    fn all_color_profiles_export() {
        let root = root("profiles");
        for color in [
            OutputColorSpace::Srgb,
            OutputColorSpace::DisplayP3,
            OutputColorSpace::AdobeRgb,
            OutputColorSpace::Rec2020,
        ] {
            let mut value = request(&root, color as i64 + 1, ExportFormat::Jpeg);
            value.settings.color_space = color;
            value.settings.filename_template = format!("profile-{}", color as u8);
            export_one(
                &MockRenderer,
                &value,
                &RenderSettings::default(),
                &AtomicBool::new(false),
            )
            .unwrap();
        }
    }
    #[test]
    fn resize_modes_preserve_aspect() {
        for (mode, expected) in [
            (ResizeMode::Original, (400, 200)),
            (ResizeMode::Width { pixels: 100 }, (100, 50)),
            (ResizeMode::Height { pixels: 100 }, (200, 100)),
            (ResizeMode::LongEdge { pixels: 100 }, (100, 50)),
            (ResizeMode::ShortEdge { pixels: 100 }, (200, 100)),
            (ResizeMode::Percentage { percent: 25.0 }, (100, 50)),
            (
                ResizeMode::FitWithin {
                    width: 80,
                    height: 80,
                },
                (80, 40),
            ),
        ] {
            assert_eq!(resize_dimensions(400, 200, mode).unwrap(), expected);
        }
        assert!(matches!(
            resize_dimensions(4, 3, ResizeMode::Width { pixels: 0 }),
            Err(ExportError::InvalidResize(_))
        ));
    }
    #[test]
    fn filename_sanitization_and_collision() {
        assert_eq!(sanitize_filename("Kyoto:<A>?*").unwrap(), "Kyoto__A___");
        assert_eq!(sanitize_filename("CON").unwrap(), "_CON");
        let root = root("collision");
        let first = request(&root, 1, ExportFormat::Jpeg);
        let a = export_one(
            &MockRenderer,
            &first,
            &RenderSettings::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let b = export_one(
            &MockRenderer,
            &first,
            &RenderSettings::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_ne!(a.destination, b.destination);
        let mut fail = first;
        fail.settings.collision = CollisionPolicy::Fail;
        assert!(matches!(
            export_one(
                &MockRenderer,
                &fail,
                &RenderSettings::default(),
                &AtomicBool::new(false)
            ),
            Err(ExportError::FileAlreadyExists(_))
        ));
    }
    #[test]
    fn batch_failure_isolation_and_cancel() {
        let root = root("batch");
        let good = request(&root, 1, ExportFormat::Png);
        let mut bad = request(&root, 2, ExportFormat::Png);
        bad.source_path = root.join("fail.fake");
        let batch = export_batch(
            &MockRenderer,
            &[good, bad],
            &RenderSettings::default(),
            &AtomicBool::new(false),
            Some(&root.join("exports.jsonl")),
        );
        assert_eq!(batch.completed.len(), 1);
        assert_eq!(batch.failed.len(), 1);
        let cancelled = export_batch(
            &MockRenderer,
            &[request(&root, 3, ExportFormat::Png)],
            &RenderSettings::default(),
            &AtomicBool::new(true),
            None,
        );
        assert_eq!(cancelled.cancelled.len(), 1);
    }
    #[test]
    fn preset_roundtrip_illegal_depth_and_memory_guard() {
        let preset = ExportPreset::new("Web", ExportSettings::default()).unwrap();
        assert_eq!(
            ExportPreset::deserialize(&preset.serialize().unwrap()).unwrap(),
            preset
        );
        let settings = ExportSettings {
            bit_depth: 16,
            ..ExportSettings::default()
        };
        assert!(matches!(
            validate_settings(&settings),
            Err(ExportError::UnsupportedBitDepth(16))
        ));
        assert!(memory_preflight(100_000, 100_000).is_err());
    }

    #[test]
    fn quality_endpoints_and_metadata_privacy_policy() {
        let root = root("metadata");
        for (id, quality) in [(1, 1), (2, 100)] {
            let mut value = request(&root, id, ExportFormat::Jpeg);
            value.settings.quality = quality;
            value.settings.filename_template = format!("quality-{quality}");
            assert!(
                export_one(
                    &MockRenderer,
                    &value,
                    &RenderSettings::default(),
                    &AtomicBool::new(false)
                )
                .is_ok()
            );
        }
        let mut none = request(&root, 3, ExportFormat::Png);
        none.settings.metadata = MetadataPolicy::None;
        none.settings.filename_template = "metadata-none".into();
        assert!(
            export_one(
                &MockRenderer,
                &none,
                &RenderSettings::default(),
                &AtomicBool::new(false)
            )
            .is_ok()
        );
        let safe = minimal_exif(Some("Nikon D750"), Some("2026-08-24"), Some("Owner"))
            .expect("safe metadata");
        let parsed = exif::Reader::new().read_raw(safe).expect("valid EXIF");
        assert!(
            parsed
                .get_field(exif::Tag::GPSInfoIFDPointer, exif::In::PRIMARY)
                .is_none()
        );
        let mut unavailable_location = request(&root, 4, ExportFormat::Jpeg);
        unavailable_location.settings.include_location = true;
        assert!(matches!(
            export_one(
                &MockRenderer,
                &unavailable_location,
                &RenderSettings::default(),
                &AtomicBool::new(false)
            ),
            Err(ExportError::MetadataWriteFailed(_))
        ));
    }

    #[test]
    fn megapixel_memory_preflight_covers_queue_classes() {
        for (width, height) in [(6000, 4000), (8192, 5492), (9500, 6316), (12_250, 8164)] {
            let bytes = memory_preflight(width, height).expect("supported memory class");
            assert_eq!(bytes, u64::from(width) * u64::from(height) * 32);
        }
        assert!(matches!(
            memory_preflight(u32::MAX, u32::MAX),
            Err(ExportError::OutOfMemory(_))
        ));
    }
    #[test]
    fn deterministic_recipe_and_atomic_cancel_cleanup() {
        let root = root("identity");
        let value = request(&root, 1, ExportFormat::Jpeg);
        assert_eq!(
            export_recipe_identity(&value).unwrap(),
            export_recipe_identity(&value).unwrap()
        );
        let result = export_one(
            &MockRenderer,
            &value,
            &RenderSettings::default(),
            &AtomicBool::new(true),
        );
        assert!(matches!(result, Err(ExportError::Cancelled)));
        assert!(
            root.join("out")
                .read_dir()
                .map(|mut value| value.next().is_none())
                .unwrap_or(true)
        );
    }
}
