//! Native rendered-image I/O for Starroom v0.2.
//! RAW remains a separate pipeline. This crate decodes common rendered formats, preserves their
//! encoded sample values, and exposes embedded metadata so color management can happen explicitly.

use image::{
    DynamicImage, ExtendedColorType, ImageBuffer, ImageDecoder, ImageEncoder, ImageFormat,
    ImageReader, Rgb,
};
use serde::{Deserialize, Serialize};
use starroom_raw::{DecodedRawImage, RawDecodeError, RawFormat, decode_raw, decode_raw_preview};
use std::{io::Cursor, path::Path};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImageIoError {
    #[error("image file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("image codec failed: {0}")]
    Codec(#[from] image::ImageError),
    #[error("image format could not be detected")]
    UnknownFormat,
    #[error("RGB buffer length does not match dimensions")]
    InvalidBufferLength,
    #[error("TIFF metadata encoder failed: {0}")]
    TiffMetadata(String),
    #[error("container metadata encoder failed: {0}")]
    ContainerMetadata(String),
    #[error(transparent)]
    Raw(#[from] RawDecodeError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DecodedSourceImage {
    Rendered(DecodedRenderedImage),
    Raw(Box<DecodedRawImage>),
}

impl DecodedSourceImage {
    pub fn width(&self) -> u32 {
        match self {
            Self::Rendered(image) => image.width,
            Self::Raw(image) => image.width,
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            Self::Rendered(image) => image.height,
            Self::Raw(image) => image.height,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RenderedFormat {
    Jpeg,
    Png,
    Tiff,
}

impl TryFrom<ImageFormat> for RenderedFormat {
    type Error = ImageIoError;

    fn try_from(value: ImageFormat) -> Result<Self, Self::Error> {
        match value {
            ImageFormat::Jpeg => Ok(Self::Jpeg),
            ImageFormat::Png => Ok(Self::Png),
            ImageFormat::Tiff => Ok(Self::Tiff),
            _ => Err(ImageIoError::UnknownFormat),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecodedRenderedImage {
    pub width: u32,
    pub height: u32,
    pub format: RenderedFormat,
    /// Encoded RGB(A) samples normalized to 0..1. These are not yet converted to Starroom's
    /// linear Rec.2020 working space; the color-management/input-transform stage owns that step.
    pub rgba: Vec<f32>,
    pub embedded_icc: Option<Vec<u8>>,
    pub exif: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LensMetadata {
    pub camera_make: String,
    pub camera_model: String,
    pub lens_make: String,
    pub lens_model: String,
    pub focal_length_mm: Option<f32>,
    pub aperture: Option<f32>,
    pub focus_distance_m: Option<f32>,
}

fn exif_text(exif: &exif::Exif, tag: exif::Tag) -> String {
    exif.get_field(tag, exif::In::PRIMARY)
        .and_then(|field| match &field.value {
            exif::Value::Ascii(values) => values
                .first()
                .map(|value| String::from_utf8_lossy(value).trim().to_owned()),
            _ => None,
        })
        .unwrap_or_default()
}

fn exif_number(exif: &exif::Exif, tag: exif::Tag) -> Option<f32> {
    exif.get_field(tag, exif::In::PRIMARY)
        .and_then(|field| match &field.value {
            exif::Value::Rational(values) => values.first().map(|value| value.to_f64() as f32),
            exif::Value::SRational(values) => values.first().map(|value| value.to_f64() as f32),
            exif::Value::Float(values) => values.first().copied(),
            exif::Value::Double(values) => values.first().map(|value| *value as f32),
            _ => field.value.get_uint(0).map(|value| value as f32),
        })
        .filter(|value| value.is_finite() && *value > 0.0)
}

/// Extracts only Lensfun matching metadata from encoded EXIF. Missing fields remain explicit;
/// callers must not invent a camera/lens profile.
pub fn lens_metadata(image: &DecodedRenderedImage) -> LensMetadata {
    let Some(bytes) = image.exif.clone() else {
        return LensMetadata::default();
    };
    let Ok(exif) = exif::Reader::new().read_raw(bytes) else {
        return LensMetadata::default();
    };
    LensMetadata {
        camera_make: exif_text(&exif, exif::Tag::Make),
        camera_model: exif_text(&exif, exif::Tag::Model),
        lens_make: exif_text(&exif, exif::Tag::LensMake),
        lens_model: exif_text(&exif, exif::Tag::LensModel),
        focal_length_mm: exif_number(&exif, exif::Tag::FocalLength),
        aperture: exif_number(&exif, exif::Tag::FNumber),
        focus_distance_m: exif_number(&exif, exif::Tag::SubjectDistance),
    }
}

fn dynamic_to_rgba_f32(image: DynamicImage) -> Vec<f32> {
    image.into_rgba32f().into_raw()
}

fn decode_rendered_inner(
    path: impl AsRef<Path>,
    max_edge: Option<u32>,
) -> Result<DecodedRenderedImage, ImageIoError> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let format = reader.format().ok_or(ImageIoError::UnknownFormat)?;
    let rendered_format = RenderedFormat::try_from(format)?;
    let mut decoder = reader.into_decoder()?;
    let (width, height) = decoder.dimensions();
    let embedded_icc = decoder.icc_profile()?;
    let exif = decoder.exif_metadata()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    if let Some(max_edge) = max_edge.filter(|edge| *edge > 0)
        && (width > max_edge || height > max_edge)
    {
        // Lanczos3 is a mature, deterministic image-crate resampler. Resizing is performed
        // before the shared color/tone graph only for interactive preview; export always
        // decodes the full source independently.
        image = image.resize(max_edge, max_edge, image::imageops::FilterType::Lanczos3);
    }
    let (decoded_width, decoded_height) = (image.width(), image.height());
    Ok(DecodedRenderedImage {
        width: decoded_width,
        height: decoded_height,
        format: rendered_format,
        rgba: dynamic_to_rgba_f32(image),
        embedded_icc,
        exif,
    })
}

pub fn decode_rendered(path: impl AsRef<Path>) -> Result<DecodedRenderedImage, ImageIoError> {
    decode_rendered_inner(path, None)
}

/// Decodes a bounded interactive-preview source while retaining metadata/profile ownership.
/// Full-resolution export must call [`decode_rendered`] instead.
pub fn decode_rendered_preview(
    path: impl AsRef<Path>,
    max_edge: u32,
) -> Result<DecodedRenderedImage, ImageIoError> {
    decode_rendered_inner(path, Some(max_edge))
}

fn raw_format(path: &Path) -> bool {
    RawFormat::from_path(path).is_ok()
}

fn resize_raw_preview(mut image: DecodedRawImage, max_edge: u32) -> DecodedRawImage {
    let max_edge = max_edge.max(1);
    if image.width <= max_edge && image.height <= max_edge {
        return image;
    }
    let scale = max_edge as f64 / f64::from(image.width.max(image.height));
    let width = (f64::from(image.width) * scale).round().max(1.0) as u32;
    let height = (f64::from(image.height) * scale).round().max(1.0) as u32;
    let source = ImageBuffer::<Rgb<f32>, Vec<f32>>::from_raw(image.width, image.height, image.rgb)
        .expect("validated LibRaw RGB buffer");
    let resized = image::imageops::resize(
        &source,
        width,
        height,
        image::imageops::FilterType::Lanczos3,
    );
    image.width = width;
    image.height = height;
    image.rgb = resized.into_raw();
    image
}

pub fn decode_source(path: impl AsRef<Path>) -> Result<DecodedSourceImage, ImageIoError> {
    let path = path.as_ref();
    if raw_format(path) {
        return Ok(DecodedSourceImage::Raw(Box::new(decode_raw(path)?)));
    }
    Ok(DecodedSourceImage::Rendered(decode_rendered(path)?))
}

pub fn decode_source_preview(
    path: impl AsRef<Path>,
    max_edge: u32,
) -> Result<DecodedSourceImage, ImageIoError> {
    let path = path.as_ref();
    if raw_format(path) {
        return Ok(DecodedSourceImage::Raw(Box::new(resize_raw_preview(
            decode_raw_preview(path)?,
            max_edge,
        ))));
    }
    Ok(DecodedSourceImage::Rendered(decode_rendered_preview(
        path, max_edge,
    )?))
}

/// Encodes an already output-transformed RGB8 buffer. The caller owns gamut mapping and output
/// ICC conversion; this function only performs JPEG compression.
pub fn encode_jpeg_rgb8(
    rgb: &[u8],
    width: u32,
    height: u32,
    quality: u8,
    icc_profile: Option<Vec<u8>>,
) -> Result<Vec<u8>, ImageIoError> {
    encode_jpeg_rgb8_with_metadata(rgb, width, height, quality, icc_profile, None, None)
}

pub fn encode_jpeg_rgb8_with_metadata(
    rgb: &[u8],
    width: u32,
    height: u32,
    quality: u8,
    icc_profile: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
    xmp: Option<Vec<u8>>,
) -> Result<Vec<u8>, ImageIoError> {
    let expected = width as usize * height as usize * 3;
    if rgb.len() != expected {
        return Err(ImageIoError::InvalidBufferLength);
    }
    let mut cursor = Cursor::new(Vec::new());
    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, quality.clamp(1, 100));
    if let Some(profile) = icc_profile {
        encoder
            .set_icc_profile(profile)
            .map_err(image::ImageError::Unsupported)?;
    }
    if let Some(exif) = exif {
        encoder
            .set_exif_metadata(exif)
            .map_err(image::ImageError::Unsupported)?;
    }
    encoder.write_image(rgb, width, height, ExtendedColorType::Rgb8)?;
    let mut encoded = cursor.into_inner();
    if let Some(xmp) = xmp {
        insert_jpeg_xmp(&mut encoded, &xmp)?;
    }
    Ok(encoded)
}

pub fn encode_png_rgb8(
    rgb: &[u8],
    width: u32,
    height: u32,
    icc_profile: Option<Vec<u8>>,
) -> Result<Vec<u8>, ImageIoError> {
    encode_png_rgb8_with_metadata(rgb, width, height, icc_profile, None, None)
}

pub fn encode_png_rgb8_with_metadata(
    rgb: &[u8],
    width: u32,
    height: u32,
    icc_profile: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
    xmp: Option<Vec<u8>>,
) -> Result<Vec<u8>, ImageIoError> {
    validate_rgb8(rgb, width, height)?;
    let mut cursor = Cursor::new(Vec::new());
    let mut encoder = image::codecs::png::PngEncoder::new(&mut cursor);
    if let Some(profile) = icc_profile {
        encoder
            .set_icc_profile(profile)
            .map_err(image::ImageError::Unsupported)?;
    }
    if let Some(exif) = exif {
        encoder
            .set_exif_metadata(exif)
            .map_err(image::ImageError::Unsupported)?;
    }
    encoder.write_image(rgb, width, height, ExtendedColorType::Rgb8)?;
    let mut encoded = cursor.into_inner();
    if let Some(xmp) = xmp {
        insert_png_xmp(&mut encoded, &xmp)?;
    }
    Ok(encoded)
}

pub fn encode_png_rgb16_with_metadata(
    rgb: &[u16],
    width: u32,
    height: u32,
    icc_profile: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
    xmp: Option<Vec<u8>>,
) -> Result<Vec<u8>, ImageIoError> {
    validate_rgb16(rgb, width, height)?;
    let native_bytes: Vec<u8> = rgb.iter().flat_map(|sample| sample.to_ne_bytes()).collect();
    let mut cursor = Cursor::new(Vec::new());
    let mut encoder = image::codecs::png::PngEncoder::new(&mut cursor);
    if let Some(profile) = icc_profile {
        encoder
            .set_icc_profile(profile)
            .map_err(image::ImageError::Unsupported)?;
    }
    if let Some(exif) = exif {
        encoder
            .set_exif_metadata(exif)
            .map_err(image::ImageError::Unsupported)?;
    }
    encoder.write_image(&native_bytes, width, height, ExtendedColorType::Rgb16)?;
    let mut encoded = cursor.into_inner();
    if let Some(xmp) = xmp {
        insert_png_xmp(&mut encoded, &xmp)?;
    }
    Ok(encoded)
}

pub fn encode_tiff_rgb8(
    rgb: &[u8],
    width: u32,
    height: u32,
    icc_profile: Option<Vec<u8>>,
) -> Result<Vec<u8>, ImageIoError> {
    encode_tiff_rgb8_with_metadata(rgb, width, height, icc_profile, None, None)
}

pub fn encode_tiff_rgb8_with_metadata(
    rgb: &[u8],
    width: u32,
    height: u32,
    icc_profile: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
    xmp: Option<Vec<u8>>,
) -> Result<Vec<u8>, ImageIoError> {
    validate_rgb8(rgb, width, height)?;
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut encoder = tiff::encoder::TiffEncoder::new(&mut cursor)
            .map_err(|error| ImageIoError::TiffMetadata(error.to_string()))?;
        let mut image = encoder
            .new_image::<tiff::encoder::colortype::RGB8>(width, height)
            .map_err(|error| ImageIoError::TiffMetadata(error.to_string()))?;
        if let Some(profile) = icc_profile {
            image
                .encoder()
                .write_tag(tiff::tags::Tag::IccProfile, profile.as_slice())
                .map_err(|error| ImageIoError::TiffMetadata(error.to_string()))?;
        }
        if let Some(xmp) = xmp {
            image
                .encoder()
                .write_tag(tiff::tags::Tag::Unknown(700), xmp.as_slice())
                .map_err(|error| ImageIoError::TiffMetadata(error.to_string()))?;
        }
        if let Some(exif) = exif
            && let Ok(parsed) = exif::Reader::new().read_raw(exif)
        {
            for (source, destination) in [
                (exif::Tag::Model, tiff::tags::Tag::Model),
                (exif::Tag::DateTime, tiff::tags::Tag::DateTime),
                (exif::Tag::Copyright, tiff::tags::Tag::Copyright),
            ] {
                let value = exif_text(&parsed, source);
                if !value.is_empty() {
                    image
                        .encoder()
                        .write_tag(destination, value.as_str())
                        .map_err(|error| ImageIoError::TiffMetadata(error.to_string()))?;
                }
            }
        }
        image
            .write_data(rgb)
            .map_err(|error| ImageIoError::TiffMetadata(error.to_string()))?;
    }
    Ok(cursor.into_inner())
}

pub fn encode_tiff_rgb16_with_metadata(
    rgb: &[u16],
    width: u32,
    height: u32,
    icc_profile: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
    xmp: Option<Vec<u8>>,
) -> Result<Vec<u8>, ImageIoError> {
    validate_rgb16(rgb, width, height)?;
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut encoder = tiff::encoder::TiffEncoder::new(&mut cursor)
            .map_err(|error| ImageIoError::TiffMetadata(error.to_string()))?;
        let mut image = encoder
            .new_image::<tiff::encoder::colortype::RGB16>(width, height)
            .map_err(|error| ImageIoError::TiffMetadata(error.to_string()))?;
        if let Some(profile) = icc_profile {
            image
                .encoder()
                .write_tag(tiff::tags::Tag::IccProfile, profile.as_slice())
                .map_err(|error| ImageIoError::TiffMetadata(error.to_string()))?;
        }
        if let Some(xmp) = xmp {
            image
                .encoder()
                .write_tag(tiff::tags::Tag::Unknown(700), xmp.as_slice())
                .map_err(|error| ImageIoError::TiffMetadata(error.to_string()))?;
        }
        if let Some(exif) = exif
            && let Ok(parsed) = exif::Reader::new().read_raw(exif)
        {
            for (source, destination) in [
                (exif::Tag::Model, tiff::tags::Tag::Model),
                (exif::Tag::DateTime, tiff::tags::Tag::DateTime),
                (exif::Tag::Copyright, tiff::tags::Tag::Copyright),
            ] {
                let value = exif_text(&parsed, source);
                if !value.is_empty() {
                    image
                        .encoder()
                        .write_tag(destination, value.as_str())
                        .map_err(|error| ImageIoError::TiffMetadata(error.to_string()))?;
                }
            }
        }
        image
            .write_data(rgb)
            .map_err(|error| ImageIoError::TiffMetadata(error.to_string()))?;
    }
    Ok(cursor.into_inner())
}

fn validate_rgb8(rgb: &[u8], width: u32, height: u32) -> Result<(), ImageIoError> {
    let expected = width as usize * height as usize * 3;
    if rgb.len() == expected {
        Ok(())
    } else {
        Err(ImageIoError::InvalidBufferLength)
    }
}

fn validate_rgb16(rgb: &[u16], width: u32, height: u32) -> Result<(), ImageIoError> {
    let expected = width as usize * height as usize * 3;
    if rgb.len() == expected {
        Ok(())
    } else {
        Err(ImageIoError::InvalidBufferLength)
    }
}

fn insert_jpeg_xmp(jpeg: &mut Vec<u8>, xmp: &[u8]) -> Result<(), ImageIoError> {
    const HEADER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
    if !jpeg.starts_with(&[0xff, 0xd8]) {
        return Err(ImageIoError::ContainerMetadata(
            "JPEG SOI marker is missing".into(),
        ));
    }
    let length = u16::try_from(HEADER.len() + xmp.len() + 2).map_err(|_| {
        ImageIoError::ContainerMetadata("JPEG XMP packet exceeds one APP1 segment".into())
    })?;
    let mut segment = Vec::with_capacity(length as usize + 2);
    segment.extend_from_slice(&[0xff, 0xe1]);
    segment.extend_from_slice(&length.to_be_bytes());
    segment.extend_from_slice(HEADER);
    segment.extend_from_slice(xmp);
    jpeg.splice(2..2, segment);
    Ok(())
}

fn insert_png_xmp(png: &mut Vec<u8>, xmp: &[u8]) -> Result<(), ImageIoError> {
    const IEND: &[u8] = &[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82];
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") || !png.ends_with(IEND) {
        return Err(ImageIoError::ContainerMetadata(
            "PNG signature or IEND chunk is invalid".into(),
        ));
    }
    let mut data = b"XML:com.adobe.xmp\0\0\0\0\0".to_vec();
    data.extend_from_slice(xmp);
    let length = u32::try_from(data.len())
        .map_err(|_| ImageIoError::ContainerMetadata("PNG XMP packet is too large".into()))?;
    let mut chunk = Vec::with_capacity(data.len() + 12);
    chunk.extend_from_slice(&length.to_be_bytes());
    chunk.extend_from_slice(b"iTXt");
    chunk.extend_from_slice(&data);
    let mut crc = crc32fast::Hasher::new();
    crc.update(b"iTXt");
    crc.update(&data);
    chunk.extend_from_slice(&crc.finalize().to_be_bytes());
    let insertion = png.len() - IEND.len();
    png.splice(insertion..insertion, chunk);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_encoder_rejects_wrong_buffer_length() {
        let result = encode_jpeg_rgb8(&[0, 0, 0], 2, 2, 90, None);
        assert!(matches!(result, Err(ImageIoError::InvalidBufferLength)));
    }

    #[test]
    fn jpeg_round_trip_decodes_dimensions() {
        let rgb = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
        let bytes = encode_jpeg_rgb8(&rgb, 2, 2, 95, None).expect("encode");
        let reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .expect("guess format");
        let decoded = reader.decode().expect("decode");
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
    }

    #[test]
    fn png16_round_trip_preserves_real_sixteen_bit_samples() {
        let rgb: Vec<u16> = (0..512_u16)
            .flat_map(|value| [value * 127, value * 127, value * 127])
            .collect();
        let bytes = encode_png_rgb16_with_metadata(
            &rgb,
            512,
            1,
            None,
            None,
            Some(b"<x:xmpmeta>M27</x:xmpmeta>".to_vec()),
        )
        .expect("PNG16 encode");
        assert!(bytes.windows(3).any(|window| window == b"M27"));
        let decoded =
            image::load_from_memory_with_format(&bytes, ImageFormat::Png).expect("PNG16 decode");
        assert_eq!(decoded.color(), image::ColorType::Rgb16);
        let values = decoded.into_rgb16().into_raw();
        assert!(values.windows(2).filter(|pair| pair[0] != pair[1]).count() > 256);
        assert_eq!(values, rgb);
    }

    #[test]
    fn tiff16_round_trip_preserves_real_sixteen_bit_samples_and_xmp() {
        let rgb: Vec<u16> = (0..512_u16)
            .flat_map(|value| [value * 127, value * 127, value * 127])
            .collect();
        let bytes = encode_tiff_rgb16_with_metadata(
            &rgb,
            512,
            1,
            None,
            None,
            Some(b"<x:xmpmeta>M27</x:xmpmeta>".to_vec()),
        )
        .expect("TIFF16 encode");
        assert!(bytes.windows(3).any(|window| window == b"M27"));
        let decoded =
            image::load_from_memory_with_format(&bytes, ImageFormat::Tiff).expect("TIFF16 decode");
        assert_eq!(decoded.color(), image::ColorType::Rgb16);
        assert_eq!(decoded.into_rgb16().into_raw(), rgb);
    }

    #[test]
    fn jpeg_xmp_is_an_explicit_app1_packet() {
        let bytes = encode_jpeg_rgb8_with_metadata(
            &[128, 128, 128],
            1,
            1,
            90,
            None,
            None,
            Some(b"<x:xmpmeta>M27</x:xmpmeta>".to_vec()),
        )
        .expect("JPEG XMP encode");
        assert_eq!(&bytes[..4], &[0xff, 0xd8, 0xff, 0xe1]);
        assert!(bytes.windows(3).any(|window| window == b"M27"));
    }
}
