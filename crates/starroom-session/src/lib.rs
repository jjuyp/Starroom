//! Atomic, pixel-free desktop session persistence and explicit crash recovery.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self},
    io::Write,
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const SESSION_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryBrowserState {
    pub collection_id: Option<i64>,
    pub search: String,
    pub page: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub version: u32,
    pub workspace: String,
    pub selected_asset_id: Option<i64>,
    pub selected_source_path: Option<PathBuf>,
    pub active_tool: String,
    pub library_panel_open: bool,
    pub filmstrip_open: bool,
    pub zoom_mode: String,
    pub zoom_scale: f32,
    pub library_context: String,
    #[serde(default)]
    pub library_browser: Option<LibraryBrowserState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    clean_shutdown: bool,
    state: SessionState,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOpen {
    pub state: Option<SessionState>,
    pub recovery_available: bool,
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("SessionInvalid: {0}")]
    Invalid(String),
    #[error("SessionPersistenceFailed: {0}")]
    Persistence(String),
}

pub fn open(path: &Path) -> Result<SessionOpen, SessionError> {
    if !path.is_file() {
        return Ok(SessionOpen {
            state: None,
            recovery_available: false,
        });
    }
    let envelope: Envelope = serde_json::from_slice(&fs::read(path).map_err(io)?)
        .map_err(|error| SessionError::Invalid(error.to_string()))?;
    validate(&envelope.state)?;
    Ok(SessionOpen {
        state: Some(envelope.state),
        recovery_available: !envelope.clean_shutdown,
    })
}

pub fn autosave(path: &Path, state: &SessionState) -> Result<(), SessionError> {
    persist(path, state, false)
}
pub fn mark_clean(path: &Path, state: &SessionState) -> Result<(), SessionError> {
    persist(path, state, true)
}
pub fn discard(path: &Path) -> Result<(), SessionError> {
    if path.is_file() {
        fs::remove_file(path).map_err(io)?;
    }
    Ok(())
}

fn validate(state: &SessionState) -> Result<(), SessionError> {
    if state.version != SESSION_VERSION
        || !state.zoom_scale.is_finite()
        || !(0.25..=6.0).contains(&state.zoom_scale)
        || state.library_browser.as_ref().is_some_and(|browser| {
            browser.collection_id.is_some_and(|id| id <= 0)
                || browser.search.len() > 16384
                || browser.page > u32::MAX / 200
        })
    {
        return Err(SessionError::Invalid(
            "unsupported version or zoom state".into(),
        ));
    }
    Ok(())
}

fn persist(path: &Path, state: &SessionState, clean_shutdown: bool) -> Result<(), SessionError> {
    validate(state)?;
    let parent = path
        .parent()
        .ok_or_else(|| SessionError::Persistence("path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(io)?;
    let bytes = serde_json::to_vec_pretty(&Envelope {
        clean_shutdown,
        state: state.clone(),
    })
    .map_err(|error| SessionError::Invalid(error.to_string()))?;
    (|| {
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(io)?;
        file.write_all(&bytes).map_err(io)?;
        file.as_file().sync_all().map_err(io)?;
        file.persist(path)
            .map(|_| ())
            .map_err(|error| io(error.error))
    })()
}
fn io(error: std::io::Error) -> SessionError {
    SessionError::Persistence(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> SessionState {
        SessionState {
            version: 1,
            workspace: "edit".into(),
            selected_asset_id: Some(7),
            selected_source_path: Some("C:/photos/a.nef".into()),
            active_tool: "light".into(),
            library_panel_open: true,
            filmstrip_open: true,
            zoom_mode: "fit".into(),
            zoom_scale: 1.0,
            library_context: "recent".into(),
            library_browser: None,
        }
    }
    #[test]
    fn crash_and_clean_states_are_explicit_and_atomic() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("session.json");
        autosave(&path, &state()).unwrap();
        assert!(open(&path).unwrap().recovery_available);
        mark_clean(&path, &state()).unwrap();
        let loaded = open(&path).unwrap();
        assert!(!loaded.recovery_available);
        assert_eq!(loaded.state, Some(state()));
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
    #[test]
    fn corrupt_and_invalid_sessions_are_typed() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("session.json");
        fs::write(&path, b"not-json").unwrap();
        assert!(matches!(open(&path), Err(SessionError::Invalid(_))));

        let mut future = state();
        future.version = SESSION_VERSION + 1;
        assert!(matches!(
            autosave(&root.path().join("future.json"), &future),
            Err(SessionError::Invalid(_))
        ));

        let mut invalid_zoom = state();
        invalid_zoom.zoom_scale = f32::NAN;
        assert!(matches!(
            mark_clean(&root.path().join("invalid-zoom.json"), &invalid_zoom),
            Err(SessionError::Invalid(_))
        ));
    }

    #[test]
    fn library_scope_round_trips_and_legacy_sessions_remain_readable() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("session.json");
        let mut saved = state();
        saved.library_browser = Some(LibraryBrowserState {
            collection_id: Some(17),
            search: "京都 RAW".into(),
            page: 3,
        });
        autosave(&path, &saved).unwrap();
        assert_eq!(open(&path).unwrap().state, Some(saved.clone()));
        mark_clean(&path, &saved).unwrap();
        assert_eq!(open(&path).unwrap().state, Some(saved));
        let mut legacy = serde_json::to_value(state()).unwrap();
        legacy.as_object_mut().unwrap().remove("libraryBrowser");
        let migrated: SessionState = serde_json::from_value(legacy).unwrap();
        assert_eq!(migrated, state());
    }

    #[test]
    fn invalid_browser_state_never_overwrites_valid_recovery() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("session.json");
        autosave(&path, &state()).unwrap();
        let before = fs::read(&path).unwrap();
        let mut invalid = state();
        invalid.library_browser = Some(LibraryBrowserState {
            collection_id: Some(-1),
            search: String::new(),
            page: 0,
        });
        assert!(mark_clean(&path, &invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
