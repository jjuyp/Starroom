//! Bounded derived pixel-stage reuse. Export never consumes these preview buffers.
use super::*;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, Weak},
};

#[derive(Clone, PartialEq)]
struct PreparationState {
    color: ColorManagementSettings,
    white_balance: WhiteBalanceSettings,
    optics: OpticsSettings,
    geometry: GeometryParameters,
    source_region: Option<SourceRegion>,
}

impl PreparationState {
    fn new(settings: &RenderSettings) -> Self {
        let mut white_balance = source_white_balance(settings.white_balance);
        if white_balance.mode != WhiteBalanceMode::NeutralPicker {
            white_balance.sample = None;
        }
        Self {
            color: settings.color_management,
            white_balance,
            optics: if settings.optics.parameters.enabled {
                settings.optics.clone()
            } else {
                OpticsSettings::default()
            },
            geometry: settings.geometry,
            source_region: settings.source_region,
        }
    }
}

struct Entry {
    source: Weak<DecodedSourceImage>,
    state: PreparationState,
    prepared: Arc<PreparedRenderSource>,
}

#[derive(Default, Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePreparationCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub builds: u64,
    pub bypasses: u64,
    pub evictions: u64,
    pub expired: u64,
    pub entries: usize,
    /// Actual retained RGB Vec capacities; excludes metadata and caller working copies.
    pub image_buffer_bytes: u64,
}

#[derive(Default)]
struct Inner {
    entries: VecDeque<Entry>,
    stats: SourcePreparationCacheStats,
}

fn image_bytes(prepared: &PreparedRenderSource) -> u64 {
    (prepared.image.data.capacity() as u64).saturating_mul(F32_BYTES)
}

impl Inner {
    fn stats(&self) -> SourcePreparationCacheStats {
        SourcePreparationCacheStats {
            entries: self.entries.len(),
            image_buffer_bytes: self
                .entries
                .iter()
                .map(|entry| image_bytes(&entry.prepared))
                .sum(),
            ..self.stats
        }
    }
}

/// Immutable decoded Arc identity includes pixel/profile/metadata identity without rehashing a
/// frame. Weak references cannot pin decoded sources or alias a recycled allocation. All actual
/// upstream settings are compared exactly; downstream creative edits are deliberately absent.
pub struct SourcePreparationCache {
    inner: Mutex<Inner>,
    budget: u64,
}

impl Default for SourcePreparationCache {
    fn default() -> Self {
        Self::new(128 * 1024 * 1024)
    }
}

impl SourcePreparationCache {
    pub fn new(budget: u64) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            budget,
        }
    }
    pub fn stats(&self) -> Result<SourcePreparationCacheStats, PipelineError> {
        self.inner
            .lock()
            .map(|inner| inner.stats())
            .map_err(|_| PipelineError::PreparationCacheUnavailable)
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Inner>, PipelineError> {
        self.inner
            .lock()
            .map_err(|_| PipelineError::PreparationCacheUnavailable)
    }
}

pub fn render_source_preview_with_preparation_cache_to_srgb8(
    decoded: &Arc<DecodedSourceImage>,
    settings: &RenderSettings,
    gpu: Option<&GpuRenderer>,
    cache: &SourcePreparationCache,
) -> Result<RenderedRgb8, PipelineError> {
    checkpoint()?;
    if settings.white_balance.mode == WhiteBalanceMode::NeutralPicker
        && settings
            .white_balance
            .sample
            .is_none_or(|sample| !sample.validated())
    {
        return Err(PipelineError::InvalidWhiteBalanceSample);
    }
    // Never add a retained full-frame buffer and clone for an oversized source. This executes
    // the same shared graph with owned temporaries, not a lower-quality or alternate renderer.
    let estimated = u64::from(decoded.width())
        .saturating_mul(u64::from(decoded.height()))
        .saturating_mul(12);
    if estimated > cache.budget {
        let mut inner = cache.lock()?;
        inner.stats.bypasses = inner.stats.bypasses.saturating_add(1);
        drop(inner);
        return render_shared_source_graph(decoded, settings, None, gpu)
            .map(RenderedRgbF32::into_rgb8);
    }
    let source = Arc::downgrade(decoded);
    let state = PreparationState::new(settings);
    let hit = {
        let mut inner = cache.lock()?;
        let before = inner.entries.len();
        inner
            .entries
            .retain(|entry| entry.source.strong_count() > 0);
        inner.stats.expired = inner
            .stats
            .expired
            .saturating_add((before - inner.entries.len()) as u64);
        if let Some(index) = inner
            .entries
            .iter()
            .position(|entry| entry.source.ptr_eq(&source) && entry.state == state)
        {
            let entry = inner
                .entries
                .remove(index)
                .ok_or(PipelineError::PreparationCacheUnavailable)?;
            let prepared = Arc::clone(&entry.prepared);
            inner.entries.push_back(entry);
            inner.stats.hits = inner.stats.hits.saturating_add(1);
            Some(prepared)
        } else {
            inner.stats.misses = inner.stats.misses.saturating_add(1);
            None
        }
    };
    let prepared = if let Some(prepared) = hit {
        profiling::record_cache(true);
        // RAW input is already working RGB from the decoder; do not invent a camera/ICC
        // execution at this preparation boundary that the uncached path never performs.
        if matches!(decoded.as_ref(), DecodedSourceImage::Rendered(_)) {
            profiling::record_stage_cache_hit(ProfileStage::CameraTransform);
        }
        for stage in [ProfileStage::WhiteBalance, ProfileStage::Geometry] {
            profiling::record_stage_cache_hit(stage);
        }
        if settings.optics.parameters.enabled {
            profiling::record_stage_cache_hit(ProfileStage::Lens);
        }
        prepared
    } else {
        profiling::record_cache(false);
        let mut prepared = prepare_render_source(decoded, settings, false)?;
        let bytes = image_bytes(&prepared);
        let mut inner = cache.lock()?;
        inner.stats.builds = inner.stats.builds.saturating_add(1);
        if bytes > cache.budget {
            inner.stats.bypasses = inner.stats.bypasses.saturating_add(1);
            drop(inner);
            apply_visible_white_balance(&mut prepared.image, settings)?;
            return render_prepared_source(prepared, settings, None, gpu)
                .map(RenderedRgbF32::into_rgb8);
        }
        // Concurrent cold builds may complete in either order; keep only one exact stage entry.
        if let Some(entry) = inner
            .entries
            .iter()
            .find(|entry| entry.source.ptr_eq(&source) && entry.state == state)
        {
            Arc::clone(&entry.prepared)
        } else {
            while inner.entries.len() >= 4
                || inner.stats().image_buffer_bytes + bytes > cache.budget
            {
                inner.entries.pop_front();
                inner.stats.evictions = inner.stats.evictions.saturating_add(1);
            }
            let prepared = Arc::new(prepared);
            inner.entries.push_back(Entry {
                source,
                state,
                prepared: Arc::clone(&prepared),
            });
            prepared
        }
    };
    checkpoint()?;
    // Processing still owns its mutable working image; the immutable cached stage is untouched.
    let mut image = prepared.image.clone();
    apply_visible_white_balance(&mut image, settings)?;
    render_prepared_working_graph(
        image,
        prepared.input,
        prepared.camera_profile.as_ref(),
        settings,
        None,
        gpu,
        prepared.semantic_map,
    )
    .map(RenderedRgbF32::into_rgb8)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Arc<DecodedSourceImage> {
        Arc::new(DecodedSourceImage::Rendered(DecodedRenderedImage {
            width: 4,
            height: 2,
            format: starroom_imageio::RenderedFormat::Png,
            rgba: (0..8)
                .flat_map(|index| [0.1 + index as f32 * 0.08, 0.2, 0.3, 1.0])
                .collect(),
            embedded_icc: None,
            exif: None,
        }))
    }

    #[test]
    fn source_preparation_reuses_pixels_and_reports_skipped_stage_execution() {
        let source = source();
        let cache = SourcePreparationCache::default();
        let mut settings = RenderSettings::default();
        settings.geometry.rotation_degrees = 90.0;
        let original = source.as_ref().clone();
        for exposure in [0.0, 0.5, 1.0, -1.0] {
            settings.tone.exposure_ev = exposure;
            settings.relative_color.temperature = exposure * 0.1;
            let (rendered, profile) = profiling::capture(|| {
                render_source_preview_with_preparation_cache_to_srgb8(
                    &source, &settings, None, &cache,
                )
            });
            assert_eq!(
                rendered.unwrap(),
                render_source_export_to_srgb8(&source, &settings).unwrap()
            );
            if exposure != 0.0 {
                assert_eq!(profile.stages[&ProfileStage::CameraTransform].executions, 0);
                assert_eq!(profile.stages[&ProfileStage::CameraTransform].cache_hits, 1);
                assert_eq!(profile.stages[&ProfileStage::Geometry].executions, 0);
                assert_eq!(profile.stages[&ProfileStage::Geometry].cache_hits, 1);
            }
        }
        let stats = cache.stats().unwrap();
        assert_eq!(stats.builds, 1);
        assert_eq!(stats.hits, 3);
        assert_eq!(source.as_ref(), &original);
    }

    #[test]
    fn source_preparation_invalidates_real_upstream_state_and_source_allocation() {
        let source = source();
        let cache = SourcePreparationCache::default();
        let mut settings = RenderSettings::default();
        for kind in 0..4 {
            match kind {
                1 => settings.white_balance.mode = WhiteBalanceMode::Auto,
                2 => settings.geometry.rotation_degrees = 90.0,
                3 => settings.color_management.black_point_compensation = false,
                _ => {}
            }
            let rendered = render_source_preview_with_preparation_cache_to_srgb8(
                &source, &settings, None, &cache,
            )
            .unwrap();
            assert_eq!(
                rendered,
                render_source_export_to_srgb8(&source, &settings).unwrap()
            );
        }
        assert_eq!(cache.stats().unwrap().builds, 4);
        let new_source = Arc::new(source.as_ref().clone());
        render_source_preview_with_preparation_cache_to_srgb8(&new_source, &settings, None, &cache)
            .unwrap();
        assert_eq!(cache.stats().unwrap().builds, 5);
        assert_eq!(cache.stats().unwrap().entries, 4);
        assert_eq!(cache.stats().unwrap().evictions, 1);
    }

    #[test]
    fn source_preparation_budget_bypass_does_not_pin_sources_or_add_cached_image_copy() {
        let source = source();
        let settings = RenderSettings::default();
        let tiny = SourcePreparationCache::new(1);
        let result =
            render_source_preview_with_preparation_cache_to_srgb8(&source, &settings, None, &tiny)
                .unwrap();
        assert_eq!(
            result,
            render_source_export_to_srgb8(&source, &settings).unwrap()
        );
        assert_eq!(tiny.stats().unwrap().entries, 0);
        assert_eq!(tiny.stats().unwrap().bypasses, 1);
        let cache = SourcePreparationCache::default();
        render_source_preview_with_preparation_cache_to_srgb8(&source, &settings, None, &cache)
            .unwrap();
        assert_eq!(Arc::strong_count(&source), 1);
        let weak = Arc::downgrade(&source);
        drop(source);
        assert!(weak.upgrade().is_none());
        let another = self::source();
        render_source_preview_with_preparation_cache_to_srgb8(&another, &settings, None, &cache)
            .unwrap();
        assert_eq!(cache.stats().unwrap().expired, 1);
        assert_eq!(cache.stats().unwrap().entries, 1);
        assert!(cache.stats().unwrap().image_buffer_bytes <= cache.budget);
    }

    #[test]
    fn source_preparation_picker_and_source_region_are_not_stale_and_invalid_picker_is_not_cached()
    {
        let source = source();
        let cache = SourcePreparationCache::default();
        let mut settings = RenderSettings::default();
        settings.white_balance.mode = WhiteBalanceMode::NeutralPicker;
        assert!(
            render_source_preview_with_preparation_cache_to_srgb8(&source, &settings, None, &cache)
                .is_err()
        );
        assert_eq!(cache.stats().unwrap().entries, 0);
        for x in [0.0, 0.5] {
            settings.white_balance.sample = Some(WhiteBalanceSample {
                x,
                y: 0.0,
                width: 0.3,
                height: 0.5,
            });
            assert_eq!(
                render_source_preview_with_preparation_cache_to_srgb8(
                    &source, &settings, None, &cache
                )
                .unwrap(),
                render_source_export_to_srgb8(&source, &settings).unwrap()
            );
        }
        settings.white_balance = WhiteBalanceSettings::default();
        settings.source_region = Some(SourceRegion {
            full_width: 8,
            full_height: 4,
            x: 1,
            y: 1,
        });
        render_source_preview_with_preparation_cache_to_srgb8(&source, &settings, None, &cache)
            .unwrap();
        settings.source_region.as_mut().unwrap().x = 2;
        render_source_preview_with_preparation_cache_to_srgb8(&source, &settings, None, &cache)
            .unwrap();
        assert_eq!(cache.stats().unwrap().builds, 3);
    }
}
