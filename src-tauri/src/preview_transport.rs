//! Presentation transport only: all color processing remains in the shared Native graph.
//! Legacy/final JPEG is SRP3. Explicit bounded interactive RGBA8 is SRP4/format 1.
use serde::Deserialize;

pub const INTERACTIVE_MAX_EDGE: u32 = 1024;

#[derive(Debug, Default, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PreviewPayloadFormat {
    #[default]
    Jpeg,
    Rgba8,
}

impl PreviewPayloadFormat {
    pub fn validate(self, interactive: bool) -> Result<(), String> {
        if self == Self::Rgba8 && !interactive {
            return Err(
                "PreviewTransportInvalid: RGBA8 requires bounded interactive preview".into(),
            );
        }
        Ok(())
    }

    pub fn cache_identity(self, interactive: bool) -> &'static str {
        match (self, interactive) {
            (Self::Jpeg, true) => "srp3-jpeg88",
            (Self::Jpeg, false) => "srp3-jpeg96",
            (Self::Rgba8, _) => "srp4-rgba8-srgb",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewFrameHeader {
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub tile_x: u32,
    pub tile_y: u32,
    pub flags: u16,
}

pub fn preview_frame(
    header: PreviewFrameHeader,
    profile_id: &str,
    payload: Vec<u8>,
) -> Result<Vec<u8>, String> {
    frame(header, profile_id, &payload, PreviewPayloadFormat::Jpeg)
}

pub fn rgba_preview_frame(
    header: PreviewFrameHeader,
    profile_id: &str,
    rgb: &[u8],
) -> Result<Vec<u8>, String> {
    let samples = (header.width as usize)
        .checked_mul(header.height as usize)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or("PreviewTransportInvalid: RGB dimensions overflow")?;
    if header.width == 0
        || header.height == 0
        || header.width > INTERACTIVE_MAX_EDGE
        || header.height > INTERACTIVE_MAX_EDGE
        || rgb.len() != samples
    {
        return Err("PreviewTransportInvalid: RGBA8 dimensions or RGB sample count invalid".into());
    }
    // Reserve the final frame once. No intermediate RGBA allocation or color conversion.
    let mut bytes = frame_prefix(
        header,
        profile_id,
        samples / 3 * 4,
        PreviewPayloadFormat::Rgba8,
    )?;
    for pixel in rgb.chunks_exact(3) {
        bytes.extend_from_slice(pixel);
        bytes.push(255);
    }
    Ok(bytes)
}

fn frame(
    header: PreviewFrameHeader,
    profile_id: &str,
    payload: &[u8],
    format: PreviewPayloadFormat,
) -> Result<Vec<u8>, String> {
    let mut bytes = frame_prefix(header, profile_id, payload.len(), format)?;
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

fn frame_prefix(
    header: PreviewFrameHeader,
    profile_id: &str,
    payload_length: usize,
    format: PreviewPayloadFormat,
) -> Result<Vec<u8>, String> {
    let profile_len = u16::try_from(profile_id.len()).map_err(|_| "profile ID is too long")?;
    let payload_len = u32::try_from(payload_length).map_err(|_| "native preview is too large")?;
    let length = 40_usize
        .checked_add(profile_id.len())
        .and_then(|length| length.checked_add(payload_length))
        .ok_or("PreviewTransportInvalid: frame length overflow")?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length).map_err(|_| {
        "PreviewTransportAllocation: insufficient memory for binary preview frame".to_owned()
    })?;
    let (magic, version, encoding): (&[u8; 4], u16, u16) = match format {
        PreviewPayloadFormat::Jpeg => (b"SRP3", 3, 0),
        PreviewPayloadFormat::Rgba8 => (b"SRP4", 4, 1),
    };
    bytes.extend_from_slice(magic);
    bytes.extend_from_slice(&version.to_le_bytes());
    bytes.extend_from_slice(&header.flags.to_le_bytes());
    for value in [
        header.width,
        header.height,
        header.source_width,
        header.source_height,
        header.tile_x,
        header.tile_y,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&profile_len.to_le_bytes());
    bytes.extend_from_slice(&encoding.to_le_bytes());
    bytes.extend_from_slice(&payload_len.to_le_bytes());
    bytes.extend_from_slice(profile_id.as_bytes());
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(width: u32, height: u32) -> PreviewFrameHeader {
        PreviewFrameHeader {
            width,
            height,
            source_width: width,
            source_height: height,
            tile_x: 0,
            tile_y: 0,
            flags: 8,
        }
    }

    #[test]
    fn rgba_transport_preserves_every_rgb_byte_without_lossy_codec() {
        let rgb: Vec<u8> = (0..=255)
            .flat_map(|value| [value, 255 - value, value / 2])
            .collect();
        let bytes = rgba_preview_frame(header(256, 1), "profile", &rgb).unwrap();
        assert_eq!(&bytes[..6], b"SRP4\x04\0");
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 1);
        assert_eq!(bytes.len(), 40 + 7 + 256 * 4);
        for (expected, actual) in rgb.chunks_exact(3).zip(bytes[47..].chunks_exact(4)) {
            assert_eq!(expected, &actual[..3]);
            assert_eq!(actual[3], 255);
        }
    }

    #[test]
    fn rgba_transport_is_bounded_and_never_silently_downgrades() {
        for (width, height, rgb) in [
            (0, 1, vec![]),
            (1, 1, vec![0, 1]),
            (1025, 1, vec![0; 1025 * 3]),
            (1, u32::MAX, vec![]),
        ] {
            assert!(
                rgba_preview_frame(header(width, height), "", &rgb)
                    .unwrap_err()
                    .starts_with("PreviewTransportInvalid:")
            );
        }
        assert!(PreviewPayloadFormat::Rgba8.validate(false).is_err());
        assert!(PreviewPayloadFormat::Rgba8.validate(true).is_ok());
        let keys = [
            PreviewPayloadFormat::Jpeg.cache_identity(true),
            PreviewPayloadFormat::Jpeg.cache_identity(false),
            PreviewPayloadFormat::Rgba8.cache_identity(true),
        ];
        assert_ne!(keys[0], keys[1]);
        assert_ne!(keys[1], keys[2]);
        assert_ne!(keys[0], keys[2]);
    }
}
