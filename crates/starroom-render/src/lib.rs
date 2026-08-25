//! Render-graph primitives for Starroom.
//! The graph is backend-neutral: CPU reference, wgpu preview and tiled export must share the
//! same logical stage order and invalidation rules.

pub mod gpu;
pub mod profiling;
pub mod scheduler;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StageId {
    Decode,
    InputTransform,
    WhiteBalance,
    AiDenoise,
    Exposure,
    Tone,
    Curve,
    ColorMixer,
    ColorGrading,
    Mask,
    Layers,
    Skin,
    Healing,
    Detail,
    Optics,
    Geometry,
    Resize,
    DisplayTransform,
    Encode,
    Export,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageNode {
    pub id: StageId,
    pub dependencies: Vec<StageId>,
    pub halo_pixels: u32,
    pub tile_safe: bool,
    pub cpu_supported: bool,
    pub gpu_supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraph {
    pub stages: Vec<StageNode>,
}

impl Default for RenderGraph {
    fn default() -> Self {
        use StageId::*;
        let linear = [
            Decode,
            InputTransform,
            WhiteBalance,
            AiDenoise,
            Exposure,
            Tone,
            Curve,
            ColorMixer,
            ColorGrading,
            Mask,
            Layers,
            Skin,
            Healing,
            Detail,
            Optics,
            Geometry,
            Resize,
            DisplayTransform,
            Encode,
            Export,
        ];
        let mut stages = Vec::with_capacity(linear.len());
        for (index, id) in linear.into_iter().enumerate() {
            let dependencies = if index == 0 {
                Vec::new()
            } else {
                vec![linear[index - 1]]
            };
            let (halo_pixels, tile_safe) = match id {
                AiDenoise | Detail | Skin | Healing => (32, true),
                Optics | Geometry => (4, true),
                _ => (0, true),
            };
            stages.push(StageNode {
                id,
                dependencies,
                halo_pixels,
                tile_safe,
                cpu_supported: true,
                gpu_supported: !matches!(id, Decode | Export),
            });
        }
        Self { stages }
    }
}

impl RenderGraph {
    pub fn node(&self, id: StageId) -> Option<&StageNode> {
        self.stages.iter().find(|stage| stage.id == id)
    }

    pub fn validate(&self) -> Result<(), GraphError> {
        let ids: BTreeSet<StageId> = self.stages.iter().map(|stage| stage.id).collect();
        if ids.len() != self.stages.len() {
            return Err(GraphError::DuplicateStage);
        }
        for stage in &self.stages {
            for dependency in &stage.dependencies {
                if !ids.contains(dependency) {
                    return Err(GraphError::MissingDependency {
                        stage: stage.id,
                        dependency: *dependency,
                    });
                }
            }
        }
        let mut indegree: BTreeMap<StageId, usize> = ids.iter().map(|id| (*id, 0)).collect();
        let mut downstream: BTreeMap<StageId, Vec<StageId>> = BTreeMap::new();
        for stage in &self.stages {
            for dependency in &stage.dependencies {
                *indegree.entry(stage.id).or_default() += 1;
                downstream.entry(*dependency).or_default().push(stage.id);
            }
        }
        let mut queue: VecDeque<StageId> = indegree
            .iter()
            .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
            .collect();
        let mut visited = 0usize;
        while let Some(stage) = queue.pop_front() {
            visited += 1;
            for next in downstream.get(&stage).into_iter().flatten() {
                let degree = indegree.get_mut(next).expect("known stage");
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(*next);
                }
            }
        }
        if visited != self.stages.len() {
            return Err(GraphError::Cycle);
        }
        Ok(())
    }

    pub fn invalidate_from(&self, changed: StageId) -> BTreeSet<StageId> {
        let mut downstream: BTreeMap<StageId, Vec<StageId>> = BTreeMap::new();
        for stage in &self.stages {
            for dependency in &stage.dependencies {
                downstream.entry(*dependency).or_default().push(stage.id);
            }
        }
        let mut invalid = BTreeSet::from([changed]);
        let mut queue = VecDeque::from([changed]);
        while let Some(current) = queue.pop_front() {
            for next in downstream.get(&current).into_iter().flatten() {
                if invalid.insert(*next) {
                    queue.push_back(*next);
                }
            }
        }
        invalid
    }

    pub fn maximum_halo(&self) -> u32 {
        self.stages
            .iter()
            .map(|stage| stage.halo_pixels)
            .max()
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    DuplicateStage,
    MissingDependency { stage: StageId, dependency: StageId },
    Cycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderTile {
    /// Final region written to the output image.
    pub output: PixelRect,
    /// Expanded source region requested from upstream to satisfy neighborhood filters.
    pub input_with_halo: PixelRect,
}

/// Plans non-overlapping output tiles with clipped halo regions. A renderer computes the larger
/// input region, then crops back to `output` when assembling the final image.
pub fn plan_tiles(width: u32, height: u32, tile_size: u32, halo: u32) -> Vec<RenderTile> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let tile_size = tile_size.max(64);
    let mut tiles = Vec::new();
    let mut y = 0;
    while y < height {
        let output_height = tile_size.min(height - y);
        let mut x = 0;
        while x < width {
            let output_width = tile_size.min(width - x);
            let input_x = x.saturating_sub(halo);
            let input_y = y.saturating_sub(halo);
            let input_right = (x + output_width).saturating_add(halo).min(width);
            let input_bottom = (y + output_height).saturating_add(halo).min(height);
            tiles.push(RenderTile {
                output: PixelRect {
                    x,
                    y,
                    width: output_width,
                    height: output_height,
                },
                input_with_halo: PixelRect {
                    x: input_x,
                    y: input_y,
                    width: input_right - input_x,
                    height: input_bottom - input_y,
                },
            });
            x = x.saturating_add(tile_size);
        }
        y = y.saturating_add(tile_size);
    }
    tiles
}

pub fn stage_cache_key(
    stage: StageId,
    source_hash: &str,
    parameter_json: &str,
    upstream_keys: &[String],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{stage:?}\0{source_hash}\0{parameter_json}\0"));
    for key in upstream_keys {
        hasher.update(key.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_graph_is_valid_and_acyclic() {
        let graph = RenderGraph::default();
        assert_eq!(graph.validate(), Ok(()));
        assert!(graph.maximum_halo() >= 32);
    }

    #[test]
    fn changing_color_mixer_does_not_invalidate_decode_or_white_balance() {
        let graph = RenderGraph::default();
        let invalid = graph.invalidate_from(StageId::ColorMixer);
        assert!(invalid.contains(&StageId::ColorMixer));
        assert!(invalid.contains(&StageId::DisplayTransform));
        assert!(invalid.contains(&StageId::Export));
        assert!(!invalid.contains(&StageId::Decode));
        assert!(!invalid.contains(&StageId::WhiteBalance));
    }

    #[test]
    fn changing_white_balance_invalidates_all_later_creative_stages() {
        let graph = RenderGraph::default();
        let invalid = graph.invalidate_from(StageId::WhiteBalance);
        assert!(invalid.contains(&StageId::Tone));
        assert!(invalid.contains(&StageId::Layers));
        assert!(invalid.contains(&StageId::Geometry));
        assert!(invalid.contains(&StageId::Export));
        assert!(!invalid.contains(&StageId::Decode));
    }

    #[test]
    fn cache_key_changes_with_parameters_but_is_stable_for_same_inputs() {
        let a = stage_cache_key(
            StageId::Tone,
            "source",
            "{\"shadows\":0.2}",
            &["upstream".into()],
        );
        let b = stage_cache_key(
            StageId::Tone,
            "source",
            "{\"shadows\":0.2}",
            &["upstream".into()],
        );
        let c = stage_cache_key(
            StageId::Tone,
            "source",
            "{\"shadows\":0.3}",
            &["upstream".into()],
        );
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn tile_plan_covers_entire_frame_without_output_overlap_gaps() {
        let tiles = plan_tiles(1000, 700, 256, 32);
        let area: u64 = tiles
            .iter()
            .map(|tile| u64::from(tile.output.width) * u64::from(tile.output.height))
            .sum();
        assert_eq!(area, 700_000);
        assert!(
            tiles
                .iter()
                .all(|tile| tile.input_with_halo.width >= tile.output.width)
        );
        assert!(
            tiles
                .iter()
                .all(|tile| tile.input_with_halo.height >= tile.output.height)
        );
    }

    #[test]
    fn edge_tiles_clip_halo_to_image_bounds() {
        let tiles = plan_tiles(300, 300, 256, 64);
        let first = tiles.first().expect("first tile");
        assert_eq!(first.input_with_halo.x, 0);
        assert_eq!(first.input_with_halo.y, 0);
        let last = tiles.last().expect("last tile");
        assert_eq!(last.input_with_halo.x + last.input_with_halo.width, 300);
        assert_eq!(last.input_with_halo.y + last.input_with_halo.height, 300);
    }
}
