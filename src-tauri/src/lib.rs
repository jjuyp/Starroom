use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use starroom_advisor::{
    AdvisorResult, AnalysisStats, Suggestion, advise, advise_detailed, analyze_detailed,
};
use starroom_ai_denoise::{
    AiDenoiseParameters, AiDenoiseResidual, ExecutionProvider as DenoiseExecutionProvider,
    MODEL_ID as NAFNET_MODEL_ID, MODEL_SHA256 as NAFNET_MODEL_SHA256,
    MODEL_VERSION as NAFNET_MODEL_VERSION, NafNetOnnxProvider,
    directml_failure_allows_cpu_fallback, infer_tiled, inference_cache_key,
};
use starroom_color::{ColorMixer, CurvePoint, ToneParameters};
use starroom_detail::{DenoiseParameters, LocalDetailParameters, SharpenParameters};
use starroom_export::{
    BatchExportResult, BatchProgress, ExportItemResult, ExportItemStatus,
    ExportRequest as ProfessionalExportRequest, ExportSettings, NativeSharedGraphRenderer,
    export_one, export_recipe_identity,
};
use starroom_geometry::GeometryParameters;
use starroom_grading::GradingParameters;
use starroom_heal::HealingOperation;
use starroom_history::{EditCommand, EditHistory, HistoryEntry, NamedSnapshot};
use starroom_imageio::{
    DecodedSourceImage, decode_source, decode_source_preview, encode_jpeg_rgb8,
};
use starroom_library::{
    AssetFlag, AssetRecord, CollectionKind, CollectionRecord, ColorLabel, ImportResult, Library,
    LibraryQuery, SmartCollectionRuleV1, ThumbnailSize,
};
use starroom_look::{
    GrainSettings, PortableCurves, PortableLook, PortableRelativeColor, VignetteSettings, blend,
    mix_weighted,
};
use starroom_optics::{LensProfileResolution, OpticsSettings};
use starroom_pipeline::{
    GeneratedMaskRaster, NativeAdjustmentLayer, PortraitMaskRaster, RelativeColorParameters,
    RenderSettings, SkinRetouchSettings, ToneCurveSet, WhiteBalanceMode, WhiteBalanceSample,
    WhiteBalanceSettings, prepare_source_for_ai_denoise, render_source_export_to_srgb8,
    render_source_preview_to_srgb8, render_source_preview_with_gpu_to_srgb8,
    resolve_source_lens_profile, sample_source_color_band,
};
use starroom_portrait::{
    AiMaskError, AiMaskModelRegistry, AiMaskOnnxProvider, AiMaskProvider, AiMaskSemantic,
    DetectedFace, GeneratedAiMask, PortraitError, PortraitModelRegistry, PortraitOnnxProvider,
    PortraitParseResult, PortraitRegion, cancellation_token,
};
use starroom_project::{GeneratedMaskSemantic, MaskDefinition, MaskTree, PortraitMaskRegion};
use starroom_reference::{ReferenceAnalysis, ReferenceMatchRecipe, analyze, match_reference};
use starroom_render::{
    RenderGraph,
    gpu::{GpuBackendKind, GpuRenderer, GpuStatus, probe_gpu_status},
    scheduler::{
        Completion, DEFAULT_TILE_EDGE, RenderCacheIdentity, RenderScheduler, SchedulerStatus,
        Viewport,
    },
};
use std::path::{Path, PathBuf};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::State;
use tauri::ipc::Response;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineCapabilities {
    version: &'static str,
    native_tone_reference: bool,
    oklab_oklch: bool,
    color_mixer: bool,
    color_grading: bool,
    render_graph: bool,
    layer_mask_schema: bool,
    local_advisor: bool,
    portrait_reference: bool,
    healing_reference: bool,
    gpu_renderer: bool,
    raw_pipeline: bool,
    ai_denoise: bool,
    reference_match: bool,
    portable_looks: bool,
    local_library: bool,
    persistent_history: bool,
    professional_export: bool,
}

#[tauri::command]
fn engine_status() -> &'static str {
    "V0_2_CORE_QUALITY"
}

#[tauri::command]
fn engine_capabilities() -> EngineCapabilities {
    EngineCapabilities {
        version: "0.2.0",
        native_tone_reference: true,
        oklab_oklch: true,
        color_mixer: true,
        color_grading: true,
        render_graph: RenderGraph::default().validate().is_ok(),
        layer_mask_schema: true,
        local_advisor: true,
        portrait_reference: true,
        healing_reference: true,
        gpu_renderer: true,
        raw_pipeline: true,
        ai_denoise: true,
        reference_match: true,
        portable_looks: true,
        local_library: true,
        persistent_history: true,
        professional_export: true,
    }
}

#[derive(Clone, Default)]
struct NativeLibraryRuntime {
    library: Arc<Mutex<Option<Library>>>,
    cancel_import: Arc<AtomicBool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryOpenResult {
    path: PathBuf,
    schema_version: i64,
}

fn default_library_path() -> Result<PathBuf, String> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "DatabaseOpenFailed: LOCALAPPDATA is unavailable".to_owned())?;
    Ok(local.join("Starroom").join("starroom-library.sqlite"))
}

#[tauri::command]
fn library_open_default(
    runtime: State<'_, NativeLibraryRuntime>,
) -> Result<LibraryOpenResult, String> {
    let path = default_library_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("DatabaseOpenFailed: {error}"))?;
    }
    let library = Library::open(&path).map_err(|error| error.to_string())?;
    let schema_version = library
        .schema_version()
        .map_err(|error| error.to_string())?;
    *runtime
        .library
        .lock()
        .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())? = Some(library);
    Ok(LibraryOpenResult {
        path,
        schema_version,
    })
}

#[tauri::command]
async fn library_import_folder(
    runtime: State<'_, NativeLibraryRuntime>,
    root: PathBuf,
) -> Result<ImportResult, String> {
    let runtime = runtime.inner().clone();
    runtime.cancel_import.store(false, Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || {
        let paths = Library::recursive_paths(&root).map_err(|error| error.to_string())?;
        let mut guard = runtime
            .library
            .lock()
            .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
        let library = guard
            .as_mut()
            .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?;
        library
            .import_paths(&paths, &runtime.cancel_import)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("ImportCancelled: worker failed: {error}"))?
}

#[tauri::command]
fn library_cancel_import(runtime: State<'_, NativeLibraryRuntime>) -> bool {
    runtime.cancel_import.store(true, Ordering::Relaxed);
    true
}

#[tauri::command]
fn library_query(
    runtime: State<'_, NativeLibraryRuntime>,
    query: LibraryQuery,
) -> Result<Vec<AssetRecord>, String> {
    let guard = runtime
        .library
        .lock()
        .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
    guard
        .as_ref()
        .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?
        .query(&query)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryWorkflowRequest {
    asset_ids: Vec<i64>,
    rating: Option<u8>,
    flag: Option<AssetFlag>,
    color_label: Option<ColorLabel>,
}

#[tauri::command]
fn library_set_workflow(
    runtime: State<'_, NativeLibraryRuntime>,
    request: LibraryWorkflowRequest,
) -> Result<(), String> {
    let guard = runtime
        .library
        .lock()
        .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
    guard
        .as_ref()
        .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?
        .set_workflow(
            &request.asset_ids,
            request.rating,
            request.flag,
            request.color_label,
        )
        .map_err(|error| error.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryKeywordRequest {
    asset_ids: Vec<i64>,
    names: Vec<String>,
}

#[tauri::command]
fn library_add_keywords(
    runtime: State<'_, NativeLibraryRuntime>,
    request: LibraryKeywordRequest,
) -> Result<(), String> {
    let mut guard = runtime
        .library
        .lock()
        .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
    guard
        .as_mut()
        .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?
        .add_keywords(&request.asset_ids, &request.names)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn library_remove_keywords(
    runtime: State<'_, NativeLibraryRuntime>,
    request: LibraryKeywordRequest,
) -> Result<(), String> {
    let mut guard = runtime
        .library
        .lock()
        .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
    guard
        .as_mut()
        .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?
        .remove_keywords(&request.asset_ids, &request.names)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryCollectionCreateRequest {
    name: String,
    kind: CollectionKind,
    rule: Option<SmartCollectionRuleV1>,
}

#[tauri::command]
fn library_collections(
    runtime: State<'_, NativeLibraryRuntime>,
) -> Result<Vec<CollectionRecord>, String> {
    let guard = runtime
        .library
        .lock()
        .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
    guard
        .as_ref()
        .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?
        .collections()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn library_collection_create(
    runtime: State<'_, NativeLibraryRuntime>,
    request: LibraryCollectionCreateRequest,
) -> Result<i64, String> {
    let mut guard = runtime
        .library
        .lock()
        .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
    guard
        .as_mut()
        .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?
        .create_collection(&request.name, request.kind, request.rule.as_ref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn library_collection_add_assets(
    runtime: State<'_, NativeLibraryRuntime>,
    collection_id: i64,
    asset_ids: Vec<i64>,
) -> Result<(), String> {
    let mut guard = runtime
        .library
        .lock()
        .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
    guard
        .as_mut()
        .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?
        .add_collection_assets(collection_id, &asset_ids)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn library_collection_assets(
    runtime: State<'_, NativeLibraryRuntime>,
    collection_id: i64,
    limit: u32,
    offset: u32,
) -> Result<Vec<AssetRecord>, String> {
    let guard = runtime
        .library
        .lock()
        .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
    guard
        .as_ref()
        .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?
        .collection_assets(collection_id, limit.min(500), offset)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn library_thumbnail(
    runtime: State<'_, NativeLibraryRuntime>,
    asset_id: i64,
    size: ThumbnailSize,
) -> Result<PathBuf, String> {
    let runtime = runtime.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = default_library_path()?
            .parent()
            .ok_or_else(|| "ThumbnailFailed: invalid cache root".to_owned())?
            .join("cache")
            .join("thumbnails");
        let guard = runtime
            .library
            .lock()
            .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
        guard
            .as_ref()
            .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?
            .generate_thumbnail(asset_id, root, size)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("ThumbnailFailed: worker failed: {error}"))?
}

#[derive(Default)]
struct NativeHistoryRuntime(Mutex<BTreeMap<i64, EditHistory>>);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeHistoryResult {
    state: serde_json::Value,
    can_undo: bool,
    can_redo: bool,
    entries: Vec<HistoryEntry>,
    snapshots: Vec<NamedSnapshot>,
    state_version: String,
}

fn history_path(asset_id: i64) -> Result<PathBuf, String> {
    let root = default_library_path()?
        .parent()
        .ok_or_else(|| "HistoryPersistenceFailed: invalid app data directory".to_owned())?
        .join("history");
    Ok(root.join(format!("asset-{asset_id}.history.json")))
}

fn history_result(history: &EditHistory) -> NativeHistoryResult {
    NativeHistoryResult {
        state: history.state().clone(),
        can_undo: history.can_undo(),
        can_redo: history.can_redo(),
        entries: history.entries().to_vec(),
        snapshots: history.snapshots().to_vec(),
        state_version: history.state_version().0,
    }
}

fn persist_history(asset_id: i64, history: &EditHistory) -> Result<(), String> {
    history
        .persist(history_path(asset_id)?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn history_open(
    runtime: State<'_, NativeHistoryRuntime>,
    asset_id: i64,
    initial_state: serde_json::Value,
) -> Result<NativeHistoryResult, String> {
    let path = history_path(asset_id)?;
    let history = if path.is_file() {
        EditHistory::load(&path)
    } else {
        EditHistory::new(initial_state)
    }
    .map_err(|error| error.to_string())?;
    let result = history_result(&history);
    runtime
        .0
        .lock()
        .map_err(|_| "HistoryCorrupt: runtime lock poisoned".to_owned())?
        .insert(asset_id, history);
    Ok(result)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryCommitRequest {
    asset_id: i64,
    description: String,
    affected_stage: String,
    before: serde_json::Value,
    after: serde_json::Value,
}

#[tauri::command]
fn history_commit(
    runtime: State<'_, NativeHistoryRuntime>,
    request: HistoryCommitRequest,
) -> Result<NativeHistoryResult, String> {
    let mut histories = runtime
        .0
        .lock()
        .map_err(|_| "HistoryCorrupt: runtime lock poisoned".to_owned())?;
    let history = histories
        .get_mut(&request.asset_id)
        .ok_or_else(|| "HistoryCorrupt: history is not open".to_owned())?;
    if history.state() != &request.before {
        return Err("InvalidHistoryEntry: before state does not match active history".into());
    }
    history
        .commit(
            request.description,
            request.affected_stage,
            EditCommand::ReplaceState {
                before: request.before,
                after: request.after,
            },
        )
        .map_err(|error| error.to_string())?;
    persist_history(request.asset_id, history)?;
    Ok(history_result(history))
}

fn history_step(
    runtime: State<'_, NativeHistoryRuntime>,
    asset_id: i64,
    undo: bool,
) -> Result<NativeHistoryResult, String> {
    let mut histories = runtime
        .0
        .lock()
        .map_err(|_| "HistoryCorrupt: runtime lock poisoned".to_owned())?;
    let history = histories
        .get_mut(&asset_id)
        .ok_or_else(|| "HistoryCorrupt: history is not open".to_owned())?;
    if undo { history.undo() } else { history.redo() }.map_err(|error| error.to_string())?;
    persist_history(asset_id, history)?;
    Ok(history_result(history))
}

#[tauri::command]
fn history_undo(
    runtime: State<'_, NativeHistoryRuntime>,
    asset_id: i64,
) -> Result<NativeHistoryResult, String> {
    history_step(runtime, asset_id, true)
}

#[tauri::command]
fn history_redo(
    runtime: State<'_, NativeHistoryRuntime>,
    asset_id: i64,
) -> Result<NativeHistoryResult, String> {
    history_step(runtime, asset_id, false)
}

#[tauri::command]
fn history_snapshot_create(
    runtime: State<'_, NativeHistoryRuntime>,
    asset_id: i64,
    name: String,
) -> Result<NativeHistoryResult, String> {
    let mut histories = runtime
        .0
        .lock()
        .map_err(|_| "HistoryCorrupt: runtime lock poisoned".to_owned())?;
    let history = histories
        .get_mut(&asset_id)
        .ok_or_else(|| "HistoryCorrupt: history is not open".to_owned())?;
    history
        .create_snapshot(name)
        .map_err(|error| error.to_string())?;
    persist_history(asset_id, history)?;
    Ok(history_result(history))
}

#[tauri::command]
fn history_snapshot_restore(
    runtime: State<'_, NativeHistoryRuntime>,
    asset_id: i64,
    snapshot_id: String,
) -> Result<NativeHistoryResult, String> {
    let mut histories = runtime
        .0
        .lock()
        .map_err(|_| "HistoryCorrupt: runtime lock poisoned".to_owned())?;
    let history = histories
        .get_mut(&asset_id)
        .ok_or_else(|| "HistoryCorrupt: history is not open".to_owned())?;
    history
        .restore_snapshot(&snapshot_id)
        .map_err(|error| error.to_string())?;
    persist_history(asset_id, history)?;
    Ok(history_result(history))
}

#[tauri::command]
fn history_snapshot_rename(
    runtime: State<'_, NativeHistoryRuntime>,
    asset_id: i64,
    snapshot_id: String,
    name: String,
) -> Result<NativeHistoryResult, String> {
    let mut histories = runtime
        .0
        .lock()
        .map_err(|_| "HistoryCorrupt: runtime lock poisoned".to_owned())?;
    let history = histories
        .get_mut(&asset_id)
        .ok_or_else(|| "HistoryCorrupt: history is not open".to_owned())?;
    history
        .rename_snapshot(&snapshot_id, &name)
        .map_err(|error| error.to_string())?;
    persist_history(asset_id, history)?;
    Ok(history_result(history))
}

#[tauri::command]
fn history_snapshot_delete(
    runtime: State<'_, NativeHistoryRuntime>,
    asset_id: i64,
    snapshot_id: String,
) -> Result<NativeHistoryResult, String> {
    let mut histories = runtime
        .0
        .lock()
        .map_err(|_| "HistoryCorrupt: runtime lock poisoned".to_owned())?;
    let history = histories
        .get_mut(&asset_id)
        .ok_or_else(|| "HistoryCorrupt: history is not open".to_owned())?;
    history
        .delete_snapshot(&snapshot_id)
        .map_err(|error| error.to_string())?;
    persist_history(asset_id, history)?;
    Ok(history_result(history))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiDenoiseModelStatus {
    model_id: &'static str,
    model_version: &'static str,
    model_hash: &'static str,
    installed: bool,
    path: PathBuf,
    active_execution_provider: Option<DenoiseExecutionProvider>,
    fallback_reason: Option<String>,
}

#[tauri::command]
fn ai_denoise_status(runtime: State<'_, NativeAiDenoiseRuntime>) -> AiDenoiseModelStatus {
    let path = local_nafnet_model();
    let active_execution_provider = runtime.provider.lock().ok().and_then(|provider| {
        provider
            .as_ref()
            .map(|provider| provider.execution_provider)
    });
    let fallback_reason = runtime
        .last_fallback_reason
        .lock()
        .ok()
        .and_then(|reason| reason.clone());
    AiDenoiseModelStatus {
        model_id: NAFNET_MODEL_ID,
        model_version: NAFNET_MODEL_VERSION,
        model_hash: NAFNET_MODEL_SHA256,
        installed: path.is_file(),
        path,
        active_execution_provider,
        fallback_reason,
    }
}

/// UI-visible M12 backend state. This intentionally reports the fallback reason instead of
/// silently treating unavailable DX12/device resources as a browser-rendering failure.
#[tauri::command]
fn gpu_preview_status(prefer_gpu: Option<bool>) -> GpuStatus {
    probe_gpu_status(prefer_gpu.unwrap_or(true))
}

#[tauri::command]
fn advise_image(stats: AnalysisStats) -> Vec<Suggestion> {
    advise(stats)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeEditSettings {
    exposure: f32,
    contrast: f32,
    highlights: f32,
    shadows: f32,
    whites: f32,
    blacks: f32,
    temperature: f32,
    tint: f32,
    vibrance: f32,
    saturation: f32,
    sharpness: f32,
    noise_reduction: f32,
    #[serde(default)]
    white_balance_mode: WhiteBalanceMode,
    #[serde(default)]
    white_balance_sample: Option<WhiteBalanceSample>,
    curve: Vec<CurvePoint>,
    #[serde(default)]
    curves: ToneCurveSet,
    #[serde(default)]
    color_mixer: ColorMixer,
    #[serde(default)]
    grading: GradingParameters,
    #[serde(default)]
    sharpen_settings: SharpenParameters,
    #[serde(default)]
    denoise_settings: DenoiseParameters,
    #[serde(default)]
    ai_denoise: AiDenoiseParameters,
    #[serde(default = "default_denoise_execution_provider")]
    ai_denoise_provider: DenoiseExecutionProvider,
    #[serde(default)]
    local_detail: LocalDetailParameters,
    #[serde(default)]
    optics: OpticsSettings,
    #[serde(default)]
    geometry: GeometryParameters,
    #[serde(default)]
    layers: Vec<NativeAdjustmentLayer>,
    #[serde(default)]
    skin_retouch: SkinRetouchSettings,
    #[serde(default)]
    healing_operations: Vec<HealingOperation>,
    #[serde(default)]
    grain: GrainSettings,
    #[serde(default)]
    vignette: VignetteSettings,
}

const fn default_denoise_execution_provider() -> DenoiseExecutionProvider {
    DenoiseExecutionProvider::DirectMl
}

impl NativeEditSettings {
    fn validated(self) -> Result<RenderSettings, String> {
        let finite = [
            self.exposure,
            self.contrast,
            self.highlights,
            self.shadows,
            self.whites,
            self.blacks,
            self.temperature,
            self.tint,
            self.vibrance,
            self.saturation,
            self.sharpness,
            self.noise_reduction,
            self.color_mixer.band_width_degrees,
            self.grading.balance,
            self.grading.blending,
            self.grading.amount,
            self.skin_retouch.parameters.smooth,
            self.skin_retouch.parameters.texture,
            self.skin_retouch.parameters.tone_evenness,
            self.skin_retouch.parameters.hue_degrees,
            self.skin_retouch.parameters.chroma,
            self.skin_retouch.parameters.exposure_ev,
            self.ai_denoise.amount,
            self.ai_denoise.detail,
            self.ai_denoise.color_noise,
            self.ai_denoise.preserve_skin,
            self.grain.amount,
            self.grain.size,
            self.grain.roughness,
            self.grain.color,
            self.vignette.amount,
            self.vignette.midpoint,
            self.vignette.roundness,
            self.vignette.feather,
            self.vignette.highlight_protect,
        ]
        .into_iter()
        .all(f32::is_finite)
            && self
                .curve
                .iter()
                .all(|point| point.x.is_finite() && point.y.is_finite());
        if !finite {
            return Err("native edit settings contain NaN or Inf".into());
        }
        if !(0.0..=1.0).contains(&self.grain.amount)
            || !(0.1..=1.0).contains(&self.grain.size)
            || !(0.0..=1.0).contains(&self.grain.roughness)
            || !(0.0..=1.0).contains(&self.grain.color)
            || !(-1.0..=1.0).contains(&self.vignette.amount)
            || !(0.0..=1.0).contains(&self.vignette.midpoint)
            || !(-1.0..=1.0).contains(&self.vignette.roundness)
            || !(0.0..=1.0).contains(&self.vignette.feather)
            || !(0.0..=1.0).contains(&self.vignette.highlight_protect)
        {
            return Err("native grain/vignette settings are out of range".into());
        }
        if self.curve.len() > 32 {
            return Err("native tone curve accepts at most 32 points".into());
        }
        if self.layers.len() > 64 {
            return Err("native layer stack accepts at most 64 layers".into());
        }
        if self.skin_retouch.faces.len() > 16
            || self
                .skin_retouch
                .faces
                .iter()
                .any(|face| face.face_id.trim().is_empty() || face.cache_key.trim().is_empty())
            || self.skin_retouch.parameters.validated().is_err()
        {
            return Err("native skin retouch settings are outside supported ranges".into());
        }
        if self.healing_operations.len() > 256
            || self
                .healing_operations
                .iter()
                .any(|operation| operation.validate().is_err())
        {
            return Err("native healing operations are outside supported ranges or request unavailable AI inpaint".into());
        }
        let mut layer_ids = std::collections::BTreeSet::new();
        if self
            .layers
            .iter()
            .any(|layer| layer.id.trim().is_empty() || !layer_ids.insert(&layer.id))
        {
            return Err("native layer identifiers must be unique and non-empty".into());
        }
        if !(30.0..=80.0).contains(&self.color_mixer.band_width_degrees)
            || self.color_mixer.bands.iter().any(|band| {
                ![band.hue_degrees, band.chroma, band.lightness]
                    .into_iter()
                    .all(f32::is_finite)
                    || !(-30.0..=30.0).contains(&band.hue_degrees)
                    || !(-1.0..=1.0).contains(&band.chroma)
                    || !(-1.0..=1.0).contains(&band.lightness)
            })
        {
            return Err("native color mixer settings are outside supported ranges".into());
        }
        let grading_wheels = [
            self.grading.shadows,
            self.grading.midtones,
            self.grading.highlights,
            self.grading.global,
        ];
        if grading_wheels.iter().any(|wheel| {
            ![wheel.hue_degrees, wheel.chroma, wheel.lightness]
                .into_iter()
                .all(f32::is_finite)
                || !(-360.0..=360.0).contains(&wheel.hue_degrees)
                || !(-1.0..=1.0).contains(&wheel.chroma)
                || !(-1.0..=1.0).contains(&wheel.lightness)
        }) || !(-1.0..=1.0).contains(&self.grading.balance)
            || !(0.0..=1.0).contains(&self.grading.blending)
            || !(0.0..=1.0).contains(&self.grading.amount)
        {
            return Err("native color grading settings are outside supported ranges".into());
        }
        let detail_values = [
            self.sharpen_settings.amount,
            self.sharpen_settings.radius,
            self.sharpen_settings.detail,
            self.sharpen_settings.masking,
            self.sharpen_settings.halo_protection,
            self.sharpen_settings.threshold,
            self.denoise_settings.luminance,
            self.denoise_settings.chroma,
            self.denoise_settings.radius,
            self.denoise_settings.detail_protection,
            self.denoise_settings.high_iso,
            self.local_detail.texture,
            self.local_detail.clarity,
            self.local_detail.dehaze,
        ];
        if !detail_values.into_iter().all(f32::is_finite)
            || !(0.0..=2.0).contains(&self.sharpen_settings.amount)
            || !(0.3..=4.0).contains(&self.sharpen_settings.radius)
            || !(0.0..=1.0).contains(&self.sharpen_settings.detail)
            || !(0.0..=1.0).contains(&self.sharpen_settings.masking)
            || !(0.0..=1.0).contains(&self.sharpen_settings.halo_protection)
            || !(0.0..=1.0).contains(&self.denoise_settings.luminance)
            || !(0.0..=1.0).contains(&self.denoise_settings.chroma)
            || !(0.6..=4.0).contains(&self.denoise_settings.radius)
            || !(0.0..=1.0).contains(&self.denoise_settings.detail_protection)
            || !(0.0..=1.0).contains(&self.denoise_settings.high_iso)
            || [
                self.local_detail.texture,
                self.local_detail.clarity,
                self.local_detail.dehaze,
            ]
            .into_iter()
            .any(|value| !(-1.0..=1.0).contains(&value))
        {
            return Err("native detail settings are outside supported ranges".into());
        }
        if self
            .optics
            .manual_identity
            .as_ref()
            .is_some_and(|identity| {
                ![identity.focal_length_mm, identity.aperture]
                    .into_iter()
                    .all(f32::is_finite)
                    || identity
                        .focus_distance_m
                        .is_some_and(|distance| !distance.is_finite() || distance <= 0.0)
            })
        {
            return Err("native manual lens metadata is invalid".into());
        }
        let geometry_values = [
            self.geometry.rotation_degrees,
            self.geometry.vertical_keystone,
            self.geometry.horizontal_keystone,
            self.geometry.scale,
            self.geometry.offset_x,
            self.geometry.offset_y,
            self.geometry.crop.left,
            self.geometry.crop.top,
            self.geometry.crop.right,
            self.geometry.crop.bottom,
            self.geometry.crop_aspect_width,
            self.geometry.crop_aspect_height,
        ];
        let four_point_finite = self.geometry.four_point.is_none_or(|points| {
            [
                points.top_left.x,
                points.top_left.y,
                points.top_right.x,
                points.top_right.y,
                points.bottom_right.x,
                points.bottom_right.y,
                points.bottom_left.x,
                points.bottom_left.y,
            ]
            .into_iter()
            .all(f32::is_finite)
        });
        if !geometry_values.into_iter().all(f32::is_finite)
            || !four_point_finite
            || !(-180.0..=180.0).contains(&self.geometry.rotation_degrees)
            || !(-1.5..=1.5).contains(&self.geometry.vertical_keystone)
            || !(-1.5..=1.5).contains(&self.geometry.horizontal_keystone)
            || !(0.05..=20.0).contains(&self.geometry.scale)
            || self.geometry.crop.left < 0.0
            || self.geometry.crop.top < 0.0
            || self.geometry.crop.right > 1.0
            || self.geometry.crop.bottom > 1.0
            || self.geometry.crop.right <= self.geometry.crop.left
            || self.geometry.crop.bottom <= self.geometry.crop.top
            || ((self.geometry.crop_aspect_width < 0.0 || self.geometry.crop_aspect_height < 0.0)
                && !(self.geometry.crop_aspect_width == -1.0
                    && self.geometry.crop_aspect_height == -1.0))
        {
            return Err("native geometry settings are outside supported ranges".into());
        }

        let unit = |value: f32| (value / 100.0).clamp(-1.0, 1.0);
        let mut curve = self.curve;
        curve.sort_by(|a, b| a.x.total_cmp(&b.x));
        if curve
            .iter()
            .any(|point| !(0.0..=1.0).contains(&point.x) || !(0.0..=1.0).contains(&point.y))
        {
            return Err("native tone curve points must stay inside 0..1".into());
        }
        Ok(RenderSettings {
            tone: ToneParameters {
                exposure_ev: self.exposure.clamp(-5.0, 5.0),
                contrast: unit(self.contrast),
                highlights: unit(self.highlights),
                shadows: unit(self.shadows),
                whites: unit(self.whites),
                blacks: unit(self.blacks),
            },
            relative_color: RelativeColorParameters {
                temperature: unit(self.temperature),
                tint: unit(self.tint),
                vibrance: unit(self.vibrance),
                saturation: unit(self.saturation),
            },
            white_balance: WhiteBalanceSettings {
                mode: self.white_balance_mode,
                sample: self.white_balance_sample,
            },
            curve,
            curves: self.curves,
            color_mixer: self.color_mixer,
            grading: self.grading,
            denoise: self.denoise_settings,
            ai_denoise: self
                .ai_denoise
                .validate()
                .map_err(|error| error.to_string())?,
            local_detail: self.local_detail,
            sharpen: self.sharpen_settings,
            optics: self.optics,
            geometry: self.geometry,
            layers: self.layers,
            skin_retouch: self.skin_retouch,
            healing_operations: self.healing_operations,
            grain: self.grain,
            vignette: self.vignette,
            ..Default::default()
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativePreviewRequest {
    request_id: String,
    source_path: PathBuf,
    max_edge: u32,
    #[serde(default = "default_prefer_gpu")]
    prefer_gpu: bool,
    #[serde(default)]
    interaction_phase: PreviewInteractionPhase,
    settings: NativeEditSettings,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum PreviewInteractionPhase {
    Interactive,
    #[default]
    Final,
}

fn preview_requested_edge(max_edge: u32, phase: PreviewInteractionPhase) -> u32 {
    match phase {
        PreviewInteractionPhase::Interactive => max_edge.min(1024),
        PreviewInteractionPhase::Final => max_edge,
    }
    .clamp(256, 4096)
}

/// Process-wide M13 scheduler state. It holds only derived preview/cache bytes and request
/// identities; the immutable source image remains on disk and full export never reads this cache.
struct NativePreviewScheduler(Mutex<RenderScheduler>);

/// Process-local M16 model/session and soft-mask cache. It never crosses the Tauri boundary:
/// IPC transports face geometry and a compact cache reference, while Preview/Export resolve the
/// source-space R16Float-compatible mask in the Native shared graph.
struct NativePortraitRuntime(Mutex<PortraitRuntimeState>);

#[derive(Default)]
struct PortraitRuntimeState {
    provider: Option<PortraitOnnxProvider>,
    parsed: BTreeMap<String, PortraitParseResult>,
}

impl Default for NativePortraitRuntime {
    fn default() -> Self {
        Self(Mutex::new(PortraitRuntimeState::default()))
    }
}

struct NativeAiMaskRuntime {
    provider: Mutex<Option<AiMaskOnnxProvider>>,
    cache: Mutex<BTreeMap<String, GeneratedAiMask>>,
    cancellations: Mutex<BTreeMap<String, Arc<AtomicBool>>>,
}

struct NativeAiDenoiseRuntime {
    provider: Mutex<Option<NafNetOnnxProvider>>,
    cache: Mutex<BTreeMap<String, AiDenoiseResidual>>,
    cancellations: Mutex<BTreeMap<String, Arc<AtomicBool>>>,
    last_fallback_reason: Mutex<Option<String>>,
}

impl Default for NativeAiDenoiseRuntime {
    fn default() -> Self {
        Self {
            provider: Mutex::new(None),
            cache: Mutex::new(BTreeMap::new()),
            cancellations: Mutex::new(BTreeMap::new()),
            last_fallback_reason: Mutex::new(None),
        }
    }
}

impl Default for NativeAiMaskRuntime {
    fn default() -> Self {
        Self {
            provider: Mutex::new(None),
            cache: Mutex::new(BTreeMap::new()),
            cancellations: Mutex::new(BTreeMap::new()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiMaskFailure {
    code: &'static str,
    message: String,
}

impl From<AiMaskError> for AiMaskFailure {
    fn from(value: AiMaskError) -> Self {
        let code = match value {
            AiMaskError::ModelMissing { .. } => "modelMissing",
            AiMaskError::ModelHashMismatch { .. } => "modelHashMismatch",
            AiMaskError::RuntimeUnavailable(_) => "runtimeUnavailable",
            AiMaskError::ProviderInitializationFailed(_) => "providerInitializationFailed",
            AiMaskError::DirectMlUnavailable(_) => "directMlUnavailable",
            AiMaskError::InferenceFailed(_) => "inferenceFailed",
            AiMaskError::InvalidTensor(_) => "invalidTensor",
            AiMaskError::InvalidOutput(_) => "invalidOutput",
            AiMaskError::OutOfMemory => "outOfMemory",
            AiMaskError::Cancelled => "cancelled",
            AiMaskError::PortraitProviderRequired(_) => "portraitProviderRequired",
        };
        Self {
            code,
            message: value.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PortraitFailure {
    code: &'static str,
    message: String,
}

impl From<PortraitError> for PortraitFailure {
    fn from(value: PortraitError) -> Self {
        let code = match value {
            PortraitError::DetectorModelMissing { .. } => "detectorModelMissing",
            PortraitError::ParserModelMissing { .. } => "parserModelMissing",
            PortraitError::ModelHashMismatch { .. } => "modelHashMismatch",
            PortraitError::RuntimeUnavailable(_) => "runtimeUnavailable",
            PortraitError::DetectorInitializationFailed(_) => "detectorInitializationFailed",
            PortraitError::ParserInitializationFailed(_) => "parserInitializationFailed",
            PortraitError::DetectionFailed(_) => "detectionFailed",
            PortraitError::ParsingFailed(_) => "parsingFailed",
            PortraitError::InvalidDetectionOutput(_) => "invalidDetectionOutput",
            PortraitError::InvalidParsingOutput(_) => "invalidParsingOutput",
            PortraitError::NoFaceDetected => "noFaceDetected",
            PortraitError::InvalidTransform(_) => "invalidTransform",
            PortraitError::UnsupportedExecutionProvider(_) => "unsupportedExecutionProvider",
        };
        Self {
            code,
            message: value.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PortraitFaceResponse {
    face: DetectedFace,
    cache_key: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PortraitDetectionResponse {
    status: &'static str,
    faces: Vec<PortraitFaceResponse>,
    detector_model_id: String,
    detector_model_version: String,
    detector_model_hash: String,
    parser_model_id: String,
    parser_model_version: String,
    parser_model_hash: String,
    execution_provider: starroom_portrait::ExecutionProvider,
    error: Option<PortraitFailure>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PortraitDetectRequest {
    source_path: PathBuf,
    #[serde(default = "default_face_crop_scale")]
    face_crop_scale: f32,
}

const fn default_face_crop_scale() -> f32 {
    1.4
}

impl Default for NativePreviewScheduler {
    fn default() -> Self {
        Self(Mutex::new(RenderScheduler::default()))
    }
}

const fn default_prefer_gpu() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeExportRequest {
    request_id: String,
    source_path: PathBuf,
    output_path: PathBuf,
    quality: u8,
    settings: NativeEditSettings,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeColorSampleRequest {
    source_path: PathBuf,
    x: f32,
    y: f32,
    settings: NativeEditSettings,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeAdvisorRequest {
    source_path: PathBuf,
    max_edge: u32,
    settings: NativeEditSettings,
}

/// M19 runs deterministic analysis locally on the same native graph that produces preview.
/// The UI receives small statistics/suggestions only; no image pixels or cloud request cross IPC.
#[tauri::command]
fn advise_native_image(
    portrait_runtime: State<'_, NativePortraitRuntime>,
    request: NativeAdvisorRequest,
) -> Result<AdvisorResult, String> {
    let mut settings = request.settings.validated()?;
    attach_portrait_masks(&mut settings, &portrait_runtime)?;
    let decoded = decode_source_preview(&request.source_path, request.max_edge.clamp(256, 2048))
        .map_err(|error| format!("advisor preview decode failed: {error}"))?;
    let rendered = render_source_preview_to_srgb8(&decoded, &settings)
        .map_err(|error| format!("advisor native graph failed: {error}"))?;
    let samples = rendered
        .data
        .as_chunks::<3>()
        .0
        .iter()
        .map(|rgb| {
            [
                rgb[0] as f32 / 255.0,
                rgb[1] as f32 / 255.0,
                rgb[2] as f32 / 255.0,
            ]
        })
        .collect::<Vec<_>>();
    let mut analysis = analyze_detailed(&samples);
    let skin_rasters = settings
        .portrait_masks
        .iter()
        .filter(|raster| raster.region == PortraitMaskRegion::Skin)
        .collect::<Vec<_>>();
    if !skin_rasters.is_empty() {
        let mut weight_sum = 0.0_f32;
        let mut luma_sum = 0.0_f32;
        let mut chroma_sum = 0.0_f32;
        for (index, rgb) in samples.iter().enumerate() {
            let x = (index % rendered.width as usize) as f32
                / rendered.width.saturating_sub(1).max(1) as f32;
            let y = (index / rendered.width as usize) as f32
                / rendered.height.saturating_sub(1).max(1) as f32;
            let weight = skin_rasters
                .iter()
                .map(|raster| {
                    let px = (x * raster.width.saturating_sub(1) as f32).round() as usize;
                    let py = (y * raster.height.saturating_sub(1) as f32).round() as usize;
                    raster
                        .values
                        .get(py * raster.width as usize + px)
                        .copied()
                        .unwrap_or(0.0)
                })
                .fold(0.0_f32, f32::max)
                .clamp(0.0, 1.0);
            let luma = rgb[0] * 0.2627 + rgb[1] * 0.6780 + rgb[2] * 0.0593;
            let chroma =
                ((rgb[0] - rgb[1]).powi(2) + (rgb[1] - rgb[2]).powi(2) + (rgb[2] - rgb[0]).powi(2))
                    .sqrt();
            weight_sum += weight;
            luma_sum += luma * weight;
            chroma_sum += chroma * weight;
        }
        if weight_sum > 0.0 {
            analysis.portrait_luminance_mean = luma_sum / weight_sum;
            analysis.portrait_chroma_mean = chroma_sum / weight_sum;
            analysis.portrait_sample_fraction = weight_sum / samples.len().max(1) as f32;
        }
    }
    Ok(AdvisorResult {
        suggestions: advise_detailed(analysis.clone()),
        analysis,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeOpticsStatusRequest {
    source_path: PathBuf,
    settings: NativeEditSettings,
}

#[tauri::command]
fn native_optics_status(
    request: NativeOpticsStatusRequest,
) -> Result<LensProfileResolution, String> {
    let settings = request.settings.validated()?;
    let decoded = decode_source_preview(&request.source_path, 512)
        .map_err(|error| format!("native optics metadata decode failed: {error}"))?;
    resolve_source_lens_profile(&decoded, &settings.optics)
        .map_err(|error| format!("native Lensfun resolution failed: {error}"))
}

#[tauri::command]
fn native_sample_color(
    request: NativeColorSampleRequest,
) -> Result<Option<starroom_color::ColorBand>, String> {
    let settings = request.settings.validated()?;
    let decoded = decode_source_preview(&request.source_path, 1800)
        .map_err(|error| format!("native color sample decode failed: {error}"))?;
    sample_source_color_band(&decoded, &settings, request.x, request.y)
        .map_err(|error| format!("native color sample failed: {error}"))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeExportResult {
    output_path: PathBuf,
    width: u32,
    height: u32,
    input_profile: String,
    camera_profile_hash: Option<String>,
    working_space: &'static str,
}

fn profile_flag(source: starroom_color_management::InputProfileSource) -> u16 {
    match source {
        starroom_color_management::InputProfileSource::EmbeddedIcc => 1,
        starroom_color_management::InputProfileSource::AssumedSrgb => 0,
        starroom_color_management::InputProfileSource::RawCameraMatrix => 2,
        starroom_color_management::InputProfileSource::RawGenericProfile => 4,
    }
}

fn preview_frame(
    width: u32,
    height: u32,
    flags: u16,
    profile_id: &str,
    jpeg: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let profile_len = u16::try_from(profile_id.len()).map_err(|_| "profile ID is too long")?;
    let payload_len = u32::try_from(jpeg.len()).map_err(|_| "native preview is too large")?;
    let mut frame = Vec::with_capacity(24 + profile_id.len() + jpeg.len());
    frame.extend_from_slice(b"SRP2");
    frame.extend_from_slice(&2_u16.to_le_bytes());
    frame.extend_from_slice(&flags.to_le_bytes());
    frame.extend_from_slice(&width.to_le_bytes());
    frame.extend_from_slice(&height.to_le_bytes());
    frame.extend_from_slice(&profile_len.to_le_bytes());
    frame.extend_from_slice(&0_u16.to_le_bytes());
    frame.extend_from_slice(&payload_len.to_le_bytes());
    frame.extend_from_slice(profile_id.as_bytes());
    frame.extend_from_slice(&jpeg);
    Ok(frame)
}

fn preview_source_identity(path: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("native preview metadata failed: {error}"))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Ok(format!("{}:{}:{modified}", path.display(), metadata.len()))
}

fn source_dimensions(decoded: &DecodedSourceImage) -> (u32, u32) {
    match decoded {
        DecodedSourceImage::Rendered(image) => (image.width, image.height),
        DecodedSourceImage::Raw(image) => (image.width, image.height),
    }
}

fn local_portrait_models() -> PortraitModelRegistry {
    let root = std::env::var_os("STARROOM_LOCAL_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("models").join("local"));
    PortraitModelRegistry::local_default(root)
}

fn local_ai_mask_models() -> AiMaskModelRegistry {
    let root = std::env::var_os("STARROOM_LOCAL_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("models").join("local"));
    AiMaskModelRegistry::local_default(root)
}

fn local_nafnet_model() -> PathBuf {
    std::env::var_os("STARROOM_LOCAL_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("models").join("local"))
        .join("nafnet-sidd-width32-512-opset20.onnx")
}

fn infer_ai_denoise_with_fallback(
    runtime: &NativeAiDenoiseRuntime,
    working: &starroom_detail::LinearImage,
    sized_identity: &str,
    requested_provider: DenoiseExecutionProvider,
    token: &AtomicBool,
) -> Result<AiDenoiseResidual, String> {
    let mut provider = runtime
        .provider
        .lock()
        .map_err(|_| "AI denoise provider lock was poisoned".to_owned())?;
    if provider
        .as_ref()
        .is_none_or(|active| active.execution_provider != requested_provider)
    {
        match NafNetOnnxProvider::initialize(local_nafnet_model(), requested_provider) {
            Ok(active) => {
                *provider = Some(active);
                if let Ok(mut reason) = runtime.last_fallback_reason.lock() {
                    *reason = None;
                }
            }
            Err(error)
                if requested_provider == DenoiseExecutionProvider::DirectMl
                    && directml_failure_allows_cpu_fallback(&error) =>
            {
                let reason = error.to_string();
                *provider = Some(
                    NafNetOnnxProvider::initialize(
                        local_nafnet_model(),
                        DenoiseExecutionProvider::Cpu,
                    )
                    .map_err(|cpu_error| {
                        format!(
                            "AI denoise DirectML failed ({reason}); explicit CPU fallback also failed: {cpu_error}"
                        )
                    })?,
                );
                *runtime
                    .last_fallback_reason
                    .lock()
                    .map_err(|_| "AI denoise fallback status lock was poisoned".to_owned())? =
                    Some(reason);
            }
            Err(error) => return Err(format!("AI denoise provider failed: {error}")),
        }
    }
    let active_provider = provider
        .as_ref()
        .expect("provider initialized")
        .execution_provider;
    let first = infer_tiled(
        provider.as_mut().expect("provider initialized"),
        working,
        sized_identity,
        token,
        active_provider,
    );
    match first {
        Ok(residual) => Ok(residual),
        Err(error)
            if active_provider == DenoiseExecutionProvider::DirectMl
                && directml_failure_allows_cpu_fallback(&error) =>
        {
            let reason = error.to_string();
            *provider = Some(
                NafNetOnnxProvider::initialize(local_nafnet_model(), DenoiseExecutionProvider::Cpu)
                    .map_err(|cpu_error| {
                        format!(
                            "AI denoise DirectML inference failed ({reason}); explicit CPU fallback also failed: {cpu_error}"
                        )
                    })?,
            );
            *runtime
                .last_fallback_reason
                .lock()
                .map_err(|_| "AI denoise fallback status lock was poisoned".to_owned())? =
                Some(reason);
            infer_tiled(
                provider.as_mut().expect("CPU fallback initialized"),
                working,
                sized_identity,
                token,
                DenoiseExecutionProvider::Cpu,
            )
            .map_err(|cpu_error| {
                format!("AI denoise explicit CPU fallback inference failed: {cpu_error}")
            })
        }
        Err(error) => Err(format!("AI denoise inference failed: {error}")),
    }
}

fn attach_ai_denoise(
    decoded: &DecodedSourceImage,
    source_path: &Path,
    settings: &mut RenderSettings,
    requested_provider: DenoiseExecutionProvider,
    request_id: &str,
    runtime: &NativeAiDenoiseRuntime,
) -> Result<(), String> {
    let source = preview_source_identity(source_path)?;
    settings.image_identity = source.clone();
    if !settings.ai_denoise.enabled {
        return Ok(());
    }
    let working = prepare_source_for_ai_denoise(decoded, settings)
        .map_err(|error| format!("AI denoise input graph failed: {error}"))?;
    let sized_identity = format!("{source}:{}x{}", working.width, working.height);
    let key = inference_cache_key(&sized_identity);
    if let Some(residual) = runtime
        .cache
        .lock()
        .map_err(|_| "AI denoise cache lock was poisoned".to_owned())?
        .get(&key)
        .cloned()
    {
        settings.ai_denoise_residual = Some(residual);
        return Ok(());
    }
    let token = Arc::new(AtomicBool::new(false));
    runtime
        .cancellations
        .lock()
        .map_err(|_| "AI denoise cancellation lock was poisoned".to_owned())?
        .insert(request_id.to_owned(), Arc::clone(&token));
    let result = infer_ai_denoise_with_fallback(
        runtime,
        &working,
        &sized_identity,
        requested_provider,
        &token,
    );
    runtime
        .cancellations
        .lock()
        .map_err(|_| "AI denoise cancellation lock was poisoned".to_owned())?
        .remove(request_id);
    let residual = result?;
    runtime
        .cache
        .lock()
        .map_err(|_| "AI denoise cache lock was poisoned".to_owned())?
        .insert(key, residual.clone());
    settings.ai_denoise_residual = Some(residual);
    Ok(())
}

#[tauri::command]
fn ai_denoise_cancel(runtime: State<'_, NativeAiDenoiseRuntime>, request_id: String) -> bool {
    runtime
        .cancellations
        .lock()
        .ok()
        .and_then(|tokens| tokens.get(&request_id).cloned())
        .is_some_and(|token| {
            token.store(true, Ordering::Relaxed);
            true
        })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeReferenceMatchRequest {
    source_path: PathBuf,
    reference_path: PathBuf,
    max_edge: u32,
    amount: f32,
    tone: f32,
    color: f32,
    grading: f32,
    protect_skin: f32,
    settings: NativeEditSettings,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeReferenceMatchResponse {
    settings: NativeEditSettings,
    source_analysis: ReferenceAnalysis,
    reference_analysis: ReferenceAnalysis,
    recipe: ReferenceMatchRecipe,
}

fn apply_reference_recipe(
    settings: &mut NativeEditSettings,
    recipe: &ReferenceMatchRecipe,
    amount: f32,
    tone: f32,
    color: f32,
    grading: f32,
) {
    if amount <= f32::EPSILON {
        return;
    }
    let base = settings_to_look(settings, "Reference base".into());
    let mut target_settings = settings.clone();
    target_settings.exposure = recipe.tone.exposure_ev;
    target_settings.contrast = unit_to_percent(recipe.tone.contrast);
    target_settings.highlights = unit_to_percent(recipe.tone.highlights);
    target_settings.shadows = unit_to_percent(recipe.tone.shadows);
    target_settings.whites = unit_to_percent(recipe.tone.whites);
    target_settings.blacks = unit_to_percent(recipe.tone.blacks);
    target_settings.temperature = unit_to_percent(recipe.white_balance.temperature);
    target_settings.tint = unit_to_percent(recipe.white_balance.tint);
    target_settings.curve = recipe.curve.clone();
    target_settings.curves.master = recipe.curve.clone();
    target_settings.color_mixer = recipe.color_mixer;
    target_settings.grading = recipe.grading;
    let target = settings_to_look(&target_settings, "Reference target".into());
    let global = amount.clamp(0.0, 1.0);
    let tone_mix = blend(
        &base,
        &target,
        global * tone.clamp(0.0, 1.0),
        "Reference tone",
    );
    let color_mix = blend(
        &base,
        &target,
        global * color.clamp(0.0, 1.0),
        "Reference color",
    );
    let grading_mix = blend(
        &base,
        &target,
        global * grading.clamp(0.0, 1.0),
        "Reference grading",
    );
    let mut combined = base;
    combined.tone = tone_mix.tone;
    combined.curves = tone_mix.curves;
    combined.relative_color = color_mix.relative_color;
    combined.color_mixer = color_mix.color_mixer;
    combined.grading = grading_mix.grading;
    apply_look(settings, &combined);
}

#[tauri::command]
fn native_reference_match(
    request: NativeReferenceMatchRequest,
) -> Result<NativeReferenceMatchResponse, String> {
    if same_file(&request.source_path, &request.reference_path) {
        return Err("reference image must be different from the source image".into());
    }
    if ![
        request.amount,
        request.tone,
        request.color,
        request.grading,
        request.protect_skin,
    ]
    .into_iter()
    .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
    {
        return Err("reference controls must be finite values in 0..1".into());
    }
    let render_settings = request.settings.clone().validated()?;
    let edge = request.max_edge.clamp(256, 2048);
    let source = decode_source_preview(&request.source_path, edge)
        .map_err(|error| format!("reference source decode failed: {error}"))?;
    let reference = decode_source_preview(&request.reference_path, edge)
        .map_err(|error| format!("reference target decode failed: {error}"))?;
    let source_analysis = analyze(
        &prepare_source_for_ai_denoise(&source, &render_settings)
            .map_err(|error| format!("reference source native graph failed: {error}"))?,
    )
    .map_err(|error| error.to_string())?;
    let reference_analysis = analyze(
        &prepare_source_for_ai_denoise(&reference, &RenderSettings::default())
            .map_err(|error| format!("reference target native graph failed: {error}"))?,
    )
    .map_err(|error| error.to_string())?;
    let recipe = match_reference(&source_analysis, &reference_analysis, request.protect_skin)
        .map_err(|error| error.to_string())?;
    let mut settings = request.settings;
    apply_reference_recipe(
        &mut settings,
        &recipe,
        request.amount,
        request.tone,
        request.color,
        request.grading,
    );
    settings.clone().validated()?;
    Ok(NativeReferenceMatchResponse {
        settings,
        source_analysis,
        reference_analysis,
        recipe,
    })
}

fn settings_to_look(settings: &NativeEditSettings, name: String) -> PortableLook {
    PortableLook {
        id: format!("look-{:x}", Sha256::digest(name.as_bytes())),
        name,
        tone: ToneParameters {
            exposure_ev: settings.exposure,
            contrast: settings.contrast / 100.0,
            highlights: settings.highlights / 100.0,
            shadows: settings.shadows / 100.0,
            whites: settings.whites / 100.0,
            blacks: settings.blacks / 100.0,
        },
        relative_color: PortableRelativeColor {
            temperature: settings.temperature / 100.0,
            tint: settings.tint / 100.0,
            vibrance: settings.vibrance / 100.0,
            saturation: settings.saturation / 100.0,
        },
        curves: PortableCurves {
            master: settings.curves.master.clone(),
            red: settings.curves.red.clone(),
            green: settings.curves.green.clone(),
            blue: settings.curves.blue.clone(),
        },
        color_mixer: settings.color_mixer,
        grading: settings.grading,
        denoise: settings.denoise_settings,
        local_detail: settings.local_detail,
        sharpen: settings.sharpen_settings,
        grain: settings.grain,
        vignette: settings.vignette,
        ..Default::default()
    }
}

/// Native UI controls persist percentage values while portable Looks use normalized units.
/// Quantizing the reverse conversion to four percentage decimals prevents representational
/// noise such as `30.0 -> 0.3 -> 30.000002` from mutating untouched categories or undo state.
fn unit_to_percent(value: f32) -> f32 {
    (value * 1_000_000.0).round() / 10_000.0
}

fn apply_look(settings: &mut NativeEditSettings, look: &PortableLook) {
    settings.exposure = look.tone.exposure_ev;
    settings.contrast = unit_to_percent(look.tone.contrast);
    settings.highlights = unit_to_percent(look.tone.highlights);
    settings.shadows = unit_to_percent(look.tone.shadows);
    settings.whites = unit_to_percent(look.tone.whites);
    settings.blacks = unit_to_percent(look.tone.blacks);
    settings.temperature = unit_to_percent(look.relative_color.temperature);
    settings.tint = unit_to_percent(look.relative_color.tint);
    settings.vibrance = unit_to_percent(look.relative_color.vibrance);
    settings.saturation = unit_to_percent(look.relative_color.saturation);
    settings.curve = look.curves.master.clone();
    settings.curves = ToneCurveSet {
        master: look.curves.master.clone(),
        red: look.curves.red.clone(),
        green: look.curves.green.clone(),
        blue: look.curves.blue.clone(),
    };
    settings.color_mixer = look.color_mixer;
    settings.grading = look.grading;
    settings.denoise_settings = look.denoise;
    settings.local_detail = look.local_detail;
    settings.sharpen_settings = look.sharpen;
    settings.grain = look.grain;
    settings.vignette = look.vignette;
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeLookSaveRequest {
    path: PathBuf,
    name: String,
    settings: NativeEditSettings,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeLookApplyRequest {
    path: PathBuf,
    amount: f32,
    settings: NativeEditSettings,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeLookMixRequest {
    path_a: PathBuf,
    path_b: PathBuf,
    weight_a: f32,
    weight_b: f32,
    amount: f32,
    settings: NativeEditSettings,
}

fn read_portable_look(path: &Path) -> Result<PortableLook, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|error| format!("look load failed for {}: {error}", path.display()))?;
    PortableLook::from_json(&json).map_err(|error| error.to_string())
}

#[tauri::command]
fn native_look_save(request: NativeLookSaveRequest) -> Result<String, String> {
    request.settings.clone().validated()?;
    let look = settings_to_look(&request.settings, request.name);
    let json = look.to_json().map_err(|error| error.to_string())?;
    std::fs::write(&request.path, json).map_err(|error| format!("look save failed: {error}"))?;
    Ok(request.path.display().to_string())
}

#[tauri::command]
fn native_look_apply(request: NativeLookApplyRequest) -> Result<NativeEditSettings, String> {
    if !request.amount.is_finite() || !(0.0..=1.0).contains(&request.amount) {
        return Err("look amount must stay inside 0..1".into());
    }
    let target = read_portable_look(&request.path)?;
    let current = settings_to_look(&request.settings, "Current".into());
    let applied = blend(&current, &target, request.amount, target.name.clone());
    let mut settings = request.settings;
    apply_look(&mut settings, &applied);
    settings.clone().validated()?;
    Ok(settings)
}

#[tauri::command]
fn native_look_mix(request: NativeLookMixRequest) -> Result<NativeEditSettings, String> {
    if !request.amount.is_finite() || !(0.0..=1.0).contains(&request.amount) {
        return Err("look amount must stay inside 0..1".into());
    }
    let a = read_portable_look(&request.path_a)?;
    let b = read_portable_look(&request.path_b)?;
    let target = mix_weighted(&a, &b, request.weight_a, request.weight_b, "Style Mix")
        .map_err(|error| error.to_string())?;
    let current = settings_to_look(&request.settings, "Current".into());
    let applied = blend(&current, &target, request.amount, target.name.clone());
    let mut settings = request.settings;
    apply_look(&mut settings, &applied);
    settings.clone().validated()?;
    Ok(settings)
}

fn source_content_hash(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("source hash read failed: {error}"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn source_rgba_for_portrait(path: &Path) -> Result<(u32, u32, Vec<u8>, String), PortraitError> {
    // M16 identity is source-image space, never the M13 preview-pyramid size.
    let decoded = decode_source(path)
        .map_err(|error| PortraitError::DetectionFailed(format!("source decode: {error}")))?;
    let rendered =
        render_source_export_to_srgb8(&decoded, &RenderSettings::default()).map_err(|error| {
            PortraitError::DetectionFailed(format!("source display transform: {error}"))
        })?;
    let mut rgba = Vec::with_capacity(rendered.width as usize * rendered.height as usize * 4);
    for rgb in rendered.data.as_chunks::<3>().0 {
        rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    let identity = preview_source_identity(path).map_err(PortraitError::DetectionFailed)?;
    Ok((rendered.width, rendered.height, rgba, identity))
}

fn collect_portrait_mask_references(
    tree: &MaskTree,
    values: &mut Vec<(String, String, PortraitMaskRegion)>,
) {
    match tree {
        MaskTree::Leaf(MaskDefinition::PortraitSemantic {
            face_id,
            region,
            cache_key,
            ..
        }) => values.push((cache_key.clone(), face_id.clone(), *region)),
        MaskTree::Leaf(_) => {}
        MaskTree::Composite(composite) => {
            for child in &composite.children {
                collect_portrait_mask_references(child, values);
            }
        }
    }
}

fn collect_generated_mask_references(
    tree: &MaskTree,
    values: &mut Vec<(String, GeneratedMaskSemantic)>,
) {
    match tree {
        MaskTree::Leaf(MaskDefinition::Generated {
            cache_identity,
            semantic_class,
            ..
        }) => {
            values.push((cache_identity.clone(), *semantic_class));
        }
        MaskTree::Leaf(_) => {}
        MaskTree::Composite(composite) => {
            for child in &composite.children {
                collect_generated_mask_references(child, values);
            }
        }
    }
}

fn attach_generated_masks(
    settings: &mut RenderSettings,
    runtime: &NativeAiMaskRuntime,
) -> Result<(), String> {
    let mut references = Vec::new();
    for layer in &settings.layers {
        collect_generated_mask_references(&layer.mask, &mut references);
    }
    if references.is_empty() {
        return Ok(());
    }
    let cache = runtime
        .cache
        .lock()
        .map_err(|_| "AI mask cache lock was poisoned".to_owned())?;
    for (cache_identity, semantic) in references {
        let generated = cache
            .get(&cache_identity)
            .ok_or_else(|| format!("AI mask cache is unavailable: {cache_identity}"))?;
        let expected = match generated.semantic {
            AiMaskSemantic::Subject => GeneratedMaskSemantic::Subject,
            AiMaskSemantic::Background => GeneratedMaskSemantic::Background,
            AiMaskSemantic::Person => GeneratedMaskSemantic::Person,
            AiMaskSemantic::Sky => GeneratedMaskSemantic::Sky,
            AiMaskSemantic::Skin => GeneratedMaskSemantic::Skin,
            AiMaskSemantic::Hair => GeneratedMaskSemantic::Hair,
        };
        if expected != semantic {
            return Err(format!("AI mask semantic cache mismatch: {cache_identity}"));
        }
        settings.generated_masks.push(GeneratedMaskRaster {
            cache_identity,
            semantic,
            width: generated.mask.width,
            height: generated.mask.height,
            values: generated.mask.values.clone(),
        });
    }
    Ok(())
}

fn attach_portrait_masks(
    settings: &mut RenderSettings,
    runtime: &NativePortraitRuntime,
) -> Result<(), String> {
    let mut references = Vec::new();
    for layer in &settings.layers {
        collect_portrait_mask_references(&layer.mask, &mut references);
    }
    for face in &settings.skin_retouch.faces {
        for region in [
            PortraitMaskRegion::Skin,
            PortraitMaskRegion::Eyes,
            PortraitMaskRegion::LeftEye,
            PortraitMaskRegion::RightEye,
            PortraitMaskRegion::Brows,
            PortraitMaskRegion::LeftBrow,
            PortraitMaskRegion::RightBrow,
            PortraitMaskRegion::Lips,
            PortraitMaskRegion::Mouth,
            PortraitMaskRegion::Hair,
        ] {
            references.push((face.cache_key.clone(), face.face_id.clone(), region));
        }
    }
    if references.is_empty() {
        return Ok(());
    }
    let state = runtime
        .0
        .lock()
        .map_err(|_| "portrait runtime lock was poisoned".to_owned())?;
    for (cache_key, face_id, region) in references {
        let parse = state
            .parsed
            .get(&cache_key)
            .ok_or_else(|| format!("portrait semantic cache is unavailable: {cache_key}"))?;
        let source_region = match region {
            PortraitMaskRegion::Face => PortraitRegion::Face,
            PortraitMaskRegion::Skin => PortraitRegion::Skin,
            PortraitMaskRegion::Eyes => PortraitRegion::Eyes,
            PortraitMaskRegion::LeftEye => PortraitRegion::LeftEye,
            PortraitMaskRegion::RightEye => PortraitRegion::RightEye,
            PortraitMaskRegion::Brows => PortraitRegion::Brows,
            PortraitMaskRegion::LeftBrow => PortraitRegion::LeftBrow,
            PortraitMaskRegion::RightBrow => PortraitRegion::RightBrow,
            PortraitMaskRegion::Lips => PortraitRegion::Lips,
            PortraitMaskRegion::Mouth => PortraitRegion::Mouth,
            PortraitMaskRegion::Hair => PortraitRegion::Hair,
        };
        let mask = parse
            .regions
            .get(&source_region)
            .ok_or_else(|| format!("portrait semantic region is unavailable: {cache_key}"))?;
        settings.portrait_masks.push(PortraitMaskRaster {
            cache_key,
            face_id,
            region,
            width: mask.width,
            height: mask.height,
            values: mask.values.clone(),
        });
    }
    Ok(())
}

#[tauri::command]
fn portrait_detect(
    runtime: State<'_, NativePortraitRuntime>,
    request: PortraitDetectRequest,
) -> PortraitDetectionResponse {
    let registry = local_portrait_models();
    let response_shell = |status, error: Option<PortraitError>| PortraitDetectionResponse {
        status,
        faces: Vec::new(),
        detector_model_id: registry.detector.id.clone(),
        detector_model_version: registry.detector.version.clone(),
        detector_model_hash: registry.detector.sha256.clone(),
        parser_model_id: registry.parser.id.clone(),
        parser_model_version: registry.parser.version.clone(),
        parser_model_hash: registry.parser.sha256.clone(),
        execution_provider: registry.execution_provider,
        error: error.map(Into::into),
    };
    if !request.face_crop_scale.is_finite() || !(1.0..=3.0).contains(&request.face_crop_scale) {
        return response_shell(
            "failed",
            Some(PortraitError::InvalidTransform(
                "face crop scale must be 1.0..3.0".into(),
            )),
        );
    }
    let (width, height, rgba, source_identity) =
        match source_rgba_for_portrait(&request.source_path) {
            Ok(value) => value,
            Err(error) => return response_shell("failed", Some(error)),
        };
    let mut state = match runtime.0.lock() {
        Ok(state) => state,
        Err(_) => {
            return response_shell(
                "failed",
                Some(PortraitError::RuntimeUnavailable(
                    "portrait runtime lock was poisoned".into(),
                )),
            );
        }
    };
    if state.provider.is_none() {
        match PortraitOnnxProvider::initialize(registry.clone()) {
            Ok(provider) => state.provider = Some(provider),
            Err(error) => return response_shell("unavailable", Some(error)),
        }
    }
    let (response_faces, parsed_results, execution_provider) = {
        let provider = state.provider.as_mut().expect("initialized above");
        let faces = match provider.detect(
            width,
            height,
            &rgba,
            request.face_crop_scale,
            &source_identity,
        ) {
            Ok(value) => value,
            Err(PortraitError::NoFaceDetected) => {
                return response_shell("noFace", Some(PortraitError::NoFaceDetected));
            }
            Err(error) => return response_shell("failed", Some(error)),
        };
        let mut response_faces = Vec::with_capacity(faces.len());
        let mut parsed_results = Vec::with_capacity(faces.len());
        for face in faces {
            let parsed = match provider.parse(width, height, &rgba, &face, &source_identity) {
                Ok(value) => value,
                Err(error) => return response_shell("failed", Some(error)),
            };
            let cache_key = format!(
                "{}:{}",
                parsed.cache_key.face_id, parsed.cache_key.crop_transform_hash
            );
            response_faces.push(PortraitFaceResponse {
                face,
                cache_key: cache_key.clone(),
            });
            parsed_results.push((cache_key, parsed));
        }
        (response_faces, parsed_results, provider.execution_provider)
    };
    for (cache_key, parsed) in parsed_results {
        state.parsed.insert(cache_key, parsed);
    }
    PortraitDetectionResponse {
        status: "ready",
        faces: response_faces,
        detector_model_id: registry.detector.id,
        detector_model_version: registry.detector.version,
        detector_model_hash: registry.detector.sha256,
        parser_model_id: registry.parser.id,
        parser_model_version: registry.parser.version,
        parser_model_hash: registry.parser.sha256,
        execution_provider,
        error: None,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiMaskGenerateRequest {
    source_path: PathBuf,
    semantic: AiMaskSemantic,
    request_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiMaskGenerateResponse {
    status: &'static str,
    provider_id: String,
    model_id: String,
    model_version: String,
    model_hash: String,
    semantic_class: AiMaskSemantic,
    cache_identity: String,
    execution_provider: starroom_portrait::ExecutionProvider,
}

#[tauri::command]
fn ai_mask_generate(
    runtime: State<'_, NativeAiMaskRuntime>,
    request: AiMaskGenerateRequest,
) -> Result<AiMaskGenerateResponse, AiMaskFailure> {
    if request.request_id.trim().is_empty() {
        return Err(AiMaskError::InvalidTensor("request id is empty".into()).into());
    }
    if matches!(
        request.semantic,
        AiMaskSemantic::Person | AiMaskSemantic::Skin | AiMaskSemantic::Hair
    ) {
        return Err(AiMaskError::PortraitProviderRequired(request.semantic).into());
    }
    let source_hash = source_content_hash(&request.source_path)
        .map_err(|error| AiMaskFailure::from(AiMaskError::InferenceFailed(error)))?;
    {
        let cache = runtime.cache.lock().map_err(|_| {
            AiMaskFailure::from(AiMaskError::RuntimeUnavailable(
                "cache lock poisoned".into(),
            ))
        })?;
        if let Some(result) = cache.values().find(|result| {
            result.semantic == request.semantic
                && result.cache_identity
                    == format!(
                        "{:x}",
                        Sha256::digest(
                            format!(
                                "{source_hash}:ai-mask-v1:{:?}:{}",
                                request.semantic, result.model_hash
                            )
                            .as_bytes()
                        )
                    )
        }) {
            return Ok(AiMaskGenerateResponse {
                status: "cached",
                provider_id: result.provider_id.clone(),
                model_id: result.model_id.clone(),
                model_version: result.model_version.clone(),
                model_hash: result.model_hash.clone(),
                semantic_class: result.semantic,
                cache_identity: result.cache_identity.clone(),
                execution_provider: result.execution_provider,
            });
        }
    }
    let (width, height, rgba, _) = source_rgba_for_portrait(&request.source_path)
        .map_err(|error| AiMaskFailure::from(AiMaskError::InferenceFailed(error.to_string())))?;
    let mut provider_guard = runtime.provider.lock().map_err(|_| {
        AiMaskFailure::from(AiMaskError::RuntimeUnavailable(
            "provider lock poisoned".into(),
        ))
    })?;
    if provider_guard.is_none() {
        *provider_guard = Some(
            AiMaskOnnxProvider::initialize(local_ai_mask_models()).map_err(AiMaskFailure::from)?,
        );
    }
    let token = cancellation_token();
    runtime
        .cancellations
        .lock()
        .map_err(|_| {
            AiMaskFailure::from(AiMaskError::RuntimeUnavailable(
                "cancellation lock poisoned".into(),
            ))
        })?
        .insert(request.request_id.clone(), Arc::clone(&token));
    let generated = provider_guard
        .as_mut()
        .expect("initialized above")
        .generate(width, height, &rgba, &source_hash, request.semantic, &token);
    drop(provider_guard);
    runtime
        .cancellations
        .lock()
        .map_err(|_| {
            AiMaskFailure::from(AiMaskError::RuntimeUnavailable(
                "cancellation lock poisoned".into(),
            ))
        })?
        .remove(&request.request_id);
    let result = generated.map_err(AiMaskFailure::from)?;
    runtime
        .cache
        .lock()
        .map_err(|_| {
            AiMaskFailure::from(AiMaskError::RuntimeUnavailable(
                "cache lock poisoned".into(),
            ))
        })?
        .insert(result.cache_identity.clone(), result.clone());
    Ok(AiMaskGenerateResponse {
        status: "ready",
        provider_id: result.provider_id,
        model_id: result.model_id,
        model_version: result.model_version,
        model_hash: result.model_hash,
        semantic_class: result.semantic,
        cache_identity: result.cache_identity,
        execution_provider: result.execution_provider,
    })
}

#[tauri::command]
fn ai_mask_cancel(runtime: State<'_, NativeAiMaskRuntime>, request_id: String) -> bool {
    runtime
        .cancellations
        .lock()
        .ok()
        .and_then(|tokens| tokens.get(&request_id).cloned())
        .is_some_and(|token| {
            token.store(true, Ordering::Relaxed);
            true
        })
}

/// Explicit M13 diagnostics for progressive preview scheduling. The UI can expose cache and
/// stale-frame statistics without receiving pixels through JSON.
#[tauri::command]
fn native_preview_scheduler_status(
    scheduler: State<'_, NativePreviewScheduler>,
) -> Result<SchedulerStatus, String> {
    scheduler
        .0
        .lock()
        .map_err(|_| "native preview scheduler lock was poisoned".to_owned())
        .map(|scheduler| scheduler.status())
}

#[tauri::command]
fn native_preview(
    scheduler: State<'_, NativePreviewScheduler>,
    portrait_runtime: State<'_, NativePortraitRuntime>,
    ai_mask_runtime: State<'_, NativeAiMaskRuntime>,
    ai_denoise_runtime: State<'_, NativeAiDenoiseRuntime>,
    request: NativePreviewRequest,
) -> Result<Response, String> {
    let source_identity = preview_source_identity(&request.source_path)?;
    let graph_identity = RenderCacheIdentity {
        source_identity: source_identity.clone(),
        render_state: serde_json::to_string(&request.settings).map_err(|error| {
            format!("native preview render identity serialization failed: {error}")
        })?,
        layer_state: serde_json::to_string(&request.settings.layers).map_err(|error| {
            format!("native preview layer identity serialization failed: {error}")
        })?,
        mask_identity: serde_json::to_string(&(
            &request.settings.layers,
            &request.settings.skin_retouch,
            &request.settings.healing_operations,
        ))
        .map_err(|error| format!("native preview mask identity serialization failed: {error}"))?,
        geometry_state: serde_json::to_string(&request.settings.geometry).map_err(|error| {
            format!("native preview geometry identity serialization failed: {error}")
        })?,
        color_transform: "display:srgb:relative-colorimetric:bpc".into(),
    }
    .fingerprint();
    let requested_denoise_provider = request.settings.ai_denoise_provider;
    let mut settings = request.settings.validated()?;
    attach_portrait_masks(&mut settings, &portrait_runtime)?;
    attach_generated_masks(&mut settings, &ai_mask_runtime)?;
    let requested_edge = preview_requested_edge(request.max_edge, request.interaction_phase);
    let level = starroom_render::scheduler::PreviewLevel::for_requested_edge(requested_edge);
    let decoded = decode_source_preview(&request.source_path, level.max_edge())
        .map_err(|error| format!("native preview decode failed: {error}"))?;
    attach_ai_denoise(
        &decoded,
        &request.source_path,
        &mut settings,
        requested_denoise_provider,
        &request.request_id,
        &ai_denoise_runtime,
    )?;
    let (source_width, source_height) = source_dimensions(&decoded);
    let job = scheduler
        .0
        .lock()
        .map_err(|_| "native preview scheduler lock was poisoned".to_owned())?
        .schedule_preview(
            source_identity,
            graph_identity,
            source_width,
            source_height,
            requested_edge,
            Viewport::full(source_width, source_height),
            DEFAULT_TILE_EDGE,
            RenderGraph::default().maximum_halo(),
        );
    let frame_tile = job.full_frame_tile();
    if let Some(frame) = scheduler
        .0
        .lock()
        .map_err(|_| "native preview scheduler lock was poisoned".to_owned())?
        .cached_tile(&frame_tile.identity)
    {
        return Ok(Response::new(frame));
    }
    let (rendered, backend_flags) = if request.prefer_gpu {
        match GpuRenderer::try_new() {
            Ok(renderer) => {
                match render_source_preview_with_gpu_to_srgb8(&decoded, &settings, &renderer) {
                    Ok(rendered) => {
                        let flag = match renderer.status().backend {
                            GpuBackendKind::Dx12 | GpuBackendKind::Other => 0x0008,
                            GpuBackendKind::CpuFallback => 0x0010,
                        };
                        (rendered, flag)
                    }
                    Err(error) => {
                        let rendered = render_source_preview_to_srgb8(&decoded, &settings)
                        .map_err(|fallback| format!("native GPU preview failed ({error}); CPU reference fallback also failed: {fallback}"))?;
                        (rendered, 0x0010)
                    }
                }
            }
            Err(_) => {
                let rendered = render_source_preview_to_srgb8(&decoded, &settings)
                    .map_err(|error| format!("native CPU preview graph failed after GPU initialization fallback: {error}"))?;
                (rendered, 0x0010)
            }
        }
    } else {
        let rendered = render_source_preview_to_srgb8(&decoded, &settings)
            .map_err(|error| format!("native CPU preview graph failed: {error}"))?;
        (rendered, 0x0010)
    };
    let flags = profile_flag(rendered.color.input) | backend_flags;
    let profile_id = rendered.color.camera_profile_id.as_deref().unwrap_or("");
    let jpeg = encode_jpeg_rgb8(&rendered.data, rendered.width, rendered.height, 91, None)
        .map_err(|error| format!("native preview encode failed: {error}"))?;
    let frame = preview_frame(rendered.width, rendered.height, flags, profile_id, jpeg)?;
    let estimated_vram_bytes = rendered.width as usize * rendered.height as usize * 8;
    let completion = scheduler
        .0
        .lock()
        .map_err(|_| "native preview scheduler lock was poisoned".to_owned())?
        .complete_tile(&frame_tile, frame.clone(), estimated_vram_bytes);
    if completion == Completion::Stale {
        return Err("native preview was superseded by a newer render request".into());
    }
    Ok(Response::new(frame))
}

fn same_file(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[tauri::command]
fn native_export_jpeg(
    portrait_runtime: State<'_, NativePortraitRuntime>,
    ai_mask_runtime: State<'_, NativeAiMaskRuntime>,
    ai_denoise_runtime: State<'_, NativeAiDenoiseRuntime>,
    request: NativeExportRequest,
) -> Result<NativeExportResult, String> {
    if same_file(&request.source_path, &request.output_path) {
        return Err("export destination must not overwrite the source image".into());
    }
    let requested_denoise_provider = request.settings.ai_denoise_provider;
    let mut settings = request.settings.validated()?;
    attach_portrait_masks(&mut settings, &portrait_runtime)?;
    attach_generated_masks(&mut settings, &ai_mask_runtime)?;
    let decoded = decode_source(&request.source_path)
        .map_err(|error| format!("native export decode failed: {error}"))?;
    attach_ai_denoise(
        &decoded,
        &request.source_path,
        &mut settings,
        requested_denoise_provider,
        &request.request_id,
        &ai_denoise_runtime,
    )?;
    let rendered = render_source_export_to_srgb8(&decoded, &settings)
        .map_err(|error| format!("native export graph failed: {error}"))?;
    let input_profile = rendered
        .color
        .camera_profile_id
        .clone()
        .unwrap_or_else(|| match rendered.color.input {
            starroom_color_management::InputProfileSource::EmbeddedIcc => "embedded ICC".into(),
            starroom_color_management::InputProfileSource::AssumedSrgb => "assumed sRGB".into(),
            starroom_color_management::InputProfileSource::RawCameraMatrix => {
                "resolved RAW camera profile".into()
            }
            starroom_color_management::InputProfileSource::RawGenericProfile => {
                "Generic RAW Profile".into()
            }
        });
    let jpeg = encode_jpeg_rgb8(
        &rendered.data,
        rendered.width,
        rendered.height,
        request.quality.clamp(1, 100),
        None,
    )
    .map_err(|error| format!("native export encode failed: {error}"))?;
    std::fs::write(&request.output_path, jpeg)
        .map_err(|error| format!("native export write failed: {error}"))?;
    Ok(NativeExportResult {
        output_path: request.output_path,
        width: rendered.width,
        height: rendered.height,
        input_profile,
        camera_profile_hash: rendered.color.camera_profile_hash,
        working_space: rendered.color.working_space,
    })
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeExportProgress {
    running: bool,
    progress: BatchProgress,
}

#[derive(Clone)]
struct NativeExportRuntime {
    cancelled: Arc<AtomicBool>,
    progress: Arc<Mutex<NativeExportProgress>>,
}

impl Default for NativeExportRuntime {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(Mutex::new(NativeExportProgress::default())),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfessionalExportItemRequest {
    asset_id: i64,
    source_path: PathBuf,
    original_name: String,
    capture_date: Option<String>,
    rating: u8,
    keywords: Vec<String>,
    camera: Option<String>,
    look: Option<String>,
    sequence: u32,
    source_fingerprint: String,
    edit_state_identity: String,
    edit_settings: NativeEditSettings,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfessionalExportBatchRequest {
    destination_directory: PathBuf,
    settings: ExportSettings,
    items: Vec<ProfessionalExportItemRequest>,
}

#[tauri::command]
async fn native_export_batch(
    portrait_runtime: State<'_, NativePortraitRuntime>,
    ai_mask_runtime: State<'_, NativeAiMaskRuntime>,
    ai_denoise_runtime: State<'_, NativeAiDenoiseRuntime>,
    export_runtime: State<'_, NativeExportRuntime>,
    request: ProfessionalExportBatchRequest,
) -> Result<BatchExportResult, String> {
    export_runtime.cancelled.store(false, Ordering::Relaxed);
    let total = request.items.len();
    set_export_progress(&export_runtime, true, total, &BatchExportResult::default())?;
    let mut result = BatchExportResult::default();
    for item in request.items {
        let professional = ProfessionalExportRequest {
            asset_id: item.asset_id,
            source_path: item.source_path.clone(),
            destination_directory: request.destination_directory.clone(),
            original_name: item.original_name,
            capture_date: item.capture_date,
            rating: item.rating,
            keywords: item.keywords,
            camera: item.camera,
            look: item.look,
            sequence: item.sequence,
            source_fingerprint: item.source_fingerprint,
            edit_state_identity: item.edit_state_identity,
            settings: request.settings.clone(),
        };
        if export_runtime.cancelled.load(Ordering::Relaxed) {
            result.cancelled.push(export_failure(
                &professional,
                ExportItemStatus::Cancelled,
                "Cancelled",
            ));
            set_export_progress(&export_runtime, true, total, &result)?;
            continue;
        }
        let prepared = (|| -> Result<RenderSettings, String> {
            let requested_provider = item.edit_settings.ai_denoise_provider;
            let mut settings = item.edit_settings.validated()?;
            attach_portrait_masks(&mut settings, &portrait_runtime)?;
            attach_generated_masks(&mut settings, &ai_mask_runtime)?;
            let decoded = decode_source(&item.source_path).map_err(|error| {
                format!("SourceMissing: {}: {error}", item.source_path.display())
            })?;
            attach_ai_denoise(
                &decoded,
                &item.source_path,
                &mut settings,
                requested_provider,
                &format!("export-{}-{}", item.asset_id, item.sequence),
                &ai_denoise_runtime,
            )?;
            Ok(settings)
        })();
        let settings = match prepared {
            Ok(settings) => settings,
            Err(error) => {
                result.failed.push(export_failure(
                    &professional,
                    ExportItemStatus::Failed,
                    &error,
                ));
                set_export_progress(&export_runtime, true, total, &result)?;
                continue;
            }
        };
        let cancelled = Arc::clone(&export_runtime.cancelled);
        let item_result = tauri::async_runtime::spawn_blocking(move || {
            export_one(
                &NativeSharedGraphRenderer,
                &professional,
                &settings,
                &cancelled,
            )
            .map_err(|error| (professional, error))
        })
        .await
        .map_err(|error| format!("AtomicWriteFailed: export worker failed: {error}"))?;
        match item_result {
            Ok(item) => result.completed.push(item),
            Err((request, starroom_export::ExportError::Cancelled)) => result.cancelled.push(
                export_failure(&request, ExportItemStatus::Cancelled, "Cancelled"),
            ),
            Err((request, error)) => result.failed.push(export_failure(
                &request,
                ExportItemStatus::Failed,
                &error.to_string(),
            )),
        }
        set_export_progress(&export_runtime, true, total, &result)?;
    }
    set_export_progress(&export_runtime, false, total, &result)?;
    Ok(result)
}

fn set_export_progress(
    runtime: &NativeExportRuntime,
    running: bool,
    total: usize,
    result: &BatchExportResult,
) -> Result<(), String> {
    let processed = result.completed.len() + result.failed.len() + result.cancelled.len();
    *runtime
        .progress
        .lock()
        .map_err(|_| "export progress lock was poisoned".to_owned())? = NativeExportProgress {
        running,
        progress: BatchProgress {
            processed,
            total,
            completed: result.completed.len(),
            failed: result.failed.len(),
            cancelled: result.cancelled.len(),
        },
    };
    Ok(())
}

#[tauri::command]
fn native_export_progress(
    runtime: State<'_, NativeExportRuntime>,
) -> Result<NativeExportProgress, String> {
    runtime
        .progress
        .lock()
        .map(|progress| *progress)
        .map_err(|_| "export progress lock was poisoned".to_owned())
}

fn export_failure(
    request: &ProfessionalExportRequest,
    status: ExportItemStatus,
    error: &str,
) -> ExportItemResult {
    ExportItemResult {
        asset_id: request.asset_id,
        status,
        destination: None,
        width: None,
        height: None,
        recipe_identity: export_recipe_identity(request).unwrap_or_default(),
        error: Some(error.into()),
    }
}

#[tauri::command]
fn native_export_cancel(runtime: State<'_, NativeExportRuntime>) -> bool {
    runtime.cancelled.store(true, Ordering::Relaxed);
    true
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(NativePreviewScheduler::default())
        .manage(NativePortraitRuntime::default())
        .manage(NativeAiMaskRuntime::default())
        .manage(NativeAiDenoiseRuntime::default())
        .manage(NativeLibraryRuntime::default())
        .manage(NativeHistoryRuntime::default())
        .manage(NativeExportRuntime::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            engine_status,
            engine_capabilities,
            ai_denoise_status,
            gpu_preview_status,
            advise_image,
            advise_native_image,
            native_preview,
            native_preview_scheduler_status,
            native_export_jpeg,
            portrait_detect,
            ai_mask_generate,
            ai_mask_cancel,
            ai_denoise_cancel,
            native_sample_color,
            native_optics_status,
            native_reference_match,
            native_look_save,
            native_look_apply,
            native_look_mix,
            library_open_default,
            library_import_folder,
            library_cancel_import,
            library_query,
            library_set_workflow,
            library_add_keywords,
            library_remove_keywords,
            library_collections,
            library_collection_create,
            library_collection_add_assets,
            library_collection_assets,
            library_thumbnail,
            history_open,
            history_commit,
            history_undo,
            history_redo,
            history_snapshot_create,
            history_snapshot_restore,
            history_snapshot_rename,
            history_snapshot_delete,
            native_export_batch,
            native_export_cancel,
            native_export_progress
        ])
        .run(tauri::generate_context!())
        .expect("error while running Starroom");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> NativeEditSettings {
        NativeEditSettings {
            exposure: 0.5,
            contrast: 10.0,
            highlights: -20.0,
            shadows: 25.0,
            whites: 0.0,
            blacks: 0.0,
            temperature: 30.0,
            tint: -10.0,
            vibrance: 0.0,
            saturation: 0.0,
            sharpness: 0.0,
            noise_reduction: 0.0,
            white_balance_mode: WhiteBalanceMode::SourceDefault,
            white_balance_sample: None,
            curve: vec![CurvePoint { x: 0.0, y: 0.0 }, CurvePoint { x: 1.0, y: 1.0 }],
            curves: ToneCurveSet::default(),
            color_mixer: ColorMixer::default(),
            grading: GradingParameters::default(),
            sharpen_settings: SharpenParameters {
                amount: 0.0,
                ..Default::default()
            },
            denoise_settings: DenoiseParameters::default(),
            ai_denoise: AiDenoiseParameters::default(),
            ai_denoise_provider: DenoiseExecutionProvider::Cpu,
            local_detail: LocalDetailParameters::default(),
            optics: OpticsSettings::default(),
            geometry: GeometryParameters::default(),
            layers: Vec::new(),
            skin_retouch: SkinRetouchSettings::default(),
            healing_operations: Vec::new(),
            grain: GrainSettings::default(),
            vignette: VignetteSettings::default(),
        }
    }

    #[test]
    fn ui_contract_maps_exposure_wb_tone_and_curve_into_shared_settings() {
        let settings = settings().validated().expect("valid settings");
        assert_eq!(settings.tone.exposure_ev, 0.5);
        assert_eq!(settings.tone.contrast, 0.1);
        assert_eq!(settings.tone.highlights, -0.2);
        assert_eq!(settings.tone.shadows, 0.25);
        assert_eq!(settings.relative_color.temperature, 0.3);
        assert_eq!(settings.relative_color.tint, -0.1);
        assert_eq!(settings.curve.len(), 2);
        assert_eq!(settings.color_mixer, ColorMixer::default());
    }

    #[test]
    fn binary_preview_contract_has_fixed_header_and_payload_length() {
        let profile = "dng-forward-matrix:test:camera";
        let frame = preview_frame(640, 480, 2, profile, vec![0xff, 0xd8, 0xff]).expect("frame");
        assert_eq!(&frame[0..4], b"SRP2");
        assert_eq!(u32::from_le_bytes(frame[8..12].try_into().unwrap()), 640);
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 480);
        assert_eq!(
            u16::from_le_bytes(frame[16..18].try_into().unwrap()) as usize,
            profile.len()
        );
        assert_eq!(u32::from_le_bytes(frame[20..24].try_into().unwrap()), 3);
        assert_eq!(&frame[24..24 + profile.len()], profile.as_bytes());
        assert_eq!(&frame[24 + profile.len()..], &[0xff, 0xd8, 0xff]);
    }

    #[test]
    fn non_finite_settings_are_rejected_before_the_graph() {
        let mut settings = settings();
        settings.exposure = f32::NAN;
        assert!(settings.validated().is_err());
    }

    #[test]
    fn m28_interactive_preview_is_bounded_and_final_restores_requested_quality() {
        assert_eq!(
            preview_requested_edge(1800, PreviewInteractionPhase::Interactive),
            1024
        );
        assert_eq!(
            preview_requested_edge(1800, PreviewInteractionPhase::Final),
            1800
        );
        assert_eq!(
            preview_requested_edge(9000, PreviewInteractionPhase::Final),
            4096
        );
        assert_eq!(
            preview_requested_edge(1, PreviewInteractionPhase::Interactive),
            256
        );
    }

    #[test]
    fn layer_contract_rejects_duplicate_ids_before_native_rendering() {
        let mut settings = settings();
        settings.layers = vec![
            NativeAdjustmentLayer {
                id: "same".into(),
                name: "First".into(),
                enabled: true,
                opacity: 1.0,
                blend_mode: Default::default(),
                mask: starroom_project::MaskDefinition::None.into(),
                adjustments: Default::default(),
            },
            NativeAdjustmentLayer {
                id: "same".into(),
                name: "Second".into(),
                enabled: true,
                opacity: 1.0,
                blend_mode: Default::default(),
                mask: starroom_project::MaskDefinition::None.into(),
                adjustments: Default::default(),
            },
        ];
        assert!(settings.validated().is_err());
    }

    #[test]
    fn reference_amount_zero_is_exact_and_category_amounts_are_isolated() {
        let recipe = ReferenceMatchRecipe {
            tone: ToneParameters {
                exposure_ev: 2.0,
                contrast: 0.5,
                ..Default::default()
            },
            curve: vec![CurvePoint { x: 0.0, y: 0.1 }, CurvePoint { x: 1.0, y: 1.0 }],
            white_balance: starroom_reference::RelativeWhiteBalance {
                temperature: 0.8,
                tint: 0.4,
            },
            color_mixer: ColorMixer::default(),
            grading: GradingParameters {
                global: starroom_grading::ColorWheel {
                    hue_degrees: 35.0,
                    chroma: 0.5,
                    lightness: 0.1,
                },
                ..Default::default()
            },
            protect_skin: 0.8,
            confidence: 0.9,
            source_fingerprint: "source".into(),
            reference_fingerprint: "reference".into(),
        };
        let serialized = serde_json::to_string(&recipe).unwrap();
        assert_eq!(
            serde_json::from_str::<ReferenceMatchRecipe>(&serialized).unwrap(),
            recipe
        );
        let mut zero = settings();
        let before = serde_json::to_string(&zero).unwrap();
        apply_reference_recipe(&mut zero, &recipe, 0.0, 1.0, 1.0, 1.0);
        assert_eq!(serde_json::to_string(&zero).unwrap(), before);

        let mut grading_only = settings();
        let original_exposure = grading_only.exposure;
        let original_temperature = grading_only.temperature;
        apply_reference_recipe(&mut grading_only, &recipe, 1.0, 0.0, 0.0, 1.0);
        assert_eq!(grading_only.exposure, original_exposure);
        assert_eq!(grading_only.temperature, original_temperature);
        assert!(grading_only.grading.global.chroma > 0.0);
    }

    #[test]
    fn reference_recipe_saved_as_look_reloads_to_the_same_portable_adjustments() {
        let source = settings();
        let look = settings_to_look(&source, "Reference Match".into());
        let reloaded = PortableLook::from_json(&look.to_json().unwrap()).unwrap();
        let mut target = settings();
        target.exposure = -2.0;
        apply_look(&mut target, &reloaded);
        assert_eq!(target.exposure, source.exposure);
        assert_eq!(target.temperature, source.temperature);
        assert_eq!(target.curves, source.curves);
        assert_eq!(target.grain, source.grain);
        assert_eq!(target.vignette, source.vignette);
    }
}
