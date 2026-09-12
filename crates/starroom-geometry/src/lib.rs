//! Geometry and perspective primitives for Starroom.
//! Coordinates are normalized to the source frame unless otherwise documented.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Point2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoordinateSpace {
    SourceSensor,
    OrientedImage,
    PostLens,
    PostGeometry,
    Viewport,
    Normalized,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpacePoint {
    pub space: CoordinateSpace,
    pub point: Point2,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FourPoint {
    pub top_left: Point2,
    pub top_right: Point2,
    pub bottom_right: Point2,
    pub bottom_left: Point2,
}

impl Default for FourPoint {
    fn default() -> Self {
        Self {
            top_left: Point2 { x: 0.0, y: 0.0 },
            top_right: Point2 { x: 1.0, y: 0.0 },
            bottom_right: Point2 { x: 1.0, y: 1.0 },
            bottom_left: Point2 { x: 0.0, y: 1.0 },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum UprightMode {
    #[default]
    Off,
    Auto,
    Level,
    Vertical,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Default for CropRect {
    fn default() -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            right: 1.0,
            bottom: 1.0,
        }
    }
}

impl CropRect {
    pub fn normalized(self) -> Self {
        let left = self.left.clamp(0.0, 1.0);
        let top = self.top.clamp(0.0, 1.0);
        let right = self.right.clamp(left + 1.0e-5, 1.0);
        let bottom = self.bottom.clamp(top + 1.0e-5, 1.0);
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeometryParameters {
    pub rotation_degrees: f32,
    pub vertical_keystone: f32,
    pub horizontal_keystone: f32,
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    pub crop: CropRect,
    /// Zero means free. -1/-1 preserves the source aspect; positive values request width:height.
    #[serde(default)]
    pub crop_aspect_width: f32,
    #[serde(default)]
    pub crop_aspect_height: f32,
    #[serde(default)]
    pub four_point: Option<FourPoint>,
    #[serde(default)]
    pub upright_mode: UprightMode,
}

impl Default for GeometryParameters {
    fn default() -> Self {
        Self {
            rotation_degrees: 0.0,
            vertical_keystone: 0.0,
            horizontal_keystone: 0.0,
            scale: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            flip_horizontal: false,
            flip_vertical: false,
            crop: CropRect::default(),
            crop_aspect_width: 0.0,
            crop_aspect_height: 0.0,
            four_point: None,
            upright_mode: UprightMode::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix3 {
    pub m: [[f32; 3]; 3],
}

impl Matrix3 {
    pub const IDENTITY: Self = Self {
        m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };

    pub fn multiply(self, other: Self) -> Self {
        let mut out = [[0.0; 3]; 3];
        for (row, row_values) in out.iter_mut().enumerate() {
            for (column, value) in row_values.iter_mut().enumerate() {
                *value = (0..3)
                    .map(|index| self.m[row][index] * other.m[index][column])
                    .sum();
            }
        }
        Self { m: out }
    }

    pub fn transform(self, point: Point2) -> Point2 {
        let x = self.m[0][0] * point.x + self.m[0][1] * point.y + self.m[0][2];
        let y = self.m[1][0] * point.x + self.m[1][1] * point.y + self.m[1][2];
        let w = self.m[2][0] * point.x + self.m[2][1] * point.y + self.m[2][2];
        let safe_w = if w.abs() < 1.0e-8 {
            1.0e-8_f32.copysign(w)
        } else {
            w
        };
        Point2 {
            x: x / safe_w,
            y: y / safe_w,
        }
    }

    pub fn inverse(self) -> Option<Self> {
        let m = self.m;
        let determinant = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
        if determinant.abs() < 1.0e-8 || !determinant.is_finite() {
            return None;
        }
        let inverse_det = 1.0 / determinant;
        Some(Self {
            m: [
                [
                    (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inverse_det,
                    (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inverse_det,
                    (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inverse_det,
                ],
                [
                    (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inverse_det,
                    (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inverse_det,
                    (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inverse_det,
                ],
                [
                    (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inverse_det,
                    (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inverse_det,
                    (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inverse_det,
                ],
            ],
        })
    }
}

fn translation(x: f32, y: f32) -> Matrix3 {
    Matrix3 {
        m: [[1.0, 0.0, x], [0.0, 1.0, y], [0.0, 0.0, 1.0]],
    }
}

fn scale(x: f32, y: f32) -> Matrix3 {
    Matrix3 {
        m: [[x, 0.0, 0.0], [0.0, y, 0.0], [0.0, 0.0, 1.0]],
    }
}

fn rotation(degrees: f32) -> Matrix3 {
    let angle = degrees.to_radians();
    let cos = angle.cos();
    let sin = angle.sin();
    Matrix3 {
        m: [[cos, -sin, 0.0], [sin, cos, 0.0], [0.0, 0.0, 1.0]],
    }
}

fn keystone(horizontal: f32, vertical: f32) -> Matrix3 {
    Matrix3 {
        m: [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [horizontal.clamp(-1.5, 1.5), vertical.clamp(-1.5, 1.5), 1.0],
        ],
    }
}

/// Builds a forward normalized-coordinate transform around image center.
pub fn build_transform(parameters: GeometryParameters) -> Matrix3 {
    let safe_scale = parameters.scale.clamp(0.05, 20.0);
    let flip_x = if parameters.flip_horizontal {
        -1.0
    } else {
        1.0
    };
    let flip_y = if parameters.flip_vertical { -1.0 } else { 1.0 };
    let centered = translation(-0.5, -0.5);
    let transform = keystone(parameters.horizontal_keystone, parameters.vertical_keystone)
        .multiply(rotation(parameters.rotation_degrees))
        .multiply(scale(safe_scale * flip_x, safe_scale * flip_y))
        .multiply(centered);
    let base =
        translation(0.5 + parameters.offset_x, 0.5 + parameters.offset_y).multiply(transform);
    parameters
        .four_point
        .and_then(homography_from_four_point)
        .map(|quad| base.multiply(quad))
        .unwrap_or(base)
}

fn solve_8x8(mut matrix: [[f32; 9]; 8]) -> Option<[f32; 8]> {
    for column in 0..8 {
        let pivot = (column..8).max_by(|left, right| {
            matrix[*left][column]
                .abs()
                .total_cmp(&matrix[*right][column].abs())
        })?;
        if matrix[pivot][column].abs() < 1.0e-7 {
            return None;
        }
        matrix.swap(column, pivot);
        let scale = matrix[column][column];
        for value in &mut matrix[column][column..=8] {
            *value /= scale;
        }
        let pivot_row = matrix[column];
        for (row, row_values) in matrix.iter_mut().enumerate() {
            if row != column {
                let factor = row_values[column];
                for (target, source) in row_values[column..=8]
                    .iter_mut()
                    .zip(&pivot_row[column..=8])
                {
                    *target -= factor * source;
                }
            }
        }
    }
    Some(std::array::from_fn(|index| matrix[index][8]))
}

/// Source four-point quadrilateral -> normalized rectified image homography.
pub fn homography_from_four_point(points: FourPoint) -> Option<Matrix3> {
    let source = [
        points.top_left,
        points.top_right,
        points.bottom_right,
        points.bottom_left,
    ];
    let target = [
        Point2 { x: 0.0, y: 0.0 },
        Point2 { x: 1.0, y: 0.0 },
        Point2 { x: 1.0, y: 1.0 },
        Point2 { x: 0.0, y: 1.0 },
    ];
    let mut equations = [[0.0_f32; 9]; 8];
    for index in 0..4 {
        let (x, y, u, v) = (
            source[index].x,
            source[index].y,
            target[index].x,
            target[index].y,
        );
        equations[index * 2] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
        equations[index * 2 + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
    }
    let h = solve_8x8(equations)?;
    Some(Matrix3 {
        m: [[h[0], h[1], h[2]], [h[3], h[4], h[5]], [h[6], h[7], 1.0]],
    })
}

pub fn constrain_crop_aspect(
    crop: CropRect,
    image_width: usize,
    image_height: usize,
    aspect_width: f32,
    aspect_height: f32,
) -> CropRect {
    let crop = crop.normalized();
    if aspect_width == 0.0 || aspect_height == 0.0 || image_width == 0 || image_height == 0 {
        return crop;
    }
    let target_normalized = if aspect_width < 0.0 && aspect_height < 0.0 {
        1.0
    } else {
        (aspect_width / aspect_height) * image_height as f32 / image_width as f32
    };
    let center_x = (crop.left + crop.right) * 0.5;
    let center_y = (crop.top + crop.bottom) * 0.5;
    let mut width = crop.right - crop.left;
    let mut height = crop.bottom - crop.top;
    if width / height > target_normalized {
        width = height * target_normalized;
    } else {
        height = width / target_normalized;
    }
    let left = (center_x - width * 0.5).clamp(0.0, 1.0 - width);
    let top = (center_y - height * 0.5).clamp(0.0, 1.0 - height);
    CropRect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeometryImage {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
    pub transform: Matrix3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryError {
    InvalidBuffer,
    SingularTransform,
    NonFiniteOutput,
}

fn image_sample(data: &[f32], width: usize, height: usize, x: f32, y: f32, channel: usize) -> f32 {
    if x < 0.0 || y < 0.0 || x > (width - 1) as f32 || y > (height - 1) as f32 {
        return 0.0;
    }
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let sample = |sx: usize, sy: usize| data[(sy * width + sx) * 3 + channel];
    (sample(x0, y0) * (1.0 - tx) + sample(x1, y0) * tx) * (1.0 - ty)
        + (sample(x0, y1) * (1.0 - tx) + sample(x1, y1) * tx) * ty
}

pub fn apply_geometry(
    width: usize,
    height: usize,
    data: &[f32],
    parameters: GeometryParameters,
) -> Result<GeometryImage, GeometryError> {
    if width == 0
        || height == 0
        || data.len() != width.saturating_mul(height).saturating_mul(3)
        || !data.iter().all(|value| value.is_finite())
    {
        return Err(GeometryError::InvalidBuffer);
    }
    let crop = constrain_crop_aspect(
        parameters.crop,
        width,
        height,
        parameters.crop_aspect_width,
        parameters.crop_aspect_height,
    );
    let output_width = ((crop.right - crop.left) * width as f32).round().max(1.0) as usize;
    let output_height = ((crop.bottom - crop.top) * height as f32).round().max(1.0) as usize;
    let transform = build_transform(parameters);
    let inverse = transform
        .inverse()
        .ok_or(GeometryError::SingularTransform)?;
    let mut output = vec![0.0_f32; output_width * output_height * 3];
    for y in 0..output_height {
        for x in 0..output_width {
            let target = Point2 {
                x: crop.left
                    + x as f32 / output_width.saturating_sub(1).max(1) as f32
                        * (crop.right - crop.left),
                y: crop.top
                    + y as f32 / output_height.saturating_sub(1).max(1) as f32
                        * (crop.bottom - crop.top),
            };
            let source = inverse.transform(target);
            for channel in 0..3 {
                let value = image_sample(
                    data,
                    width,
                    height,
                    source.x * (width - 1) as f32,
                    source.y * (height - 1) as f32,
                    channel,
                );
                if !value.is_finite() {
                    return Err(GeometryError::NonFiniteOutput);
                }
                output[(y * output_width + x) * 3 + channel] = value;
            }
        }
    }
    Ok(GeometryImage {
        width: output_width,
        height: output_height,
        data: output,
        transform,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UprightAnalysis {
    pub mode: UprightMode,
    pub rotation_degrees: f32,
    pub vertical_keystone: f32,
    pub horizontal_keystone: f32,
    pub confidence: f32,
    pub line_samples: u32,
}

/// Image-derived Sobel line analysis. Level uses near-horizontal line energy; Vertical and Full
/// additionally correlate vertical/horizontal line deviations with position to estimate keystone.
pub fn analyze_upright(
    width: usize,
    height: usize,
    data: &[f32],
    mode: UprightMode,
) -> UprightAnalysis {
    if mode == UprightMode::Off || width < 3 || height < 3 || data.len() != width * height * 3 {
        return UprightAnalysis {
            mode,
            ..Default::default()
        };
    }
    let luma = |x: usize, y: usize| {
        let base = (y * width + x) * 3;
        0.2627 * data[base] + 0.6780 * data[base + 1] + 0.0593 * data[base + 2]
    };
    let mut horizontal_sin = 0.0;
    let mut horizontal_cos = 0.0;
    let mut horizontal_weight = 0.0;
    let mut vertical_corr = 0.0;
    let mut vertical_norm = 0.0;
    let mut horizontal_corr = 0.0;
    let mut horizontal_norm = 0.0;
    let mut samples = 0_u32;
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let gx = -luma(x - 1, y - 1) + luma(x + 1, y - 1) - 2.0 * luma(x - 1, y)
                + 2.0 * luma(x + 1, y)
                - luma(x - 1, y + 1)
                + luma(x + 1, y + 1);
            let gy = -luma(x - 1, y - 1) - 2.0 * luma(x, y - 1) - luma(x + 1, y - 1)
                + luma(x - 1, y + 1)
                + 2.0 * luma(x, y + 1)
                + luma(x + 1, y + 1);
            let magnitude = (gx * gx + gy * gy).sqrt();
            if magnitude < 0.015 {
                continue;
            }
            let mut line_angle = gy.atan2(gx).to_degrees() + 90.0;
            while line_angle >= 90.0 {
                line_angle -= 180.0;
            }
            while line_angle < -90.0 {
                line_angle += 180.0;
            }
            let radians2 = (line_angle * 2.0).to_radians();
            if line_angle.abs() < 35.0 {
                horizontal_sin += radians2.sin() * magnitude;
                horizontal_cos += radians2.cos() * magnitude;
                horizontal_weight += magnitude;
                let position = y as f32 / (height - 1) as f32 - 0.5;
                horizontal_corr += line_angle.to_radians() * position * magnitude;
                horizontal_norm += position * position * magnitude;
            } else if line_angle.abs() > 55.0 {
                let deviation = if line_angle > 0.0 {
                    line_angle - 90.0
                } else {
                    line_angle + 90.0
                };
                let position = x as f32 / (width - 1) as f32 - 0.5;
                vertical_corr += deviation.to_radians() * position * magnitude;
                vertical_norm += position * position * magnitude;
            }
            samples += 1;
        }
    }
    let roll = if horizontal_weight > 0.0 {
        0.5 * horizontal_sin.atan2(horizontal_cos).to_degrees()
    } else {
        0.0
    };
    let vertical = if vertical_norm > 0.0 {
        (vertical_corr / vertical_norm * 0.8).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let horizontal = if horizontal_norm > 0.0 {
        (horizontal_corr / horizontal_norm * 0.8).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let confidence = (horizontal_weight / (width * height) as f32 * 12.0).clamp(0.0, 1.0);
    let effective = if mode == UprightMode::Auto && confidence < 0.08 {
        UprightMode::Off
    } else {
        mode
    };
    UprightAnalysis {
        mode: effective,
        rotation_degrees: if matches!(effective, UprightMode::Off) {
            0.0
        } else {
            -roll
        },
        vertical_keystone: if matches!(
            effective,
            UprightMode::Vertical | UprightMode::Full | UprightMode::Auto
        ) {
            -vertical
        } else {
            0.0
        },
        horizontal_keystone: if effective == UprightMode::Full {
            -horizontal
        } else {
            0.0
        },
        confidence,
        line_samples: samples,
    }
}

pub fn apply_upright(
    mut parameters: GeometryParameters,
    analysis: UprightAnalysis,
) -> GeometryParameters {
    if analysis.mode != UprightMode::Off {
        parameters.rotation_degrees += analysis.rotation_degrees;
        parameters.vertical_keystone += analysis.vertical_keystone;
        parameters.horizontal_keystone += analysis.horizontal_keystone;
    }
    parameters.upright_mode = analysis.mode;
    parameters
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoordinateMapper {
    pub source_to_post_lens: Matrix3,
    pub post_lens_to_geometry: Matrix3,
    pub geometry_to_viewport: Matrix3,
}

impl CoordinateMapper {
    pub fn map(&self, point: SpacePoint, target: CoordinateSpace) -> Option<SpacePoint> {
        let source_to_view = self
            .geometry_to_viewport
            .multiply(self.post_lens_to_geometry)
            .multiply(self.source_to_post_lens);
        let matrix = match (point.space, target) {
            (CoordinateSpace::SourceSensor, CoordinateSpace::PostLens) => self.source_to_post_lens,
            (CoordinateSpace::SourceSensor, CoordinateSpace::PostGeometry) => self
                .post_lens_to_geometry
                .multiply(self.source_to_post_lens),
            (CoordinateSpace::SourceSensor, CoordinateSpace::Viewport) => source_to_view,
            (CoordinateSpace::PostLens, CoordinateSpace::PostGeometry) => {
                self.post_lens_to_geometry
            }
            (CoordinateSpace::PostGeometry, CoordinateSpace::Viewport) => self.geometry_to_viewport,
            (from, to) if from == to => Matrix3::IDENTITY,
            _ => return None,
        };
        Some(SpacePoint {
            space: target,
            point: matrix.transform(point.point),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1.0e-4
    }

    #[test]
    fn identity_transform_preserves_point() {
        let transform = build_transform(GeometryParameters::default());
        let point = transform.transform(Point2 { x: 0.2, y: 0.8 });
        assert!(close(point.x, 0.2));
        assert!(close(point.y, 0.8));
    }

    #[test]
    fn inverse_round_trip_restores_point() {
        let parameters = GeometryParameters {
            rotation_degrees: 13.0,
            vertical_keystone: 0.12,
            horizontal_keystone: -0.08,
            scale: 1.1,
            offset_x: 0.03,
            offset_y: -0.02,
            ..Default::default()
        };
        let transform = build_transform(parameters);
        let inverse = transform.inverse().expect("invertible");
        let source = Point2 { x: 0.33, y: 0.72 };
        let projected = transform.transform(source);
        let restored = inverse.transform(projected);
        assert!(close(source.x, restored.x));
        assert!(close(source.y, restored.y));
    }

    #[test]
    fn horizontal_flip_mirrors_around_center() {
        let transform = build_transform(GeometryParameters {
            flip_horizontal: true,
            ..Default::default()
        });
        let output = transform.transform(Point2 { x: 0.2, y: 0.5 });
        assert!(close(output.x, 0.8));
        assert!(close(output.y, 0.5));
    }

    #[test]
    fn crop_is_clamped_to_valid_normalized_rectangle() {
        let crop = CropRect {
            left: -0.2,
            top: 0.1,
            right: 1.4,
            bottom: 0.9,
        }
        .normalized();
        assert_eq!(crop.left, 0.0);
        assert_eq!(crop.top, 0.1);
        assert_eq!(crop.right, 1.0);
        assert_eq!(crop.bottom, 0.9);
    }

    #[test]
    fn m11_four_point_homography_rectifies_all_corners() {
        let points = FourPoint {
            top_left: Point2 { x: 0.1, y: 0.05 },
            top_right: Point2 { x: 0.9, y: 0.12 },
            bottom_right: Point2 { x: 0.82, y: 0.92 },
            bottom_left: Point2 { x: 0.18, y: 0.84 },
        };
        let transform = homography_from_four_point(points).expect("homography");
        for (source, expected) in [
            (points.top_left, Point2 { x: 0.0, y: 0.0 }),
            (points.top_right, Point2 { x: 1.0, y: 0.0 }),
            (points.bottom_right, Point2 { x: 1.0, y: 1.0 }),
            (points.bottom_left, Point2 { x: 0.0, y: 1.0 }),
        ] {
            let actual = transform.transform(source);
            assert!(close(actual.x, expected.x));
            assert!(close(actual.y, expected.y));
        }
    }

    #[test]
    fn m11_crop_ratio_and_geometry_resample_are_finite() {
        let crop = constrain_crop_aspect(CropRect::default(), 12, 8, 1.0, 1.0);
        assert!(close(
            (crop.right - crop.left) * 12.0,
            (crop.bottom - crop.top) * 8.0
        ));
        let data: Vec<f32> = (0..96)
            .flat_map(|index| {
                let value = index as f32 / 96.0;
                [value, value, value]
            })
            .collect();
        let result = apply_geometry(
            12,
            8,
            &data,
            GeometryParameters {
                rotation_degrees: 7.5,
                vertical_keystone: 0.12,
                crop_aspect_width: 1.0,
                crop_aspect_height: 1.0,
                ..Default::default()
            },
        )
        .expect("geometry");
        assert_eq!(result.width, result.height);
        assert!(result.data.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn m11_original_ratio_is_distinct_from_free_crop() {
        let crop = CropRect {
            left: 0.1,
            top: 0.1,
            right: 0.9,
            bottom: 0.7,
        };
        let free = constrain_crop_aspect(crop, 12, 8, 0.0, 0.0);
        let original = constrain_crop_aspect(crop, 12, 8, -1.0, -1.0);
        assert_eq!(free, crop);
        assert!(close(
            original.right - original.left,
            original.bottom - original.top
        ));
    }

    #[test]
    fn m11_upright_is_derived_from_image_edges() {
        let width = 32;
        let height = 24;
        let mut data = vec![0.05_f32; width * height * 3];
        // A sloped high-contrast horizon is actual analysis input, not a fixed mode preset.
        for x in 0..width {
            let y_line = 6 + x / 6;
            for y in y_line..height {
                for channel in 0..3 {
                    data[(y * width + x) * 3 + channel] = 0.8;
                }
            }
        }
        let analysis = analyze_upright(width, height, &data, UprightMode::Level);
        assert!(analysis.line_samples > 0);
        assert!(analysis.rotation_degrees.abs() > 1.0);
        assert_eq!(analysis.vertical_keystone, 0.0);
        assert_eq!(analysis.horizontal_keystone, 0.0);
    }

    #[test]
    fn m11_coordinate_spaces_are_explicit_and_composable() {
        let mapper = CoordinateMapper {
            source_to_post_lens: Matrix3::IDENTITY,
            post_lens_to_geometry: build_transform(GeometryParameters {
                offset_x: 0.1,
                ..Default::default()
            }),
            geometry_to_viewport: Matrix3::IDENTITY,
        };
        let mapped = mapper
            .map(
                SpacePoint {
                    space: CoordinateSpace::SourceSensor,
                    point: Point2 { x: 0.2, y: 0.3 },
                },
                CoordinateSpace::Viewport,
            )
            .expect("mapping");
        assert_eq!(mapped.space, CoordinateSpace::Viewport);
        assert!(close(mapped.point.x, 0.3));
    }
}
