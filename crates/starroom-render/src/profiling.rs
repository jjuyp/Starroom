//! Low-overhead, thread-local measurements for the production shared graph.

use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap, time::Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfileStage {
    RawDecode,
    CameraTransform,
    WhiteBalance,
    AiDenoise,
    Tone,
    Curve,
    ColorMixer,
    ColorGrading,
    Detail,
    Lens,
    Geometry,
    Mask,
    AiMask,
    Skin,
    Healing,
    Resize,
    ColorTransform,
    Encode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageMeasurement {
    pub cpu_nanoseconds: u64,
    pub gpu_nanoseconds: Option<u64>,
    pub peak_working_bytes: u64,
    pub executions: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderProfile {
    pub stages: BTreeMap<ProfileStage, StageMeasurement>,
    pub total_cpu_nanoseconds: u64,
    pub peak_working_bytes: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

thread_local! {
    static ACTIVE: RefCell<Option<ActiveProfile>> = const { RefCell::new(None) };
}

struct ActiveProfile {
    started: Instant,
    report: RenderProfile,
}

/// Measures the actual graph executed by `work`. Nested profiling is intentionally rejected so a
/// caller cannot accidentally double-count stage time.
pub fn capture<T>(work: impl FnOnce() -> T) -> (T, RenderProfile) {
    ACTIVE.with(|active| {
        assert!(
            active.borrow().is_none(),
            "nested render profiling is unsupported"
        );
        *active.borrow_mut() = Some(ActiveProfile {
            started: Instant::now(),
            report: RenderProfile::default(),
        });
    });
    let value = work();
    let mut profile = ACTIVE.with(|active| {
        let active = active.borrow_mut().take().expect("active render profile");
        let mut report = active.report;
        report.total_cpu_nanoseconds = nanos(active.started.elapsed().as_nanos());
        report
    });
    profile.peak_working_bytes = profile
        .stages
        .values()
        .map(|stage| stage.peak_working_bytes)
        .max()
        .unwrap_or(0);
    (value, profile)
}

pub fn measure<T>(stage: ProfileStage, working_bytes: u64, work: impl FnOnce() -> T) -> T {
    let enabled = ACTIVE.with(|active| active.borrow().is_some());
    if !enabled {
        return work();
    }
    let started = Instant::now();
    let value = work();
    let elapsed = nanos(started.elapsed().as_nanos());
    ACTIVE.with(|active| {
        if let Some(active) = active.borrow_mut().as_mut() {
            let entry = active.report.stages.entry(stage).or_default();
            entry.cpu_nanoseconds = entry.cpu_nanoseconds.saturating_add(elapsed);
            entry.peak_working_bytes = entry.peak_working_bytes.max(working_bytes);
            entry.executions = entry.executions.saturating_add(1);
        }
    });
    value
}

pub fn record_gpu(stage: ProfileStage, elapsed_nanoseconds: u64) {
    ACTIVE.with(|active| {
        if let Some(active) = active.borrow_mut().as_mut() {
            let entry = active.report.stages.entry(stage).or_default();
            entry.gpu_nanoseconds = Some(
                entry
                    .gpu_nanoseconds
                    .unwrap_or(0)
                    .saturating_add(elapsed_nanoseconds),
            );
        }
    });
}

pub fn record_cache(hit: bool) {
    ACTIVE.with(|active| {
        if let Some(active) = active.borrow_mut().as_mut() {
            if hit {
                active.report.cache_hits = active.report.cache_hits.saturating_add(1);
            } else {
                active.report.cache_misses = active.report.cache_misses.saturating_add(1);
            }
        }
    });
}

const fn nanos(value: u128) -> u64 {
    if value > u64::MAX as u128 {
        u64::MAX
    } else {
        value as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_real_stage_time_memory_and_cache_counts() {
        let (value, profile) = capture(|| {
            record_cache(false);
            let value = measure(ProfileStage::Tone, 4096, || 42);
            record_cache(true);
            value
        });
        assert_eq!(value, 42);
        assert_eq!(profile.cache_hits, 1);
        assert_eq!(profile.cache_misses, 1);
        assert_eq!(profile.peak_working_bytes, 4096);
        assert_eq!(profile.stages[&ProfileStage::Tone].executions, 1);
        assert!(
            profile.total_cpu_nanoseconds >= profile.stages[&ProfileStage::Tone].cpu_nanoseconds
        );
    }
}
