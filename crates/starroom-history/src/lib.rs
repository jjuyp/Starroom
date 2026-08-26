//! Versioned non-destructive command history with periodic state checkpoints and named snapshots.
//! Only compact project state and commands are persisted; pixel buffers and cache rasters are
//! deliberately outside this crate.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

pub const HISTORY_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_CHECKPOINT_INTERVAL: usize = 100;

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("InvalidHistoryEntry: {0}")]
    InvalidHistoryEntry(String),
    #[error("HistoryCorrupt: {0}")]
    HistoryCorrupt(String),
    #[error("SnapshotNotFound: {0}")]
    SnapshotNotFound(String),
    #[error("SnapshotVersionUnsupported: {0}")]
    SnapshotVersionUnsupported(u32),
    #[error("CheckpointCorrupt: {0}")]
    CheckpointCorrupt(String),
    #[error("UndoUnavailable")]
    UndoUnavailable,
    #[error("RedoUnavailable")]
    RedoUnavailable,
    #[error("HistoryPersistenceFailed: {0}")]
    HistoryPersistenceFailed(String),
    #[error("HistoryReplayFailed: {0}")]
    HistoryReplayFailed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EditStateVersion(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EditCommand {
    SetValue {
        pointer: String,
        before: Value,
        after: Value,
    },
    ReplaceState {
        before: Value,
        after: Value,
    },
    Layer {
        operation: String,
        before: Value,
        after: Value,
    },
    MaskTree {
        operation: String,
        before: Value,
        after: Value,
    },
    Healing {
        operation: String,
        before: Value,
        after: Value,
    },
    BrushStroke {
        stroke_id: String,
        pointer: String,
        before: Value,
        after: Value,
    },
    RestoreSnapshot {
        snapshot_id: String,
        before: Value,
        after: Value,
    },
}

impl EditCommand {
    fn forward(&self, state: &mut Value) -> Result<(), HistoryError> {
        match self {
            Self::SetValue { pointer, after, .. } | Self::BrushStroke { pointer, after, .. } => {
                set_pointer(state, pointer, after.clone())
            }
            Self::ReplaceState { after, .. }
            | Self::Layer { after, .. }
            | Self::MaskTree { after, .. }
            | Self::Healing { after, .. }
            | Self::RestoreSnapshot { after, .. } => {
                *state = after.clone();
                Ok(())
            }
        }
    }
    fn backward(&self, state: &mut Value) -> Result<(), HistoryError> {
        match self {
            Self::SetValue {
                pointer, before, ..
            }
            | Self::BrushStroke {
                pointer, before, ..
            } => set_pointer(state, pointer, before.clone()),
            Self::ReplaceState { before, .. }
            | Self::Layer { before, .. }
            | Self::MaskTree { before, .. }
            | Self::Healing { before, .. }
            | Self::RestoreSnapshot { before, .. } => {
                *state = before.clone();
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub sequence: u64,
    pub parent_version: EditStateVersion,
    pub version: EditStateVersion,
    pub timestamp: i64,
    pub description: String,
    pub affected_stage: String,
    pub command: EditCommand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCheckpoint {
    pub sequence: u64,
    pub state_version: EditStateVersion,
    pub state: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedSnapshot {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub schema_version: u32,
    pub state_version: EditStateVersion,
    pub state: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedHistory {
    pub schema_version: u32,
    pub initial_state: Value,
    pub entries: Vec<HistoryEntry>,
    pub cursor: usize,
    pub checkpoints: Vec<HistoryCheckpoint>,
    pub snapshots: Vec<NamedSnapshot>,
    pub checkpoint_interval: usize,
}

#[derive(Debug, Clone)]
pub struct EditHistory {
    state: Value,
    document: PersistedHistory,
}

#[derive(Debug, Clone)]
pub struct EditInteraction {
    pointer: String,
    before: Value,
    latest: Value,
    description: String,
    affected_stage: String,
}

impl EditInteraction {
    pub fn update(&mut self, value: Value) {
        self.latest = value;
    }
}

impl EditHistory {
    pub fn new(initial_state: Value) -> Result<Self, HistoryError> {
        validate_state(&initial_state)?;
        Ok(Self {
            state: initial_state.clone(),
            document: PersistedHistory {
                schema_version: HISTORY_SCHEMA_VERSION,
                initial_state,
                entries: Vec::new(),
                cursor: 0,
                checkpoints: Vec::new(),
                snapshots: Vec::new(),
                checkpoint_interval: DEFAULT_CHECKPOINT_INTERVAL,
            },
        })
    }
    pub fn with_checkpoint_interval(
        initial_state: Value,
        interval: usize,
    ) -> Result<Self, HistoryError> {
        if interval == 0 {
            return Err(HistoryError::InvalidHistoryEntry(
                "checkpoint interval must be positive".into(),
            ));
        }
        let mut value = Self::new(initial_state)?;
        value.document.checkpoint_interval = interval;
        Ok(value)
    }
    pub fn state(&self) -> &Value {
        &self.state
    }
    pub fn state_version(&self) -> EditStateVersion {
        state_version(&self.state)
    }
    pub fn cursor(&self) -> usize {
        self.document.cursor
    }
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.document.entries
    }
    pub fn snapshots(&self) -> &[NamedSnapshot] {
        &self.document.snapshots
    }
    pub fn checkpoints(&self) -> &[HistoryCheckpoint] {
        &self.document.checkpoints
    }
    pub fn can_undo(&self) -> bool {
        self.document.cursor > 0
    }
    pub fn can_redo(&self) -> bool {
        self.document.cursor < self.document.entries.len()
    }

    pub fn begin_interaction(
        &self,
        pointer: impl Into<String>,
        description: impl Into<String>,
        affected_stage: impl Into<String>,
    ) -> Result<EditInteraction, HistoryError> {
        let pointer = pointer.into();
        let before = self.state.pointer(&pointer).cloned().ok_or_else(|| {
            HistoryError::InvalidHistoryEntry(format!("pointer does not exist: {pointer}"))
        })?;
        Ok(EditInteraction {
            pointer,
            before: before.clone(),
            latest: before,
            description: description.into(),
            affected_stage: affected_stage.into(),
        })
    }
    pub fn commit_interaction(
        &mut self,
        interaction: EditInteraction,
    ) -> Result<Option<EditStateVersion>, HistoryError> {
        if interaction.before == interaction.latest {
            return Ok(None);
        }
        let command = EditCommand::SetValue {
            pointer: interaction.pointer,
            before: interaction.before,
            after: interaction.latest,
        };
        self.commit(interaction.description, interaction.affected_stage, command)
            .map(Some)
    }
    pub fn commit(
        &mut self,
        description: impl Into<String>,
        affected_stage: impl Into<String>,
        command: EditCommand,
    ) -> Result<EditStateVersion, HistoryError> {
        let description = description.into();
        let affected_stage = affected_stage.into();
        if description.trim().is_empty() || affected_stage.trim().is_empty() {
            return Err(HistoryError::InvalidHistoryEntry(
                "description and affected stage are required".into(),
            ));
        }
        if self.document.cursor < self.document.entries.len() {
            self.document.entries.truncate(self.document.cursor);
            self.document
                .checkpoints
                .retain(|value| value.sequence as usize <= self.document.cursor);
        }
        let parent = self.state_version();
        let mut next = self.state.clone();
        command.forward(&mut next)?;
        validate_state(&next)?;
        let version = state_version(&next);
        let sequence = self.document.cursor as u64 + 1;
        self.document.entries.push(HistoryEntry {
            sequence,
            parent_version: parent,
            version: version.clone(),
            timestamp: now(),
            description,
            affected_stage,
            command,
        });
        self.document.cursor += 1;
        self.state = next;
        if self
            .document
            .cursor
            .is_multiple_of(self.document.checkpoint_interval)
        {
            self.document.checkpoints.push(HistoryCheckpoint {
                sequence,
                state_version: version.clone(),
                state: self.state.clone(),
            });
        }
        Ok(version)
    }
    pub fn undo(&mut self) -> Result<EditStateVersion, HistoryError> {
        if !self.can_undo() {
            return Err(HistoryError::UndoUnavailable);
        }
        let entry = &self.document.entries[self.document.cursor - 1];
        entry.command.backward(&mut self.state)?;
        self.document.cursor -= 1;
        let version = self.state_version();
        if version != entry.parent_version {
            return Err(HistoryError::HistoryReplayFailed(format!(
                "undo version mismatch at sequence {}",
                entry.sequence
            )));
        }
        Ok(version)
    }
    pub fn redo(&mut self) -> Result<EditStateVersion, HistoryError> {
        if !self.can_redo() {
            return Err(HistoryError::RedoUnavailable);
        }
        let entry = &self.document.entries[self.document.cursor];
        if self.state_version() != entry.parent_version {
            return Err(HistoryError::HistoryReplayFailed(format!(
                "redo parent mismatch at sequence {}",
                entry.sequence
            )));
        }
        entry.command.forward(&mut self.state)?;
        self.document.cursor += 1;
        let version = self.state_version();
        if version != entry.version {
            return Err(HistoryError::HistoryReplayFailed(format!(
                "redo version mismatch at sequence {}",
                entry.sequence
            )));
        }
        Ok(version)
    }
    pub fn create_snapshot(&mut self, name: impl Into<String>) -> Result<String, HistoryError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(HistoryError::InvalidHistoryEntry(
                "snapshot name is required".into(),
            ));
        }
        let version = self.state_version();
        let id = format!("snapshot-{}-{}", now(), &version.0[..12]);
        self.document.snapshots.push(NamedSnapshot {
            id: id.clone(),
            name: name.trim().into(),
            created_at: now(),
            schema_version: HISTORY_SCHEMA_VERSION,
            state_version: version,
            state: self.state.clone(),
        });
        Ok(id)
    }
    pub fn rename_snapshot(&mut self, id: &str, name: &str) -> Result<(), HistoryError> {
        if name.trim().is_empty() {
            return Err(HistoryError::InvalidHistoryEntry(
                "snapshot name is required".into(),
            ));
        }
        let snapshot = self
            .document
            .snapshots
            .iter_mut()
            .find(|value| value.id == id)
            .ok_or_else(|| HistoryError::SnapshotNotFound(id.into()))?;
        snapshot.name = name.trim().into();
        Ok(())
    }
    pub fn delete_snapshot(&mut self, id: &str) -> Result<(), HistoryError> {
        let before = self.document.snapshots.len();
        self.document.snapshots.retain(|value| value.id != id);
        if before == self.document.snapshots.len() {
            return Err(HistoryError::SnapshotNotFound(id.into()));
        }
        Ok(())
    }
    pub fn restore_snapshot(&mut self, id: &str) -> Result<EditStateVersion, HistoryError> {
        let snapshot = self
            .document
            .snapshots
            .iter()
            .find(|value| value.id == id)
            .cloned()
            .ok_or_else(|| HistoryError::SnapshotNotFound(id.into()))?;
        if snapshot.schema_version != HISTORY_SCHEMA_VERSION {
            return Err(HistoryError::SnapshotVersionUnsupported(
                snapshot.schema_version,
            ));
        }
        self.commit(
            format!("Restore Snapshot \"{}\"", snapshot.name),
            "snapshot",
            EditCommand::RestoreSnapshot {
                snapshot_id: id.into(),
                before: self.state.clone(),
                after: snapshot.state,
            },
        )
    }
    pub fn snapshot_state(&self, id: &str) -> Result<&Value, HistoryError> {
        self.document
            .snapshots
            .iter()
            .find(|value| value.id == id)
            .map(|value| &value.state)
            .ok_or_else(|| HistoryError::SnapshotNotFound(id.into()))
    }
    pub fn persist(&self, path: impl AsRef<Path>) -> Result<(), HistoryError> {
        let bytes = serde_json::to_vec_pretty(&self.document)
            .map_err(|error| HistoryError::HistoryPersistenceFailed(error.to_string()))?;
        atomic_write(path.as_ref(), &bytes)
    }
    pub fn load(path: impl AsRef<Path>) -> Result<Self, HistoryError> {
        let bytes = fs::read(path)
            .map_err(|error| HistoryError::HistoryPersistenceFailed(error.to_string()))?;
        let document: PersistedHistory = serde_json::from_slice(&bytes)
            .map_err(|error| HistoryError::HistoryCorrupt(error.to_string()))?;
        Self::from_document(document)
    }
    pub fn from_document(document: PersistedHistory) -> Result<Self, HistoryError> {
        if document.schema_version != HISTORY_SCHEMA_VERSION {
            return Err(HistoryError::HistoryCorrupt(format!(
                "unsupported history schema {}",
                document.schema_version
            )));
        }
        if document.cursor > document.entries.len() || document.checkpoint_interval == 0 {
            return Err(HistoryError::HistoryCorrupt(
                "invalid cursor/checkpoint interval".into(),
            ));
        }
        validate_state(&document.initial_state)?;
        let mut state = document.initial_state.clone();
        let start = match document
            .checkpoints
            .iter()
            .filter(|value| value.sequence as usize <= document.cursor)
            .max_by_key(|value| value.sequence)
        {
            Some(value) => {
                if state_version(&value.state) != value.state_version {
                    return Err(HistoryError::CheckpointCorrupt(format!(
                        "sequence {} hash mismatch",
                        value.sequence
                    )));
                }
                state = value.state.clone();
                value.sequence as usize
            }
            None => 0,
        };
        for entry in &document.entries[start..document.cursor] {
            if state_version(&state) != entry.parent_version {
                return Err(HistoryError::HistoryReplayFailed(format!(
                    "parent mismatch at sequence {}",
                    entry.sequence
                )));
            }
            entry.command.forward(&mut state)?;
            if state_version(&state) != entry.version {
                return Err(HistoryError::HistoryReplayFailed(format!(
                    "version mismatch at sequence {}",
                    entry.sequence
                )));
            }
        }
        Ok(Self { state, document })
    }
}

pub fn graph_cache_identity(source_identity: &str, state: &Value) -> String {
    let mut hash = Sha256::new();
    hash.update(b"starroom-graph-state-v1\0");
    hash.update(source_identity.as_bytes());
    hash.update(canonical_bytes(state));
    hex(hash.finalize())
}
fn set_pointer(root: &mut Value, pointer: &str, value: Value) -> Result<(), HistoryError> {
    if pointer.is_empty() {
        *root = value;
        return Ok(());
    }
    let target = root.pointer_mut(pointer).ok_or_else(|| {
        HistoryError::InvalidHistoryEntry(format!("pointer does not exist: {pointer}"))
    })?;
    *target = value;
    Ok(())
}
fn validate_state(state: &Value) -> Result<(), HistoryError> {
    fn visit(value: &Value) -> bool {
        match value {
            Value::Number(number) => number.as_f64().is_none_or(f64::is_finite),
            Value::Array(values) => values.iter().all(visit),
            Value::Object(values) => values.values().all(visit),
            _ => true,
        }
    }
    if visit(state) {
        Ok(())
    } else {
        Err(HistoryError::InvalidHistoryEntry(
            "state contains non-finite number".into(),
        ))
    }
}
fn canonical_bytes(state: &Value) -> Vec<u8> {
    serde_json::to_vec(state).unwrap_or_default()
}
fn state_version(state: &Value) -> EditStateVersion {
    let mut hash = Sha256::new();
    hash.update(b"starroom-edit-state-v1\0");
    hash.update(canonical_bytes(state));
    EditStateVersion(hex(hash.finalize()))
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64
}
fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), HistoryError> {
    let parent = path
        .parent()
        .ok_or_else(|| HistoryError::HistoryPersistenceFailed("path has no parent".into()))?;
    fs::create_dir_all(parent)
        .map_err(|error| HistoryError::HistoryPersistenceFailed(error.to_string()))?;
    let temporary = temporary_path(path);
    let result = (|| {
        let mut file = File::create(&temporary)
            .map_err(|error| HistoryError::HistoryPersistenceFailed(error.to_string()))?;
        file.write_all(bytes)
            .map_err(|error| HistoryError::HistoryPersistenceFailed(error.to_string()))?;
        file.sync_all()
            .map_err(|error| HistoryError::HistoryPersistenceFailed(error.to_string()))?;
        fs::rename(&temporary, path)
            .map_err(|error| HistoryError::HistoryPersistenceFailed(error.to_string()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::env;
    fn state() -> Value {
        json!({"tone":{"exposure":0.0,"contrast":0.0},"layers":[],"masks":[],"healing":[],"aiDenoise":{"amount":0.0},"look":null})
    }
    fn set(pointer: &str, before: Value, after: Value) -> EditCommand {
        EditCommand::SetValue {
            pointer: pointer.into(),
            before,
            after,
        }
    }
    #[test]
    fn simple_undo_redo_and_branch_truncation() {
        let mut history = EditHistory::new(state()).unwrap();
        history
            .commit(
                "Exposure",
                "tone",
                set("/tone/exposure", json!(0.0), json!(0.5)),
            )
            .unwrap();
        let edited = history.state().clone();
        history.undo().unwrap();
        assert_eq!(history.state().pointer("/tone/exposure"), Some(&json!(0.0)));
        history.redo().unwrap();
        assert_eq!(history.state(), &edited);
        history.undo().unwrap();
        history
            .commit(
                "Contrast",
                "tone",
                set("/tone/contrast", json!(0.0), json!(2.0)),
            )
            .unwrap();
        assert!(!history.can_redo());
        assert_eq!(history.entries().len(), 1);
    }
    #[test]
    fn one_hundred_edits_and_checkpoint_replay_are_deterministic() {
        let mut history = EditHistory::with_checkpoint_interval(state(), 10).unwrap();
        for index in 0..100 {
            history
                .commit(
                    format!("Exposure {index}"),
                    "tone",
                    set(
                        "/tone/exposure",
                        json!(index as f64 / 10.0),
                        json!((index + 1) as f64 / 10.0),
                    ),
                )
                .unwrap();
        }
        assert_eq!(history.checkpoints().len(), 10);
        let loaded = EditHistory::from_document(history.document.clone()).unwrap();
        assert_eq!(loaded.state(), history.state());
        for _ in 0..100 {
            history.undo().unwrap();
        }
        assert_eq!(history.state(), &state());
        for _ in 0..100 {
            history.redo().unwrap();
        }
        assert_eq!(history.state(), loaded.state());
    }
    #[test]
    fn interaction_and_brush_coalesce() {
        let mut history = EditHistory::new(state()).unwrap();
        let mut drag = history
            .begin_interaction("/tone/exposure", "Exposure drag", "tone")
            .unwrap();
        for value in [0.1, 0.2, 0.3, 0.4, 0.5] {
            drag.update(json!(value));
        }
        history.commit_interaction(drag).unwrap();
        assert_eq!(history.entries().len(), 1);
        let before = history.state().pointer("/masks").unwrap().clone();
        history
            .commit(
                "Brush stroke",
                "mask",
                EditCommand::BrushStroke {
                    stroke_id: "stroke-1".into(),
                    pointer: "/masks".into(),
                    before,
                    after: json!([{"id":"stroke-1"}]),
                },
            )
            .unwrap();
        assert_eq!(history.entries().len(), 2);
        history.undo().unwrap();
        assert_eq!(history.state().pointer("/masks"), Some(&json!([])));
    }
    #[test]
    fn structured_operations_undo_redo() {
        let mut history = EditHistory::new(state()).unwrap();
        for (stage, pointer, value) in [
            ("layer", "/layers", json!([{"id":"a"}])),
            ("mask", "/masks", json!([{"id":"m"}])),
            ("healing", "/healing", json!([{"id":"h"}])),
            ("ai", "/aiDenoise", json!({"amount":0.8})),
            ("look", "/look", json!({"id":"warm"})),
        ] {
            let before = history.state().pointer(pointer).unwrap().clone();
            history
                .commit(stage, stage, set(pointer, before, value))
                .unwrap();
        }
        let final_state = history.state().clone();
        for _ in 0..5 {
            history.undo().unwrap();
        }
        assert_eq!(history.state(), &state());
        for _ in 0..5 {
            history.redo().unwrap();
        }
        assert_eq!(history.state(), &final_state);
    }
    #[test]
    fn snapshots_restore_is_undoable() {
        let mut history = EditHistory::new(state()).unwrap();
        history
            .commit(
                "Warm",
                "tone",
                set("/tone/exposure", json!(0.0), json!(0.7)),
            )
            .unwrap();
        let snapshot = history.create_snapshot("Warm").unwrap();
        history.rename_snapshot(&snapshot, "Warm Film").unwrap();
        history
            .commit(
                "Cool",
                "tone",
                set("/tone/exposure", json!(0.7), json!(-0.3)),
            )
            .unwrap();
        let cool = history.state().clone();
        history.restore_snapshot(&snapshot).unwrap();
        assert_eq!(history.state().pointer("/tone/exposure"), Some(&json!(0.7)));
        history.undo().unwrap();
        assert_eq!(history.state(), &cool);
        history.delete_snapshot(&snapshot).unwrap();
        assert!(matches!(
            history.snapshot_state(&snapshot),
            Err(HistoryError::SnapshotNotFound(_))
        ));
    }
    #[test]
    fn persistence_reload_and_cache_identity() {
        let root = env::temp_dir().join(format!("starroom-history-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let path = root.join("history.json");
        let mut history = EditHistory::new(state()).unwrap();
        let before = graph_cache_identity("asset-a", history.state());
        history
            .commit(
                "Exposure",
                "tone",
                set("/tone/exposure", json!(0.0), json!(1.0)),
            )
            .unwrap();
        assert_ne!(before, graph_cache_identity("asset-a", history.state()));
        history.persist(&path).unwrap();
        let loaded = EditHistory::load(&path).unwrap();
        assert_eq!(loaded.state(), history.state());
        assert_eq!(loaded.entries(), history.entries());
    }
    #[test]
    fn corrupt_checkpoint_is_typed() {
        let mut history = EditHistory::with_checkpoint_interval(state(), 1).unwrap();
        history
            .commit(
                "Exposure",
                "tone",
                set("/tone/exposure", json!(0.0), json!(1.0)),
            )
            .unwrap();
        history.document.checkpoints[0].state = json!({"corrupt":true});
        assert!(matches!(
            EditHistory::from_document(history.document),
            Err(HistoryError::CheckpointCorrupt(_))
        ));
    }
    #[test]
    fn m28_ten_thousand_entries_checkpoint_restore_undo_redo_and_branch_scale() {
        let mut history = EditHistory::with_checkpoint_interval(state(), 100).unwrap();
        let started = std::time::Instant::now();
        for index in 0..10_000 {
            history
                .commit(
                    "Exposure",
                    "tone",
                    set("/tone/exposure", json!(index), json!(index + 1)),
                )
                .unwrap();
        }
        assert_eq!(history.checkpoints().len(), 100);
        let final_state = history.state().clone();
        let mut loaded = EditHistory::from_document(history.document).unwrap();
        assert_eq!(loaded.state(), &final_state);
        let snapshot = loaded.create_snapshot("10k final").unwrap();
        for _ in 0..100 {
            loaded.undo().unwrap();
        }
        for _ in 0..50 {
            loaded.redo().unwrap();
        }
        loaded
            .commit(
                "Branch",
                "tone",
                set("/tone/exposure", json!(9_950), json!(42)),
            )
            .unwrap();
        assert!(!loaded.can_redo());
        loaded.restore_snapshot(&snapshot).unwrap();
        assert_eq!(loaded.state(), &final_state);
        eprintln!(
            "M28 10,000 history commands full exercise: {:.3} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}
