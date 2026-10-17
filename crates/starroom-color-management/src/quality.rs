//! Native color-quality measurements, delegated to the pinned LittleCMS Lab/CIEDE2000 provider.
//! XYZ is relative to D65 with Y=1 reference white; this is not an absolute HDR appearance metric.
use crate::{ColorManagementError, D50, D65, Xyz};
use lcms2::{CIELabExt, CIEXYZ, CIEXYZExt};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorDifference {
    pub xyz_rmse: f64,
    pub delta_e_2000: f64,
    pub luminance_error: f64,
}

pub fn compare_xyz_d65(
    actual: Xyz,
    expected: Xyz,
) -> Result<ColorDifference, ColorManagementError> {
    let components = |value: Xyz| [value.x, value.y, value.z];
    for (pixel, value) in [actual, expected].into_iter().enumerate() {
        for (channel, component) in components(value).into_iter().enumerate() {
            if !component.is_finite() {
                return Err(ColorManagementError::NonFinitePixel {
                    stage: "quality-input",
                    pixel,
                    channel,
                });
            }
        }
    }
    let xyz = |value: Xyz| CIEXYZ {
        X: f64::from(value.x),
        Y: f64::from(value.y),
        Z: f64::from(value.z),
    };
    let white = xyz(D50);
    let lab = |value| {
        xyz(value)
            .adapt_to_illuminant(&xyz(D65), &white)
            .map(|adapted| adapted.to_lab(&white))
            .ok_or(ColorManagementError::InvalidWhitePoint)
    };
    let actual_lab = lab(actual)?;
    let expected_lab = lab(expected)?;
    let delta_e_2000 = actual_lab.cie2000_delta_e(&expected_lab, 1.0, 1.0, 1.0);
    let squared: f64 = components(actual)
        .into_iter()
        .zip(components(expected))
        .map(|(a, b)| (f64::from(a) - f64::from(b)).powi(2))
        .sum();
    let result = ColorDifference {
        xyz_rmse: (squared / 3.0).sqrt(),
        delta_e_2000,
        luminance_error: (f64::from(actual.y) - f64::from(expected.y)).abs(),
    };
    if ![result.xyz_rmse, result.delta_e_2000, result.luminance_error]
        .into_iter()
        .all(f64::is_finite)
    {
        return Err(ColorManagementError::NonFinitePixel {
            stage: "quality-output",
            pixel: 0,
            channel: 0,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identical_color_is_exact_and_repeatable() {
        for value in [
            Xyz {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            D65,
            Xyz {
                x: 0.4,
                y: 0.2,
                z: 0.1,
            },
        ] {
            let result = compare_xyz_d65(value, value).unwrap();
            assert_eq!(
                result,
                ColorDifference {
                    xyz_rmse: 0.0,
                    delta_e_2000: 0.0,
                    luminance_error: 0.0
                }
            );
            assert_eq!(result, compare_xyz_d65(value, value).unwrap());
        }
    }
    #[test]
    fn lcms_ciede2000_matches_independent_achromatic_lightness_case() {
        let gray = |lightness: f64| {
            let y = ((lightness + 16.0) / 116.0).powi(3) as f32;
            Xyz {
                x: D65.x * y,
                y,
                z: D65.z * y,
            }
        };
        let actual = compare_xyz_d65(gray(50.0), gray(60.0)).unwrap();
        let expected = 10.0 / (1.0 + 0.015 * 25.0 / 45.0_f64.sqrt());
        assert!((actual.delta_e_2000 - expected).abs() < 2e-5);
        assert_eq!(actual, compare_xyz_d65(gray(60.0), gray(50.0)).unwrap());
        assert!(actual.xyz_rmse > 0.0 && actual.luminance_error > 0.0);
    }
    #[test]
    fn nonfinite_quality_inputs_are_typed_errors() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(matches!(
                compare_xyz_d65(
                    Xyz {
                        x: invalid,
                        y: 0.0,
                        z: 0.0
                    },
                    D65
                ),
                Err(ColorManagementError::NonFinitePixel {
                    stage: "quality-input",
                    ..
                })
            ));
            assert!(
                compare_xyz_d65(
                    D65,
                    Xyz {
                        x: 0.0,
                        y: invalid,
                        z: 0.0
                    }
                )
                .is_err()
            );
        }
    }
}
