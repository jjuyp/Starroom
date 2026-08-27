//! Production LibRaw adapter for Starroom's RAW sensor boundary.
//!
//! LibRaw 0.22.2 is vendored under `vendor/libraw-0.22.2` and selected under the
//! CDDL-1.0 license path. The bridge always calls sensor `unpack()` followed by
//! LibRaw's demosaic path; embedded thumbnails are never used for Develop.

mod profile;

pub use profile::{
    CAMERA_PROFILE_RESOLVER_VERSION, CalibrationIlluminant, CameraFamily, CameraProfileDescriptor,
    CameraProfileInput, CameraProfileResolver, CameraProfileSource, CameraProfileStatus,
    DngMatrixSet,
};

use serde::{Deserialize, Serialize};
use starroom_color_management::{Xyz, xyz_d65_to_rec2020_linear};
use std::{ffi::CStr, fs, os::raw::c_char, path::Path, slice};
use thiserror::Error;

pub const LIBRAW_PINNED_VERSION: &str = "0.22.2";
pub const LIBRAW_TAG_OBJECT: &str = "24fa7e5463cbf8b8615dbd2b16c933a294d52400";
pub const LIBRAW_COMMIT: &str = "b93f6e45c194f5df9b02a43b1af9a54b4f41f33f";

#[derive(Debug, Error)]
pub enum RawDecodeError {
    #[error("could not read RAW source: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported RAW input: {detail} (LibRaw {code})")]
    Unsupported { code: i32, detail: String },
    #[error("RAW sensor data is corrupt or invalid: {detail} (LibRaw {code})")]
    InvalidData { code: i32, detail: String },
    #[error("RAW decode failed: {detail} (LibRaw {code})")]
    Decode { code: i32, detail: String },
    #[error("RAW decoder returned an invalid RGB buffer")]
    InvalidRgbBuffer,
    #[error("RAW decoder returned a non-finite sample")]
    NonFiniteSample,
    #[error("camera profile transform returned a non-finite sample")]
    NonFiniteCameraTransform,
    #[error("RAW format is not enabled for Develop: {extension}")]
    UnsupportedExtension { extension: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RawFormat {
    Nef,
    Arw,
    Cr2,
    Cr3,
    Dng,
    Raf,
}

impl RawFormat {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, RawDecodeError> {
        let extension = path
            .as_ref()
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "nef" => Ok(Self::Nef),
            "arw" => Ok(Self::Arw),
            "cr2" => Ok(Self::Cr2),
            "cr3" => Ok(Self::Cr3),
            "dng" => Ok(Self::Dng),
            "raf" => Ok(Self::Raf),
            _ => Err(RawDecodeError::UnsupportedExtension { extension }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SensorLayout {
    Bayer,
    XTrans,
    LinearOrMultichannel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RawDevelopSource {
    Sensor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawMetadata {
    pub develop_source: RawDevelopSource,
    pub format: RawFormat,
    pub make: String,
    pub model: String,
    pub lens_make: String,
    pub lens_model: String,
    pub focal_length_mm: f32,
    pub aperture: f32,
    pub focus_distance_m: Option<f32>,
    pub decoder: String,
    pub raw_width: u32,
    pub raw_height: u32,
    pub active_width: u32,
    pub active_height: u32,
    pub left_margin: u32,
    pub top_margin: u32,
    pub orientation: i32,
    pub cfa_filters: u32,
    pub xtrans: Option<[[u8; 6]; 6]>,
    pub sensor_layout: SensorLayout,
    pub colors: u32,
    pub dng_version: u32,
    pub black_level: u32,
    pub channel_black_levels: [u32; 4],
    pub white_level: u32,
    pub as_shot_multipliers: [f32; 4],
    pub camera_neutral: [f32; 4],
    pub pre_multipliers: [f32; 4],
    pub dng_color: [DngMatrixSet; 2],
    pub camera_profile: CameraProfileDescriptor,
    pub output_space: String,
    pub demosaic_provider: String,
    pub libraw_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawDecodeTimings {
    pub file_read_milliseconds: f64,
    pub sensor_unpack_milliseconds: f64,
    pub demosaic_process_milliseconds: f64,
    pub total_decode_milliseconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecodedRawImage {
    pub width: u32,
    pub height: u32,
    /// Unbounded f32 linear Rec.2020/D65 samples produced by the explicit camera-profile stage.
    pub rgb: Vec<f32>,
    pub metadata: RawMetadata,
    pub timings: RawDecodeTimings,
    pub preview_half_size: bool,
}

#[repr(C)]
struct BridgeResult {
    width: u32,
    height: u32,
    raw_width: u32,
    raw_height: u32,
    active_width: u32,
    active_height: u32,
    left_margin: u32,
    top_margin: u32,
    orientation: i32,
    filters: u32,
    colors: u32,
    dng_version: u32,
    black: u32,
    maximum: u32,
    cblack: [u32; 4],
    camera_multipliers: [f32; 4],
    pre_multipliers: [f32; 4],
    cam_xyz: [f32; 12],
    dng_parsed_fields: [u32; 2],
    dng_illuminants: [u16; 2],
    dng_calibration: [f32; 32],
    dng_color_matrix: [f32; 24],
    dng_forward_matrix: [f32; 24],
    xtrans: [u8; 36],
    sensor_layout: u8,
    used_half_size: u8,
    reserved: u16,
    unpack_milliseconds: f64,
    process_milliseconds: f64,
    focal_length_mm: f32,
    aperture: f32,
    focus_distance_m: f32,
    make: [c_char; 64],
    model: [c_char; 64],
    lens_make: [c_char; 128],
    lens_model: [c_char; 128],
    decoder: [c_char; 128],
    rgb16: *mut u16,
    rgb16_length: usize,
}

impl Default for BridgeResult {
    fn default() -> Self {
        // The C bridge treats this as an output-only POD and initializes every field.
        unsafe { std::mem::zeroed() }
    }
}

unsafe extern "C" {
    fn sr_libraw_decode_buffer(
        bytes: *const u8,
        byte_length: usize,
        half_size: i32,
        result: *mut BridgeResult,
        error: *mut c_char,
        error_capacity: usize,
    ) -> i32;
    fn sr_libraw_free(buffer: *mut u16);
    fn sr_libraw_version() -> *const c_char;
}

struct OwnedBridgeBuffer(*mut u16);

impl Drop for OwnedBridgeBuffer {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { sr_libraw_free(self.0) };
        }
    }
}

fn bridge_text(value: &[c_char]) -> String {
    unsafe { CStr::from_ptr(value.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn camera_neutral(multipliers: [f32; 4]) -> [f32; 4] {
    let mut neutral = multipliers.map(|value| {
        if value.is_finite() && value > 0.0 {
            1.0 / value
        } else {
            0.0
        }
    });
    let green = neutral[1];
    if green > 0.0 {
        for value in &mut neutral {
            *value /= green;
        }
    }
    neutral
}

fn decode_error(code: i32, detail: String) -> RawDecodeError {
    match code {
        -2 | -3 => RawDecodeError::Unsupported { code, detail },
        -100008 | -100009 | -100011 => RawDecodeError::InvalidData { code, detail },
        _ => RawDecodeError::Decode { code, detail },
    }
}

fn bridge_dng_set(bridge: &BridgeResult, set: usize) -> DngMatrixSet {
    let mut result = DngMatrixSet {
        parsed_fields: bridge.dng_parsed_fields[set],
        illuminant: bridge.dng_illuminants[set],
        ..DngMatrixSet::default()
    };
    for row in 0..4 {
        for column in 0..4 {
            result.calibration[row][column] = bridge.dng_calibration[set * 16 + row * 4 + column];
        }
        for xyz in 0..3 {
            result.color_matrix[row][xyz] = bridge.dng_color_matrix[set * 12 + row * 3 + xyz];
        }
    }
    for xyz in 0..3 {
        for channel in 0..4 {
            result.forward_matrix[xyz][channel] =
                bridge.dng_forward_matrix[set * 12 + xyz * 4 + channel];
        }
    }
    result
}

fn bridge_cam_xyz(bridge: &BridgeResult) -> [[f32; 3]; 4] {
    let mut result = [[0.0; 3]; 4];
    for (channel, values) in result.iter_mut().enumerate() {
        for (xyz, value) in values.iter_mut().enumerate() {
            *value = bridge.cam_xyz[channel * 3 + xyz];
        }
    }
    result
}

fn decode_inner(
    path: impl AsRef<Path>,
    half_size: bool,
) -> Result<DecodedRawImage, RawDecodeError> {
    let path = path.as_ref();
    let format = RawFormat::from_path(path)?;
    let read_start = std::time::Instant::now();
    let bytes = fs::read(path)?;
    let read_elapsed = read_start.elapsed();
    let total_start = std::time::Instant::now();
    let mut bridge = BridgeResult::default();
    let mut error = [0_i8; 512];
    let code = unsafe {
        sr_libraw_decode_buffer(
            bytes.as_ptr(),
            bytes.len(),
            i32::from(half_size),
            &mut bridge,
            error.as_mut_ptr(),
            error.len(),
        )
    };
    if code != 0 {
        let detail = unsafe { CStr::from_ptr(error.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        return Err(decode_error(code, detail));
    }
    let owned = OwnedBridgeBuffer(bridge.rgb16);
    let expected = bridge.width as usize * bridge.height as usize * 3;
    if owned.0.is_null() || bridge.rgb16_length != expected {
        return Err(RawDecodeError::InvalidRgbBuffer);
    }
    let source = unsafe { slice::from_raw_parts(owned.0, bridge.rgb16_length) };

    let sensor_layout = match bridge.sensor_layout {
        1 => SensorLayout::Bayer,
        2 => SensorLayout::XTrans,
        _ => SensorLayout::LinearOrMultichannel,
    };
    let xtrans = (sensor_layout == SensorLayout::XTrans).then(|| {
        let mut pattern = [[0_u8; 6]; 6];
        for (index, value) in bridge.xtrans.into_iter().enumerate() {
            pattern[index / 6][index % 6] = value;
        }
        pattern
    });
    let libraw_version = unsafe { CStr::from_ptr(sr_libraw_version()) }
        .to_string_lossy()
        .into_owned();
    if !matches_pinned_libraw_version(&libraw_version) {
        return Err(RawDecodeError::Decode {
            code: -1,
            detail: format!(
                "binary LibRaw version {libraw_version} does not match pin {LIBRAW_PINNED_VERSION}"
            ),
        });
    }
    let make = bridge_text(&bridge.make);
    let model = bridge_text(&bridge.model);
    let neutral = camera_neutral(bridge.camera_multipliers);
    let dng_color = [bridge_dng_set(&bridge, 0), bridge_dng_set(&bridge, 1)];
    let camera_profile = CameraProfileResolver::resolve(&CameraProfileInput {
        make: make.clone(),
        model: model.clone(),
        dng_version: bridge.dng_version,
        libraw_cam_xyz: bridge_cam_xyz(&bridge),
        camera_neutral: neutral,
        dng: dng_color.clone(),
    });
    let mut rgb = Vec::with_capacity(source.len());
    for camera in source.as_chunks::<3>().0 {
        let xyz = camera_profile.camera_rgb_to_xyz_d65([
            f32::from(camera[0]) / 65_535.0,
            f32::from(camera[1]) / 65_535.0,
            f32::from(camera[2]) / 65_535.0,
        ]);
        let working = xyz_d65_to_rec2020_linear(Xyz {
            x: xyz[0],
            y: xyz[1],
            z: xyz[2],
        });
        if !working.r.is_finite() || !working.g.is_finite() || !working.b.is_finite() {
            return Err(RawDecodeError::NonFiniteCameraTransform);
        }
        rgb.extend_from_slice(&[working.r, working.g, working.b]);
    }
    let demosaic_provider = match sensor_layout {
        SensorLayout::Bayer => "LibRaw AHD".to_owned(),
        SensorLayout::XTrans => "LibRaw X-Trans 3-pass".to_owned(),
        SensorLayout::LinearOrMultichannel => "LibRaw multichannel".to_owned(),
    };
    let total_elapsed = total_start.elapsed();
    Ok(DecodedRawImage {
        width: bridge.width,
        height: bridge.height,
        rgb,
        metadata: RawMetadata {
            develop_source: RawDevelopSource::Sensor,
            format,
            make,
            model,
            lens_make: bridge_text(&bridge.lens_make),
            lens_model: bridge_text(&bridge.lens_model),
            focal_length_mm: bridge.focal_length_mm,
            aperture: bridge.aperture,
            focus_distance_m: (bridge.focus_distance_m.is_finite()
                && bridge.focus_distance_m > 0.0)
                .then_some(bridge.focus_distance_m),
            decoder: bridge_text(&bridge.decoder),
            raw_width: bridge.raw_width,
            raw_height: bridge.raw_height,
            active_width: bridge.active_width,
            active_height: bridge.active_height,
            left_margin: bridge.left_margin,
            top_margin: bridge.top_margin,
            orientation: bridge.orientation,
            cfa_filters: bridge.filters,
            xtrans,
            sensor_layout,
            colors: bridge.colors,
            dng_version: bridge.dng_version,
            black_level: bridge.black,
            channel_black_levels: bridge.cblack,
            white_level: bridge.maximum,
            as_shot_multipliers: bridge.camera_multipliers,
            camera_neutral: neutral,
            pre_multipliers: bridge.pre_multipliers,
            dng_color,
            camera_profile,
            output_space: "linear Rec.2020 D65".to_owned(),
            demosaic_provider,
            libraw_version,
        },
        timings: RawDecodeTimings {
            file_read_milliseconds: read_elapsed.as_secs_f64() * 1_000.0,
            sensor_unpack_milliseconds: bridge.unpack_milliseconds,
            demosaic_process_milliseconds: bridge.process_milliseconds,
            total_decode_milliseconds: total_elapsed.as_secs_f64() * 1_000.0,
        },
        preview_half_size: bridge.used_half_size != 0,
    })
}

pub fn decode_raw(path: impl AsRef<Path>) -> Result<DecodedRawImage, RawDecodeError> {
    decode_inner(path, false)
}

/// Preview uses LibRaw's half-size sensor path, never an embedded JPEG thumbnail.
pub fn decode_raw_preview(path: impl AsRef<Path>) -> Result<DecodedRawImage, RawDecodeError> {
    decode_inner(path, true)
}

fn matches_pinned_libraw_version(version: &str) -> bool {
    version == LIBRAW_PINNED_VERSION || version == format!("{LIBRAW_PINNED_VERSION}-Release")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_contract_is_explicit() {
        assert_eq!(RawFormat::from_path("photo.NEF").unwrap(), RawFormat::Nef);
        assert_eq!(RawFormat::from_path("photo.RAF").unwrap(), RawFormat::Raf);
        assert!(matches!(
            RawFormat::from_path("photo.jpg"),
            Err(RawDecodeError::UnsupportedExtension { .. })
        ));
    }

    #[test]
    fn camera_neutral_is_reciprocal_and_green_normalized() {
        let neutral = camera_neutral([2.0, 1.0, 0.5, 1.0]);
        assert_eq!(neutral, [0.5, 1.0, 2.0, 1.0]);
    }

    #[test]
    fn pinned_version_accepts_only_the_official_release_suffix() {
        assert!(matches_pinned_libraw_version("0.22.2"));
        assert!(matches_pinned_libraw_version("0.22.2-Release"));
        assert!(!matches_pinned_libraw_version("0.22.1-Release"));
        assert!(!matches_pinned_libraw_version("0.22.2-custom"));
    }
}
