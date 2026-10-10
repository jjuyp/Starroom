#[path = "support/controlled_dng.rs"]
mod fixture;

#[test]
fn actual_raw_region_copies_only_selected_samples_and_retains_profile_metadata() {
    use starroom_imageio::{DecodedSourceImage, ImageIoError};
    let fixture = fixture::controlled_fixture().unwrap();
    let source = DecodedSourceImage::Raw(Box::new(starroom_raw::decode_raw(&fixture.0).unwrap()));
    let DecodedSourceImage::Raw(original) = &source else {
        unreachable!()
    };
    let original_pointer = original.rgb.as_ptr();
    for [x, y, width, height] in [[1, 3, 7, 5], [63, 63, 1, 1], [0, 0, 64, 64]] {
        let DecodedSourceImage::Raw(cropped) = source.crop(x, y, width, height).unwrap() else {
            unreachable!()
        };
        assert_eq!((cropped.width, cropped.height), (width, height));
        assert_eq!(cropped.metadata, original.metadata);
        assert_eq!(cropped.timings, original.timings);
        assert_eq!(cropped.preview_half_size, original.preview_half_size);
        for row in 0..height {
            let start = (((y + row) * original.width + x) * 3) as usize;
            let destination = (row * width * 3) as usize;
            assert_eq!(
                &cropped.rgb[destination..destination + width as usize * 3],
                &original.rgb[start..start + width as usize * 3]
            );
        }
    }
    assert_eq!(original.rgb.as_ptr(), original_pointer);
    let mut invalid = (*original).clone();
    invalid.rgb.pop();
    assert!(matches!(
        DecodedSourceImage::Raw(invalid).crop(0, 0, 1, 1),
        Err(ImageIoError::InvalidBufferLength)
    ));
}

#[test]
fn actual_libraw_forward_calibration_uses_reference_neutral_before_working_conversion() {
    use tiff::encoder::Rational;
    let source = fixture::controlled_fixture_with_dng_profile(
        Some([
            Rational { n: 2, d: 1 },
            Rational { n: 1, d: 1 },
            Rational { n: 1, d: 2 },
        ]),
        true,
    )
    .unwrap();
    let original = std::fs::read(&source.0).unwrap();
    // Independently expanded DNG equation plus double-precision Bradford D50->D65.
    let expected_matrix: [[f64; 3]; 3] = [
        [4.607521323749922, -3.7091746365417504, 0.05212331496888902],
        [
            -0.1622826511018206,
            1.1449469863345783,
            0.017335692145308813,
        ],
        [
            0.059815830180290123,
            -0.06844077790763342,
            1.0974548964377184,
        ],
    ];
    for decoded in [
        starroom_raw::decode_raw(&source.0).unwrap(),
        starroom_raw::decode_raw_preview(&source.0).unwrap(),
    ] {
        assert_eq!(decoded.metadata.dng_color[0].parsed_fields, 15);
        assert_eq!(
            decoded.metadata.camera_profile.source,
            starroom_raw::CameraProfileSource::DngForwardMatrix
        );
        assert_eq!(
            decoded.metadata.camera_profile.status,
            starroom_raw::CameraProfileStatus::Resolved
        );
        for (actual, expected) in decoded
            .metadata
            .camera_profile
            .camera_to_xyz_d65
            .iter()
            .flatten()
            .zip(expected_matrix.iter().flatten())
        {
            assert!((f64::from(*actual) - expected).abs() < 2e-5);
        }
        for (column, sensor) in [1024, 2048, 4096, 6144, 8192, 10240, 12288, 14336]
            .into_iter()
            .enumerate()
        {
            let x = (column * 8 + 4) * decoded.width as usize / 64;
            let index = ((decoded.height as usize / 2) * decoded.width as usize + x) * 3;
            let signal = f64::from(sensor) / 16383.0;
            let camera = [signal * 2.0, signal, signal * 4.0];
            let xyz = expected_matrix
                .map(|row| row.into_iter().zip(camera).map(|(a, b)| a * b).sum::<f64>());
            let expected = [
                1.716_651_2 * xyz[0] - 0.355_670_78 * xyz[1] - 0.253_366_3 * xyz[2],
                -0.666_684_3 * xyz[0] + 1.616_481_2 * xyz[1] + 0.015_768_546 * xyz[2],
                0.017_639_857 * xyz[0] - 0.042_770_613 * xyz[1] + 0.942_103_1 * xyz[2],
            ];
            for (actual, expected) in decoded.rgb[index..index + 3].iter().zip(expected) {
                assert!(
                    (f64::from(*actual) - expected).abs() < 4e-4,
                    "sensor={sensor} actual={actual} expected={expected}"
                );
            }
        }
        assert!(decoded.rgb.iter().all(|v| v.is_finite()));
    }
    assert_eq!(original, std::fs::read(&source.0).unwrap());
}

#[test]
fn actual_libraw_analog_balance_reaches_camera_transform_and_metadata_round_trip() {
    use starroom_raw::{CameraProfileSource, CameraProfileStatus};
    use tiff::encoder::Rational;
    let fixture = fixture::controlled_fixture_with_dng_profile(
        Some([
            Rational { n: 2, d: 1 },
            Rational { n: 1, d: 1 },
            Rational { n: 1, d: 2 },
        ]),
        false,
    )
    .unwrap();
    let original = std::fs::read(&fixture.0).unwrap();
    for decoded in [
        starroom_raw::decode_raw(&fixture.0).unwrap(),
        starroom_raw::decode_raw_preview(&fixture.0).unwrap(),
    ] {
        assert_eq!(decoded.metadata.analog_balance[..3], [2.0, 1.0, 0.5]);
        assert_eq!(
            decoded.metadata.dng_color[0].parsed_fields,
            (1 << 1) | (1 << 2)
        );
        assert_eq!(decoded.metadata.dng_color[1].parsed_fields, 0);
        assert_eq!(
            decoded.metadata.camera_profile.status,
            CameraProfileStatus::Resolved,
            "metadata={:?}",
            decoded.metadata
        );
        assert_eq!(
            decoded.metadata.camera_profile.source,
            CameraProfileSource::DngColorMatrix
        );
        // CM=identity, AB=(2,1,.5), neutral=(.5,1,.25). Thus source white is
        // (.25,1,.5) and inverseAB * undoWB maps equal balanced RGB to that white.
        // Independent double-precision Bradford oracle, not the production resolver output.
        let expected_matrix = [
            [0.530_089_44, 0.408_576_16, 0.011_804_4],
            [0.158_061_76, 0.855_113, -0.013_174_78],
            [0.013_055_22, -0.117_500_275, 1.193_275],
        ];
        for (actual, expected) in decoded
            .metadata
            .camera_profile
            .camera_to_xyz_d65
            .iter()
            .flatten()
            .zip(expected_matrix.iter().flatten())
        {
            assert!((actual - expected).abs() < 1e-6);
        }
        for (column, sensor) in [1024, 2048, 4096, 6144, 8192, 10240, 12288, 14336]
            .into_iter()
            .enumerate()
        {
            let x = (column * 8 + 4) * decoded.width as usize / 64;
            let index = ((decoded.height as usize / 2) * decoded.width as usize + x) * 3;
            let signal = sensor as f32 / 16383.0;
            let camera = [signal * 2.0, signal, signal * 4.0];
            let xyz = expected_matrix
                .map(|row| row.into_iter().zip(camera).map(|(a, b)| a * b).sum::<f32>());
            let expected = [
                1.716_651_2 * xyz[0] - 0.355_670_78 * xyz[1] - 0.253_366_3 * xyz[2],
                -0.666_684_3 * xyz[0] + 1.616_481_2 * xyz[1] + 0.015_768_546 * xyz[2],
                0.017_639_857 * xyz[0] - 0.042_770_613 * xyz[1] + 0.942_103_1 * xyz[2],
            ];
            for (actual, expected) in decoded.rgb[index..index + 3].iter().zip(expected) {
                assert!(
                    (actual - expected).abs() < 4e-4,
                    "sensor={sensor} actual={actual} expected={expected}"
                );
            }
        }
        assert!(decoded.rgb.iter().all(|v| v.is_finite()));
        let bytes = serde_json::to_vec(&decoded.metadata).unwrap();
        let restored: starroom_raw::RawMetadata = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored, decoded.metadata);
        let mut legacy = serde_json::to_value(&decoded.metadata).unwrap();
        legacy.as_object_mut().unwrap().remove("analogBalance");
        let restored: starroom_raw::RawMetadata = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.analog_balance, [1.0; 4]);
    }
    assert_eq!(original, std::fs::read(&fixture.0).unwrap());
}

#[test]
fn real_libraw_preserves_unsaturated_sensor_wb_headroom_and_declared_white_level() {
    let fixture = fixture::controlled_fixture().unwrap();
    let original = std::fs::read(&fixture.0).unwrap();
    for decoded in [
        starroom_raw::decode_raw(&fixture.0).unwrap(),
        starroom_raw::decode_raw_preview(&fixture.0).unwrap(),
    ] {
        assert_eq!(decoded.metadata.white_level, 16383);
        assert_eq!(decoded.metadata.black_level, 0);
        assert_eq!(decoded.metadata.wb_headroom_scale, 4.0);
        assert_eq!(decoded.metadata.as_shot_multipliers[..3], [2.0, 1.0, 4.0]);
        let mut last = [0.0; 3];
        for (column, sensor) in [1024, 2048, 4096, 6144, 8192, 10240, 12288, 14336]
            .into_iter()
            .enumerate()
        {
            let x = (column * 8 + 4) * decoded.width as usize / 64;
            let index = ((decoded.height as usize / 2) * decoded.width as usize + x) * 3;
            let actual = &decoded.rgb[index..index + 3];
            let signal = sensor as f32 / 16383.0;
            let xyz = decoded.metadata.camera_profile.camera_rgb_to_xyz_d65([
                signal * 2.0,
                signal,
                signal * 4.0,
            ]);
            // Independent published XYZ->Rec.2020 matrix, only a sensor normalization oracle.
            let expected = [
                1.716_651_2 * xyz[0] - 0.355_670_78 * xyz[1] - 0.253_366_3 * xyz[2],
                -0.666_684_3 * xyz[0] + 1.616_481_2 * xyz[1] + 0.015_768_546 * xyz[2],
                0.017_639_857 * xyz[0] - 0.042_770_613 * xyz[1] + 0.942_103_1 * xyz[2],
            ];
            for channel in 0..3 {
                assert!(
                    (actual[channel] - expected[channel]).abs() < 4.0e-4,
                    "sensor={sensor} half={} actual={actual:?} expected={expected:?}",
                    decoded.preview_half_size
                );
                assert!(
                    actual[channel] > last[channel],
                    "unsaturated ramp must not plateau from camera WB"
                );
            }
            last.copy_from_slice(actual);
        }
        assert!(last[2] > 3.0, "working headroom must survive beyond 1.0");
        assert!(decoded.rgb.iter().all(|v| v.is_finite()));
    }
    assert_eq!(original, std::fs::read(&fixture.0).unwrap());
}
