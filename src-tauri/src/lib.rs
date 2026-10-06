use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use starroom_advisor::{
    AdvisorResult, AnalysisStats, Suggestion, advise, advise_detailed, analyze_detailed,
};
use starroom_ai_denoise::{
    AiDenoiseError, AiDenoiseParameters, AiDenoiseResidual,
    ExecutionProvider as DenoiseExecutionProvider, MODEL_ID as NAFNET_MODEL_ID,
    MODEL_SHA256 as NAFNET_MODEL_SHA256, MODEL_VERSION as NAFNET_MODEL_VERSION, NafNetOnnxProvider,
    directml_failure_allows_cpu_fallback, infer_tiled, inference_cache_key,
    verify_model as verify_ai_denoise_model,
};
use starroom_color::{ColorMixer, CurvePoint, ToneParameters};
use starroom_detail::{DenoiseParameters, LocalDetailParameters, SharpenParameters};
use starroom_export::{
    BatchExportResult, BatchProgress, ExportItemResult, ExportItemStatus,
    ExportRequest as ProfessionalExportRequest, ExportSettings, NativeSharedGraphRenderer,
    export_one, export_one_with_destination_guard, export_recipe_identity,
};
use starroom_geometry::GeometryParameters;
use starroom_grading::GradingParameters;
use starroom_heal::{HealMode, HealingOperation, SourceMode};
use starroom_history::{EditCommand, EditHistory, HistoryEntry, NamedSnapshot};
use starroom_imageio::{
    DecodedSourceImage, decode_source, decode_source_preview, decode_source_region,
    encode_jpeg_rgb8, is_raw_source,
};
use starroom_library::{
    AssetFlag, AssetMetadata, AssetRecord, CollectionKind, CollectionRecord, ColorLabel,
    ImportResult, Library, LibraryQuery, SmartCollectionRuleV1, ThumbnailSize,
};
use starroom_look::{
    GrainSettings, PortableCurves, PortableLook, PortableRelativeColor, VignetteSettings, blend,
    mix_weighted,
};
use starroom_optics::{LensProfileResolution, OpticsSettings};
use starroom_pipeline::{
    GeneratedMaskRaster, NativeAdjustmentLayer, PortraitMaskRaster, RelativeColorParameters,
    RenderSettings, SkinRetouchSettings, SourceRegion, ToneCurveSet, WhiteBalanceMode,
    WhiteBalanceSample, WhiteBalanceSettings, prepare_source_for_ai_denoise,
    render_source_export_to_srgb8, render_source_preview_to_srgb8,
    render_source_preview_with_gpu_to_srgb8, resolve_source_lens_profile, sample_source_color_band,
    sample_source_portrait_weights,
};
use starroom_portrait::{
    AiMaskError, AiMaskModelRegistry, AiMaskOnnxProvider, AiMaskProvider, AiMaskSemantic,
    DetectedFace, FaceCropTransform, GeneratedAiMask, PortraitError, PortraitModelRegistry,
    PortraitOnnxProvider, PortraitParseResult, PortraitRegion, cancellation_token,
};
use starroom_project::{
    GeneratedMaskSemantic, MaskDefinition, MaskTree, PortraitMaskRegion, PortraitSourceCrop,
};
use starroom_reference::{ReferenceAnalysis, ReferenceMatchRecipe, analyze, match_reference};
use starroom_render::{
    GpuStageCacheKeys, PixelRect, RenderGraph, StageId, StageStateIdentity,
    gpu::{GpuBackendKind, GpuRenderer, GpuStatus, probe_gpu_status},
    profiling::{self, ProfileStage, RenderProfile},
    scheduler::{
        Completion, DEFAULT_TILE_EDGE, RenderScheduler, SchedulerStatus, Viewport,
        healing_dirty_bounds,
    },
};
use starroom_session::{SessionOpen, SessionState};
use std::path::{Path, PathBuf};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::ipc::Response;
use tauri::{Manager, State};

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
    root: Option<PathBuf>,
    paths: Option<Vec<PathBuf>>,
) -> Result<ImportResult, String> {
    let runtime = runtime.inner().clone();
    runtime.cancel_import.store(false, Ordering::Relaxed);
    let worker = runtime.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut inputs = paths.unwrap_or_default();
        if let Some(root) = root {
            inputs.push(root);
        }
        let mut paths = Vec::new();
        for input in inputs {
            if input.is_dir() {
                paths.extend(Library::recursive_paths(&input).map_err(|error| error.to_string())?);
            } else {
                paths.push(input);
            }
        }
        paths.sort();
        paths.dedup();
        let mut guard = worker
            .library
            .lock()
            .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
        let library = guard
            .as_mut()
            .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?;
        library
            .import_paths(&paths, &worker.cancel_import)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("ImportCancelled: worker failed: {error}"))??;
    // Stage B never delays registration. Decode metadata without the SQLite mutex, then keep
    // each update transaction short so search/rating/selection remain usable.
    let ids = result.imported.clone();
    tauri::async_runtime::spawn_blocking(move || {
        for id in ids {
            if runtime.cancel_import.load(Ordering::Relaxed) {
                break;
            }
            let asset = runtime.library.lock().ok().and_then(|guard| {
                guard
                    .as_ref()
                    .and_then(|library| library.asset(id).ok().flatten())
            });
            let Some(asset) = asset else { continue };
            let Ok(metadata) = Library::extract_metadata(&asset.source_path) else {
                continue;
            };
            if let Ok(guard) = runtime.library.lock()
                && let Some(library) = guard.as_ref()
            {
                let _ = library.update_metadata(id, &metadata);
            }
        }
    });
    Ok(result)
}

#[tauri::command]
fn library_cancel_import(runtime: State<'_, NativeLibraryRuntime>) -> bool {
    runtime.cancel_import.store(true, Ordering::Relaxed);
    true
}

#[tauri::command]
async fn library_query(
    runtime: State<'_, NativeLibraryRuntime>,
    mut query: LibraryQuery,
    edited_only: Option<bool>,
) -> Result<Vec<AssetRecord>, String> {
    let runtime = runtime.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if edited_only.unwrap_or(false) {
            query.asset_ids = Some(persisted_edited_asset_ids(
                &history_path(0)?.with_file_name(""),
            )?);
        }
        let guard = runtime
            .library
            .lock()
            .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
        guard
            .as_ref()
            .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?
            .query(&query)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("InvalidQuery: worker failed: {error}"))?
}

#[derive(Serialize)]
struct LibraryCounts {
    all: u64,
    recent: u64,
    five: u64,
    edited: u64,
}

#[tauri::command]
async fn library_counts(runtime: State<'_, NativeLibraryRuntime>) -> Result<LibraryCounts, String> {
    let runtime = runtime.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let edited = persisted_edited_asset_ids(&history_path(0)?.with_file_name(""))?;
        let guard = runtime
            .library
            .lock()
            .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
        let library = guard
            .as_ref()
            .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?;
        let count = |query| library.count(&query).map_err(|error| error.to_string());
        Ok(LibraryCounts {
            all: count(LibraryQuery::default())?,
            recent: count(LibraryQuery {
                recent_batch: true,
                ..Default::default()
            })?,
            five: count(LibraryQuery {
                minimum_rating: Some(5),
                ..Default::default()
            })?,
            edited: count(LibraryQuery {
                asset_ids: Some(edited),
                ..Default::default()
            })?,
        })
    })
    .await
    .map_err(|error| format!("InvalidQuery: count worker failed: {error}"))?
}

#[tauri::command]
async fn library_query_ids(
    runtime: State<'_, NativeLibraryRuntime>,
    mut query: LibraryQuery,
    edited_only: Option<bool>,
) -> Result<Vec<i64>, String> {
    let runtime = runtime.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if edited_only.unwrap_or(false) {
            query.asset_ids = Some(persisted_edited_asset_ids(
                &history_path(0)?.with_file_name(""),
            )?);
        }
        runtime
            .library
            .lock()
            .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?
            .as_ref()
            .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?
            .query_ids(&query)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("InvalidQuery: worker failed: {error}"))?
}

fn library_metadata_refresh_snapshot(
    runtime: &NativeLibraryRuntime,
    asset_id: i64,
) -> Result<AssetRecord, String> {
    let guard = runtime
        .library
        .lock()
        .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
    let library = guard
        .as_ref()
        .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?;
    library
        .asset(asset_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("MissingSource: {asset_id}"))
}

fn apply_library_metadata_refresh(
    runtime: &NativeLibraryRuntime,
    snapshot: &AssetRecord,
    metadata: &AssetMetadata,
) -> Result<AssetRecord, String> {
    let guard = runtime
        .library
        .lock()
        .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
    let library = guard
        .as_ref()
        .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?;
    let current = library
        .asset(snapshot.id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("MissingSource: {}", snapshot.id))?;
    if current.source_path != snapshot.source_path
        || current.content_fingerprint != snapshot.content_fingerprint
        || current.source_identity != snapshot.source_identity
    {
        return Err("MetadataRefreshSourceChanged: source changed during metadata refresh".into());
    }
    // Update only descriptive metadata. Ratings, keywords, project/history identity and newer
    // workflow choices made during the unlocked decode remain authoritative in the catalog.
    library
        .update_metadata(snapshot.id, metadata)
        .map_err(|error| error.to_string())?;
    library
        .asset(snapshot.id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("MissingSource: {}", snapshot.id))
}

fn library_refresh_metadata_inner(
    runtime: &NativeLibraryRuntime,
    asset_id: i64,
) -> Result<AssetRecord, String> {
    let snapshot = library_metadata_refresh_snapshot(runtime, asset_id)?;
    if snapshot.missing || !snapshot.source_path.is_file() {
        return Err(format!("MissingSource: {asset_id}"));
    }
    // File identity and LibRaw work deliberately run without the SQLite mutex. One selected
    // legacy record is repaired lazily; never clear a catalog or decode the whole Library.
    let before = starroom_library::fingerprint_file(&snapshot.source_path)
        .map_err(|error| error.to_string())?;
    if before.digest != snapshot.content_fingerprint || before.byte_length != snapshot.file_size {
        return Err(
            "MetadataRefreshSourceChanged: source no longer matches catalog identity".into(),
        );
    }
    let metadata =
        Library::extract_metadata(&snapshot.source_path).map_err(|error| error.to_string())?;
    let after = starroom_library::fingerprint_file(&snapshot.source_path)
        .map_err(|error| error.to_string())?;
    if after.digest != before.digest || after.byte_length != before.byte_length {
        return Err("MetadataRefreshSourceChanged: source changed during metadata decode".into());
    }
    apply_library_metadata_refresh(runtime, &snapshot, &metadata)
}

#[tauri::command]
async fn library_refresh_metadata(
    runtime: State<'_, NativeLibraryRuntime>,
    asset_id: i64,
) -> Result<AssetRecord, String> {
    let runtime = runtime.inner().clone();
    tauri::async_runtime::spawn_blocking(move || library_refresh_metadata_inner(&runtime, asset_id))
        .await
        .map_err(|error| format!("MetadataReadFailed: metadata worker failed: {error}"))?
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
async fn library_remove_assets(
    runtime: State<'_, NativeLibraryRuntime>,
    asset_ids: Vec<i64>,
) -> Result<usize, String> {
    let runtime = runtime.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        runtime
            .library
            .lock()
            .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?
            .as_mut()
            .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?
            .remove_assets(&asset_ids)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("LibraryRemoveFailed: {error}"))?
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
    app: tauri::AppHandle,
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
        let asset = guard
            .as_ref()
            .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?
            .asset(asset_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("MissingSource: {asset_id}"))?;
        drop(guard);
        let path = Library::generate_asset_thumbnail(&asset, root, size)
            .map_err(|error| error.to_string())?;
        // Grant only the generated cache file, never the source directory or a wildcard.
        // convertFileSrc does not grant WebView access by itself.
        app.asset_protocol_scope()
            .allow_file(&path)
            .map_err(|error| format!("ThumbnailFailed: cache access denied: {error}"))?;
        Ok(path)
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
    edited: Option<bool>,
}

fn history_path(asset_id: i64) -> Result<PathBuf, String> {
    let root = default_library_path()?
        .parent()
        .ok_or_else(|| "HistoryPersistenceFailed: invalid app data directory".to_owned())?
        .join("history");
    Ok(root.join(format!("asset-{asset_id}.history.json")))
}

// History is the durable edit authority. Query only its small state files, never source pixels,
// and apply the resulting IDs in SQL before paging. No second, potentially stale edit database.
fn persisted_edited_asset_ids(root: &Path) -> Result<Vec<i64>, String> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("HistoryPersistenceFailed: {error}")),
    };
    let mut ids = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("HistoryPersistenceFailed: {error}"))?;
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|name| name.strip_prefix("asset-"))
            .and_then(|name| name.strip_suffix(".history.json"))
            .and_then(|value| value.parse::<i64>().ok())
        else {
            continue;
        };
        let history = EditHistory::load(entry.path())
            .map_err(|error| format!("HistoryCorrupt: asset {id}: {error}"))?;
        if native_state_is_edited(history.state())
            .map_err(|error| format!("HistoryCorrupt: asset {id}: {error}"))?
        {
            ids.push(id);
        }
    }
    ids.sort_unstable();
    Ok(ids)
}

fn native_state_is_edited(value: &serde_json::Value) -> Result<bool, String> {
    let neutral: NativeEditSettings = serde_json::from_str(include_str!(
        "../../fixtures/contracts/native-default-settings.json"
    ))
    .map_err(|error| format!("HistoryCorrupt: neutral contract: {error}"))?;
    let neutral =
        serde_json::to_value(neutral).map_err(|error| format!("HistoryCorrupt: {error}"))?;
    let mut state: NativeEditSettings = serde_json::from_value(value.clone())
        .map_err(|error| format!("HistoryCorrupt: {error}"))?;
    state
        .clone()
        .validated()
        .map_err(|error| format!("HistoryCorrupt: {error}"))?;
    // Execution backend is not a photographic edit.
    state.ai_denoise_provider = default_denoise_execution_provider();
    // The UI serializes its two identity endpoints; Native defaults use an empty curve.
    for curve in [
        &mut state.curve,
        &mut state.curves.master,
        &mut state.curves.red,
        &mut state.curves.green,
        &mut state.curves.blue,
    ] {
        if curve.len() == 2
            && curve[0].x == 0.0
            && curve[0].y == 0.0
            && curve[1].x == 1.0
            && curve[1].y == 1.0
        {
            curve.clear();
        }
    }
    Ok(serde_json::to_value(state).map_err(|error| format!("HistoryCorrupt: {error}"))? != neutral)
}

fn session_path() -> Result<PathBuf, String> {
    default_library_path()?
        .parent()
        .map(|root| root.join("session.json"))
        .ok_or_else(|| "SessionPersistenceFailed: invalid app data directory".to_owned())
}

#[tauri::command]
fn session_open() -> Result<SessionOpen, String> {
    starroom_session::open(&session_path()?).map_err(|error| error.to_string())
}

#[tauri::command]
fn session_autosave(state: SessionState) -> Result<(), String> {
    starroom_session::autosave(&session_path()?, &state).map_err(|error| error.to_string())
}

#[tauri::command]
fn session_mark_clean(state: SessionState) -> Result<(), String> {
    starroom_session::mark_clean(&session_path()?, &state).map_err(|error| error.to_string())
}

#[tauri::command]
fn session_discard_recovery() -> Result<(), String> {
    starroom_session::discard(&session_path()?).map_err(|error| error.to_string())
}

fn history_result(history: &EditHistory) -> NativeHistoryResult {
    NativeHistoryResult {
        state: history.state().clone(),
        can_undo: history.can_undo(),
        can_redo: history.can_redo(),
        entries: history.entries().to_vec(),
        snapshots: history.snapshots().to_vec(),
        state_version: history.state_version().0,
        edited: native_state_is_edited(history.state()).ok(),
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiFeatureAvailability {
    state: &'static str,
    detail: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiAvailabilityStatus {
    face_skin: AiFeatureAvailability,
    subject_background: AiFeatureAvailability,
    sky: AiFeatureAvailability,
    denoise: AiFeatureAvailability,
}

fn portrait_availability() -> AiFeatureAvailability {
    match local_portrait_models().verify() {
        Ok(()) => AiFeatureAvailability {
            state: "ready",
            detail: "Local YuNet and BiSeNet models verified",
        },
        Err(
            PortraitError::DetectorModelMissing { .. } | PortraitError::ParserModelMissing { .. },
        ) => AiFeatureAvailability {
            state: "modelNotInstalled",
            detail: "Model not installed. BiSeNet remains a local-only option.",
        },
        Err(PortraitError::ModelHashMismatch { .. }) => AiFeatureAvailability {
            state: "invalid",
            detail: "Installed portrait model failed hash verification",
        },
        Err(_) => AiFeatureAvailability {
            state: "error",
            detail: "Portrait model verification failed",
        },
    }
}

fn ai_mask_availability(
    result: Result<(), AiMaskError>,
    label: &'static str,
) -> AiFeatureAvailability {
    match result {
        Ok(()) => AiFeatureAvailability {
            state: "ready",
            detail: label,
        },
        Err(AiMaskError::ModelMissing { .. }) => AiFeatureAvailability {
            state: "modelNotInstalled",
            detail: "Model not installed",
        },
        Err(AiMaskError::ModelHashMismatch { .. }) => AiFeatureAvailability {
            state: "invalid",
            detail: "Installed model failed hash verification",
        },
        Err(_) => AiFeatureAvailability {
            state: "error",
            detail: "Local model verification failed",
        },
    }
}

#[tauri::command]
fn ai_availability_status() -> AiAvailabilityStatus {
    let masks = local_ai_mask_models();
    let denoise = match verify_ai_denoise_model(local_nafnet_model()) {
        Ok(()) => AiFeatureAvailability {
            state: "ready",
            detail: "Local NAFNet model verified",
        },
        Err(AiDenoiseError::ModelMissing(_)) => AiFeatureAvailability {
            state: "modelNotInstalled",
            detail: "AI Denoise model not installed",
        },
        Err(AiDenoiseError::HashMismatch { .. }) => AiFeatureAvailability {
            state: "invalid",
            detail: "Installed AI Denoise model failed hash verification",
        },
        Err(_) => AiFeatureAvailability {
            state: "error",
            detail: "AI Denoise model verification failed",
        },
    };
    AiAvailabilityStatus {
        face_skin: portrait_availability(),
        subject_background: ai_mask_availability(
            masks.verify_foreground(),
            "Local Subject/Background model verified",
        ),
        sky: ai_mask_availability(masks.verify_scene(), "Local Sky model verified"),
        denoise,
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
        // Persisted projects retain u64 seeds, but this IPC boundary uses JSON numbers.
        // Reject unrepresentable intent before any render/history commit instead of allowing
        // JavaScript to round a seed and silently change deterministic grain on the next edit.
        if self.grain.seed > 9_007_199_254_740_991 {
            return Err(
                "UnsafeRenderSeed: grain seed exceeds the exact JavaScript JSON integer range"
                    .into(),
            );
        }
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
    #[serde(default)]
    resolution_mode: PreviewResolutionMode,
    #[serde(default)]
    viewport: Option<PreviewViewportRequest>,
    settings: NativeEditSettings,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PreviewViewportRequest {
    source_width: u32,
    source_height: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum PreviewInteractionPhase {
    Interactive,
    #[default]
    Final,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum PreviewResolutionMode {
    #[default]
    Fit,
    HighResolution,
}

fn preview_requested_edge(max_edge: u32, phase: PreviewInteractionPhase) -> u32 {
    match phase {
        // Keep drag feedback detailed enough for modern HiDPI displays while remaining bounded.
        PreviewInteractionPhase::Interactive => max_edge.min(1024),
        PreviewInteractionPhase::Final => max_edge,
    }
    .clamp(256, 4096)
}

fn wants_high_resolution(mode: PreviewResolutionMode, phase: PreviewInteractionPhase) -> bool {
    mode == PreviewResolutionMode::HighResolution && phase == PreviewInteractionPhase::Final
}

fn viewport_graph_is_tile_safe(settings: &RenderSettings) -> bool {
    !settings.ai_denoise.enabled
        && settings.ai_denoise_residual.is_none()
        && settings.white_balance.sample.is_none()
        && !matches!(
            settings.white_balance.mode,
            WhiteBalanceMode::Auto | WhiteBalanceMode::NeutralPicker
        )
        && !settings.optics.parameters.enabled
        && settings.geometry == GeometryParameters::default()
        && settings.skin_retouch.parameters == Default::default()
        && settings.skin_retouch.faces.is_empty()
        && settings.healing_operations.iter().all(|operation| {
            !operation.enabled
                || (operation.mode != HealMode::AiInpaint
                    && operation.source_mode == SourceMode::Manual
                    && operation.source.is_some())
        })
        && settings.grain.amount == 0.0
        && settings.vignette.amount == 0.0
}

/// Process-wide M13 scheduler state. It holds only derived preview/cache bytes and request
/// identities; the immutable source image remains on disk and full export never reads this cache.
struct NativePreviewScheduler {
    scheduler: Mutex<RenderScheduler>,
    last_profile: Mutex<Option<RenderProfile>>,
    cancellations: Mutex<BTreeMap<String, Arc<AtomicBool>>>,
    cancellation_times: Mutex<BTreeMap<String, Instant>>,
    workers: PreviewWorkerPool,
    decoded: Mutex<VecDeque<(String, u32, Arc<DecodedSourceImage>)>>,
    viewport_frames: Mutex<VecDeque<(String, Vec<u8>)>>,
    gpu: Mutex<Option<Result<GpuRenderer, String>>>,
}

/// A small process-wide worker pool bounds memory without serializing the complete lifetime of
/// Before and After renders. Waiting is cancellation-aware, so stale requests never hold a slot.
struct PreviewWorkerPool {
    active: Mutex<usize>,
    changed: Condvar,
    limit: usize,
}

struct PreviewWorkerPermit<'a>(&'a PreviewWorkerPool);

impl PreviewWorkerPool {
    fn new(limit: usize) -> Self {
        Self {
            active: Mutex::new(0),
            changed: Condvar::new(),
            limit: limit.max(1),
        }
    }

    fn acquire(&self, cancelled: &AtomicBool) -> Result<PreviewWorkerPermit<'_>, String> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| "PreviewWorkerFailed: poisoned worker pool".to_owned())?;
        while *active >= self.limit {
            if cancelled.load(Ordering::Acquire) {
                return Err("PreviewCancelled: request was superseded while queued".into());
            }
            active = self
                .changed
                .wait_timeout(active, Duration::from_millis(4))
                .map_err(|_| "PreviewWorkerFailed: poisoned worker pool".to_owned())?
                .0;
        }
        if cancelled.load(Ordering::Acquire) {
            return Err("PreviewCancelled: request was superseded before rendering".into());
        }
        *active += 1;
        Ok(PreviewWorkerPermit(self))
    }
}

impl Drop for PreviewWorkerPermit<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.0.active.lock() {
            *active = active.saturating_sub(1);
            self.0.changed.notify_one();
        }
    }
}

const DECODE_CACHE_BUDGET: u64 = 512 * 1024 * 1024;

fn cached_decoded_source(
    scheduler: &NativePreviewScheduler,
    source_identity: &str,
    edge: u32,
) -> Result<Option<Arc<DecodedSourceImage>>, String> {
    let mut cache = scheduler
        .decoded
        .lock()
        .map_err(|_| "PreviewCacheFailed: poisoned decode cache".to_owned())?;
    let value = cache
        .iter()
        .position(|(identity, cached_edge, _)| identity == source_identity && *cached_edge == edge)
        .and_then(|index| cache.remove(index))
        .map(|entry| {
            let image = entry.2.clone();
            cache.push_back(entry);
            image
        });
    profiling::record_cache(value.is_some());
    Ok(value)
}

fn cache_decoded_source(
    scheduler: &NativePreviewScheduler,
    source_identity: String,
    edge: u32,
    image: Arc<DecodedSourceImage>,
) -> Result<(), String> {
    let bytes = u64::from(image.width()) * u64::from(image.height()) * 16;
    if bytes > DECODE_CACHE_BUDGET {
        return Ok(());
    }
    let mut cache = scheduler
        .decoded
        .lock()
        .map_err(|_| "PreviewCacheFailed: poisoned decode cache".to_owned())?;
    while !cache.is_empty()
        && (cache.len() >= 4
            || cache
                .iter()
                .map(|(_, _, value)| u64::from(value.width()) * u64::from(value.height()) * 16)
                .sum::<u64>()
                + bytes
                > DECODE_CACHE_BUDGET)
    {
        cache.pop_front();
    }
    cache.push_back((source_identity, edge, image));
    Ok(())
}

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
    /// Bounded metadata identities -> byte SHA; avoids rereading a RAW on every slider update.
    source_hashes: Mutex<BTreeMap<String, String>>,
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
            source_hashes: Mutex::new(BTreeMap::new()),
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
        Self {
            scheduler: Mutex::new(RenderScheduler::default()),
            last_profile: Mutex::new(None),
            cancellations: Mutex::new(BTreeMap::new()),
            cancellation_times: Mutex::new(BTreeMap::new()),
            workers: PreviewWorkerPool::new(2),
            decoded: Mutex::new(VecDeque::new()),
            viewport_frames: Mutex::new(VecDeque::new()),
            gpu: Mutex::new(None),
        }
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
    #[serde(default = "default_color_sample_edge")]
    max_edge: u32,
    #[serde(default)]
    coordinate_space: NativeSampleCoordinateSpace,
    settings: NativeEditSettings,
}

const fn default_color_sample_edge() -> u32 {
    1800
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum NativeSampleCoordinateSpace {
    #[default]
    PostGeometry,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeAdvisorRequest {
    source_path: PathBuf,
    max_edge: u32,
    settings: NativeEditSettings,
}

struct AdvisorRuntimeRefs<'a> {
    portrait: &'a NativePortraitRuntime,
    masks: &'a NativeAiMaskRuntime,
    denoise: &'a NativeAiDenoiseRuntime,
}

fn render_advisor_shared_graph(
    decoded: &DecodedSourceImage,
    source_path: &Path,
    settings: &mut RenderSettings,
    denoise_provider: DenoiseExecutionProvider,
    runtimes: AdvisorRuntimeRefs<'_>,
) -> Result<starroom_pipeline::RenderedRgb8, String> {
    attach_portrait_masks(settings, source_path, runtimes.portrait)?;
    attach_generated_masks(settings, source_path, runtimes.masks)?;
    attach_ai_denoise(
        decoded,
        source_path,
        settings,
        denoise_provider,
        "native-advisor",
        runtimes.denoise,
    )?;
    render_source_preview_to_srgb8(decoded, settings)
        .map_err(|error| format!("advisor native graph failed: {error}"))
}

/// M19 runs deterministic analysis locally on the same native graph that produces preview.
/// The UI receives small statistics/suggestions only; no image pixels or cloud request cross IPC.
#[tauri::command]
fn advise_native_image(
    portrait_runtime: State<'_, NativePortraitRuntime>,
    ai_mask_runtime: State<'_, NativeAiMaskRuntime>,
    ai_denoise_runtime: State<'_, NativeAiDenoiseRuntime>,
    request: NativeAdvisorRequest,
) -> Result<AdvisorResult, String> {
    let requested_denoise_provider = request.settings.ai_denoise_provider;
    let mut settings = request.settings.validated()?;
    let decoded = decode_source_preview(&request.source_path, request.max_edge.clamp(256, 2048))
        .map_err(|error| format!("advisor preview decode failed: {error}"))?;
    let rendered = render_advisor_shared_graph(
        &decoded,
        &request.source_path,
        &mut settings,
        requested_denoise_provider,
        AdvisorRuntimeRefs {
            portrait: &portrait_runtime,
            masks: &ai_mask_runtime,
            denoise: &ai_denoise_runtime,
        },
    )?;
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
    if let Some(skin_weights) =
        sample_source_portrait_weights(&decoded, &settings, PortraitMaskRegion::Skin)
            .map_err(|error| format!("advisor portrait geometry failed: {error}"))?
    {
        if skin_weights.len() != samples.len() {
            return Err(
                "AdvisorPortraitShapeMismatch: shared graph coverage dimensions differ".into(),
            );
        }
        let mut weight_sum = 0.0_f32;
        let mut luma_sum = 0.0_f32;
        let mut chroma_sum = 0.0_f32;
        for (rgb, weight) in samples.iter().zip(skin_weights) {
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
    if !(256..=4096).contains(&request.max_edge) {
        return Err("NativeColorSampleInvalid: decode edge is outside supported ranges".into());
    }
    let NativeSampleCoordinateSpace::PostGeometry = request.coordinate_space;
    let settings = request.settings.validated()?;
    let decoded = decode_source_preview(&request.source_path, request.max_edge)
        .map_err(|error| format!("native color sample decode failed: {error}"))?;
    sample_source_color_band(&decoded, &settings, request.x, request.y)
        .map_err(|error| format!("native color sample failed: {error}"))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeWhiteBalanceInfo {
    source: &'static str,
    as_shot_kelvin: Option<f32>,
}

/// Exposes only truthful source WB metadata. Rendered RGB files remain explicitly relative.
#[tauri::command]
async fn native_white_balance_info(source_path: PathBuf) -> Result<NativeWhiteBalanceInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let decoded = decode_source_preview(&source_path, 256)
            .map_err(|error| format!("native white-balance metadata decode failed: {error}"))?;
        Ok(match decoded {
            DecodedSourceImage::Raw(raw) => NativeWhiteBalanceInfo {
                source: "rawMetadata",
                as_shot_kelvin: raw.metadata.as_shot_kelvin,
            },
            DecodedSourceImage::Rendered(_) => NativeWhiteBalanceInfo {
                source: "renderedRelative",
                as_shot_kelvin: None,
            },
        })
    })
    .await
    .map_err(|error| format!("native white-balance metadata task failed: {error}"))?
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreviewFrameHeader {
    width: u32,
    height: u32,
    source_width: u32,
    source_height: u32,
    tile_x: u32,
    tile_y: u32,
    flags: u16,
}

fn preview_frame(
    header: PreviewFrameHeader,
    profile_id: &str,
    jpeg: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let profile_len = u16::try_from(profile_id.len()).map_err(|_| "profile ID is too long")?;
    let payload_len = u32::try_from(jpeg.len()).map_err(|_| "native preview is too large")?;
    let mut frame = Vec::with_capacity(40 + profile_id.len() + jpeg.len());
    frame.extend_from_slice(b"SRP3");
    frame.extend_from_slice(&3_u16.to_le_bytes());
    frame.extend_from_slice(&header.flags.to_le_bytes());
    frame.extend_from_slice(&header.width.to_le_bytes());
    frame.extend_from_slice(&header.height.to_le_bytes());
    frame.extend_from_slice(&header.source_width.to_le_bytes());
    frame.extend_from_slice(&header.source_height.to_le_bytes());
    frame.extend_from_slice(&header.tile_x.to_le_bytes());
    frame.extend_from_slice(&header.tile_y.to_le_bytes());
    frame.extend_from_slice(&profile_len.to_le_bytes());
    frame.extend_from_slice(&0_u16.to_le_bytes());
    frame.extend_from_slice(&payload_len.to_le_bytes());
    frame.extend_from_slice(profile_id.as_bytes());
    frame.extend_from_slice(&jpeg);
    Ok(frame)
}

fn validated_viewport(
    requested: Option<PreviewViewportRequest>,
    width: u32,
    height: u32,
) -> Result<PreviewViewportRequest, String> {
    let viewport = requested.unwrap_or(PreviewViewportRequest {
        source_width: width,
        source_height: height,
        x: 0,
        y: 0,
        width,
        height,
    });
    if viewport.source_width != width
        || viewport.source_height != height
        || viewport.width == 0
        || viewport.height == 0
        || viewport
            .x
            .checked_add(viewport.width)
            .is_none_or(|right| right > width)
        || viewport
            .y
            .checked_add(viewport.height)
            .is_none_or(|bottom| bottom > height)
    {
        return Err("PreviewViewportInvalid: requested tile is outside source bounds".into());
    }
    Ok(viewport)
}

fn expand_viewport(
    viewport: PreviewViewportRequest,
    width: u32,
    height: u32,
    halo: u32,
) -> PreviewViewportRequest {
    let x = viewport.x.saturating_sub(halo);
    let y = viewport.y.saturating_sub(halo);
    let right = viewport
        .x
        .saturating_add(viewport.width)
        .saturating_add(halo)
        .min(width);
    let bottom = viewport
        .y
        .saturating_add(viewport.height)
        .saturating_add(halo)
        .min(height);
    PreviewViewportRequest {
        source_width: width,
        source_height: height,
        x,
        y,
        width: right - x,
        height: bottom - y,
    }
}

fn union_viewport_with_rect(
    viewport: PreviewViewportRequest,
    rect: PixelRect,
) -> PreviewViewportRequest {
    if rect.width == 0 || rect.height == 0 {
        return viewport;
    }
    let x = viewport.x.min(rect.x);
    let y = viewport.y.min(rect.y);
    let right = viewport
        .x
        .saturating_add(viewport.width)
        .max(rect.x.saturating_add(rect.width))
        .min(viewport.source_width);
    let bottom = viewport
        .y
        .saturating_add(viewport.height)
        .max(rect.y.saturating_add(rect.height))
        .min(viewport.source_height);
    PreviewViewportRequest {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
        ..viewport
    }
}

fn crop_rgb8(
    data: &[u8],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    crop_width: u32,
    crop_height: u32,
) -> Result<Vec<u8>, String> {
    if data.len() != width as usize * height as usize * 3
        || crop_width == 0
        || crop_height == 0
        || x.checked_add(crop_width).is_none_or(|right| right > width)
        || y.checked_add(crop_height)
            .is_none_or(|bottom| bottom > height)
    {
        return Err("PreviewTileInvalid: rendered tile crop is outside frame bounds".into());
    }
    let mut cropped = Vec::with_capacity(crop_width as usize * crop_height as usize * 3);
    for row in y..y + crop_height {
        let start = (row as usize * width as usize + x as usize) * 3;
        cropped.extend_from_slice(&data[start..start + crop_width as usize * 3]);
    }
    Ok(cropped)
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

/// Keep driver/validation panics inside the GPU boundary while the cache guard remains alive.
/// A previously poisoned device is discarded; the caller can continue on the labelled CPU graph.
fn recoverable_gpu_attempt<T>(
    cache: &Mutex<Option<Result<GpuRenderer, String>>>,
    operation: impl FnOnce(&mut Option<Result<GpuRenderer, String>>) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = match cache.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            let mut guard = poisoned.into_inner();
            *guard = Some(Err(
                "GPU device cache recovered after a driver failure".into()
            ));
            cache.clear_poison();
            guard
        }
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(&mut guard))) {
        Ok(result) => result,
        Err(_) => {
            let reason = "GPU device failure; using the Native CPU render graph".to_owned();
            *guard = Some(Err(reason.clone()));
            Err(reason)
        }
    }
}

fn gpu_failure_requires_retirement(error: &starroom_pipeline::PipelineError) -> bool {
    use starroom_render::gpu::GpuError;
    matches!(
        error,
        starroom_pipeline::PipelineError::Gpu(
            GpuError::DeviceLost
                | GpuError::OutOfMemory
                | GpuError::ReadbackTimeout
                | GpuError::Validation(_)
                | GpuError::Readback(_)
        )
    )
}

fn resolve_local_model_root(
    configured: Option<std::ffi::OsString>,
    executable: Option<&Path>,
    working_directory: &Path,
) -> PathBuf {
    if let Some(configured) = configured {
        return PathBuf::from(configured);
    }
    if let Some(executable) = executable.and_then(Path::parent) {
        let bundled = executable.join("models").join("local");
        if bundled.is_dir() {
            return bundled;
        }
    }
    working_directory.join("models").join("local")
}

static APP_LOCAL_MODEL_ROOT: OnceLock<PathBuf> = OnceLock::new();

fn local_model_root() -> PathBuf {
    if let Some(root) = APP_LOCAL_MODEL_ROOT.get() {
        return root.clone();
    }
    resolve_local_model_root(
        std::env::var_os("STARROOM_LOCAL_MODELS"),
        std::env::current_exe().ok().as_deref(),
        &std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    )
}

fn local_portrait_models() -> PortraitModelRegistry {
    let mut registry = PortraitModelRegistry::local_default(local_model_root());
    registry.detector.path = local_model_file("face_detection_yunet_2026may.onnx");
    registry.parser.path = local_model_file("bisenet_resnet18.onnx");
    registry
}

fn local_model_file(name: &str) -> PathBuf {
    let local = local_model_root().join(name);
    // Installed GUI uses AppData for personal models, but approved bundled models stay
    // available per file. Never mask an invalid user model by substituting a bundled one.
    let bundled = resolve_local_model_root(
        None,
        std::env::current_exe().ok().as_deref(),
        &std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    )
    .join(name);
    choose_local_model_file(
        local,
        bundled,
        std::env::var_os("STARROOM_LOCAL_MODELS").is_some(),
    )
}

fn choose_local_model_file(local: PathBuf, bundled: PathBuf, explicit_root: bool) -> PathBuf {
    if local.is_file() || explicit_root {
        local
    } else {
        bundled
    }
}

#[tauri::command]
fn portrait_models_install_local(
    detector_path: String,
    parser_path: String,
) -> Result<AiAvailabilityStatus, String> {
    let mut selected = local_portrait_models();
    selected.detector.path = PathBuf::from(detector_path);
    selected.parser.path = PathBuf::from(parser_path);
    selected.verify().map_err(|error| error.to_string())?;

    let target = local_portrait_models();
    let root = local_model_root();
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("could not create local model directory: {error}"))?;
    let detector_temp = root.join("face_detection_yunet_2026may.onnx.installing");
    let parser_temp = root.join("bisenet_resnet18.onnx.installing");
    std::fs::copy(&selected.detector.path, &detector_temp)
        .map_err(|error| format!("could not stage YuNet model: {error}"))?;
    if let Err(error) = std::fs::copy(&selected.parser.path, &parser_temp) {
        let _ = std::fs::remove_file(&detector_temp);
        return Err(format!("could not stage BiSeNet model: {error}"));
    }
    for (staged, destination) in [
        (&detector_temp, &target.detector.path),
        (&parser_temp, &target.parser.path),
    ] {
        if destination.exists() {
            std::fs::remove_file(destination)
                .map_err(|error| format!("could not replace local model: {error}"))?;
        }
        std::fs::rename(staged, destination)
            .map_err(|error| format!("could not activate local model: {error}"))?;
    }
    target.verify().map_err(|error| error.to_string())?;
    Ok(ai_availability_status())
}

fn local_ai_mask_models() -> AiMaskModelRegistry {
    let mut registry = AiMaskModelRegistry::local_default(local_model_root());
    registry.foreground.path = local_model_file("BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx");
    registry.scene.path = local_model_file("segformer-b0-ade20k-489d5cd.onnx");
    registry
}

fn local_nafnet_model() -> PathBuf {
    local_model_file("nafnet-sidd-width32-512-opset20.onnx")
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

fn ai_denoise_input_identity(
    source_identity: &str,
    width: usize,
    height: usize,
    settings: &RenderSettings,
) -> Result<String, String> {
    // NAFNet consumes the image after source color/WB, Lensfun and geometry. Dimensions alone
    // cannot distinguish a rotation/flip/WB/profile edit. Creative sliders remain intentionally
    // excluded: they occur after inference and must reuse its expensive immutable residual.
    let input_stage = serde_json::to_vec(&(
        settings.color_management,
        settings.white_balance,
        &settings.optics,
        settings.geometry,
        settings
            .source_region
            .map(|region| (region.full_width, region.full_height, region.x, region.y)),
    ))
    .map_err(|error| format!("AI denoise input identity failed: {error}"))?;
    Ok(format!(
        "{source_identity}:{width}x{height}:precreative-{:x}",
        Sha256::digest(input_stage)
    ))
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
    let sized_identity =
        ai_denoise_input_identity(&source, working.width, working.height, settings)?;
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

fn collect_portrait_mask_references(tree: &MaskTree, values: &mut Vec<PortraitRestoreReference>) {
    match tree {
        MaskTree::Leaf(MaskDefinition::PortraitSemantic {
            face_id,
            region,
            cache_key,
            source_crop,
            model_id,
            model_version,
            model_hash,
            ..
        }) => values.push(PortraitRestoreReference {
            cache_key: cache_key.clone(),
            face_id: face_id.clone(),
            region: *region,
            source_crop: *source_crop,
            model_identity: Some((model_id.clone(), model_version.clone(), model_hash.clone())),
        }),
        MaskTree::Leaf(_) => {}
        MaskTree::Composite(composite) => {
            for child in &composite.children {
                collect_portrait_mask_references(child, values);
            }
        }
    }
}

struct PortraitRestoreReference {
    cache_key: String,
    face_id: String,
    region: PortraitMaskRegion,
    source_crop: Option<PortraitSourceCrop>,
    model_identity: Option<(String, String, String)>,
}

struct GeneratedRestoreReference {
    cache_identity: String,
    semantic: GeneratedMaskSemantic,
    provider_id: String,
    model_id: String,
    model_version: String,
    model_hash: String,
    execution_provider: Option<String>,
}

fn collect_generated_mask_references(tree: &MaskTree, values: &mut Vec<GeneratedRestoreReference>) {
    match tree {
        MaskTree::Leaf(MaskDefinition::Generated {
            cache_identity,
            semantic_class,
            provider_id,
            model_id,
            model_version,
            model_hash,
            metadata,
            ..
        }) => {
            values.push(GeneratedRestoreReference {
                cache_identity: cache_identity.clone(),
                semantic: *semantic_class,
                provider_id: provider_id.clone(),
                model_id: model_id.clone(),
                model_version: model_version.clone(),
                model_hash: model_hash.clone(),
                execution_provider: metadata.get("executionProvider").cloned(),
            });
        }
        MaskTree::Leaf(_) => {}
        MaskTree::Composite(composite) => {
            for child in &composite.children {
                collect_generated_mask_references(child, values);
            }
        }
    }
}

fn cached_source_content_hash(
    path: &Path,
    runtime: &NativeAiMaskRuntime,
) -> Result<String, String> {
    let source_identity = preview_source_identity(path)?;
    let mut hashes = runtime
        .source_hashes
        .lock()
        .map_err(|_| "AI mask source identity lock was poisoned".to_owned())?;
    if let Some(hash) = hashes.get(&source_identity) {
        return Ok(hash.clone());
    }
    let hash = source_content_hash(path)?;
    if hashes.len() >= 32 {
        hashes.pop_first();
    }
    hashes.insert(source_identity, hash.clone());
    Ok(hash)
}

fn attach_generated_masks(
    settings: &mut RenderSettings,
    source_path: &Path,
    runtime: &NativeAiMaskRuntime,
) -> Result<(), String> {
    let mut references = Vec::new();
    for layer in &settings.layers {
        if layer.enabled && layer.opacity > 0.0 {
            collect_generated_mask_references(&layer.mask, &mut references);
        }
    }
    if references.is_empty() {
        return Ok(());
    }
    let source_hash = cached_source_content_hash(source_path, runtime)?;
    let registry = local_ai_mask_models();
    let mut rgba_source = None;
    for reference in references {
        let (semantic, descriptor, provider_id) = match reference.semantic {
            GeneratedMaskSemantic::Subject => {
                (AiMaskSemantic::Subject, &registry.foreground, "foreground")
            }
            GeneratedMaskSemantic::Background => (
                AiMaskSemantic::Background,
                &registry.foreground,
                "foreground",
            ),
            GeneratedMaskSemantic::Sky => (AiMaskSemantic::Sky, &registry.scene, "semantic-scene"),
            GeneratedMaskSemantic::Person => {
                (AiMaskSemantic::Person, &registry.scene, "semantic-scene")
            }
            _ => {
                return Err(
                    "PortraitProviderRequired: use the verified portrait provider for Skin/Hair"
                        .into(),
                );
            }
        };
        if reference.provider_id != provider_id
            || reference.model_id != descriptor.id
            || reference.model_version != descriptor.version
            || reference.model_hash != descriptor.sha256
        {
            return Err("MaskModelMismatch: saved AI mask requires a different provider/model version or SHA".into());
        }
        let expected_key =
            AiMaskOnnxProvider::cache_identity(&source_hash, semantic, &descriptor.sha256);
        if reference.cache_identity != expected_key {
            return Err("MaskSourceMismatch: saved AI mask belongs to a different source; regenerate this mask for the current photo".into());
        }
        let execution_provider = match reference.execution_provider.as_deref() {
            Some("cpu") => starroom_portrait::ExecutionProvider::Cpu,
            Some("directMl") => starroom_portrait::ExecutionProvider::DirectMl,
            None => registry.execution_provider,
            _ => {
                return Err("MaskProviderMismatch: saved execution provider is unsupported".into());
            }
        };
        let cached = runtime
            .cache
            .lock()
            .map_err(|_| "AI mask cache lock was poisoned".to_owned())?
            .get(&expected_key)
            .cloned();
        let generated = if let Some(generated) = cached {
            generated
        } else {
            starroom_pipeline::cancellation::checkpoint().map_err(|error| error.to_string())?;
            if rgba_source.is_none() {
                rgba_source =
                    Some(source_rgba_for_portrait(source_path).map_err(|error| error.to_string())?);
            }
            let (width, height, rgba, _) = rgba_source.as_ref().expect("prepared above");
            let mut provider = runtime
                .provider
                .lock()
                .map_err(|_| "AI mask provider lock was poisoned".to_owned())?;
            if provider.as_ref().is_none_or(|active| {
                !active.supports(semantic) || active.execution_provider != execution_provider
            }) {
                let mut exact_registry = registry.clone();
                exact_registry.execution_provider = execution_provider;
                *provider = Some(
                    AiMaskOnnxProvider::initialize_for(exact_registry, semantic)
                        .map_err(|error| error.to_string())?,
                );
            }
            let generated = provider
                .as_mut()
                .expect("initialized above")
                .generate(
                    *width,
                    *height,
                    rgba,
                    &source_hash,
                    semantic,
                    &AtomicBool::new(false),
                )
                .map_err(|error| error.to_string())?;
            starroom_pipeline::cancellation::checkpoint().map_err(|error| error.to_string())?;
            runtime
                .cache
                .lock()
                .map_err(|_| "AI mask cache lock was poisoned".to_owned())?
                .insert(expected_key.clone(), generated.clone());
            generated
        };
        if generated.semantic != semantic
            || generated.cache_identity != expected_key
            || generated.provider_id != reference.provider_id
            || generated.model_id != reference.model_id
            || generated.model_version != reference.model_version
            || generated.model_hash != reference.model_hash
        {
            return Err(
                "MaskCacheMismatch: generated raster identity does not match the saved edit".into(),
            );
        }
        if settings
            .generated_masks
            .iter()
            .any(|mask| mask.cache_identity == expected_key && mask.semantic == reference.semantic)
        {
            continue;
        }
        settings.generated_masks.push(GeneratedMaskRaster {
            cache_identity: expected_key,
            semantic: reference.semantic,
            width: generated.mask.width,
            height: generated.mask.height,
            values: generated.mask.values.clone(),
        });
    }
    Ok(())
}

fn attach_portrait_masks(
    settings: &mut RenderSettings,
    source_path: &Path,
    runtime: &NativePortraitRuntime,
) -> Result<(), String> {
    let mut references = Vec::new();
    for layer in &settings.layers {
        if layer.enabled && layer.opacity > 0.0 {
            collect_portrait_mask_references(&layer.mask, &mut references);
        }
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
            references.push(PortraitRestoreReference {
                cache_key: face.cache_key.clone(),
                face_id: face.face_id.clone(),
                region,
                source_crop: face.source_crop,
                model_identity: None,
            });
        }
    }
    if references.is_empty() {
        return Ok(());
    }
    let source_identity = preview_source_identity(source_path)?;
    let registry = local_portrait_models();
    let mut source = None;
    let mut detected_faces = None;
    let mut state = runtime
        .0
        .lock()
        .map_err(|_| "portrait runtime lock was poisoned".to_owned())?;
    for reference in references {
        let PortraitRestoreReference {
            cache_key,
            face_id,
            region,
            source_crop,
            model_identity,
        } = reference;
        if model_identity.is_some_and(|(id, version, hash)| {
            id != registry.parser.id
                || version != registry.parser.version
                || hash != registry.parser.sha256
        }) {
            return Err("PortraitModelMismatch: saved portrait requires a different pinned parser version or SHA".into());
        }
        if source_crop.is_some_and(|crop| !crop.is_valid()) {
            return Err(
                "PortraitRestoreMetadataInvalid: sourceCrop must be finite version 1 metadata"
                    .into(),
            );
        }
        if !state.parsed.contains_key(&cache_key) {
            starroom_pipeline::cancellation::checkpoint().map_err(|error| error.to_string())?;
            if source.is_none() {
                source =
                    Some(source_rgba_for_portrait(source_path).map_err(|error| error.to_string())?);
            }
            let (width, height, rgba, _) = source.as_ref().expect("prepared above");
            if state.provider.is_none() {
                state.provider = Some(
                    PortraitOnnxProvider::initialize(registry.clone())
                        .map_err(|error| error.to_string())?,
                );
            }
            let provider = state.provider.as_mut().expect("initialized above");
            if detected_faces.is_none() {
                detected_faces = Some(provider.detect(*width, *height, rgba, default_face_crop_scale(), &source_identity)
                    .map_err(|error| format!("PortraitSourceMismatch: cannot resolve saved face from current source: {error}"))?);
            }
            let mut face = detected_faces.as_ref().expect("detected above").iter()
                .find(|face| face.id == face_id).cloned()
                .ok_or_else(|| "PortraitSourceMismatch: saved face belongs to another source; detect/select faces for the current photo".to_owned())?;
            if let Some(crop) = source_crop {
                face.crop = FaceCropTransform {
                    center_x: crop.center_x,
                    center_y: crop.center_y,
                    side: crop.side,
                    rotation_degrees: crop.rotation_degrees,
                };
                if !(0.0..=*width as f32).contains(&crop.center_x)
                    || !(0.0..=*height as f32).contains(&crop.center_y)
                {
                    return Err(
                        "PortraitRestoreMetadataInvalid: crop center is outside the source".into(),
                    );
                }
            } else if format!("{}:{}", face.id, face.crop.identity_hash()) != cache_key {
                // Legacy versions exposed only the default 1.4 and the early 1.0 crop. Restore
                // a candidate only when the complete original transform SHA proves equality.
                face.crop = FaceCropTransform::from_face(
                    face.bounds,
                    *width,
                    *height,
                    1.0,
                    face.landmarks[0],
                    face.landmarks[1],
                )
                .map_err(|error| error.to_string())?;
            }
            if format!("{}:{}", face.id, face.crop.identity_hash()) != cache_key {
                return Err("PortraitRestoreMetadataMissing: saved crop cannot be reproduced exactly; detect/select the face again".into());
            }
            let parsed = provider
                .parse(*width, *height, rgba, &face, &source_identity)
                .map_err(|error| error.to_string())?;
            starroom_pipeline::cancellation::checkpoint().map_err(|error| error.to_string())?;
            state.parsed.insert(cache_key.clone(), parsed);
        }
        let parse = state
            .parsed
            .get(&cache_key)
            .ok_or_else(|| format!("portrait semantic cache is unavailable: {cache_key}"))?;
        if parse.cache_key.source_identity != source_identity
            || parse.face_id != face_id
            || parse.cache_key.face_id != face_id
        {
            return Err("PortraitSourceMismatch: cached face belongs to another photo; detect/select the current source".into());
        }
        if parse.cache_key.parser_model_hash != registry.parser.sha256
            || format!("{}:{}", face_id, parse.cache_key.crop_transform_hash) != cache_key
        {
            return Err(
                "PortraitCacheMismatch: cached model/crop identity differs from the saved edit"
                    .into(),
            );
        }
        if let Some(crop) = source_crop {
            let crop = FaceCropTransform {
                center_x: crop.center_x,
                center_y: crop.center_y,
                side: crop.side,
                rotation_degrees: crop.rotation_degrees,
            };
            if crop.identity_hash() != parse.cache_key.crop_transform_hash {
                return Err(
                    "PortraitRestoreMetadataInvalid: sourceCrop does not match saved transform SHA"
                        .into(),
                );
            }
        }
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
        if settings.portrait_masks.iter().any(|mask| {
            mask.cache_key == cache_key && mask.face_id == face_id && mask.region == region
        }) {
            continue;
        }
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
        AiMaskSemantic::Skin | AiMaskSemantic::Hair
    ) {
        return Err(AiMaskError::PortraitProviderRequired(request.semantic).into());
    }
    let source_hash = cached_source_content_hash(&request.source_path, &runtime)
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
                    == AiMaskOnnxProvider::cache_identity(
                        &source_hash,
                        request.semantic,
                        &result.model_hash,
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
    if provider_guard
        .as_ref()
        .is_none_or(|provider| !provider.supports(request.semantic))
    {
        *provider_guard = Some(
            AiMaskOnnxProvider::initialize_for(local_ai_mask_models(), request.semantic)
                .map_err(AiMaskFailure::from)?,
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
        .scheduler
        .lock()
        .map_err(|_| "native preview scheduler lock was poisoned".to_owned())
        .map(|scheduler| scheduler.status())
}

#[tauri::command]
fn native_preview_profile(
    scheduler: State<'_, NativePreviewScheduler>,
) -> Result<Option<RenderProfile>, String> {
    scheduler
        .last_profile
        .lock()
        .map(|profile| profile.clone())
        .map_err(|_| "native preview profile lock was poisoned".to_owned())
}

#[tauri::command]
fn native_preview_cancel(
    scheduler: State<'_, NativePreviewScheduler>,
    request_id: String,
) -> Result<bool, String> {
    scheduler
        .cancellation_times
        .lock()
        .map_err(|_| "PreviewCancelled: poisoned cancellation timing registry".to_owned())?
        .entry(request_id.clone())
        .or_insert_with(Instant::now);
    let mut tokens = scheduler
        .cancellations
        .lock()
        .map_err(|_| "PreviewCancelled: poisoned registry".to_owned())?;
    // Preserve an early cancellation if its IPC arrives before the render command.
    if tokens.len() >= 128 && !tokens.contains_key(&request_id) {
        tokens.retain(|_, token| Arc::strong_count(token) > 1);
    }
    tokens
        .entry(request_id)
        .or_insert_with(|| Arc::new(AtomicBool::new(false)))
        .store(true, Ordering::Release);
    Ok(true)
}

#[tauri::command]
async fn native_preview(
    app: tauri::AppHandle,
    request: NativePreviewRequest,
) -> Result<Response, String> {
    let request_id = request.request_id.clone();
    let token = app
        .state::<NativePreviewScheduler>()
        .cancellations
        .lock()
        .map_err(|_| "PreviewCancelled: poisoned registry".to_owned())?
        .entry(request_id.clone())
        .or_insert_with(|| Arc::new(AtomicBool::new(false)))
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        let scheduler = app.state::<NativePreviewScheduler>();
        let queued_at = Instant::now();
        let _permit = match scheduler.workers.acquire(&token) {
            Ok(permit) => permit,
            Err(error) => {
                if let Ok(mut cancellations) = scheduler.cancellations.lock() {
                    cancellations.remove(&request_id);
                }
                if let Ok(mut timings) = scheduler.cancellation_times.lock() {
                    timings.remove(&request_id);
                }
                return Err(error);
            }
        };
        let queue_wait = u64::try_from(queued_at.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let (result, mut profile) = profiling::capture(|| {
            starroom_pipeline::cancellation::with_cancellation(token.clone(), || {
                starroom_pipeline::cancellation::checkpoint().map_err(|error| error.to_string())?;
                let result = native_preview_inner(
                    &scheduler,
                    &app.state::<NativePortraitRuntime>(),
                    &app.state::<NativeAiMaskRuntime>(),
                    &app.state::<NativeAiDenoiseRuntime>(),
                    request,
                )?;
                starroom_pipeline::cancellation::checkpoint().map_err(|error| error.to_string())?;
                Ok(result)
            })
        });
        profile.queue_wait_nanoseconds = queue_wait;
        profile.cancel_latency_nanoseconds = scheduler
            .cancellation_times
            .lock()
            .map_err(|_| "PreviewCancelled: poisoned cancellation timing registry".to_owned())?
            .remove(&request_id)
            .map(|started| u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
        scheduler
            .cancellations
            .lock()
            .map_err(|_| "PreviewCancelled: poisoned registry".to_owned())?
            .remove(&request_id);
        if !token.load(Ordering::Acquire) {
            *scheduler
                .last_profile
                .lock()
                .map_err(|_| "native preview profile lock was poisoned".to_owned())? =
                Some(profile);
        }
        result
    })
    .await
    .map_err(|error| format!("PreviewWorkerFailed: {error}"))?
}

fn preview_stage_identity(
    source_identity: &str,
    settings: &RenderSettings,
) -> Result<GpuStageCacheKeys, String> {
    fn encoded<T: Serialize>(value: &T) -> Result<String, String> {
        serde_json::to_string(value).map_err(|error| format!("PreviewCacheIdentityFailed: {error}"))
    }
    let mut parameters = BTreeMap::new();
    parameters.insert(
        StageId::InputTransform,
        encoded(&(
            settings.color_management,
            starroom_pipeline::RAW_DECODE_POLICY_VERSION,
            starroom_pipeline::COLOR_POLICY_VERSION,
        ))?,
    );
    let source_wb = match settings.white_balance.mode {
        WhiteBalanceMode::NeutralPicker => WhiteBalanceSettings::default(),
        _ => WhiteBalanceSettings {
            mode: settings.white_balance.mode,
            sample: None,
        },
    };
    parameters.insert(StageId::SourceWhiteBalance, encoded(&source_wb)?);
    let measured_wb = if settings.white_balance.mode == WhiteBalanceMode::NeutralPicker {
        settings.white_balance
    } else {
        WhiteBalanceSettings::default()
    };
    parameters.insert(StageId::WhiteBalance, encoded(&measured_wb)?);
    parameters.insert(StageId::AiDenoise, encoded(&settings.ai_denoise)?);
    parameters.insert(StageId::RelativeColor, encoded(&settings.relative_color)?);
    parameters.insert(StageId::Exposure, encoded(&settings.tone.exposure_ev)?);
    parameters.insert(
        StageId::Tone,
        encoded(&(
            settings.tone.contrast,
            settings.tone.highlights,
            settings.tone.shadows,
            settings.tone.whites,
            settings.tone.blacks,
        ))?,
    );
    parameters.insert(
        StageId::Curve,
        encoded(&(&settings.curve, &settings.curves))?,
    );
    parameters.insert(StageId::ColorMixer, encoded(&settings.color_mixer)?);
    parameters.insert(StageId::ColorGrading, encoded(&settings.grading)?);
    parameters.insert(
        StageId::Finishing,
        encoded(&(settings.grain, settings.vignette))?,
    );
    let portrait_mask_keys = settings
        .portrait_masks
        .iter()
        .map(|mask| {
            format!(
                "{}:{}:{:?}:{}x{}",
                mask.cache_key, mask.face_id, mask.region, mask.width, mask.height
            )
        })
        .collect::<Vec<_>>();
    let generated_mask_keys = settings
        .generated_masks
        .iter()
        .map(|mask| {
            format!(
                "{}:{:?}:{}x{}",
                mask.cache_identity, mask.semantic, mask.width, mask.height
            )
        })
        .collect::<Vec<_>>();
    parameters.insert(
        StageId::Mask,
        encoded(&(portrait_mask_keys, generated_mask_keys))?,
    );
    parameters.insert(StageId::Layers, encoded(&settings.layers)?);
    parameters.insert(StageId::Skin, encoded(&settings.skin_retouch)?);
    parameters.insert(StageId::Healing, encoded(&settings.healing_operations)?);
    parameters.insert(
        StageId::Detail,
        encoded(&(settings.denoise, settings.local_detail, settings.sharpen))?,
    );
    parameters.insert(StageId::Optics, encoded(&settings.optics)?);
    parameters.insert(StageId::Geometry, encoded(&settings.geometry)?);
    parameters.insert(
        StageId::DisplayTransform,
        "display:srgb:relative-colorimetric:bpc".into(),
    );
    let identity = StageStateIdentity::build(&RenderGraph::default(), source_identity, &parameters)
        .map_err(|error| format!("PreviewCacheIdentityFailed: {error:?}"))?;
    GpuStageCacheKeys::from_stage_identity(&identity)
        .ok_or_else(|| "PreviewCacheIdentityFailed: required GPU boundary is missing".into())
}

fn native_preview_inner(
    scheduler: &NativePreviewScheduler,
    portrait_runtime: &NativePortraitRuntime,
    ai_mask_runtime: &NativeAiMaskRuntime,
    ai_denoise_runtime: &NativeAiDenoiseRuntime,
    request: NativePreviewRequest,
) -> Result<Response, String> {
    let source_identity = preview_source_identity(&request.source_path)?;
    let requested_denoise_provider = request.settings.ai_denoise_provider;
    let mut settings = request.settings.validated()?;
    attach_portrait_masks(&mut settings, &request.source_path, portrait_runtime)?;
    attach_generated_masks(&mut settings, &request.source_path, ai_mask_runtime)?;
    let gpu_cache_keys = preview_stage_identity(&source_identity, &settings)?;
    let graph_identity = gpu_cache_keys.display.clone();
    settings.gpu_cache_keys = Some(gpu_cache_keys);
    let requested_edge = preview_requested_edge(request.max_edge, request.interaction_phase);
    let high_resolution = wants_high_resolution(request.resolution_mode, request.interaction_phase);
    let level = starroom_render::scheduler::PreviewLevel::for_requested_edge(requested_edge);
    let declared_tile = if high_resolution && viewport_graph_is_tile_safe(&settings) {
        request
            .viewport
            .map(|viewport| {
                validated_viewport(
                    Some(viewport),
                    viewport.source_width,
                    viewport.source_height,
                )
            })
            .transpose()?
    } else {
        None
    };
    let region = if let Some(viewport) = declared_tile {
        let mut expanded = expand_viewport(
            viewport,
            viewport.source_width,
            viewport.source_height,
            RenderGraph::default().maximum_halo(),
        );
        for operation in settings
            .healing_operations
            .iter()
            .filter(|operation| operation.enabled)
        {
            let bounds = healing_dirty_bounds(
                operation.source.map(|point| (point.x, point.y)),
                (operation.target.x, operation.target.y),
                operation.radius,
                operation.feather,
                operation.scale,
                viewport.source_width,
                viewport.source_height,
                RenderGraph::default().maximum_halo(),
            );
            expanded = union_viewport_with_rect(expanded, bounds);
        }
        let (source_width, source_height, image) = if is_raw_source(&request.source_path) {
            let full = if let Some(image) =
                cached_decoded_source(scheduler, &source_identity, u32::MAX)?
            {
                image
            } else {
                let image = Arc::new(
                    profiling::measure(ProfileStage::RawDecode, 0, || {
                        decode_source(&request.source_path)
                    })
                    .map_err(|error| {
                        format!("native high-resolution RAW decode failed: {error}")
                    })?,
                );
                cache_decoded_source(scheduler, source_identity.clone(), u32::MAX, image.clone())?;
                image
            };
            let dimensions = source_dimensions(&full);
            let crop = full
                .crop(expanded.x, expanded.y, expanded.width, expanded.height)
                .map_err(|error| format!("native high-resolution RAW crop failed: {error}"))?;
            (dimensions.0, dimensions.1, crop)
        } else {
            let decoded = profiling::measure(ProfileStage::RawDecode, 0, || {
                decode_source_region(
                    &request.source_path,
                    expanded.x,
                    expanded.y,
                    expanded.width,
                    expanded.height,
                )
            })
            .map_err(|error| format!("native high-resolution region decode failed: {error}"))?;
            (decoded.source_width, decoded.source_height, decoded.image)
        };
        if source_width != viewport.source_width || source_height != viewport.source_height {
            return Err(
                "PreviewViewportInvalid: source dimensions changed since Library registration"
                    .into(),
            );
        }
        Some((viewport, expanded, Arc::new(image)))
    } else {
        None
    };
    // Full sensor data and preview tiers have distinct identities. Reusing the immutable
    // full decode avoids running LibRaw again for every 1:1 slider edit.
    let decode_edge = if high_resolution {
        u32::MAX
    } else {
        level.max_edge()
    };
    let cached = if region.is_some() {
        None
    } else {
        cached_decoded_source(scheduler, &source_identity, decode_edge)?
    };
    let decoded = if let Some((_, _, image)) = &region {
        image.clone()
    } else if let Some(image) = cached {
        image
    } else {
        let image = Arc::new(
            profiling::measure(ProfileStage::RawDecode, 0, || {
                if high_resolution {
                    decode_source(&request.source_path)
                } else {
                    decode_source_preview(&request.source_path, level.max_edge())
                }
            })
            .map_err(|error| format!("native preview decode failed: {error}"))?,
        );
        // Keep a bounded source-resolution-tier cache, not an edit-state cache. Exposure changes
        // reuse immutable decoded sensor data; a source identity/preview-tier change cannot hit.
        cache_decoded_source(
            scheduler,
            source_identity.clone(),
            decode_edge,
            image.clone(),
        )?;
        image
    };
    let (source_width, source_height) = region
        .as_ref()
        .map(|(viewport, _, _)| (viewport.source_width, viewport.source_height))
        .unwrap_or_else(|| source_dimensions(&decoded));
    let viewport = region
        .as_ref()
        .map(|(viewport, _, _)| *viewport)
        .unwrap_or(validated_viewport(
            high_resolution.then_some(request.viewport).flatten(),
            source_width,
            source_height,
        )?);
    let tile_requested = high_resolution
        && (viewport.x != 0
            || viewport.y != 0
            || viewport.width != source_width
            || viewport.height != source_height);
    let tile_optimized = tile_requested && region.is_some();
    let halo = RenderGraph::default().maximum_halo();
    let expanded = if tile_optimized {
        region
            .as_ref()
            .map(|(_, expanded, _)| *expanded)
            .unwrap_or_else(|| expand_viewport(viewport, source_width, source_height, halo))
    } else {
        PreviewViewportRequest {
            source_width,
            source_height,
            x: 0,
            y: 0,
            width: source_width,
            height: source_height,
        }
    };
    if tile_optimized {
        settings.source_region = Some(SourceRegion {
            full_width: source_width,
            full_height: source_height,
            x: expanded.x,
            y: expanded.y,
        });
    }
    let render_decoded = decoded.clone();
    let viewport_cache_key = format!(
        "{source_identity}:{graph_identity}:{}:{}:{}:{}:{tile_optimized}",
        viewport.x, viewport.y, viewport.width, viewport.height
    );
    if high_resolution {
        let mut cache = scheduler
            .viewport_frames
            .lock()
            .map_err(|_| "PreviewCacheFailed: poisoned viewport cache".to_owned())?;
        if let Some(index) = cache.iter().position(|(key, _)| key == &viewport_cache_key) {
            let entry = cache.remove(index).expect("located viewport cache entry");
            let frame = entry.1.clone();
            cache.push_back(entry);
            profiling::record_tile_cache(true);
            return Ok(Response::new(frame));
        }
        profiling::record_tile_cache(false);
    }
    starroom_pipeline::cancellation::checkpoint().map_err(|error| error.to_string())?;
    attach_ai_denoise(
        &render_decoded,
        &request.source_path,
        &mut settings,
        requested_denoise_provider,
        &request.request_id,
        ai_denoise_runtime,
    )?;
    let job = scheduler
        .scheduler
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
    if !high_resolution
        && let Some(frame) = scheduler
            .scheduler
            .lock()
            .map_err(|_| "native preview scheduler lock was poisoned".to_owned())?
            .cached_tile(&frame_tile.identity)
    {
        profiling::record_tile_cache(true);
        return Ok(Response::new(frame));
    }
    if !high_resolution {
        profiling::record_tile_cache(false);
    }
    let (rendered, backend_flags) = if request.prefer_gpu {
        match recoverable_gpu_attempt(&scheduler.gpu, |gpu| {
            let renderer = gpu
                .get_or_insert_with(|| GpuRenderer::try_new().map_err(|error| error.to_string()))
                .as_ref()
                .map_err(Clone::clone)?;
            let result =
                render_source_preview_with_gpu_to_srgb8(&render_decoded, &settings, renderer);
            let rendered = match result {
                Ok(rendered) => rendered,
                Err(error) => {
                    let retire = gpu_failure_requires_retirement(&error);
                    let reason = renderer
                        .failure_diagnostic()
                        .map_or_else(|| error.to_string(), |detail| format!("{error}: {detail}"));
                    if retire {
                        *gpu = Some(Err(reason.clone()));
                    }
                    return Err(reason);
                }
            };
            let flag = match renderer.status().backend {
                GpuBackendKind::Dx12 | GpuBackendKind::Other => 0x0008,
                GpuBackendKind::CpuFallback => 0x0010,
            };
            Ok((rendered, flag))
        }) {
            Ok(result) => result,
            Err(error) => {
                let rendered = render_source_preview_to_srgb8(&render_decoded, &settings)
                    .map_err(|fallback| format!("native GPU preview failed ({error}); CPU reference fallback also failed: {fallback}"))?;
                // Binary contract explicitly marks the shared Native CPU graph, never Browser math.
                (rendered, 0x0010)
            }
        }
    } else {
        let rendered = render_source_preview_to_srgb8(&render_decoded, &settings)
            .map_err(|error| format!("native CPU preview graph failed: {error}"))?;
        (rendered, 0x0010)
    };
    starroom_pipeline::cancellation::checkpoint().map_err(|error| error.to_string())?;
    let flags = profile_flag(rendered.color.input)
        | backend_flags
        | if tile_requested { 0x0020 } else { 0 }
        | if tile_optimized { 0x0040 } else { 0 };
    let profile_id = rendered.color.camera_profile_id.as_deref().unwrap_or("");
    let (tile_data, tile_width, tile_height) = if tile_requested {
        let crop_x = viewport.x - expanded.x;
        let crop_y = viewport.y - expanded.y;
        (
            crop_rgb8(
                &rendered.data,
                rendered.width,
                rendered.height,
                crop_x,
                crop_y,
                viewport.width,
                viewport.height,
            )?,
            viewport.width,
            viewport.height,
        )
    } else {
        (rendered.data, rendered.width, rendered.height)
    };
    let preview_quality = match request.interaction_phase {
        PreviewInteractionPhase::Interactive => 88,
        PreviewInteractionPhase::Final => 96,
    };
    let jpeg = profiling::measure(ProfileStage::Encode, 0, || {
        encode_jpeg_rgb8(&tile_data, tile_width, tile_height, preview_quality, None)
    })
    .map_err(|error| format!("native preview encode failed: {error}"))?;
    let original_dimensions = match decoded.as_ref() {
        DecodedSourceImage::Rendered(_) => {
            starroom_imageio::encoded_dimensions(&request.source_path)
                .map_err(|error| format!("PreviewDimensionsFailed: {error}"))?
        }
        DecodedSourceImage::Raw(raw) => {
            let dimensions = (raw.metadata.active_width, raw.metadata.active_height);
            if matches!(raw.metadata.orientation, 5..=8) {
                (dimensions.1, dimensions.0)
            } else {
                dimensions
            }
        }
    };
    let frame = preview_frame(
        PreviewFrameHeader {
            width: tile_width,
            height: tile_height,
            source_width: original_dimensions.0,
            source_height: original_dimensions.1,
            tile_x: if tile_requested { viewport.x } else { 0 },
            tile_y: if tile_requested { viewport.y } else { 0 },
            flags,
        },
        profile_id,
        jpeg,
    )?;
    let estimated_vram_bytes = tile_width as usize * tile_height as usize * 8;
    if !high_resolution {
        let completion = scheduler
            .scheduler
            .lock()
            .map_err(|_| "native preview scheduler lock was poisoned".to_owned())?
            .complete_tile(&frame_tile, frame.clone(), estimated_vram_bytes);
        if completion == Completion::Stale {
            return Err("native preview was superseded by a newer render request".into());
        }
    } else {
        const VIEWPORT_CACHE_BUDGET: usize = 64 * 1024 * 1024;
        let mut cache = scheduler
            .viewport_frames
            .lock()
            .map_err(|_| "PreviewCacheFailed: poisoned viewport cache".to_owned())?;
        while !cache.is_empty()
            && (cache.len() >= 12
                || cache.iter().map(|(_, bytes)| bytes.len()).sum::<usize>() + frame.len()
                    > VIEWPORT_CACHE_BUDGET)
        {
            cache.pop_front();
        }
        if frame.len() <= VIEWPORT_CACHE_BUDGET {
            cache.push_back((viewport_cache_key, frame.clone()));
        }
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
    attach_portrait_masks(&mut settings, &request.source_path, &portrait_runtime)?;
    attach_generated_masks(&mut settings, &request.source_path, &ai_mask_runtime)?;
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

struct ExportRunGuard(NativeExportRuntime);

impl Drop for ExportRunGuard {
    fn drop(&mut self) {
        if let Ok(mut progress) = self.0.progress.lock() {
            progress.running = false;
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

impl ProfessionalExportItemRequest {
    fn professional(
        &self,
        destination: &Path,
        settings: &ExportSettings,
    ) -> ProfessionalExportRequest {
        ProfessionalExportRequest {
            asset_id: self.asset_id,
            source_path: self.source_path.clone(),
            destination_directory: destination.to_owned(),
            original_name: self.original_name.clone(),
            capture_date: self.capture_date.clone(),
            rating: self.rating,
            keywords: self.keywords.clone(),
            camera: self.camera.clone(),
            look: self.look.clone(),
            sequence: self.sequence,
            source_fingerprint: self.source_fingerprint.clone(),
            edit_state_identity: self.edit_state_identity.clone(),
            settings: settings.clone(),
        }
    }
}

fn guard_library_export_destination(
    runtime: &NativeLibraryRuntime,
    destination: &Path,
) -> Result<(), starroom_export::ExportError> {
    let guard = runtime.library.lock().map_err(|_| {
        starroom_export::ExportError::ProjectInvalid(
            "CorruptDatabase: library lock poisoned".into(),
        )
    })?;
    if let Some(library) = guard.as_ref()
        && library
            .is_registered_source_destination(destination)
            .map_err(|error| starroom_export::ExportError::ProjectInvalid(error.to_string()))?
    {
        return Err(starroom_export::ExportError::SourceOverwriteForbidden(
            destination.to_owned(),
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfessionalExportBatchRequest {
    destination_directory: PathBuf,
    settings: ExportSettings,
    #[serde(default)]
    items: Vec<ProfessionalExportItemRequest>,
    #[serde(default)]
    asset_ids: Vec<i64>,
    active_edit: Option<LibraryExportActiveEdit>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryExportActiveEdit {
    asset_id: i64,
    edit_settings: NativeEditSettings,
}

fn library_export_item(
    library: &Library,
    history_root: &Path,
    asset_id: i64,
    sequence: u32,
    active: Option<&LibraryExportActiveEdit>,
) -> Result<ProfessionalExportItemRequest, String> {
    let asset = library
        .asset(asset_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("MissingSource: library asset {asset_id}"))?;
    let path = history_root.join(format!("asset-{asset_id}.history.json"));
    // Even an active override cannot hide damaged durable edits. Only a genuinely absent
    // History file means neutral; permission/corruption errors must fail this item explicitly.
    let history = match std::fs::metadata(&path) {
        Ok(_) => Some(
            EditHistory::load(&path)
                .map_err(|error| format!("HistoryCorrupt: asset {asset_id}: {error}"))?,
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "HistoryPersistenceFailed: asset {asset_id}: {error}"
            ));
        }
    };
    let durable: Option<NativeEditSettings> = history
        .as_ref()
        .map(|history| {
            let state: NativeEditSettings = serde_json::from_value(history.state().clone())
                .map_err(|error| format!("HistoryCorrupt: asset {asset_id}: {error}"))?;
            state
                .clone()
                .validated()
                .map_err(|error| format!("HistoryCorrupt: asset {asset_id}: {error}"))?;
            Ok::<_, String>(state)
        })
        .transpose()?;
    let edit_settings = if let Some(active) = active.filter(|edit| edit.asset_id == asset_id) {
        active.edit_settings.clone()
    } else if let Some(state) = durable {
        state
    } else {
        serde_json::from_str(include_str!(
            "../../fixtures/contracts/native-default-settings.json"
        ))
        .map_err(|error| format!("HistoryCorrupt: neutral contract: {error}"))?
    };
    edit_settings.clone().validated()?;
    let edit_state_identity = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&edit_settings)
                .map_err(|error| format!("ProjectInvalid: {error}"))?
        )
    );
    let original_name = asset
        .source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("ProjectInvalid: invalid source name for asset {asset_id}"))?
        .to_owned();
    let camera = [
        asset.metadata.camera_make.as_deref(),
        asset.metadata.camera_model.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
    Ok(ProfessionalExportItemRequest {
        asset_id,
        original_name,
        capture_date: library
            .export_capture_date(asset_id)
            .map_err(|error| error.to_string())?,
        source_path: asset.source_path,
        rating: asset.rating,
        keywords: asset.keywords,
        camera: (!camera.is_empty()).then_some(camera),
        look: None,
        sequence,
        source_fingerprint: asset.content_fingerprint,
        edit_state_identity,
        edit_settings,
    })
}

#[tauri::command]
async fn native_export_batch(
    library_runtime: State<'_, NativeLibraryRuntime>,
    portrait_runtime: State<'_, NativePortraitRuntime>,
    ai_mask_runtime: State<'_, NativeAiMaskRuntime>,
    ai_denoise_runtime: State<'_, NativeAiDenoiseRuntime>,
    export_runtime: State<'_, NativeExportRuntime>,
    request: ProfessionalExportBatchRequest,
) -> Result<BatchExportResult, String> {
    if request.items.is_empty() && request.asset_ids.is_empty() {
        return Err("ProjectInvalid: export selection is empty".into());
    }
    if !request.items.is_empty() && !request.asset_ids.is_empty() {
        return Err("ProjectInvalid: choose either direct items or Library IDs".into());
    }
    if request.asset_ids.iter().any(|id| *id <= 0)
        || request
            .asset_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != request.asset_ids.len()
        || request
            .active_edit
            .as_ref()
            .is_some_and(|edit| !request.asset_ids.contains(&edit.asset_id))
    {
        return Err(
            "ProjectInvalid: invalid/duplicate Library selection or active override".into(),
        );
    }
    let total = request.items.len() + request.asset_ids.len();
    {
        let mut progress = export_runtime
            .progress
            .lock()
            .map_err(|_| "export progress lock was poisoned".to_owned())?;
        if progress.running {
            return Err("ProjectInvalid: another export batch is running".into());
        }
        export_runtime.cancelled.store(false, Ordering::Relaxed);
        *progress = NativeExportProgress {
            running: true,
            progress: BatchProgress {
                total,
                ..Default::default()
            },
        };
    }
    let _run_guard = ExportRunGuard(export_runtime.inner().clone());
    let mut result = BatchExportResult::default();
    let mut direct = request.items.into_iter();
    for index in 0..total {
        let item = if let Some(item) = direct.next() {
            item
        } else {
            let asset_id = request.asset_ids[index];
            if export_runtime.cancelled.load(Ordering::Relaxed) {
                result.cancelled.push(ExportItemResult {
                    asset_id,
                    status: ExportItemStatus::Cancelled,
                    destination: None,
                    width: None,
                    height: None,
                    recipe_identity: String::new(),
                    error: Some("Cancelled".into()),
                });
                set_export_progress(&export_runtime, true, total, &result)?;
                continue;
            }
            let runtime = library_runtime.inner().clone();
            let active = request.active_edit.clone();
            let prepared = tauri::async_runtime::spawn_blocking(move || {
                let path = history_path(asset_id)?;
                let guard = runtime
                    .library
                    .lock()
                    .map_err(|_| "CorruptDatabase: library lock poisoned".to_owned())?;
                let library = guard
                    .as_ref()
                    .ok_or_else(|| "DatabaseOpenFailed: library is not open".to_owned())?;
                library_export_item(
                    library,
                    path.parent().unwrap(),
                    asset_id,
                    (index + 1) as u32,
                    active.as_ref(),
                )
            })
            .await
            .map_err(|error| format!("ProjectInvalid: Library export worker failed: {error}"))
            .and_then(|prepared| prepared);
            match prepared {
                Ok(item) => item,
                Err(error) => {
                    result.failed.push(ExportItemResult {
                        asset_id,
                        status: ExportItemStatus::Failed,
                        destination: None,
                        width: None,
                        height: None,
                        recipe_identity: String::new(),
                        error: Some(error),
                    });
                    set_export_progress(&export_runtime, true, total, &result)?;
                    continue;
                }
            }
        };
        let professional = item.professional(&request.destination_directory, &request.settings);
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
            attach_portrait_masks(&mut settings, &item.source_path, &portrait_runtime)?;
            attach_generated_masks(&mut settings, &item.source_path, &ai_mask_runtime)?;
            // The production FullResolutionRenderer owns the one normal source decode. The M26
            // adapter previously decoded every item here as well even when AI Denoise was off,
            // doubling RAW/raster open work and peak buffer churn. Only model inference needs a
            // prepared decoded image before the shared export graph.
            if settings.ai_denoise.enabled {
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
            }
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
        let library = library_runtime.inner().clone();
        let (professional, item_result) = tauri::async_runtime::spawn_blocking(move || {
            let result = export_one_with_destination_guard(
                &NativeSharedGraphRenderer,
                &professional,
                &settings,
                &cancelled,
                |destination| guard_library_export_destination(&library, destination),
            );
            (professional, result)
        })
        .await
        .map_err(|error| format!("AtomicWriteFailed: export worker failed: {error}"))?;
        match item_result {
            Ok(item) => result.completed.push(item),
            Err(starroom_export::ExportError::Cancelled) => result.cancelled.push(export_failure(
                &professional,
                ExportItemStatus::Cancelled,
                "Cancelled",
            )),
            Err(error) => result.failed.push(export_failure(
                &professional,
                ExportItemStatus::Failed,
                &error.to_string(),
            )),
        }
        set_export_progress(&export_runtime, true, total, &result)?;
    }
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseSelfTestReport {
    schema_version: u32,
    library: &'static str,
    history: &'static str,
    session: &'static str,
    native_export: &'static str,
    deterministic_export: bool,
    source_immutable: bool,
    face_skin: &'static str,
    subject_background: &'static str,
    sky: &'static str,
    ai_denoise: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseAiSelfTestReport {
    schema_version: u32,
    subject_background: &'static str,
    model_hash: String,
    execution_provider: starroom_portrait::ExecutionProvider,
    output_width: u32,
    output_height: u32,
    finite: bool,
}

fn release_ai_state(feature: AiFeatureAvailability) -> Result<&'static str, String> {
    match feature.state {
        "ready" => Ok("available"),
        "modelNotInstalled" => Ok("typed-unavailable"),
        "invalid" => Err(format!("installed AI model is invalid: {}", feature.detail)),
        _ => Err(format!("AI availability check failed: {}", feature.detail)),
    }
}

/// Runs one real, offline Subject inference through the packaged ONNX Runtime provider. The
/// release installer invokes this in addition to the cheap availability report so a copied weight
/// file cannot be mistaken for a usable clean-install capability.
pub fn release_ai_self_test() -> Result<ReleaseAiSelfTestReport, String> {
    let mut registry = local_ai_mask_models();
    registry.execution_provider = starroom_portrait::ExecutionProvider::Cpu;
    let mut provider = AiMaskOnnxProvider::initialize_for(registry, AiMaskSemantic::Subject)
        .map_err(|error| format!("release Subject provider initialization failed: {error}"))?;
    let width = 16_u32;
    let height = 16_u32;
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&[
                ((x * 255) / (width - 1)) as u8,
                ((y * 255) / (height - 1)) as u8,
                96,
                255,
            ]);
        }
    }
    let token = AtomicBool::new(false);
    let result = provider
        .generate(
            width,
            height,
            &rgba,
            "release-ai-self-test-fixture-v1",
            AiMaskSemantic::Subject,
            &token,
        )
        .map_err(|error| format!("release Subject inference failed: {error}"))?;
    let finite = !result.mask.values.is_empty()
        && result.mask.values.iter().all(|value| value.is_finite())
        && result
            .mask
            .values
            .iter()
            .any(|value| *value > 0.0 && *value < 1.0);
    if !finite {
        return Err("release Subject inference returned an invalid probability mask".into());
    }
    Ok(ReleaseAiSelfTestReport {
        schema_version: 1,
        subject_background: "ok",
        model_hash: result.model_hash,
        execution_provider: result.execution_provider,
        output_width: result.mask.width,
        output_height: result.mask.height,
        finite,
    })
}

/// Executes a bounded, offline production-API workflow from the packaged executable.
///
/// This is a release diagnostic, not an alternate renderer: it imports a generated encoded source,
/// persists Library/History/Session state and renders twice through `NativeSharedGraphRenderer`.
/// The caller must provide a new or empty directory so no user state can be overwritten.
pub fn release_self_test(root: &Path) -> Result<ReleaseSelfTestReport, String> {
    if root.exists()
        && root
            .read_dir()
            .map_err(|error| error.to_string())?
            .next()
            .is_some()
    {
        return Err(format!(
            "release self-test directory is not empty: {}",
            root.display()
        ));
    }
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;

    let width = 64;
    let height = 48;
    let mut rgb = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            rgb.extend_from_slice(&[(x * 4) as u8, (y * 5) as u8, ((x + y) * 2) as u8]);
        }
    }
    let source = root.join("release-self-test.jpg");
    let encoded = encode_jpeg_rgb8(&rgb, width as u32, height as u32, 95, None)
        .map_err(|error| error.to_string())?;
    std::fs::write(&source, encoded).map_err(|error| error.to_string())?;
    let source_before = Sha256::digest(std::fs::read(&source).map_err(|error| error.to_string())?);

    let mut library =
        Library::open(root.join("library.sqlite")).map_err(|error| error.to_string())?;
    let asset_id = library
        .import_paths(std::slice::from_ref(&source), &AtomicBool::new(false))
        .map_err(|error| error.to_string())?
        .imported
        .into_iter()
        .next()
        .ok_or_else(|| "release self-test source was not imported".to_owned())?;
    library
        .set_workflow(&[asset_id], Some(5), Some(AssetFlag::Pick), None)
        .map_err(|error| error.to_string())?;

    library
        .add_keywords(&[asset_id], &["release-self-test".into()])
        .map_err(|error| error.to_string())?;
    let initial: serde_json::Value = serde_json::from_str(include_str!(
        "../../fixtures/contracts/native-default-settings.json"
    ))
    .map_err(|error| error.to_string())?;
    let mut changed = initial.clone();
    changed["exposure"] = serde_json::json!(0.25);
    let mut history = EditHistory::new(initial).map_err(|error| error.to_string())?;
    history
        .commit(
            "Release exposure",
            "tone",
            EditCommand::ReplaceState {
                before: history.state().clone(),
                after: changed,
            },
        )
        .map_err(|error| error.to_string())?;
    history
        .create_snapshot("Release checkpoint")
        .map_err(|error| error.to_string())?;
    let history_root = root.join("history");
    let history_path = history_root.join(format!("asset-{asset_id}.history.json"));
    history
        .persist(&history_path)
        .map_err(|error| error.to_string())?;
    let loaded_history = EditHistory::load(&history_path).map_err(|error| error.to_string())?;
    if loaded_history.state_version() != history.state_version() {
        return Err("release History round-trip changed the edit identity".into());
    }

    let session_path = root.join("session.json");
    let session = SessionState {
        version: starroom_session::SESSION_VERSION,
        workspace: "edit".into(),
        selected_asset_id: Some(asset_id),
        selected_source_path: Some(source.clone()),
        active_tool: "light".into(),
        library_panel_open: true,
        filmstrip_open: true,
        zoom_mode: "fit".into(),
        zoom_scale: 1.0,
        library_context: "all".into(),
        library_browser: Some(starroom_session::LibraryBrowserState {
            collection_id: None,
            search: "release-self-test".into(),
            page: 0,
        }),
    };
    starroom_session::autosave(&session_path, &session).map_err(|error| error.to_string())?;
    if !starroom_session::open(&session_path)
        .map_err(|error| error.to_string())?
        .recovery_available
    {
        return Err("release Session autosave did not expose recovery state".into());
    }
    starroom_session::mark_clean(&session_path, &session).map_err(|error| error.to_string())?;
    let restored_session =
        starroom_session::open(&session_path).map_err(|error| error.to_string())?;
    if restored_session.recovery_available || restored_session.state.as_ref() != Some(&session) {
        return Err("release Session clean close did not preserve complete workspace state".into());
    }

    drop(library);
    let library = Library::open(root.join("library.sqlite")).map_err(|error| error.to_string())?;
    let item = library_export_item(&library, &history_root, asset_id, 1, None)?;
    let render_settings = item.edit_settings.clone().validated()?;
    if render_settings.tone.exposure_ev != 0.25
        || item.rating != 5
        || item.keywords != ["release-self-test"]
    {
        return Err("release Library export did not restore persisted edits and metadata".into());
    }
    let request = item.professional(&root.join("exports"), &ExportSettings::default());
    let first = export_one(
        &NativeSharedGraphRenderer,
        &request,
        &render_settings,
        &AtomicBool::new(false),
    )
    .map_err(|error| error.to_string())?
    .destination
    .ok_or_else(|| "release export returned no destination".to_owned())?;
    let second = export_one(
        &NativeSharedGraphRenderer,
        &request,
        &render_settings,
        &AtomicBool::new(false),
    )
    .map_err(|error| error.to_string())?
    .destination
    .ok_or_else(|| "second release export returned no destination".to_owned())?;
    let deterministic_export = std::fs::read(first).map_err(|error| error.to_string())?
        == std::fs::read(second).map_err(|error| error.to_string())?;
    if !deterministic_export {
        return Err("release Native exports are not deterministic".into());
    }
    let source_after = Sha256::digest(std::fs::read(&source).map_err(|error| error.to_string())?);
    let source_immutable = source_before == source_after;
    if !source_immutable {
        return Err("release workflow modified source pixels".into());
    }

    let availability = ai_availability_status();
    let face_skin = release_ai_state(availability.face_skin)?;
    let subject_background = release_ai_state(availability.subject_background)?;
    let sky = release_ai_state(availability.sky)?;
    let ai_denoise = release_ai_state(availability.denoise)?;

    Ok(ReleaseSelfTestReport {
        schema_version: 2,
        library: "ok",
        history: "ok",
        session: "ok",
        native_export: "ok",
        deterministic_export,
        source_immutable,
        face_skin,
        subject_background,
        sky,
        ai_denoise,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if std::env::var_os("STARROOM_LOCAL_MODELS").is_none() {
                let root = app
                    .path()
                    .app_local_data_dir()?
                    .join("models")
                    .join("local");
                let _ = APP_LOCAL_MODEL_ROOT.set(root);
            }
            Ok(())
        })
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
            ai_availability_status,
            gpu_preview_status,
            advise_image,
            advise_native_image,
            native_preview,
            native_preview_cancel,
            native_preview_scheduler_status,
            native_preview_profile,
            native_export_jpeg,
            portrait_detect,
            portrait_models_install_local,
            ai_mask_generate,
            ai_mask_cancel,
            ai_denoise_cancel,
            native_sample_color,
            native_white_balance_info,
            native_optics_status,
            native_reference_match,
            native_look_save,
            native_look_apply,
            native_look_mix,
            library_open_default,
            library_import_folder,
            library_cancel_import,
            library_query,
            library_query_ids,
            library_counts,
            library_refresh_metadata,
            library_set_workflow,
            library_add_keywords,
            library_remove_keywords,
            library_remove_assets,
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
            session_open,
            session_autosave,
            session_mark_clean,
            session_discard_recovery,
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

    #[test]
    fn professional_desktop_export_protects_another_library_original() {
        let root = std::env::temp_dir().join(format!(
            "starroom-desktop-export-safety-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let source = root.join("source-a.png");
        let protected = root.join("source-b.png");
        let a = starroom_imageio::encode_png_rgb8(&[20, 80, 140], 1, 1, None).unwrap();
        let b = starroom_imageio::encode_png_rgb8(&[210, 90, 40], 1, 1, None).unwrap();
        std::fs::write(&source, &a).unwrap();
        std::fs::write(&protected, &b).unwrap();
        let mut library = Library::open(root.join("library.sqlite")).unwrap();
        library
            .import_paths(
                &[source.clone(), protected.clone()],
                &AtomicBool::new(false),
            )
            .unwrap();
        let runtime = NativeLibraryRuntime {
            library: Arc::new(Mutex::new(Some(library))),
            ..Default::default()
        };
        let request = ProfessionalExportRequest {
            asset_id: 1,
            source_path: source.clone(),
            destination_directory: root.clone(),
            original_name: "source-a.png".into(),
            capture_date: None,
            rating: 0,
            keywords: Vec::new(),
            camera: None,
            look: None,
            sequence: 1,
            source_fingerprint: "owned-test".into(),
            edit_state_identity: "neutral".into(),
            settings: ExportSettings {
                format: starroom_export::ExportFormat::Png,
                filename_template: "source-b".into(),
                collision: starroom_export::CollisionPolicy::Overwrite,
                ..Default::default()
            },
        };
        let result = export_one_with_destination_guard(
            &NativeSharedGraphRenderer,
            &request,
            &RenderSettings::default(),
            &AtomicBool::new(false),
            |path| guard_library_export_destination(&runtime, path),
        );
        assert!(matches!(
            result,
            Err(starroom_export::ExportError::SourceOverwriteForbidden(_))
        ));
        assert_eq!(std::fs::read(source).unwrap(), a);
        assert_eq!(std::fs::read(protected).unwrap(), b);
        drop(runtime);
        let resolved = root.canonicalize().unwrap();
        assert_eq!(
            resolved.parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        std::fs::remove_dir_all(resolved).unwrap();
    }

    #[test]
    fn library_export_reads_off_page_durable_edits_and_does_not_hide_corruption() {
        let root = std::env::temp_dir().join(format!(
            "starroom-library-export-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/raw/sources/nikon-d1.nef");
        let source_before = std::fs::read(&source).unwrap();
        let mut library = Library::open(root.join("library.sqlite")).unwrap();
        let id = library
            .import_paths(std::slice::from_ref(&source), &AtomicBool::new(false))
            .unwrap()
            .imported[0];
        library.set_workflow(&[id], Some(4), None, None).unwrap();
        library.add_keywords(&[id], &["批次".into()]).unwrap();
        let mut metadata = Library::extract_metadata(&source).unwrap();
        metadata.capture_time = Some(0);
        library.update_metadata(id, &metadata).unwrap();
        let neutral = library_export_item(&library, &root, id, 501, None).unwrap();
        assert_eq!(neutral.sequence, 501);
        assert_eq!(neutral.rating, 4);
        assert_eq!(neutral.keywords, vec!["批次"]);
        assert_eq!(neutral.capture_date.as_deref(), Some("1970-01-01"));
        assert_eq!(neutral.edit_settings.exposure, 0.0);
        let initial = serde_json::to_value(&neutral.edit_settings).unwrap();
        let mut changed = initial.clone();
        changed["exposure"] = serde_json::json!(1.25);
        let mut history = EditHistory::new(initial.clone()).unwrap();
        history
            .commit(
                "Exposure",
                "tone",
                EditCommand::ReplaceState {
                    before: initial,
                    after: changed,
                },
            )
            .unwrap();
        let history_path = root.join(format!("asset-{id}.history.json"));
        history.persist(&history_path).unwrap();
        let history_bytes = std::fs::read(&history_path).unwrap();
        let saved = library_export_item(&library, &root, id, 501, None).unwrap();
        assert_eq!(saved.edit_settings.exposure, 1.25);
        assert_ne!(saved.edit_state_identity, neutral.edit_state_identity);
        let active = LibraryExportActiveEdit {
            asset_id: id,
            edit_settings: NativeEditSettings {
                exposure: 2.0,
                ..neutral.edit_settings.clone()
            },
        };
        let active_item = library_export_item(&library, &root, id, 501, Some(&active)).unwrap();
        assert_eq!(active_item.edit_settings.exposure, 2.0);
        assert_ne!(active_item.edit_state_identity, saved.edit_state_identity);
        assert_eq!(std::fs::read(&history_path).unwrap(), history_bytes);
        assert!(
            library_export_item(&library, &root, id + 10000, 1, None)
                .unwrap_err()
                .starts_with("MissingSource:")
        );
        // Actual sensor decode -> shared graph -> output, not a mocked renderer or thumbnail.
        let settings = saved.edit_settings.clone().validated().unwrap();
        let professional = saved.professional(&root.join("out"), &ExportSettings::default());
        let output = export_one(
            &NativeSharedGraphRenderer,
            &professional,
            &settings,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(output.width.unwrap() > 1000);
        let exported_bytes = std::fs::read(output.destination.unwrap()).unwrap();
        assert!(!exported_bytes.is_empty());
        let mut reference = professional.clone();
        reference.destination_directory = root.join("reference");
        let repeated = export_one(
            &NativeSharedGraphRenderer,
            &reference,
            &settings,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            std::fs::read(repeated.destination.unwrap()).unwrap(),
            exported_bytes
        );
        reference.destination_directory = root.join("neutral");
        let unedited = export_one(
            &NativeSharedGraphRenderer,
            &reference,
            &neutral.edit_settings.validated().unwrap(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_ne!(
            std::fs::read(unedited.destination.unwrap()).unwrap(),
            exported_bytes
        );
        assert_eq!(std::fs::read(&source).unwrap(), source_before);
        std::fs::write(&history_path, b"{corrupt").unwrap();
        assert!(
            library_export_item(&library, &root, id, 1, Some(&active))
                .unwrap_err()
                .starts_with("HistoryCorrupt:")
        );
        metadata.capture_time = None;
        library.update_metadata(id, &metadata).unwrap();
        assert_eq!(library.export_capture_date(id).unwrap(), None);
        drop(library);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn edited_album_uses_saved_state_across_restart_undo_and_corruption() {
        let root = std::env::temp_dir().join(format!(
            "starroom-edited-album-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(persisted_edited_asset_ids(&root).unwrap().is_empty());
        std::fs::create_dir_all(&root).unwrap();
        let neutral: serde_json::Value = serde_json::from_str(include_str!(
            "../../fixtures/contracts/native-default-settings.json"
        ))
        .unwrap();
        let mut history = EditHistory::new(neutral.clone()).unwrap();
        assert_eq!(history_result(&history).edited, Some(false));
        let mut changed = neutral.clone();
        changed["exposure"] = serde_json::json!(1.0);
        history
            .commit(
                "Exposure",
                "tone",
                EditCommand::ReplaceState {
                    before: neutral.clone(),
                    after: changed,
                },
            )
            .unwrap();
        let path = root.join("asset-7.history.json");
        history.persist(&path).unwrap();
        assert_eq!(history_result(&history).edited, Some(true));
        assert_eq!(persisted_edited_asset_ids(&root).unwrap(), vec![7]);
        let mut reopened = EditHistory::load(&path).unwrap();
        reopened.undo().unwrap();
        assert_eq!(history_result(&reopened).edited, Some(false));
        reopened.persist(&path).unwrap();
        assert!(persisted_edited_asset_ids(&root).unwrap().is_empty());
        reopened.redo().unwrap();
        assert_eq!(history_result(&reopened).edited, Some(true));
        reopened.persist(&path).unwrap();
        let mut ui_neutral = neutral;
        ui_neutral["curve"] = serde_json::json!([{"x":0,"y":0},{"x":1,"y":1}]);
        ui_neutral["curves"]["red"] = ui_neutral["curve"].clone();
        ui_neutral["aiDenoiseProvider"] = serde_json::json!("cpu");
        EditHistory::new(ui_neutral)
            .unwrap()
            .persist(root.join("asset-8.history.json"))
            .unwrap();
        assert_eq!(persisted_edited_asset_ids(&root).unwrap(), vec![7]);
        std::fs::write(root.join("asset-9.history.json"), b"broken").unwrap();
        assert!(
            persisted_edited_asset_ids(&root)
                .unwrap_err()
                .contains("HistoryCorrupt: asset 9")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_ai_restoration_rejects_foreign_source_and_model_before_inference() {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/golden/sources/astronaut-eileen-collins.png");
        let hash = source_content_hash(&source).unwrap();
        let registry = local_ai_mask_models();
        let leaf = MaskDefinition::Generated {
            provider_id: "foreground".into(),
            model_id: registry.foreground.id,
            model_version: registry.foreground.version,
            model_hash: registry.foreground.sha256.clone(),
            semantic_class: GeneratedMaskSemantic::Subject,
            threshold: 0.5,
            feather: 0.1,
            invert: false,
            cache_identity: AiMaskOnnxProvider::cache_identity(
                "different-source",
                AiMaskSemantic::Subject,
                &registry.foreground.sha256,
            ),
            metadata: BTreeMap::new(),
        };
        let mut settings = RenderSettings::default();
        settings.layers.push(NativeAdjustmentLayer {
            id: "restored".into(),
            name: "Restored".into(),
            enabled: true,
            opacity: 1.0,
            blend_mode: starroom_pipeline::LayerBlendMode::Normal,
            mask: leaf.clone().into(),
            adjustments: Default::default(),
        });
        let runtime = NativeAiMaskRuntime::default();
        let error = attach_generated_masks(&mut settings, &source, &runtime).unwrap_err();
        assert!(error.starts_with("MaskSourceMismatch:"), "{error}");
        assert!(settings.generated_masks.is_empty() && runtime.cache.lock().unwrap().is_empty());
        assert!(
            runtime.provider.lock().unwrap().is_none(),
            "foreign mask must never initialize a provider"
        );
        let mut wrong_model = leaf;
        if let MaskDefinition::Generated {
            model_version,
            cache_identity,
            ..
        } = &mut wrong_model
        {
            *model_version = "different-model-version".into();
            *cache_identity = AiMaskOnnxProvider::cache_identity(
                &hash,
                AiMaskSemantic::Subject,
                &registry.foreground.sha256,
            );
        }
        settings.layers[0].mask = wrong_model.into();
        assert!(
            attach_generated_masks(&mut settings, &source, &runtime)
                .unwrap_err()
                .starts_with("MaskModelMismatch:")
        );
        settings.layers[0].enabled = false;
        assert!(attach_generated_masks(&mut settings, &source, &runtime).is_ok());
        settings.layers[0].enabled = true;
        settings.layers[0].opacity = 0.0;
        assert!(attach_generated_masks(&mut settings, &source, &runtime).is_ok());
        assert_eq!(source_content_hash(&source).unwrap(), hash);
    }

    #[test]
    fn native_portrait_restore_metadata_matches_shared_wire_and_rejects_invalid_versions() {
        let crop: PortraitSourceCrop = serde_json::from_str(include_str!(
            "../../fixtures/contracts/native-portrait-source-crop.json"
        ))
        .unwrap();
        let reference = starroom_pipeline::SkinRetouchFaceReference {
            face_id: "face".into(),
            cache_key: "exact".into(),
            source_crop: Some(crop),
        };
        let json = serde_json::to_value(&reference).unwrap();
        assert_eq!(json["sourceCrop"], serde_json::to_value(crop).unwrap());
        assert!(json.get("source_crop").is_none());
        let old = serde_json::json!({ "faceId": "face", "cacheKey": "exact" });
        let legacy: starroom_pipeline::SkinRetouchFaceReference =
            serde_json::from_value(old.clone()).unwrap();
        assert_eq!(serde_json::to_value(legacy).unwrap(), old);
        let mut settings = RenderSettings::default();
        settings
            .skin_retouch
            .faces
            .push(starroom_pipeline::SkinRetouchFaceReference {
                source_crop: Some(PortraitSourceCrop { version: 2, ..crop }),
                ..reference
            });
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/golden/sources/astronaut-eileen-collins.png");
        assert!(
            attach_portrait_masks(&mut settings, &source, &NativePortraitRuntime::default())
                .unwrap_err()
                .starts_with("PortraitRestoreMetadataInvalid:")
        );
    }

    #[test]
    fn native_color_sample_ipc_transports_geometry_and_validates_coordinate_space() {
        let mut settings = neutral_settings();
        settings.geometry.rotation_degrees = 90.0;
        settings.geometry.flip_horizontal = true;
        settings.geometry.crop = starroom_geometry::CropRect {
            left: 0.1,
            top: 0.2,
            right: 0.9,
            bottom: 0.8,
        };
        let request = serde_json::json!({ "sourcePath": "photo.nef", "x": 0.2, "y": 0.7,
            "maxEdge": 2048, "coordinateSpace": "postGeometry", "settings": settings });
        let parsed: NativeColorSampleRequest = serde_json::from_value(request.clone()).unwrap();
        assert_eq!(parsed.max_edge, 2048);
        assert_eq!(parsed.settings.geometry, settings.geometry);
        let mut unsupported = request;
        unsupported["coordinateSpace"] = serde_json::json!("sourceSensor");
        assert!(serde_json::from_value::<NativeColorSampleRequest>(unsupported).is_err());
        let invalid = NativeColorSampleRequest {
            source_path: "missing.png".into(),
            x: 0.5,
            y: 0.5,
            max_edge: 128,
            coordinate_space: NativeSampleCoordinateSpace::PostGeometry,
            settings: neutral_settings(),
        };
        assert!(
            native_sample_color(invalid)
                .unwrap_err()
                .starts_with("NativeColorSampleInvalid:")
        );
    }

    #[test]
    fn native_color_sample_command_reads_actual_png_after_geometry() {
        let path = std::env::temp_dir().join(format!(
            "starroom-native-sampling-{}.png",
            std::process::id()
        ));
        let png = starroom_imageio::encode_png_rgb8(
            &[180, 20, 18, 20, 170, 25, 15, 22, 180, 170, 145, 20],
            2,
            2,
            None,
        )
        .unwrap();
        std::fs::write(&path, png).unwrap();
        let decoded = decode_source_preview(&path, 1800).unwrap();
        let mut native = neutral_settings();
        native.geometry.rotation_degrees = 90.0;
        native.geometry.flip_horizontal = true;
        let expected =
            sample_source_color_band(&decoded, &native.clone().validated().unwrap(), 0.0, 0.0)
                .unwrap();
        let result = native_sample_color(NativeColorSampleRequest {
            source_path: path.clone(),
            x: 0.0,
            y: 0.0,
            max_edge: 1800,
            coordinate_space: NativeSampleCoordinateSpace::PostGeometry,
            settings: native,
        })
        .unwrap();
        assert!(result.is_some());
        assert_eq!(result, expected);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn ai_denoise_input_cache_invalidates_precreative_changes_not_creative_sliders() {
        let settings = neutral_settings().validated().unwrap();
        let identity = |settings: &RenderSettings| {
            ai_denoise_input_identity("unchanged-canonical-source", 1800, 1200, settings).unwrap()
        };
        let base = identity(&settings);
        let mut changes = vec![];
        let mut wb = settings.clone();
        wb.white_balance.mode = WhiteBalanceMode::Auto;
        changes.push(wb);
        let mut picker = settings.clone();
        picker.white_balance.mode = WhiteBalanceMode::NeutralPicker;
        picker.white_balance.sample = Some(WhiteBalanceSample {
            x: 0.2,
            y: 0.3,
            width: 0.01,
            height: 0.01,
        });
        changes.push(picker.clone());
        picker.white_balance.sample.as_mut().unwrap().x = 0.7;
        changes.push(picker);
        let mut geometry = settings.clone();
        geometry.geometry.rotation_degrees = 180.0;
        changes.push(geometry.clone());
        geometry.geometry.rotation_degrees = 0.0;
        geometry.geometry.flip_horizontal = true;
        changes.push(geometry.clone());
        geometry.geometry.crop.left = 0.1;
        changes.push(geometry);
        let mut lens = settings.clone();
        lens.optics.parameters.enabled = true;
        changes.push(lens.clone());
        lens.optics.parameters.distortion = false;
        changes.push(lens);
        let mut color = settings.clone();
        color.color_management.intent =
            starroom_color_management::RenderingIntent::AbsoluteColorimetric;
        changes.push(color);
        let mut tile = settings.clone();
        tile.source_region = Some(SourceRegion {
            full_width: 6000,
            full_height: 4000,
            x: 100,
            y: 200,
        });
        changes.push(tile.clone());
        tile.source_region.as_mut().unwrap().x = 600;
        changes.push(tile);
        let mut identities = std::collections::BTreeSet::from([base.clone()]);
        for changed in changes {
            assert!(
                identities.insert(identity(&changed)),
                "input edit must invalidate old residual"
            );
        }
        assert_ne!(
            base,
            ai_denoise_input_identity("another-source", 1800, 1200, &settings).unwrap()
        );
        assert_ne!(
            base,
            ai_denoise_input_identity("unchanged-canonical-source", 900, 600, &settings).unwrap()
        );
        let mut creative = settings.clone();
        creative.tone.exposure_ev = 1.0;
        creative.relative_color.temperature = 0.4;
        creative.curves.red.push(CurvePoint { x: 0.5, y: 0.6 });
        creative.local_detail.texture = 0.5;
        creative.denoise.luminance = 0.4;
        creative.sharpen.amount = 0.6;
        creative.ai_denoise.amount = 0.7;
        creative.ai_denoise.preserve_skin = 1.0;
        creative.layers.push(NativeAdjustmentLayer {
            id: "creative".into(),
            name: "creative".into(),
            enabled: true,
            opacity: 1.0,
            blend_mode: starroom_pipeline::LayerBlendMode::Normal,
            mask: MaskDefinition::None.into(),
            adjustments: starroom_pipeline::LayerAdjustments::default(),
        });
        assert_eq!(
            base,
            identity(&creative),
            "creative sliders must retain expensive inference cache"
        );
        assert!(base.starts_with("unchanged-canonical-source:1800x1200:precreative-"));
        assert!(
            settings.image_identity.is_empty(),
            "canonical image/model identity is not mutated"
        );
    }

    #[test]
    fn lazy_raw_metadata_refresh_repairs_legacy_dimensions_without_workflow_loss() {
        let root =
            std::env::temp_dir().join(format!("starroom-lazy-raw-metadata-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/raw/sources/nikon-d1.nef");
        let source_hash = source_content_hash(&source).unwrap();
        let mut library = Library::open(root.join("library.sqlite")).unwrap();
        let id = library
            .import_paths(std::slice::from_ref(&source), &AtomicBool::new(false))
            .unwrap()
            .imported[0];
        let mut legacy = Library::extract_metadata(&source).unwrap();
        legacy.width = Some(32);
        legacy.height = Some(21);
        library.update_metadata(id, &legacy).unwrap();
        library
            .set_workflow(
                &[id],
                Some(4),
                Some(AssetFlag::Pick),
                Some(ColorLabel::Purple),
            )
            .unwrap();
        library
            .add_keywords(&[id], &["原生 RAW 測試".into()])
            .unwrap();
        library
            .set_project_reference(id, Some("projects/non-destructive.starroom.json"))
            .unwrap();
        let before = library.asset(id).unwrap().unwrap();
        let history_path = root.join(format!("asset-{id}.history.json"));
        let initial = serde_json::json!({"exposure": 0.0});
        let mut history = EditHistory::new(initial.clone()).unwrap();
        history
            .commit(
                "Exposure",
                "Tone",
                EditCommand::ReplaceState {
                    before: initial,
                    after: serde_json::json!({"exposure": 0.5}),
                },
            )
            .unwrap();
        history.persist(&history_path).unwrap();
        let history_before = std::fs::read(&history_path).unwrap();
        let runtime = NativeLibraryRuntime {
            library: Arc::new(Mutex::new(Some(library))),
            ..Default::default()
        };

        let refreshed = library_refresh_metadata_inner(&runtime, id).unwrap();
        assert_eq!(
            (refreshed.metadata.width, refreshed.metadata.height),
            (Some(2012), Some(1324))
        );
        assert_eq!(refreshed.id, before.id);
        assert_eq!(refreshed.source_path, before.source_path);
        assert_eq!(refreshed.source_identity, before.source_identity);
        assert_eq!(refreshed.content_fingerprint, before.content_fingerprint);
        assert_eq!(refreshed.rating, before.rating);
        assert_eq!(refreshed.flag, before.flag);
        assert_eq!(refreshed.color_label, before.color_label);
        assert_eq!(refreshed.keywords, before.keywords);
        assert_eq!(refreshed.project_reference, before.project_reference);
        assert_eq!(std::fs::read(&history_path).unwrap(), history_before);
        assert_eq!(source_content_hash(&source).unwrap(), source_hash);

        // Another UI action can change workflow while LibRaw works outside the lock. Applying
        // the metadata must return the current workflow, never restore the stale snapshot.
        let snapshot = library_metadata_refresh_snapshot(&runtime, id).unwrap();
        runtime
            .library
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .set_workflow(&[id], Some(5), None, None)
            .unwrap();
        let merged =
            apply_library_metadata_refresh(&runtime, &snapshot, &refreshed.metadata).unwrap();
        assert_eq!(merged.rating, 5);
        drop(runtime);
        let reopened = Library::open(root.join("library.sqlite")).unwrap();
        assert_eq!(reopened.asset(id).unwrap().unwrap(), merged);
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lazy_metadata_refresh_rejects_relink_and_fingerprint_races() {
        let root = std::env::temp_dir().join(format!(
            "starroom-lazy-metadata-races-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("original.png");
        let relocated = root.join("relocated.png");
        let encoded = starroom_imageio::encode_png_rgb8(&[80, 60, 40], 1, 1, None).unwrap();
        std::fs::write(&source, &encoded).unwrap();
        std::fs::write(&relocated, &encoded).unwrap();
        let mut library = Library::open(root.join("library.sqlite")).unwrap();
        let id = library
            .import_paths(std::slice::from_ref(&source), &AtomicBool::new(false))
            .unwrap()
            .imported[0];
        let runtime = NativeLibraryRuntime {
            library: Arc::new(Mutex::new(Some(library))),
            ..Default::default()
        };
        let snapshot = library_metadata_refresh_snapshot(&runtime, id).unwrap();
        let metadata = Library::extract_metadata(&source).unwrap();
        runtime
            .library
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .relink(id, &relocated)
            .unwrap();
        let before = library_metadata_refresh_snapshot(&runtime, id).unwrap();
        assert!(
            apply_library_metadata_refresh(&runtime, &snapshot, &metadata)
                .unwrap_err()
                .starts_with("MetadataRefreshSourceChanged:")
        );
        assert_eq!(
            library_metadata_refresh_snapshot(&runtime, id).unwrap(),
            before
        );

        let mut wrong_fingerprint = before.clone();
        wrong_fingerprint.content_fingerprint = "stale-content".into();
        assert!(
            apply_library_metadata_refresh(&runtime, &wrong_fingerprint, &metadata)
                .unwrap_err()
                .starts_with("MetadataRefreshSourceChanged:")
        );
        // Replacing the on-disk source before decoding is also explicit, not a hidden relink.
        std::fs::write(&relocated, b"changed source").unwrap();
        assert!(
            library_refresh_metadata_inner(&runtime, id)
                .unwrap_err()
                .starts_with("MetadataRefreshSourceChanged:")
        );
        assert_eq!(
            library_metadata_refresh_snapshot(&runtime, id).unwrap(),
            before
        );
        drop(runtime);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_stage_cache_identity_tracks_real_upstream_and_finishing_parameters() {
        let base = RenderSettings::default();
        let baseline = preview_stage_identity("same-source", &base).unwrap();
        let mut settings = base.clone();
        settings.tone.exposure_ev = 0.5;
        let exposure = preview_stage_identity("same-source", &settings).unwrap();
        assert_eq!(baseline.source_texture, exposure.source_texture);
        assert_eq!(baseline.geometry, exposure.geometry);
        assert_eq!(baseline.input_white_balance, exposure.input_white_balance);
        assert_ne!(baseline.global_creative, exposure.global_creative);
        settings = base.clone();
        settings.geometry.rotation_degrees = 90.0;
        let geometry = preview_stage_identity("same-source", &settings).unwrap();
        assert_ne!(baseline.geometry, geometry.geometry);
        assert_ne!(baseline.input_white_balance, geometry.input_white_balance);
        assert_ne!(baseline.global_creative, geometry.global_creative);
        settings = base.clone();
        settings.vignette.amount = 0.5;
        let finishing = preview_stage_identity("same-source", &settings).unwrap();
        assert_eq!(baseline.global_creative, finishing.global_creative);
        assert_eq!(baseline.local_composite, finishing.local_composite);
        assert_ne!(baseline.display, finishing.display);
        settings = base.clone();
        settings.relative_color.temperature = 0.4;
        let relative = preview_stage_identity("same-source", &settings).unwrap();
        assert_eq!(baseline.input_white_balance, relative.input_white_balance);
        assert_eq!(baseline.geometry, relative.geometry);
        assert_ne!(baseline.global_creative, relative.global_creative);
        settings = base.clone();
        settings.white_balance = WhiteBalanceSettings {
            mode: WhiteBalanceMode::NeutralPicker,
            sample: Some(WhiteBalanceSample {
                x: 0.2,
                y: 0.2,
                width: 0.1,
                height: 0.1,
            }),
        };
        let picker = preview_stage_identity("same-source", &settings).unwrap();
        assert_eq!(baseline.geometry, picker.geometry);
        assert_ne!(baseline.input_white_balance, picker.input_white_balance);
        settings.white_balance.mode = WhiteBalanceMode::Auto;
        let auto = preview_stage_identity("same-source", &settings).unwrap();
        settings.white_balance.sample = None;
        assert_eq!(
            auto,
            preview_stage_identity("same-source", &settings).unwrap(),
            "Auto ignores picker rectangles"
        );
    }

    #[test]
    fn gpu_retirement_preserves_source_and_capability_error_classification() {
        use starroom_pipeline::PipelineError;
        use starroom_render::gpu::GpuError;
        for error in [
            GpuError::DeviceLost,
            GpuError::OutOfMemory,
            GpuError::ReadbackTimeout,
            GpuError::Readback("closed callback".into()),
            GpuError::Validation("driver operation".into()),
        ] {
            assert!(gpu_failure_requires_retirement(&PipelineError::Gpu(error)));
        }
        for error in [
            PipelineError::Geometry,
            PipelineError::Gpu(GpuError::InvalidPixels),
            PipelineError::Gpu(GpuError::Unsupported("oversized full-frame request".into())),
        ] {
            assert!(!gpu_failure_requires_retirement(&error));
        }
    }

    #[test]
    fn gpu_cache_poison_and_render_panic_become_recoverable_errors() {
        let poisoned: Arc<Mutex<Option<Result<GpuRenderer, String>>>> = Arc::new(Mutex::new(None));
        let worker_cache = Arc::clone(&poisoned);
        let _ = std::thread::spawn(move || {
            let _guard = worker_cache.lock().unwrap();
            panic!("simulated driver panic");
        })
        .join();
        assert!(poisoned.is_poisoned());
        let error = recoverable_gpu_attempt(&poisoned, |cache| {
            cache
                .as_ref()
                .unwrap()
                .as_ref()
                .map(|_| ())
                .map_err(Clone::clone)
        })
        .unwrap_err();
        assert!(error.contains("recovered"));
        assert!(!poisoned.is_poisoned());

        let error =
            recoverable_gpu_attempt::<()>(&poisoned, |_| panic!("render panic")).unwrap_err();
        assert!(error.contains("Native CPU render graph"));
        assert!(!poisoned.is_poisoned());
        assert!(recoverable_gpu_attempt(&poisoned, |_| Ok::<_, String>(())).is_ok());
    }

    #[test]
    fn poisoned_gpu_cache_still_renders_an_edited_native_preview() {
        let root = std::env::temp_dir().join(format!(
            "starroom-poisoned-gpu-preview-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("gradient.png");
        let mut gradient = Vec::with_capacity(128 * 128 * 3);
        for _ in 0..128 {
            for x in 0..128 {
                let value = (x * 255 / 127) as u8;
                gradient.extend_from_slice(&[value; 3]);
            }
        }
        let encoded = starroom_imageio::encode_png_rgb8(&gradient, 128, 128, None).unwrap();
        std::fs::write(&source, encoded).unwrap();
        let source_hash = source_content_hash(&source).unwrap();
        let scheduler = Arc::new(NativePreviewScheduler::default());
        let worker = Arc::clone(&scheduler);
        let _ = std::thread::spawn(move || {
            let _guard = worker.gpu.lock().unwrap();
            panic!("simulate a failed GPU device");
        })
        .join();

        let render = |request_id: &str, edit: NativeEditSettings, prefer_gpu: bool| {
            let response = native_preview_inner(
                &scheduler,
                &NativePortraitRuntime::default(),
                &NativeAiMaskRuntime::default(),
                &NativeAiDenoiseRuntime::default(),
                NativePreviewRequest {
                    request_id: request_id.into(),
                    source_path: source.clone(),
                    max_edge: 128,
                    prefer_gpu,
                    interaction_phase: PreviewInteractionPhase::Final,
                    resolution_mode: PreviewResolutionMode::Fit,
                    viewport: None,
                    settings: edit,
                },
            )
            .expect("CPU graph must render after GPU cache poison");
            let frame = match tauri::ipc::IpcResponse::body(response).unwrap() {
                tauri::ipc::InvokeResponseBody::Raw(bytes) => bytes,
                tauri::ipc::InvokeResponseBody::Json(_) => panic!("preview must use binary IPC"),
            };
            assert_eq!(&frame[0..4], b"SRP3");
            assert_eq!(
                u16::from_le_bytes(frame[6..8].try_into().unwrap()) & 0x0010,
                0x0010,
                "the CPU fallback must be explicit in the result contract"
            );
            let profile_len = u16::from_le_bytes(frame[32..34].try_into().unwrap()) as usize;
            let payload_len = u32::from_le_bytes(frame[36..40].try_into().unwrap()) as usize;
            let payload_start = 40 + profile_len;
            assert_eq!(frame.len(), payload_start + payload_len);
            // Compare decoded payload pixels, never different backend flags/profile headers.
            let payload_path = root.join(format!("{request_id}.jpg"));
            std::fs::write(&payload_path, &frame[payload_start..]).unwrap();
            let image = starroom_imageio::decode_rendered(&payload_path).unwrap();
            assert_eq!((image.width, image.height), (128, 128));
            image
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|pixel| pixel[..3].iter().map(|value| (value * 255.0).round() as u8))
                .collect::<Vec<_>>()
        };
        let original = render("original", neutral_settings(), true);
        type ControlEdit = fn(&mut NativeEditSettings);
        let controls: [(&str, ControlEdit); 6] = [
            ("exposure", |edit| edit.exposure = 1.0),
            ("contrast", |edit| edit.contrast = 75.0),
            ("highlights", |edit| edit.highlights = -75.0),
            ("shadows", |edit| edit.shadows = 75.0),
            ("whites", |edit| edit.whites = -75.0),
            ("blacks", |edit| edit.blacks = 75.0),
        ];
        for (name, change) in controls {
            let mut edit = neutral_settings();
            change(&mut edit);
            let adjusted = render(name, edit.clone(), true);
            let changed_samples = original
                .iter()
                .zip(&adjusted)
                .filter(|(before, after)| before.abs_diff(**after) > 1)
                .count();
            assert!(
                changed_samples > 128,
                "{name} must visibly change Native pixels"
            );
            assert_eq!(
                adjusted,
                render(&format!("{name}-cpu"), edit, false),
                "{name}: recovered GPU request must use the identical CPU graph"
            );
        }

        let masks = [
            (
                "radial",
                MaskDefinition::Radial {
                    x: 0.5,
                    y: 0.5,
                    width: 0.25,
                    height: 0.25,
                    rotation: 0.0,
                    feather: 0.2,
                    invert: false,
                },
            ),
            (
                "linear",
                MaskDefinition::Linear {
                    start_x: 0.2,
                    start_y: 0.5,
                    end_x: 0.8,
                    end_y: 0.5,
                    feather: 0.2,
                    invert: false,
                },
            ),
            (
                "brush",
                MaskDefinition::Brush {
                    points: vec![starroom_project::BrushPoint {
                        x: 0.5,
                        y: 0.5,
                        pressure: 1.0,
                    }],
                    radius: 0.2,
                    feather: 0.2,
                    flow: 1.0,
                    erase: false,
                },
            ),
            (
                "luminance",
                MaskDefinition::Luminance {
                    minimum: 0.05,
                    maximum: 0.4,
                    feather: 0.05,
                    invert: false,
                },
            ),
            (
                "color-range",
                MaskDefinition::ColorRange {
                    reference: [0.25; 3],
                    tolerance: 0.15,
                    feather: 0.1,
                    invert: false,
                },
            ),
        ];
        for (name, mask) in masks {
            let mut edit = neutral_settings();
            edit.layers.push(NativeAdjustmentLayer {
                id: name.into(),
                name: name.into(),
                enabled: true,
                opacity: 1.0,
                blend_mode: starroom_pipeline::LayerBlendMode::Normal,
                mask: mask.into(),
                adjustments: starroom_pipeline::LayerAdjustments {
                    tone: ToneParameters {
                        exposure_ev: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            });
            let adjusted = render(name, edit.clone(), true);
            let changed_samples = original
                .iter()
                .zip(&adjusted)
                .filter(|(before, after)| before.abs_diff(**after) > 1)
                .count();
            assert!(
                changed_samples > 128,
                "{name} local edit must affect actual Native pixels"
            );
            assert_eq!(
                adjusted,
                render(&format!("{name}-cpu"), edit.clone(), false)
            );
            edit.layers[0].enabled = false;
            assert_eq!(
                original,
                render(&format!("{name}-disabled"), edit.clone(), true)
            );
            edit.layers[0].enabled = true;
            edit.layers[0].opacity = 0.0;
            assert_eq!(original, render(&format!("{name}-transparent"), edit, true));
        }
        assert_eq!(source_content_hash(&source).unwrap(), source_hash);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_model_selection_never_hides_an_existing_personal_model() {
        let root =
            std::env::temp_dir().join(format!("starroom-model-selection-{}", std::process::id()));
        let personal = root.join("personal").join("model.onnx");
        let bundled = root.join("bundled").join("model.onnx");
        std::fs::create_dir_all(personal.parent().unwrap()).unwrap();
        std::fs::create_dir_all(bundled.parent().unwrap()).unwrap();
        std::fs::write(&bundled, b"approved bundle").unwrap();
        assert_eq!(
            choose_local_model_file(personal.clone(), bundled.clone(), false),
            bundled
        );
        std::fs::write(&personal, b"invalid personal weight").unwrap();
        assert_eq!(
            choose_local_model_file(personal.clone(), bundled.clone(), false),
            personal
        );
        std::fs::remove_file(&personal).unwrap();
        assert_eq!(
            choose_local_model_file(personal.clone(), bundled, true),
            personal
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// Run explicitly with STARROOM_LOCAL_MODELS pointing at privately installed weights.
    /// Public CI cannot run this because BiSeNet, SegFormer and NAFNet are not redistributable.
    #[test]
    #[ignore = "requires privately installed, hash-verified AI weights"]
    fn private_local_ai_providers_load_and_run_offline() {
        let total_started = Instant::now();
        let stage_started = Instant::now();
        let mut portrait_registry = local_portrait_models();
        portrait_registry.execution_provider = starroom_portrait::ExecutionProvider::Cpu;
        let mut portrait = PortraitOnnxProvider::initialize(portrait_registry).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/golden/sources/astronaut-eileen-collins.png");
        let original_hash = source_content_hash(&source).unwrap();
        let (width, height, rgba, identity) = source_rgba_for_portrait(&source).unwrap();
        let faces = portrait
            .detect(width, height, &rgba, 1.0, &identity)
            .expect("YuNet must detect the real NASA portrait, not a synthetic face");
        assert_eq!(
            faces.len(),
            1,
            "the public NASA fixture contains one portrait"
        );
        let face = &faces[0];
        assert!(face.confidence.is_finite() && face.confidence > 0.8);
        assert!(face.bounds.left < 0.45 && face.bounds.right > 0.45);
        assert!(face.bounds.top < 0.24 && face.bounds.bottom > 0.24);
        assert!(
            face.landmarks
                .iter()
                .all(|point| point.x.is_finite() && point.y.is_finite())
        );
        let parsing = portrait
            .parse(width, height, &rgba, face, &identity)
            .unwrap();
        eprintln!(
            "private NASA512² YuNet + BiSeNet detection/parsing: {:.3}s",
            stage_started.elapsed().as_secs_f64()
        );
        assert_eq!(parsing.face_id, face.id);
        assert!(parsing.regions.values().all(|mask| {
            mask.width == width
                && mask.height == height
                && mask.values.len() == width as usize * height as usize
                && mask
                    .values
                    .iter()
                    .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        }));
        for region in [
            PortraitRegion::Skin,
            PortraitRegion::Eyes,
            PortraitRegion::Hair,
        ] {
            let occupied = parsing.regions[&region]
                .values
                .iter()
                .filter(|value| **value > 0.1)
                .count();
            assert!(
                occupied > 20,
                "real {region:?} parsing cannot be an empty placeholder"
            );
            assert!(
                occupied < width as usize * height as usize / 2,
                "real {region:?} parsing cannot cover the whole photo"
            );
        }

        // Resolve the real parser artifacts into the same production Skin stage used by export.
        let stage_started = Instant::now();
        let cache_key = format!(
            "{}:{}",
            parsing.cache_key.face_id, parsing.cache_key.crop_transform_hash
        );
        let mut skin_settings = RenderSettings::default();
        for (region, project_region) in [
            (PortraitRegion::Skin, PortraitMaskRegion::Skin),
            (PortraitRegion::Eyes, PortraitMaskRegion::Eyes),
            (PortraitRegion::LeftEye, PortraitMaskRegion::LeftEye),
            (PortraitRegion::RightEye, PortraitMaskRegion::RightEye),
            (PortraitRegion::Brows, PortraitMaskRegion::Brows),
            (PortraitRegion::LeftBrow, PortraitMaskRegion::LeftBrow),
            (PortraitRegion::RightBrow, PortraitMaskRegion::RightBrow),
            (PortraitRegion::Lips, PortraitMaskRegion::Lips),
            (PortraitRegion::Mouth, PortraitMaskRegion::Mouth),
            (PortraitRegion::Hair, PortraitMaskRegion::Hair),
        ] {
            let raster = &parsing.regions[&region];
            skin_settings.portrait_masks.push(PortraitMaskRaster {
                cache_key: cache_key.clone(),
                face_id: face.id.clone(),
                region: project_region,
                width,
                height,
                values: raster.values.clone(),
            });
        }
        skin_settings
            .skin_retouch
            .faces
            .push(starroom_pipeline::SkinRetouchFaceReference {
                face_id: face.id.clone(),
                cache_key,
                source_crop: None,
            });
        skin_settings.skin_retouch.parameters = starroom_portrait::SkinRetouchParameters {
            smooth: 0.5,
            tone_evenness: 0.3,
            exposure_ev: 0.3,
            ..Default::default()
        };
        let decoded = decode_source(&source).unwrap();
        let original = render_source_export_to_srgb8(&decoded, &RenderSettings::default()).unwrap();
        let retouched = render_source_export_to_srgb8(&decoded, &skin_settings).unwrap();
        assert_eq!((retouched.width, retouched.height), (width, height));
        assert!(
            original
                .data
                .iter()
                .zip(&retouched.data)
                .filter(|(before, after)| before.abs_diff(**after) > 1)
                .count()
                > 60,
            "real parsed skin must change actual production graph pixels"
        );
        let preview = render_source_preview_to_srgb8(&decoded, &skin_settings).unwrap();
        assert_eq!(
            preview.data, retouched.data,
            "real Face/Skin Preview and Export share one graph"
        );
        assert_eq!(source_content_hash(&source).unwrap(), original_hash);
        eprintln!(
            "private NASA512² Skin preview/export/identity: {:.3}s",
            stage_started.elapsed().as_secs_f64()
        );

        let stage_started = Instant::now();
        let mut mask_registry = local_ai_mask_models();
        mask_registry.execution_provider = starroom_portrait::ExecutionProvider::Cpu;
        let mut sky =
            AiMaskOnnxProvider::initialize_for(mask_registry, AiMaskSemantic::Sky).unwrap();
        let sky_result = sky
            .generate(
                width,
                height,
                &rgba,
                &original_hash,
                AiMaskSemantic::Sky,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            sky_result.mask.values.len(),
            sky_result.mask.width as usize * sky_result.mask.height as usize
        );
        assert!(sky_result.mask.width > 0 && sky_result.mask.height > 0);
        assert!(
            sky_result
                .mask
                .values
                .iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        );
        let mut sky_settings = RenderSettings::default();
        sky_settings.generated_masks.push(GeneratedMaskRaster {
            cache_identity: sky_result.cache_identity.clone(),
            semantic: GeneratedMaskSemantic::Sky,
            width: sky_result.mask.width,
            height: sky_result.mask.height,
            values: sky_result.mask.values.clone(),
        });
        sky_settings.layers.push(NativeAdjustmentLayer {
            id: "actual-sky".into(),
            name: "Actual sky".into(),
            enabled: true,
            opacity: 1.0,
            blend_mode: starroom_pipeline::LayerBlendMode::Normal,
            mask: MaskDefinition::Generated {
                provider_id: sky_result.provider_id.clone(),
                model_id: sky_result.model_id.clone(),
                model_version: sky_result.model_version.clone(),
                model_hash: sky_result.model_hash.clone(),
                semantic_class: GeneratedMaskSemantic::Sky,
                threshold: 0.5,
                feather: 0.1,
                invert: false,
                cache_identity: sky_result.cache_identity.clone(),
                metadata: BTreeMap::from([("executionProvider".into(), "cpu".into())]),
            }
            .into(),
            adjustments: starroom_pipeline::LayerAdjustments {
                tone: ToneParameters {
                    exposure_ev: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            },
        });
        let sky_preview = render_source_preview_to_srgb8(&decoded, &sky_settings).unwrap();
        let sky_export = render_source_export_to_srgb8(&decoded, &sky_settings).unwrap();
        assert_eq!(sky_preview.data, sky_export.data);
        eprintln!(
            "private NASA512² SegFormer + native mask parity: {:.3}s",
            stage_started.elapsed().as_secs_f64()
        );

        // Whole Person is ADE20K class 12. Reuse the scene session, never substitute a face mask.
        assert!(sky.supports(AiMaskSemantic::Person));
        let person_result = sky
            .generate(
                width,
                height,
                &rgba,
                &original_hash,
                AiMaskSemantic::Person,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_ne!(person_result.cache_identity, sky_result.cache_identity);
        assert!(
            person_result
                .mask
                .values
                .iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        );
        let probability = |x: f32, y: f32| {
            let mask = &person_result.mask;
            mask.values[(y * mask.height as f32) as usize * mask.width as usize
                + (x * mask.width as f32) as usize]
        };
        eprintln!(
            "Person body={}, background={}",
            probability(0.45, 0.75),
            probability(0.95, 0.05)
        );
        assert!(
            probability(0.45, 0.75) > 0.5,
            "whole-person mask must include the suit below the face"
        );
        assert!(
            probability(0.95, 0.05) < 0.5,
            "whole-person mask must exclude the background"
        );
        let mut person_settings = RenderSettings::default();
        person_settings.generated_masks.push(GeneratedMaskRaster {
            cache_identity: person_result.cache_identity.clone(),
            semantic: GeneratedMaskSemantic::Person,
            width: person_result.mask.width,
            height: person_result.mask.height,
            values: person_result.mask.values.clone(),
        });
        let mut person_layer = sky_settings.layers[0].clone();
        person_layer.id = "actual-person".into();
        person_layer.name = "Actual whole person".into();
        person_layer.mask = MaskDefinition::Generated {
            provider_id: person_result.provider_id.clone(),
            model_id: person_result.model_id.clone(),
            model_version: person_result.model_version.clone(),
            model_hash: person_result.model_hash.clone(),
            semantic_class: GeneratedMaskSemantic::Person,
            threshold: 0.5,
            feather: 0.1,
            invert: false,
            cache_identity: person_result.cache_identity.clone(),
            metadata: BTreeMap::new(),
        }
        .into();
        person_settings.layers.push(person_layer);
        let person_preview = render_source_preview_to_srgb8(&decoded, &person_settings).unwrap();
        let person_export = render_source_export_to_srgb8(&decoded, &person_settings).unwrap();
        assert_eq!(person_preview.data, person_export.data);
        assert_ne!(
            person_export.data, original.data,
            "Person must affect production pixels"
        );
        // Persist only the editable model reference, then regenerate from the actual file on restart.
        let saved_layers = serde_json::to_string(&person_settings.layers).unwrap();
        person_settings.layers = serde_json::from_str(&saved_layers).unwrap();
        person_settings.generated_masks.clear();
        let person_runtime = NativeAiMaskRuntime::default();
        attach_generated_masks(&mut person_settings, &source, &person_runtime).unwrap();
        assert_eq!(person_runtime.cache.lock().unwrap().len(), 1);
        assert_eq!(
            render_source_export_to_srgb8(&decoded, &person_settings)
                .unwrap()
                .data,
            person_export.data
        );
        assert_eq!(source_content_hash(&source).unwrap(), original_hash);

        let stage_started = Instant::now();
        let mut denoise =
            NafNetOnnxProvider::initialize(local_nafnet_model(), DenoiseExecutionProvider::Cpu)
                .unwrap();
        let working = prepare_source_for_ai_denoise(&decoded, &RenderSettings::default()).unwrap();
        let native_source_identity = preview_source_identity(&source).unwrap();
        let residual_input_identity = ai_denoise_input_identity(
            &native_source_identity,
            working.width,
            working.height,
            &RenderSettings::default(),
        )
        .unwrap();
        let residual = infer_tiled(
            &mut denoise,
            &working,
            &residual_input_identity,
            &AtomicBool::new(false),
            DenoiseExecutionProvider::Cpu,
        )
        .unwrap();
        assert_eq!(residual.values.len(), working.data.len());
        assert!(residual.values.iter().all(|value| value.is_finite()));
        assert!(
            residual.values.iter().any(|value| value.abs() > 1.0e-6),
            "real photo inference cannot be an identity placeholder"
        );
        let denoise_settings = RenderSettings {
            ai_denoise: AiDenoiseParameters {
                enabled: true,
                amount: 0.5,
                ..Default::default()
            },
            ai_denoise_residual: Some(residual.clone()),
            ..Default::default()
        };
        let denoise_preview = render_source_preview_to_srgb8(&decoded, &denoise_settings).unwrap();
        let denoise_export = render_source_export_to_srgb8(&decoded, &denoise_settings).unwrap();
        assert_eq!(denoise_preview.data, denoise_export.data);
        assert_ne!(
            denoise_export.data, original.data,
            "real NAFNet changes production pixels"
        );
        assert_eq!(source_content_hash(&source).unwrap(), original_hash);
        // The real advisor handler shares these exact cache-attachment/render functions.
        // Verify restored active AI Mask + Skin + NAFNet edits are not omitted or rejected.
        let portrait_runtime = NativePortraitRuntime::default();
        let parsing_cache_key = skin_settings.skin_retouch.faces[0].cache_key.clone();
        portrait_runtime
            .0
            .lock()
            .unwrap()
            .parsed
            .insert(parsing_cache_key, parsing);
        let mask_runtime = NativeAiMaskRuntime::default();
        mask_runtime
            .cache
            .lock()
            .unwrap()
            .insert(sky_result.cache_identity.clone(), sky_result);
        let denoise_runtime = NativeAiDenoiseRuntime::default();
        denoise_runtime
            .cache
            .lock()
            .unwrap()
            .insert(inference_cache_key(&residual_input_identity), residual);
        let mut restored = skin_settings.clone();
        restored.layers = sky_settings.layers.clone();
        restored.portrait_masks.clear();
        restored.ai_denoise = denoise_settings.ai_denoise;
        let advisor_frame = render_advisor_shared_graph(
            &decoded,
            &source,
            &mut restored,
            DenoiseExecutionProvider::Cpu,
            AdvisorRuntimeRefs {
                portrait: &portrait_runtime,
                masks: &mask_runtime,
                denoise: &denoise_runtime,
            },
        )
        .unwrap();
        assert!(!restored.portrait_masks.is_empty() && !restored.generated_masks.is_empty());
        assert!(restored.ai_denoise_residual.is_some());
        assert_eq!(
            advisor_frame,
            render_source_export_to_srgb8(&decoded, &restored).unwrap()
        );
        assert_eq!(
            denoise_runtime.cache.lock().unwrap().len(),
            1,
            "creative AI layers reuse the real residual"
        );
        assert_eq!(source_content_hash(&source).unwrap(), original_hash);
        eprintln!(
            "private NASA512² NAFNet + native denoise parity: {:.3}s; total: {:.3}s",
            stage_started.elapsed().as_secs_f64(),
            total_started.elapsed().as_secs_f64()
        );

        // Simulate a real process restart: persist only the compact edit state, drop every
        // runtime/provider/cache, then regenerate the exact pinned native artifacts locally.
        let stage_started = Instant::now();
        let mut cold_settings = restored.clone();
        cold_settings.portrait_masks.clear();
        cold_settings.generated_masks.clear();
        cold_settings.ai_denoise_residual = None;
        cold_settings.skin_retouch.faces[0].source_crop = Some(PortraitSourceCrop {
            version: 1,
            center_x: face.crop.center_x,
            center_y: face.crop.center_y,
            side: face.crop.side,
            rotation_degrees: face.crop.rotation_degrees,
        });
        let serialized_skin = serde_json::to_string(&cold_settings.skin_retouch).unwrap();
        cold_settings.skin_retouch = serde_json::from_str(&serialized_skin).unwrap();
        drop((portrait_runtime, mask_runtime, denoise_runtime));
        let cold_portrait = NativePortraitRuntime::default();
        let cold_masks = NativeAiMaskRuntime::default();
        let cold_denoise = NativeAiDenoiseRuntime::default();
        let reopened_frame = render_advisor_shared_graph(
            &decoded,
            &source,
            &mut cold_settings,
            DenoiseExecutionProvider::Cpu,
            AdvisorRuntimeRefs {
                portrait: &cold_portrait,
                masks: &cold_masks,
                denoise: &cold_denoise,
            },
        )
        .unwrap();
        assert_eq!(
            reopened_frame, advisor_frame,
            "same-source close/reopen must reproduce exact Mask + Skin + NAFNet export pixels"
        );
        assert_eq!(cold_portrait.0.lock().unwrap().parsed.len(), 1);
        assert_eq!(cold_masks.cache.lock().unwrap().len(), 1);
        assert_eq!(cold_denoise.cache.lock().unwrap().len(), 1);
        assert_eq!(
            render_source_export_to_srgb8(&decoded, &cold_settings).unwrap(),
            advisor_frame
        );

        // The established legacy 1.0 crop also restores, but only after an exact transform SHA
        // match. This is not an automatic substitution of a newer/default 1.4 crop.
        cold_portrait.0.lock().unwrap().parsed.clear();
        let mut legacy = skin_settings.clone();
        legacy.portrait_masks.clear();
        assert!(legacy.skin_retouch.faces[0].source_crop.is_none());
        attach_portrait_masks(&mut legacy, &source, &cold_portrait).unwrap();
        assert_eq!(
            render_source_export_to_srgb8(&decoded, &legacy).unwrap(),
            retouched
        );

        // Any API crop scale can now be reproduced from the tiny versioned crop metadata;
        // exact reference proof remains mandatory even when detector geometry is unchanged.
        let mut wide_face = face.clone();
        wide_face.crop = FaceCropTransform::from_face(
            face.bounds,
            width,
            height,
            2.2,
            face.landmarks[0],
            face.landmarks[1],
        )
        .unwrap();
        let wide_parsing = portrait
            .parse(width, height, &rgba, &wide_face, &identity)
            .unwrap();
        let wide_key = format!("{}:{}", face.id, wide_parsing.cache_key.crop_transform_hash);
        let mut wide = skin_settings.clone();
        wide.portrait_masks.clear();
        wide.skin_retouch.faces[0].cache_key = wide_key.clone();
        wide.skin_retouch.faces[0].source_crop = Some(PortraitSourceCrop {
            version: 1,
            center_x: wide_face.crop.center_x,
            center_y: wide_face.crop.center_y,
            side: wide_face.crop.side,
            rotation_degrees: wide_face.crop.rotation_degrees,
        });
        let wide_runtime = NativePortraitRuntime::default();
        wide_runtime
            .0
            .lock()
            .unwrap()
            .parsed
            .insert(wide_key, wide_parsing);
        attach_portrait_masks(&mut wide, &source, &wide_runtime).unwrap();
        let wide_before = render_source_export_to_srgb8(&decoded, &wide).unwrap();
        drop(wide_runtime);
        wide.portrait_masks.clear();
        attach_portrait_masks(&mut wide, &source, &NativePortraitRuntime::default()).unwrap();
        assert_eq!(
            render_source_export_to_srgb8(&decoded, &wide).unwrap(),
            wide_before
        );

        let foreign_source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/golden/sources/wild-cherry-texture.jpg");
        let mut foreign_skin = cold_settings.clone();
        foreign_skin.portrait_masks.clear();
        assert!(
            attach_portrait_masks(&mut foreign_skin, &foreign_source, &cold_portrait)
                .unwrap_err()
                .starts_with("PortraitSourceMismatch:")
        );
        let mut foreign_mask = sky_settings.clone();
        foreign_mask.generated_masks.clear();
        assert!(
            attach_generated_masks(&mut foreign_mask, &foreign_source, &cold_masks)
                .unwrap_err()
                .starts_with("MaskSourceMismatch:")
        );
        assert_eq!(source_content_hash(&source).unwrap(), original_hash);
        eprintln!(
            "private NASA512² cold-restart exact native AI restoration + legacy/custom crop/source guards: {:.3}s; total {:.3}s",
            stage_started.elapsed().as_secs_f64(),
            total_started.elapsed().as_secs_f64()
        );
    }

    #[test]
    fn preview_worker_pool_allows_before_after_without_global_serialization() {
        let pool = PreviewWorkerPool::new(2);
        let token = AtomicBool::new(false);
        let first = pool.acquire(&token).unwrap();
        let second = pool.acquire(&token).unwrap();
        assert_eq!(*pool.active.lock().unwrap(), 2);
        drop((first, second));
        assert_eq!(*pool.active.lock().unwrap(), 0);
    }

    #[test]
    fn queued_preview_worker_observes_superseding_cancellation() {
        let pool = Arc::new(PreviewWorkerPool::new(1));
        let active_token = AtomicBool::new(false);
        let _active = pool.acquire(&active_token).unwrap();
        let queued_pool = pool.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let queued_cancelled = cancelled.clone();
        let queued = std::thread::spawn(move || queued_pool.acquire(&queued_cancelled).is_err());
        std::thread::sleep(Duration::from_millis(10));
        cancelled.store(true, Ordering::Release);
        assert!(queued.join().unwrap());
    }

    #[test]
    fn decoded_preview_cache_is_tiered_by_source_and_resolution() {
        let scheduler = NativePreviewScheduler::default();
        let image = Arc::new(DecodedSourceImage::Rendered(
            starroom_imageio::DecodedRenderedImage {
                width: 2,
                height: 2,
                format: starroom_imageio::RenderedFormat::Png,
                rgba: vec![0.25; 16],
                embedded_icc: None,
                exif: None,
            },
        ));
        cache_decoded_source(&scheduler, "photo-a".into(), u32::MAX, image.clone()).unwrap();
        assert!(
            cached_decoded_source(&scheduler, "photo-a", 1024)
                .unwrap()
                .is_none()
        );
        assert!(
            cached_decoded_source(&scheduler, "photo-b", u32::MAX)
                .unwrap()
                .is_none()
        );
        let restored = cached_decoded_source(&scheduler, "photo-a", u32::MAX)
            .unwrap()
            .expect("full-resolution source should be cached");
        assert!(Arc::ptr_eq(&restored, &image));
    }

    #[test]
    fn model_root_prefers_explicit_then_bundled_then_development_location() {
        let base = std::env::temp_dir().join(format!("starroom-model-root-{}", std::process::id()));
        let executable = base.join("installed").join("Starroom.exe");
        let bundled = executable.parent().unwrap().join("models").join("local");
        std::fs::create_dir_all(&bundled).unwrap();
        assert_eq!(
            resolve_local_model_root(None, Some(&executable), &base.join("working")),
            bundled
        );
        assert_eq!(
            resolve_local_model_root(
                Some(std::ffi::OsString::from("D:\\reviewed-models")),
                Some(&executable),
                &base,
            ),
            PathBuf::from("D:\\reviewed-models")
        );
        std::fs::remove_dir_all(executable.parent().unwrap()).unwrap();
        assert_eq!(
            resolve_local_model_root(None, Some(&executable), &base.join("working")),
            base.join("working").join("models").join("local")
        );
        let _ = std::fs::remove_dir_all(base);
    }

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

    fn neutral_settings() -> NativeEditSettings {
        NativeEditSettings {
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            temperature: 0.0,
            tint: 0.0,
            ..settings()
        }
    }

    #[test]
    fn m30_packaged_release_self_test_exercises_production_workflow() {
        let root = std::env::temp_dir().join(format!(
            "starroom-m30-release-self-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let report = release_self_test(&root).unwrap();
        assert_eq!(report.schema_version, 2);
        assert_eq!(report.library, "ok");
        assert_eq!(report.history, "ok");
        assert_eq!(report.session, "ok");
        assert_eq!(report.native_export, "ok");
        assert!(report.deterministic_export);
        assert!(report.source_immutable);
        for state in [
            report.face_skin,
            report.subject_background,
            report.sky,
            report.ai_denoise,
        ] {
            assert!(matches!(state, "available" | "typed-unavailable"));
        }
        std::fs::write(root.join("keep"), b"user state").unwrap();
        assert!(release_self_test(&root).is_err());
    }

    #[test]
    fn native_preview_deserializes_real_manual_lens_and_legacy_identity_contract() {
        let optics: serde_json::Value = serde_json::from_str(include_str!(
            "../../fixtures/contracts/native-manual-lens.json"
        ))
        .unwrap();
        let mut edit = serde_json::to_value(neutral_settings()).unwrap();
        edit["optics"] = optics.clone();
        let parsed: NativeEditSettings = serde_json::from_value(edit).unwrap();
        let identity = parsed.optics.manual_identity.as_ref().unwrap();
        assert!(identity.metadata_complete());
        let profile = starroom_optics::LensfunProvider
            .resolve_profile(identity, starroom_optics::LensMatchMode::Manual)
            .unwrap();
        assert_eq!(
            profile.status,
            starroom_optics::LensProfileStatus::ManualMatched
        );
        let expected: OpticsSettings = serde_json::from_value(optics).unwrap();
        assert_eq!(parsed.optics, expected);
        let wire_identity = serde_json::to_value(identity).unwrap();
        assert!(wire_identity.get("cameraMake").is_some());
        assert!(wire_identity.get("camera_make").is_none());
        let mut legacy = serde_json::to_value(parsed).unwrap();
        let lens = legacy["optics"]["manualIdentity"].as_object_mut().unwrap();
        for (camel, snake) in [
            ("cameraMake", "camera_make"),
            ("cameraModel", "camera_model"),
            ("lensMake", "lens_make"),
            ("lensModel", "lens_model"),
            ("focalLengthMm", "focal_length_mm"),
            ("focusDistanceM", "focus_distance_m"),
        ] {
            let value = lens.remove(camel).unwrap();
            lens.insert(snake.into(), value);
        }
        let restored: NativeEditSettings = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.optics, expected);
    }

    #[test]
    fn native_preview_deserializes_shared_local_layer_wire_contract() {
        let layers: serde_json::Value = serde_json::from_str(include_str!(
            "../../fixtures/contracts/native-local-layers.json"
        ))
        .expect("shared frontend/native layer contract");
        let workflow: serde_json::Value = serde_json::from_str(include_str!(
            "../../fixtures/contracts/native-local-workflow.json"
        ))
        .expect("shared frontend/native skin and healing contract");
        let mut edit = serde_json::to_value(neutral_settings()).unwrap();
        edit["layers"] = layers.clone();
        edit["skinRetouch"] = workflow["skinRetouch"].clone();
        edit["healingOperations"] = workflow["healingOperations"].clone();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/golden/sources/astronaut-eileen-collins.png");
        let wire = serde_json::json!({
            "requestId": "shared-local-layer-contract",
            "sourcePath": source,
            "maxEdge": 512,
            "preferGpu": true,
            "interactionPhase": "final",
            "resolutionMode": "fit",
            "settings": edit,
        });
        let request: NativePreviewRequest = serde_json::from_value(wire.clone())
            .expect("actual native_preview IPC request accepts frontend wire");
        assert_eq!(request.request_id, "shared-local-layer-contract");
        assert_eq!(request.source_path, source);
        assert_eq!(request.max_edge, 512);
        assert!(request.prefer_gpu);
        let serialized_settings = serde_json::to_string(&request.settings).unwrap();
        let restored: NativeEditSettings = serde_json::from_str(&serialized_settings).unwrap();
        let expected_skin: SkinRetouchSettings =
            serde_json::from_value(workflow["skinRetouch"].clone()).unwrap();
        let expected_healing: Vec<HealingOperation> =
            serde_json::from_value(workflow["healingOperations"].clone()).unwrap();
        assert_eq!(request.settings.skin_retouch, expected_skin);
        assert_eq!(request.settings.healing_operations, expected_healing);
        assert_eq!(restored.skin_retouch, expected_skin);
        assert_eq!(restored.healing_operations, expected_healing);
        assert_eq!(restored.layers, request.settings.layers);
        let validated = request
            .settings
            .validated()
            .expect("valid native layer controls");
        assert_eq!(validated.layers.len(), layers.as_array().unwrap().len());
        fn mask_types(
            mask: &starroom_project::MaskTree,
            types: &mut std::collections::BTreeSet<&'static str>,
        ) {
            use starroom_project::{MaskDefinition, MaskTree};
            match mask {
                MaskTree::Composite(composite) => {
                    for child in &composite.children {
                        mask_types(child, types);
                    }
                }
                MaskTree::Leaf(leaf) => {
                    types.insert(match leaf {
                        MaskDefinition::None => "none",
                        MaskDefinition::Radial { .. } => "radial",
                        MaskDefinition::Linear { .. } => "linear",
                        MaskDefinition::Brush { .. } => "brush",
                        MaskDefinition::Luminance { .. } => "luminance",
                        MaskDefinition::ColorRange { .. } => "colorRange",
                        MaskDefinition::PortraitSemantic { .. } => "portraitSemantic",
                        MaskDefinition::Generated { .. } => "generated",
                        MaskDefinition::Provider { .. } => "provider",
                    });
                }
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for (layer, original) in validated.layers.iter().zip(layers.as_array().unwrap()) {
            assert_eq!(layer.id, original["id"].as_str().unwrap());
            let expected: NativeAdjustmentLayer = serde_json::from_value(original.clone()).unwrap();
            assert_eq!(
                layer, &expected,
                "all local tone/color/curve/mixer/grading and mask intent"
            );
            assert!(layer.adjustments.tone.exposure_ev.is_finite());
            let round_trip: NativeAdjustmentLayer =
                serde_json::from_str(&serde_json::to_string(layer).unwrap()).unwrap();
            assert_eq!(&round_trip, layer);
            mask_types(&layer.mask, &mut seen);
        }
        assert_eq!(
            seen,
            std::collections::BTreeSet::from([
                "none",
                "radial",
                "linear",
                "brush",
                "luminance",
                "colorRange",
                "portraitSemantic",
                "generated"
            ])
        );
        assert_eq!(validated.skin_retouch, expected_skin);
        assert_eq!(validated.healing_operations, expected_healing);
        assert_eq!(validated.layers[0].adjustments.tone.exposure_ev, 0.35);
        assert_eq!(validated.layers[1].adjustments.tone.exposure_ev, -0.4);
        // Catch the exact field regression instead of silently dropping a non-neutral layer EV.
        let mut invalid = wire;
        let tone = invalid["settings"]["layers"][0]["adjustments"]["tone"]
            .as_object_mut()
            .unwrap();
        let exposure = tone.remove("exposure_ev").unwrap();
        tone.insert("exposureEv".into(), exposure);
        let error = serde_json::from_value::<NativePreviewRequest>(invalid).unwrap_err();
        assert!(error.to_string().contains("exposure_ev"));
    }

    #[test]
    fn native_json_boundary_rejects_unsafe_grain_seed_without_rounding() {
        let mut edit = neutral_settings();
        edit.grain.seed = 9_007_199_254_740_991;
        assert_eq!(
            edit.clone().validated().unwrap().grain.seed,
            edit.grain.seed
        );
        edit.grain.seed += 1;
        assert!(
            edit.validated()
                .unwrap_err()
                .starts_with("UnsafeRenderSeed:")
        );
    }

    #[test]
    fn native_legacy_missing_families_have_explicit_effective_defaults() {
        let wire = serde_json::json!({
            "exposure": 0, "contrast": 0, "highlights": 0, "shadows": 0, "whites": 0, "blacks": 0,
            "temperature": 0, "tint": 0, "vibrance": 0, "saturation": 0, "sharpness": 0, "noiseReduction": 0,
            "curve": [],
        });
        let effective: NativeEditSettings = serde_json::from_value(wire).unwrap();
        let canonical = serde_json::to_string_pretty(&effective).unwrap();
        let shared: NativeEditSettings = serde_json::from_str(include_str!(
            "../../fixtures/contracts/native-default-settings.json"
        ))
        .expect("production Rust serde defaults shared with frontend");
        assert_eq!(
            serde_json::to_string(&effective).unwrap(),
            serde_json::to_string(&shared).unwrap()
        );
        let restored: NativeEditSettings = serde_json::from_str(&canonical).unwrap();
        let effective = effective.validated().unwrap();
        let restored = restored.validated().unwrap();
        assert_eq!(effective, restored);
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
        let frame = preview_frame(
            PreviewFrameHeader {
                width: 640,
                height: 480,
                source_width: 6000,
                source_height: 4000,
                tile_x: 512,
                tile_y: 256,
                flags: 2 | 0x20 | 0x40,
            },
            profile,
            vec![0xff, 0xd8, 0xff],
        )
        .expect("frame");
        assert_eq!(&frame[0..4], b"SRP3");
        assert_eq!(u32::from_le_bytes(frame[8..12].try_into().unwrap()), 640);
        assert_eq!(u32::from_le_bytes(frame[12..16].try_into().unwrap()), 480);
        assert_eq!(u32::from_le_bytes(frame[16..20].try_into().unwrap()), 6000);
        assert_eq!(u32::from_le_bytes(frame[20..24].try_into().unwrap()), 4000);
        assert_eq!(u32::from_le_bytes(frame[24..28].try_into().unwrap()), 512);
        assert_eq!(u32::from_le_bytes(frame[28..32].try_into().unwrap()), 256);
        assert_eq!(
            u16::from_le_bytes(frame[32..34].try_into().unwrap()) as usize,
            profile.len()
        );
        assert_eq!(u32::from_le_bytes(frame[36..40].try_into().unwrap()), 3);
        assert_eq!(&frame[40..40 + profile.len()], profile.as_bytes());
        assert_eq!(&frame[40 + profile.len()..], &[0xff, 0xd8, 0xff]);
    }

    #[test]
    fn preview_viewport_validates_and_expands_at_source_edges() {
        let viewport = validated_viewport(
            Some(PreviewViewportRequest {
                source_width: 1000,
                source_height: 800,
                x: 100,
                y: 50,
                width: 512,
                height: 400,
            }),
            1000,
            800,
        )
        .expect("viewport");
        assert_eq!(
            expand_viewport(viewport, 1000, 800, 32),
            PreviewViewportRequest {
                source_width: 1000,
                source_height: 800,
                x: 68,
                y: 18,
                width: 576,
                height: 464
            }
        );
        assert!(
            validated_viewport(
                Some(PreviewViewportRequest {
                    source_width: 1000,
                    source_height: 800,
                    x: 900,
                    y: 0,
                    width: 200,
                    height: 100
                }),
                1000,
                800,
            )
            .is_err()
        );
    }

    #[test]
    fn rgb_tile_crop_is_exact_and_row_ordered() {
        let data: Vec<u8> = (0..36).collect();
        let tile = crop_rgb8(&data, 4, 3, 1, 1, 2, 2).expect("tile");
        assert_eq!(tile, [15, 16, 17, 18, 19, 20, 27, 28, 29, 30, 31, 32]);
    }

    #[test]
    fn viewport_optimization_accepts_local_composites_and_rejects_global_stages() {
        let base = settings().validated().expect("settings");
        assert!(viewport_graph_is_tile_safe(&base));
        let mut local = base.clone();
        local.layers.push(NativeAdjustmentLayer {
            id: "tile-local".into(),
            name: "Tile local".into(),
            enabled: true,
            opacity: 1.0,
            blend_mode: starroom_pipeline::LayerBlendMode::Normal,
            mask: MaskDefinition::Radial {
                x: 0.5,
                y: 0.5,
                width: 0.25,
                height: 0.25,
                rotation: 0.0,
                feather: 0.2,
                invert: false,
            }
            .into(),
            adjustments: starroom_pipeline::LayerAdjustments::default(),
        });
        assert!(viewport_graph_is_tile_safe(&local));
        let mut manual_heal = base.clone();
        manual_heal.healing_operations.push(HealingOperation {
            id: "tile-heal".into(),
            enabled: true,
            mode: HealMode::Heal,
            target: starroom_heal::HealPoint { x: 0.6, y: 0.6 },
            source: Some(starroom_heal::HealPoint { x: 0.4, y: 0.4 }),
            radius: 12.0,
            feather: 0.5,
            opacity: 1.0,
            rotation_degrees: 0.0,
            scale: 1.0,
            tone_adaptation: true,
            texture_adaptation: true,
            source_mode: SourceMode::Manual,
            metadata: BTreeMap::new(),
        });
        assert!(viewport_graph_is_tile_safe(&manual_heal));
        manual_heal.healing_operations[0].source_mode = SourceMode::Auto;
        assert!(!viewport_graph_is_tile_safe(&manual_heal));
        let mut geometry = base.clone();
        geometry.geometry.rotation_degrees = 1.0;
        assert!(!viewport_graph_is_tile_safe(&geometry));
        let mut picker = base.clone();
        picker.white_balance.mode = WhiteBalanceMode::NeutralPicker;
        picker.white_balance.sample = Some(WhiteBalanceSample {
            x: 0.4,
            y: 0.4,
            width: 0.1,
            height: 0.1,
        });
        assert!(!viewport_graph_is_tile_safe(&picker));
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
        assert!(!wants_high_resolution(
            PreviewResolutionMode::HighResolution,
            PreviewInteractionPhase::Interactive
        ));
        assert!(wants_high_resolution(
            PreviewResolutionMode::HighResolution,
            PreviewInteractionPhase::Final
        ));
    }

    #[test]
    #[ignore = "release-only 24 MP RC2 performance gate"]
    fn rc2_release_preview_cache_interaction_and_viewport_timings() {
        if std::env::var_os("STARROOM_RC2_PERFORMANCE_GATE").is_none() {
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "starroom-rc2-preview-performance-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("24mp.jpg");
        let (source_width, source_height) = (6000_u32, 4000_u32);
        let mut rgb = Vec::with_capacity(source_width as usize * source_height as usize * 3);
        for y in 0..source_height {
            for x in 0..source_width {
                rgb.extend_from_slice(&[
                    ((x * 255) / (source_width - 1)) as u8,
                    ((y * 255) / (source_height - 1)) as u8,
                    (((x + y) * 255) / (source_width + source_height - 2)) as u8,
                ]);
            }
        }
        let encoded =
            starroom_imageio::encode_jpeg_rgb8(&rgb, source_width, source_height, 95, None)
                .expect("24 MP fixture encode");
        std::fs::write(&source, encoded).unwrap();
        drop(rgb);

        let scheduler = NativePreviewScheduler::default();
        let portrait = NativePortraitRuntime::default();
        let masks = NativeAiMaskRuntime::default();
        let denoise = NativeAiDenoiseRuntime::default();
        let render =
            |request_id: &str, interaction_phase, resolution_mode, viewport, edit_settings| {
                let started = std::time::Instant::now();
                let (result, profile) = profiling::capture(|| {
                    native_preview_inner(
                        &scheduler,
                        &portrait,
                        &masks,
                        &denoise,
                        NativePreviewRequest {
                            request_id: request_id.into(),
                            source_path: source.clone(),
                            max_edge: 1800,
                            prefer_gpu: true,
                            interaction_phase,
                            resolution_mode,
                            viewport,
                            settings: edit_settings,
                        },
                    )
                });
                result.expect("native preview");
                (started.elapsed(), profile)
            };

        let (first, _) = render(
            "first-fit",
            PreviewInteractionPhase::Final,
            PreviewResolutionMode::Fit,
            None,
            settings(),
        );
        let (reopen, _) = render(
            "cached-reopen",
            PreviewInteractionPhase::Final,
            PreviewResolutionMode::Fit,
            None,
            settings(),
        );
        let raw_source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/raw/sources/nikon-d1.nef");
        let render_raw = |request_id: &str| {
            let started = std::time::Instant::now();
            native_preview_inner(
                &scheduler,
                &portrait,
                &masks,
                &denoise,
                NativePreviewRequest {
                    request_id: request_id.into(),
                    source_path: raw_source.clone(),
                    max_edge: 1800,
                    prefer_gpu: true,
                    interaction_phase: PreviewInteractionPhase::Final,
                    resolution_mode: PreviewResolutionMode::Fit,
                    viewport: None,
                    settings: settings(),
                },
            )
            .expect("RAW native preview");
            started.elapsed()
        };
        let raw_first = render_raw("raw-first-fit");
        let raw_reopen = render_raw("raw-cached-reopen");
        let _ = render(
            "interactive-warm",
            PreviewInteractionPhase::Interactive,
            PreviewResolutionMode::Fit,
            None,
            settings(),
        );
        let mut dragged = settings();
        dragged.exposure = 1.0;
        let (interactive, interactive_profile) = render(
            "interactive-exposure",
            PreviewInteractionPhase::Interactive,
            PreviewResolutionMode::Fit,
            None,
            dragged.clone(),
        );
        let (refine, refine_profile) = render(
            "final-exposure",
            PreviewInteractionPhase::Final,
            PreviewResolutionMode::Fit,
            None,
            dragged.clone(),
        );
        let (tile_100, tile_100_profile) = render(
            "tile-100",
            PreviewInteractionPhase::Final,
            PreviewResolutionMode::HighResolution,
            Some(PreviewViewportRequest {
                source_width,
                source_height,
                x: 2400,
                y: 1600,
                width: 1200,
                height: 800,
            }),
            dragged.clone(),
        );
        let (tile_200, tile_200_profile) = render(
            "tile-200",
            PreviewInteractionPhase::Final,
            PreviewResolutionMode::HighResolution,
            Some(PreviewViewportRequest {
                source_width,
                source_height,
                x: 2700,
                y: 1800,
                width: 600,
                height: 400,
            }),
            dragged,
        );
        assert_eq!(scheduler.decoded.lock().unwrap().len(), 3);
        assert!(!scheduler.viewport_frames.lock().unwrap().is_empty());
        assert!(
            interactive < first,
            "warmed interactive response {interactive:?} must beat cold open {first:?}"
        );
        eprintln!(
            "GPU_RESIDENT_PREVIEW_PERF jpeg_first_fit_ms={:.3} jpeg_cached_reopen_ms={:.3} raw_first_fit_ms={:.3} raw_cached_reopen_ms={:.3} interactive_ms={:.3} final_refine_ms={:.3} tile_100_ms={:.3} tile_200_ms={:.3}",
            first.as_secs_f64() * 1000.0,
            reopen.as_secs_f64() * 1000.0,
            raw_first.as_secs_f64() * 1000.0,
            raw_reopen.as_secs_f64() * 1000.0,
            interactive.as_secs_f64() * 1000.0,
            refine.as_secs_f64() * 1000.0,
            tile_100.as_secs_f64() * 1000.0,
            tile_200.as_secs_f64() * 1000.0,
        );
        eprintln!(
            "GPU_RESIDENT_STAGE_BREAKDOWN interactive={} final={} tile100={} tile200={}",
            serde_json::to_string(&interactive_profile).unwrap(),
            serde_json::to_string(&refine_profile).unwrap(),
            serde_json::to_string(&tile_100_profile).unwrap(),
            serde_json::to_string(&tile_200_profile).unwrap(),
        );
        // Exercise changed settings, not cached identical frames. These are production
        // slider requests on the same 24 MP source, with five distinct edits per control.
        type ControlMutation = fn(&mut NativeEditSettings, f32);
        let controls: &[(&str, ControlMutation)] = &[
            ("Exposure", |s, v| s.exposure = v / 30.0),
            ("Contrast", |s, v| s.contrast = v),
            ("Highlights", |s, v| s.highlights = -v),
            ("Shadows", |s, v| s.shadows = v),
            ("Whites", |s, v| s.whites = v),
            ("Blacks", |s, v| s.blacks = -v),
            ("Temperature", |s, v| s.temperature = v),
            ("Tint", |s, v| s.tint = v),
            ("Vibrance", |s, v| s.vibrance = v),
            ("Saturation", |s, v| s.saturation = v),
            ("MasterCurve", |s, v| {
                s.curves.master = vec![
                    CurvePoint { x: 0.0, y: 0.0 },
                    CurvePoint {
                        x: 0.5,
                        y: 0.5 + v / 1000.0,
                    },
                    CurvePoint { x: 1.0, y: 1.0 },
                ]
            }),
            ("RedCurve", |s, v| {
                s.curves.red = vec![
                    CurvePoint { x: 0.0, y: 0.0 },
                    CurvePoint {
                        x: 0.5,
                        y: 0.5 + v / 1000.0,
                    },
                    CurvePoint { x: 1.0, y: 1.0 },
                ]
            }),
            ("GreenCurve", |s, v| {
                s.curves.green = vec![
                    CurvePoint { x: 0.0, y: 0.0 },
                    CurvePoint {
                        x: 0.5,
                        y: 0.5 + v / 1000.0,
                    },
                    CurvePoint { x: 1.0, y: 1.0 },
                ]
            }),
            ("BlueCurve", |s, v| {
                s.curves.blue = vec![
                    CurvePoint { x: 0.0, y: 0.0 },
                    CurvePoint {
                        x: 0.5,
                        y: 0.5 + v / 1000.0,
                    },
                    CurvePoint { x: 1.0, y: 1.0 },
                ]
            }),
            ("MixerHue", |s, v| {
                s.color_mixer.bands[0].hue_degrees = v / 2.0
            }),
            ("MixerChroma", |s, v| {
                s.color_mixer.bands[0].chroma = v / 100.0
            }),
            ("MixerLightness", |s, v| {
                s.color_mixer.bands[0].lightness = v / 100.0
            }),
            ("GradingGlobal", |s, v| {
                s.grading.global.hue_degrees = 220.0;
                s.grading.global.chroma = v / 300.0;
            }),
            ("GradingShadows", |s, v| {
                s.grading.shadows.hue_degrees = 220.0;
                s.grading.shadows.chroma = v / 300.0;
            }),
            ("GradingMidtones", |s, v| {
                s.grading.midtones.hue_degrees = 35.0;
                s.grading.midtones.chroma = v / 300.0;
            }),
            ("GradingHighlights", |s, v| {
                s.grading.highlights.hue_degrees = 35.0;
                s.grading.highlights.chroma = v / 300.0;
            }),
            ("Sharpen", |s, v| {
                s.sharpness = v;
                s.sharpen_settings.amount = v / 50.0;
            }),
            ("LumaDenoise", |s, v| {
                s.noise_reduction = v;
                s.denoise_settings.luminance = v / 100.0;
            }),
            ("ChromaDenoise", |s, v| {
                s.denoise_settings.chroma = v / 100.0
            }),
            ("Texture", |s, v| s.local_detail.texture = v / 100.0),
            ("Clarity", |s, v| s.local_detail.clarity = v / 100.0),
            ("Dehaze", |s, v| s.local_detail.dehaze = v / 100.0),
        ];
        for (name, mutate) in controls {
            let mut samples = Vec::with_capacity(5);
            for index in 0..5 {
                let mut edit = neutral_settings();
                mutate(&mut edit, 32.0 + index as f32 * 3.0);
                let (duration, profile) = render(
                    &format!("control-{name}-{index}"),
                    PreviewInteractionPhase::Interactive,
                    PreviewResolutionMode::Fit,
                    None,
                    edit,
                );
                assert!(
                    profile.stages.contains_key(&ProfileStage::Encode),
                    "{name} changed request must render/encode, not reuse an identical cached frame"
                );
                samples.push(duration.as_secs_f64() * 1000.0);
            }
            samples.sort_by(f64::total_cmp);
            eprintln!(
                "NATIVE_CONTROL_PERF control={name} changed_samples=5 edge=1024 median_ms={:.3} p95_ms={:.3}",
                samples[2], samples[4]
            );
        }
        let _ = std::fs::remove_dir_all(root);
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
