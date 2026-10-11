//! BSD-3-Clause-derived scalar port of Colour Developers' Robertson (1968) provider.
//! Copyright 2013 Colour Developers. Source: colour-science/colour v0.4.7,
//! colour/temperature/robertson1968.py, Git blob 380b8bf8327098e8532eedd219084e8386547530.
//! Retain the BSD notice recorded in provenance; this is correlated temperature, not spectral recovery.
use crate::{ColorManagementError, Xyz};

const LINES: [[f64; 4]; 31] = [
    [0.0, 0.18006, 0.26352, -0.24341],
    [10.0, 0.18066, 0.26589, -0.25479],
    [20.0, 0.18133, 0.26846, -0.26876],
    [30.0, 0.18208, 0.27119, -0.28539],
    [40.0, 0.18293, 0.27407, -0.30470],
    [50.0, 0.18388, 0.27709, -0.32675],
    [60.0, 0.18494, 0.28021, -0.35156],
    [70.0, 0.18611, 0.28342, -0.37915],
    [80.0, 0.18740, 0.28668, -0.40955],
    [90.0, 0.18880, 0.28997, -0.44278],
    [100.0, 0.19032, 0.29326, -0.47888],
    [125.0, 0.19462, 0.30141, -0.58204],
    [150.0, 0.19962, 0.30921, -0.70471],
    [175.0, 0.20525, 0.31647, -0.84901],
    [200.0, 0.21142, 0.32312, -1.0182],
    [225.0, 0.21807, 0.32909, -1.2168],
    [250.0, 0.22511, 0.33439, -1.4512],
    [275.0, 0.23247, 0.33904, -1.7298],
    [300.0, 0.24010, 0.34308, -2.0637],
    [325.0, 0.24792, 0.34655, -2.4681],
    [350.0, 0.25591, 0.34951, -2.9641],
    [375.0, 0.26400, 0.35200, -3.5814],
    [400.0, 0.27218, 0.35407, -4.3633],
    [425.0, 0.28039, 0.35577, -5.3762],
    [450.0, 0.28863, 0.35714, -6.7262],
    [475.0, 0.29685, 0.35823, -8.5955],
    [500.0, 0.30505, 0.35907, -11.324],
    [525.0, 0.31320, 0.35968, -15.628],
    [550.0, 0.32129, 0.36011, -23.325],
    [575.0, 0.32931, 0.36038, -40.770],
    [600.0, 0.33724, 0.36051, -116.45],
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CorrelatedTemperature {
    pub kelvin: f64,
    pub delta_uv: f64,
}

fn direction(line: [f64; 4]) -> [f64; 2] {
    let length = 1.0_f64.hypot(line[3]);
    [1.0 / length, line[3] / length]
}
fn mix(a: f64, b: f64, f: f64) -> f64 {
    a * f + b * (1.0 - f)
}

pub fn temperature_to_uv(value: CorrelatedTemperature) -> Result<[f64; 2], ColorManagementError> {
    if !value.kelvin.is_finite()
        || !(1e6 / 600.0..=100000.0).contains(&value.kelvin)
        || !value.delta_uv.is_finite()
        || value.delta_uv.abs() > 0.05
    {
        return Err(ColorManagementError::InvalidWhitePoint);
    }
    let reciprocal = 1e6 / value.kelvin;
    let index = (0..30).find(|i| reciprocal < LINES[i + 1][0]).unwrap_or(29);
    let a = LINES[index];
    let b = LINES[index + 1];
    let f = (b[0] - reciprocal) / (b[0] - a[0]);
    let da = direction(a);
    let db = direction(b);
    let du = mix(da[0], db[0], f);
    let dv = mix(da[1], db[1], f);
    let length = du.hypot(dv);
    Ok([
        mix(a[1], b[1], f) - du / length * value.delta_uv,
        mix(a[2], b[2], f) - dv / length * value.delta_uv,
    ])
}

pub fn uv_to_temperature(uv: [f64; 2]) -> Result<CorrelatedTemperature, ColorManagementError> {
    if !uv.into_iter().all(f64::is_finite) || uv[0] <= 0.0 || uv[1] <= 0.0 {
        return Err(ColorManagementError::InvalidWhitePoint);
    }
    let distance = |line: [f64; 4]| {
        let d = direction(line);
        -(uv[0] - line[1]) * d[1] + (uv[1] - line[2]) * d[0]
    };
    let index = (1..31).find(|i| distance(LINES[*i]) <= 0.0).unwrap_or(30);
    let a = LINES[index - 1];
    let b = LINES[index];
    let current = (-distance(b)).max(0.0);
    let previous = distance(a);
    let f = if index == 1 {
        0.0
    } else {
        current / (previous + current)
    };
    let duv_a = direction(a);
    let duv_b = direction(b);
    let du = mix(duv_a[0], duv_b[0], f);
    let dv = mix(duv_a[1], duv_b[1], f);
    let length = du.hypot(dv);
    let delta_uv =
        -((uv[0] - mix(a[1], b[1], f)) * du / length + (uv[1] - mix(a[2], b[2], f)) * dv / length);
    let result = CorrelatedTemperature {
        kelvin: 1e6 / mix(a[0], b[0], f),
        delta_uv,
    };
    if !result.kelvin.is_finite()
        || result.kelvin < 1e6 / 600.0
        || !delta_uv.is_finite()
        || delta_uv.abs() > 0.05
    {
        return Err(ColorManagementError::InvalidWhitePoint);
    }
    Ok(result)
}

pub fn temperature_to_xyz(value: CorrelatedTemperature) -> Result<Xyz, ColorManagementError> {
    let [u, v] = temperature_to_uv(value)?;
    let denominator = 2.0 * u - 8.0 * v + 4.0;
    let x = 3.0 * u / denominator;
    let y = 2.0 * v / denominator;
    let result = Xyz {
        x: (x / y) as f32,
        y: 1.0,
        z: ((1.0 - x - y) / y) as f32,
    };
    if ![result.x, result.y, result.z]
        .into_iter()
        .all(|v| v.is_finite() && v > 0.0)
    {
        return Err(ColorManagementError::InvalidWhitePoint);
    }
    Ok(result)
}

pub fn xyz_to_temperature(value: Xyz) -> Result<CorrelatedTemperature, ColorManagementError> {
    if ![value.x, value.y, value.z]
        .into_iter()
        .all(|v| v.is_finite() && v > 0.0)
    {
        return Err(ColorManagementError::InvalidWhitePoint);
    }
    let x = f64::from(value.x);
    let y = f64::from(value.y);
    let z = f64::from(value.z);
    let denominator = x + 15.0 * y + 3.0 * z;
    uv_to_temperature([4.0 * x / denominator, 6.0 * y / denominator])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_fixed_upstream_robertson_example() {
        let uv = [0.193741375998230, 0.315221043940594];
        let result = uv_to_temperature(uv).unwrap();
        assert!((result.kelvin - 6500.0162).abs() < 0.001);
        assert!((result.delta_uv - 0.0083333289).abs() < 1e-9);
        let recovered = temperature_to_uv(result).unwrap();
        for (a, b) in recovered.into_iter().zip(uv) {
            assert!((a - b).abs() < 1e-7);
        }
    }
    #[test]
    fn supports_tungsten_daylight_and_separate_tint_without_nonfinite_results() {
        for kelvin in [2000.0, 2856.0, 4000.0, 5000.0, 6504.0, 10000.0, 25000.0] {
            for delta_uv in [-0.01, 0.0, 0.01] {
                let input = CorrelatedTemperature { kelvin, delta_uv };
                let actual = uv_to_temperature(temperature_to_uv(input).unwrap()).unwrap();
                assert!((actual.kelvin - kelvin).abs() < 2.0, "{input:?} {actual:?}");
                assert!((actual.delta_uv - delta_uv).abs() < 1e-5);
                let white = temperature_to_xyz(input).unwrap();
                assert_eq!(white.y, 1.0);
                assert!(xyz_to_temperature(white).is_ok());
            }
        }
    }
    #[test]
    fn invalid_temperature_and_white_are_explicit_errors() {
        for kelvin in [0.0, 1000.0, f64::NAN, f64::INFINITY] {
            assert!(
                temperature_to_uv(CorrelatedTemperature {
                    kelvin,
                    delta_uv: 0.0
                })
                .is_err()
            );
        }
        assert!(uv_to_temperature([f64::NAN, 0.3]).is_err());
        assert!(
            xyz_to_temperature(Xyz {
                x: 0.0,
                y: 0.0,
                z: 0.0
            })
            .is_err()
        );
    }
}
