use serde::Deserialize;
use sha2::{Digest, Sha256};
use starroom_raw::{
    CameraProfileSource, CameraProfileStatus, LIBRAW_PINNED_VERSION, RawDevelopSource, RawFormat,
    SensorLayout, decode_raw,
};
use std::{fs, path::PathBuf};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    fixtures: Vec<Fixture>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    id: String,
    path: String,
    camera_make: String,
    camera_model: String,
    sensor_layout: String,
    sha256: String,
    byte_length: u64,
}

fn raw_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/raw")
}

fn manifest() -> Manifest {
    let bytes = fs::read(raw_root().join("manifest.json")).expect("RAW manifest");
    serde_json::from_slice(&bytes).expect("valid RAW manifest")
}

#[test]
fn all_public_raw_fixtures_decode_from_sensor_through_libraw() {
    for fixture in manifest().fixtures {
        let path = raw_root().join(&fixture.path);
        let before = fs::read(&path).expect("fixture bytes");
        assert_eq!(before.len() as u64, fixture.byte_length, "{}", fixture.id);
        assert_eq!(
            format!("{:x}", Sha256::digest(&before)),
            fixture.sha256,
            "{}",
            fixture.id
        );

        let decoded = decode_raw(&path).unwrap_or_else(|error| panic!("{}: {error}", fixture.id));
        assert_eq!(decoded.metadata.develop_source, RawDevelopSource::Sensor);
        assert!(
            decoded.metadata.libraw_version == LIBRAW_PINNED_VERSION
                || decoded.metadata.libraw_version == format!("{LIBRAW_PINNED_VERSION}-Release"),
            "{}: unexpected LibRaw binary version {}",
            fixture.id,
            decoded.metadata.libraw_version
        );
        assert!(decoded.width > 0 && decoded.height > 0, "{}", fixture.id);
        assert_eq!(
            decoded.rgb.len(),
            decoded.width as usize * decoded.height as usize * 3,
            "{}",
            fixture.id
        );
        assert!(
            decoded.rgb.iter().all(|sample| sample.is_finite()),
            "{}",
            fixture.id
        );
        assert!(
            decoded.rgb.iter().any(|sample| *sample > 0.0),
            "{}",
            fixture.id
        );
        assert!(
            decoded.metadata.raw_width >= decoded.metadata.active_width,
            "{}",
            fixture.id
        );
        assert!(
            decoded.metadata.raw_height >= decoded.metadata.active_height,
            "{}",
            fixture.id
        );
        assert!(
            decoded.metadata.white_level > decoded.metadata.black_level,
            "{}",
            fixture.id
        );
        assert!(
            decoded.metadata.as_shot_multipliers[..3]
                .iter()
                .all(|value| value.is_finite() && *value > 0.0),
            "{}",
            fixture.id
        );
        assert!(
            decoded.metadata.camera_neutral[..3]
                .iter()
                .all(|value| value.is_finite() && *value > 0.0),
            "{}",
            fixture.id
        );
        assert_eq!(
            decoded.metadata.camera_profile.hash.len(),
            64,
            "{}",
            fixture.id
        );
        assert!(
            decoded
                .metadata
                .camera_profile
                .camera_to_xyz_d65
                .iter()
                .flatten()
                .all(|value| value.is_finite()),
            "{}",
            fixture.id
        );
        if decoded.metadata.format == RawFormat::Dng {
            match decoded.metadata.camera_profile.status {
                CameraProfileStatus::Resolved => {
                    assert!(
                        matches!(
                            decoded.metadata.camera_profile.source,
                            CameraProfileSource::DngForwardMatrix
                                | CameraProfileSource::DngColorMatrix
                                | CameraProfileSource::DngForwardAndColorMatrix
                        ),
                        "resolved DNG must use its embedded matrix: {}",
                        fixture.id
                    );
                    assert!(
                        !decoded
                            .metadata
                            .camera_profile
                            .calibration_illuminants
                            .is_empty(),
                        "resolved DNG must expose CalibrationIlluminant: {}",
                        fixture.id
                    );
                }
                CameraProfileStatus::Generic => assert_eq!(
                    decoded.metadata.camera_profile.source,
                    CameraProfileSource::GenericLinearSrgb,
                    "DNG without a valid embedded profile must be explicitly generic: {}",
                    fixture.id
                ),
            }
        } else {
            assert_eq!(
                decoded.metadata.camera_profile.status,
                CameraProfileStatus::Resolved,
                "{} must resolve LibRaw's identified camera profile",
                fixture.id
            );
        }
        assert!(
            decoded.metadata.make.to_ascii_lowercase().contains(
                fixture
                    .camera_make
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .as_str()
            ),
            "{}: {}",
            fixture.id,
            decoded.metadata.make
        );
        assert!(
            !decoded.metadata.model.is_empty(),
            "{} expected {}",
            fixture.id,
            fixture.camera_model
        );
        match fixture.sensor_layout.as_str() {
            "bayer" => assert_eq!(decoded.metadata.sensor_layout, SensorLayout::Bayer),
            "xTrans" => {
                assert_eq!(decoded.metadata.sensor_layout, SensorLayout::XTrans);
                assert!(decoded.metadata.xtrans.is_some());
            }
            other => panic!("unexpected fixture layout {other}"),
        }
        assert!(decoded.timings.total_decode_milliseconds.is_finite());
        assert!(
            decoded.timings.total_decode_milliseconds < 120_000.0,
            "{}",
            fixture.id
        );
        eprintln!(
            "RAW_METRIC id={} unpack_ms={:.2} process_ms={:.2} total_ms={:.2}",
            fixture.id,
            decoded.timings.sensor_unpack_milliseconds,
            decoded.timings.demosaic_process_milliseconds,
            decoded.timings.total_decode_milliseconds
        );

        let after = fs::read(&path).expect("fixture bytes after decode");
        assert_eq!(before, after, "RAW source was modified: {}", fixture.id);
    }
}

#[test]
fn unsupported_or_rendered_extensions_are_typed_errors() {
    let result = starroom_raw::RawFormat::from_path("photo.jpg");
    assert!(matches!(
        result,
        Err(starroom_raw::RawDecodeError::UnsupportedExtension { .. })
    ));
}
