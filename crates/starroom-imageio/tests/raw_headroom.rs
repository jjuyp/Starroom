#[path = "support/controlled_dng.rs"]
mod fixture;

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
