//! Local-first photo library. SQLite stores only identities and workflow metadata; source pixels
//! and raster caches stay on the filesystem.

use rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use starroom_imageio::{
    DecodedSourceImage, decode_source_preview, encode_jpeg_rgb8, lens_metadata,
};
use starroom_pipeline::{RenderSettings, render_source_preview_to_srgb8};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

pub const SCHEMA_VERSION: i64 = 1;
pub const FINGERPRINT_VERSION: &str = "StarroomAssetFingerprintV1";
const SAMPLE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("DatabaseOpenFailed: {0}")]
    DatabaseOpenFailed(String),
    #[error("MigrationFailed: {0}")]
    MigrationFailed(String),
    #[error("DatabaseBusy")]
    DatabaseBusy,
    #[error("ImportCancelled")]
    ImportCancelled,
    #[error("UnsupportedFile: {0}")]
    UnsupportedFile(PathBuf),
    #[error("MetadataReadFailed: {path}: {reason}")]
    MetadataReadFailed { path: PathBuf, reason: String },
    #[error("FingerprintFailed: {path}: {reason}")]
    FingerprintFailed { path: PathBuf, reason: String },
    #[error("ThumbnailFailed: {0}")]
    ThumbnailFailed(String),
    #[error("DuplicateAsset: {0}")]
    DuplicateAsset(String),
    #[error("MissingSource: {0}")]
    MissingSource(i64),
    #[error("RelinkMismatch: {0}")]
    RelinkMismatch(i64),
    #[error("InvalidQuery: {0}")]
    InvalidQuery(String),
    #[error("InvalidCollection: {0}")]
    InvalidCollection(i64),
    #[error("CorruptDatabase: {0}")]
    CorruptDatabase(String),
    #[error("database operation failed: {0}")]
    Sql(#[from] rusqlite::Error),
}

fn sql_error(error: rusqlite::Error) -> LibraryError {
    match &error {
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::DatabaseBusy =>
        {
            LibraryError::DatabaseBusy
        }
        _ => LibraryError::Sql(error),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AssetFlag {
    #[default]
    Unflagged,
    Pick,
    Reject,
}

impl AssetFlag {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unflagged => "unflagged",
            Self::Pick => "pick",
            Self::Reject => "reject",
        }
    }
    fn parse(value: &str) -> Self {
        match value {
            "pick" => Self::Pick,
            "reject" => Self::Reject,
            _ => Self::Unflagged,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ColorLabel {
    #[default]
    None,
    Red,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl ColorLabel {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Red => "red",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Purple => "purple",
        }
    }
    fn parse(value: &str) -> Self {
        match value {
            "red" => Self::Red,
            "yellow" => Self::Yellow,
            "green" => Self::Green,
            "blue" => Self::Blue,
            "purple" => Self::Purple,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AssetMetadata {
    pub file_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub orientation: Option<i32>,
    pub capture_time: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_make: Option<String>,
    pub lens_model: Option<String>,
    pub focal_length: Option<f32>,
    pub aperture: Option<f32>,
    pub shutter_speed: Option<f32>,
    pub iso: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRecord {
    pub id: i64,
    pub source_path: PathBuf,
    pub source_identity: String,
    pub content_fingerprint: String,
    pub file_size: u64,
    pub modified_time: i64,
    pub metadata: AssetMetadata,
    pub rating: u8,
    pub flag: AssetFlag,
    pub color_label: ColorLabel,
    pub missing: bool,
    pub project_reference: Option<String>,
    pub thumbnail_cache_key: Option<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortField {
    CaptureTime,
    ImportTime,
    Filename,
    Rating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LibraryQuery {
    pub text: Option<String>,
    pub filename: Option<String>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub keyword: Option<String>,
    pub minimum_rating: Option<u8>,
    pub flag: Option<AssetFlag>,
    pub color_label: Option<ColorLabel>,
    pub file_types: Vec<String>,
    pub minimum_iso: Option<f32>,
    pub maximum_iso: Option<f32>,
    pub capture_from: Option<i64>,
    pub capture_to: Option<i64>,
    pub missing: Option<bool>,
    pub sort: SortField,
    pub direction: SortDirection,
    pub limit: u32,
    pub offset: u32,
}

impl Default for LibraryQuery {
    fn default() -> Self {
        Self {
            text: None,
            filename: None,
            camera: None,
            lens: None,
            keyword: None,
            minimum_rating: None,
            flag: None,
            color_label: None,
            file_types: Vec::new(),
            minimum_iso: None,
            maximum_iso: None,
            capture_from: None,
            capture_to: None,
            missing: None,
            sort: SortField::ImportTime,
            direction: SortDirection::Descending,
            limit: 200,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartCollectionRuleV1 {
    pub all: Vec<SmartPredicate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "field", rename_all = "camelCase")]
pub enum SmartPredicate {
    Rating {
        minimum: u8,
    },
    Flag {
        value: AssetFlag,
    },
    ColorLabel {
        value: ColorLabel,
    },
    Camera {
        value: String,
    },
    Lens {
        value: String,
    },
    FileType {
        value: String,
    },
    IsoRange {
        minimum: Option<f32>,
        maximum: Option<f32>,
    },
    CaptureDate {
        from: Option<i64>,
        to: Option<i64>,
    },
    Keyword {
        value: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CollectionKind {
    Normal,
    Smart,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionRecord {
    pub id: i64,
    pub name: String,
    pub kind: CollectionKind,
    pub rule: Option<SmartCollectionRuleV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: Vec<i64>,
    pub already_present: Vec<PathBuf>,
    pub duplicates: Vec<PathBuf>,
    pub relink_candidates: Vec<(i64, PathBuf)>,
    pub unsupported: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetFingerprintV1 {
    pub byte_length: u64,
    pub digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThumbnailSize {
    Small256,
    Medium512,
    Large1024,
}

impl ThumbnailSize {
    pub fn pixels(self) -> u32 {
        match self {
            Self::Small256 => 256,
            Self::Medium512 => 512,
            Self::Large1024 => 1024,
        }
    }
}

pub struct Library {
    connection: Connection,
}

impl Library {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LibraryError> {
        let connection = Connection::open(path.as_ref())
            .map_err(|error| LibraryError::DatabaseOpenFailed(error.to_string()))?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(sql_error)?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(sql_error)?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(sql_error)?;
        let mut library = Self { connection };
        library.migrate()?;
        Ok(library)
    }

    pub fn open_in_memory() -> Result<Self, LibraryError> {
        let connection = Connection::open_in_memory()
            .map_err(|error| LibraryError::DatabaseOpenFailed(error.to_string()))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(sql_error)?;
        let mut library = Self { connection };
        library.migrate()?;
        Ok(library)
    }

    pub fn schema_version(&self) -> Result<i64, LibraryError> {
        self.connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(sql_error)
    }

    fn migrate(&mut self) -> Result<(), LibraryError> {
        let version: i64 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(sql_error)?;
        if version > SCHEMA_VERSION {
            return Err(LibraryError::MigrationFailed(format!(
                "database version {version} is newer than supported {SCHEMA_VERSION}"
            )));
        }
        if version == 0 {
            let transaction = self.connection.transaction().map_err(sql_error)?;
            transaction
                .execute_batch(MIGRATION_V1)
                .map_err(|error| LibraryError::MigrationFailed(error.to_string()))?;
            transaction
                .pragma_update(None, "user_version", SCHEMA_VERSION)
                .map_err(sql_error)?;
            transaction.commit().map_err(sql_error)?;
        }
        Ok(())
    }

    pub fn import_paths(
        &mut self,
        paths: &[PathBuf],
        cancelled: &AtomicBool,
    ) -> Result<ImportResult, LibraryError> {
        let mut result = ImportResult::default();
        for chunk in paths.chunks(250) {
            if cancelled.load(Ordering::Relaxed) {
                result.cancelled = true;
                break;
            }
            let transaction = self.connection.transaction().map_err(sql_error)?;
            for path in chunk {
                if cancelled.load(Ordering::Relaxed) {
                    result.cancelled = true;
                    break;
                }
                match import_one(&transaction, path) {
                    Ok(ImportDisposition::Imported(id)) => result.imported.push(id),
                    Ok(ImportDisposition::AlreadyPresent) => {
                        result.already_present.push(path.clone())
                    }
                    Ok(ImportDisposition::Duplicate) => result.duplicates.push(path.clone()),
                    Ok(ImportDisposition::RelinkCandidate(id)) => {
                        result.relink_candidates.push((id, path.clone()))
                    }
                    Err(LibraryError::UnsupportedFile(_)) => result.unsupported.push(path.clone()),
                    Err(error) => result.failed.push((path.clone(), error.to_string())),
                }
            }
            if result.cancelled {
                transaction.rollback().map_err(sql_error)?;
                break;
            }
            transaction.commit().map_err(sql_error)?;
        }
        Ok(result)
    }

    pub fn recursive_paths(root: impl AsRef<Path>) -> Result<Vec<PathBuf>, LibraryError> {
        fn walk(path: &Path, output: &mut Vec<PathBuf>) -> Result<(), LibraryError> {
            for entry in fs::read_dir(path).map_err(|error| LibraryError::MetadataReadFailed {
                path: path.to_owned(),
                reason: error.to_string(),
            })? {
                let entry = entry.map_err(|error| LibraryError::MetadataReadFailed {
                    path: path.to_owned(),
                    reason: error.to_string(),
                })?;
                let kind = entry
                    .file_type()
                    .map_err(|error| LibraryError::MetadataReadFailed {
                        path: entry.path(),
                        reason: error.to_string(),
                    })?;
                if kind.is_dir() {
                    walk(&entry.path(), output)?;
                } else if kind.is_file() {
                    output.push(entry.path());
                }
            }
            Ok(())
        }
        let mut output = Vec::new();
        walk(root.as_ref(), &mut output)?;
        output.sort();
        Ok(output)
    }

    pub fn query(&self, query: &LibraryQuery) -> Result<Vec<AssetRecord>, LibraryError> {
        if query.limit == 0 || query.limit > 2_000 {
            return Err(LibraryError::InvalidQuery("limit must be 1..2000".into()));
        }
        if query.minimum_rating.is_some_and(|value| value > 5) {
            return Err(LibraryError::InvalidQuery("rating must be 0..5".into()));
        }
        let (where_sql, values) = build_where(query)?;
        let sort = match query.sort {
            SortField::CaptureTime => "a.capture_time",
            SortField::ImportTime => "a.import_time",
            SortField::Filename => "a.source_path_normalized",
            SortField::Rating => "a.rating",
        };
        let direction = match query.direction {
            SortDirection::Ascending => "ASC",
            SortDirection::Descending => "DESC",
        };
        let sql = format!(
            "SELECT a.id,a.source_path,a.source_identity,a.content_fingerprint,a.file_size,a.modified_time,a.file_type,a.width,a.height,a.orientation,a.capture_time,a.camera_make,a.camera_model,a.lens_make,a.lens_model,a.focal_length,a.aperture,a.shutter_speed,a.iso,a.rating,a.flag,a.color_label,a.missing,a.project_reference,a.thumbnail_cache_key FROM assets a {where_sql} ORDER BY {sort} {direction}, a.id {direction} LIMIT ? OFFSET ?"
        );
        let mut bound = values;
        bound.push(rusqlite::types::Value::Integer(i64::from(query.limit)));
        bound.push(rusqlite::types::Value::Integer(i64::from(query.offset)));
        let mut statement = self.connection.prepare(&sql).map_err(sql_error)?;
        let rows = statement
            .query_map(params_from_iter(bound), row_to_asset)
            .map_err(sql_error)?;
        let mut assets = Vec::new();
        for row in rows {
            let mut asset = row.map_err(sql_error)?;
            asset.keywords = self.keywords_for(asset.id)?;
            assets.push(asset);
        }
        Ok(assets)
    }

    pub fn asset(&self, id: i64) -> Result<Option<AssetRecord>, LibraryError> {
        let query = LibraryQuery {
            limit: 1,
            ..Default::default()
        };
        let (mut where_sql, mut values) = build_where(&query)?;
        where_sql.push_str(if where_sql.is_empty() {
            " WHERE a.id = ?"
        } else {
            " AND a.id = ?"
        });
        values.push(id.into());
        let sql = format!(
            "SELECT a.id,a.source_path,a.source_identity,a.content_fingerprint,a.file_size,a.modified_time,a.file_type,a.width,a.height,a.orientation,a.capture_time,a.camera_make,a.camera_model,a.lens_make,a.lens_model,a.focal_length,a.aperture,a.shutter_speed,a.iso,a.rating,a.flag,a.color_label,a.missing,a.project_reference,a.thumbnail_cache_key FROM assets a{where_sql}"
        );
        let mut asset = self
            .connection
            .query_row(&sql, params_from_iter(values), row_to_asset)
            .optional()
            .map_err(sql_error)?;
        if let Some(value) = &mut asset {
            value.keywords = self.keywords_for(value.id)?;
        }
        Ok(asset)
    }

    pub fn set_workflow(
        &self,
        ids: &[i64],
        rating: Option<u8>,
        flag: Option<AssetFlag>,
        label: Option<ColorLabel>,
    ) -> Result<(), LibraryError> {
        if rating.is_some_and(|value| value > 5) {
            return Err(LibraryError::InvalidQuery("rating must be 0..5".into()));
        }
        for id in ids {
            if let Some(value) = rating {
                self.connection
                    .execute(
                        "UPDATE assets SET rating=?,updated_at=? WHERE id=?",
                        params![value, now(), id],
                    )
                    .map_err(sql_error)?;
            }
            if let Some(value) = flag {
                self.connection
                    .execute(
                        "UPDATE assets SET flag=?,updated_at=? WHERE id=?",
                        params![value.as_str(), now(), id],
                    )
                    .map_err(sql_error)?;
            }
            if let Some(value) = label {
                self.connection
                    .execute(
                        "UPDATE assets SET color_label=?,updated_at=? WHERE id=?",
                        params![value.as_str(), now(), id],
                    )
                    .map_err(sql_error)?;
            }
        }
        Ok(())
    }

    pub fn add_keywords(
        &mut self,
        asset_ids: &[i64],
        names: &[String],
    ) -> Result<(), LibraryError> {
        let transaction = self.connection.transaction().map_err(sql_error)?;
        for name in names {
            let normalized = normalize_keyword(name);
            if normalized.is_empty() {
                continue;
            }
            transaction.execute("INSERT INTO keywords(name,normalized_name) VALUES(?,?) ON CONFLICT(normalized_name) DO NOTHING", params![name.trim(), normalized]).map_err(sql_error)?;
            let keyword_id: i64 = transaction
                .query_row(
                    "SELECT id FROM keywords WHERE normalized_name=?",
                    [normalize_keyword(name)],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            for asset_id in asset_ids {
                transaction
                    .execute(
                        "INSERT OR IGNORE INTO asset_keywords(asset_id,keyword_id) VALUES(?,?)",
                        params![asset_id, keyword_id],
                    )
                    .map_err(sql_error)?;
            }
        }
        transaction.commit().map_err(sql_error)
    }

    pub fn remove_keywords(
        &mut self,
        asset_ids: &[i64],
        names: &[String],
    ) -> Result<(), LibraryError> {
        let transaction = self.connection.transaction().map_err(sql_error)?;
        for name in names {
            for asset_id in asset_ids {
                transaction.execute("DELETE FROM asset_keywords WHERE asset_id=? AND keyword_id=(SELECT id FROM keywords WHERE normalized_name=?)", params![asset_id, normalize_keyword(name)]).map_err(sql_error)?;
            }
        }
        transaction.commit().map_err(sql_error)
    }

    fn keywords_for(&self, asset_id: i64) -> Result<Vec<String>, LibraryError> {
        let mut statement = self.connection.prepare("SELECT k.name FROM keywords k JOIN asset_keywords ak ON ak.keyword_id=k.id WHERE ak.asset_id=? ORDER BY k.normalized_name").map_err(sql_error)?;
        statement
            .query_map([asset_id], |row| row.get(0))
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)
    }

    pub fn create_collection(
        &self,
        name: &str,
        kind: CollectionKind,
        rule: Option<&SmartCollectionRuleV1>,
    ) -> Result<i64, LibraryError> {
        if name.trim().is_empty() || (kind == CollectionKind::Smart) != rule.is_some() {
            return Err(LibraryError::InvalidQuery(
                "collection name/rule mismatch".into(),
            ));
        }
        let rule_json = rule
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| LibraryError::InvalidQuery(error.to_string()))?;
        self.connection
            .execute(
                "INSERT INTO collections(name,type,created_at,updated_at) VALUES(?,?,?,?)",
                params![
                    name.trim(),
                    if kind == CollectionKind::Normal {
                        "normal"
                    } else {
                        "smart"
                    },
                    now(),
                    now()
                ],
            )
            .map_err(sql_error)?;
        let id = self.connection.last_insert_rowid();
        if let Some(rule_json) = rule_json {
            self.connection.execute("INSERT INTO smart_collection_rules(collection_id,schema_version,rule_json) VALUES(?,1,?)", params![id, rule_json]).map_err(sql_error)?;
        }
        Ok(id)
    }

    pub fn add_collection_assets(
        &mut self,
        collection_id: i64,
        asset_ids: &[i64],
    ) -> Result<(), LibraryError> {
        let kind: Option<String> = self
            .connection
            .query_row(
                "SELECT type FROM collections WHERE id=?",
                [collection_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if kind.as_deref() != Some("normal") {
            return Err(LibraryError::InvalidCollection(collection_id));
        }
        let transaction = self.connection.transaction().map_err(sql_error)?;
        let mut position: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(position)+1,0) FROM collection_assets WHERE collection_id=?",
                [collection_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        for asset_id in asset_ids {
            transaction.execute("INSERT OR IGNORE INTO collection_assets(collection_id,asset_id,position) VALUES(?,?,?)", params![collection_id,asset_id,position]).map_err(sql_error)?;
            position += 1;
        }
        transaction.commit().map_err(sql_error)
    }

    pub fn collection_assets(
        &self,
        collection_id: i64,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<AssetRecord>, LibraryError> {
        let collection = self
            .collection(collection_id)?
            .ok_or(LibraryError::InvalidCollection(collection_id))?;
        match collection.kind {
            CollectionKind::Normal => {
                let mut statement = self.connection.prepare("SELECT asset_id FROM collection_assets WHERE collection_id=? ORDER BY position,id LIMIT ? OFFSET ?").map_err(sql_error)?;
                let ids = statement
                    .query_map(params![collection_id, limit, offset], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map_err(sql_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(sql_error)?;
                ids.into_iter()
                    .map(|id| {
                        self.asset(id)?.ok_or(LibraryError::CorruptDatabase(format!(
                            "collection references missing asset {id}"
                        )))
                    })
                    .collect()
            }
            CollectionKind::Smart => self.query(&rule_to_query(
                collection
                    .rule
                    .as_ref()
                    .ok_or(LibraryError::InvalidCollection(collection_id))?,
                limit,
                offset,
            )?),
        }
    }

    pub fn collection(&self, id: i64) -> Result<Option<CollectionRecord>, LibraryError> {
        let value: Option<(String,String,Option<String>)> = self.connection.query_row("SELECT c.name,c.type,s.rule_json FROM collections c LEFT JOIN smart_collection_rules s ON s.collection_id=c.id WHERE c.id=?", [id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional().map_err(sql_error)?;
        value
            .map(|(name, kind, rule)| {
                Ok(CollectionRecord {
                    id,
                    name,
                    kind: if kind == "normal" {
                        CollectionKind::Normal
                    } else {
                        CollectionKind::Smart
                    },
                    rule: rule
                        .map(|value| serde_json::from_str(&value))
                        .transpose()
                        .map_err(|error| LibraryError::CorruptDatabase(error.to_string()))?,
                })
            })
            .transpose()
    }

    pub fn collections(&self) -> Result<Vec<CollectionRecord>, LibraryError> {
        let mut statement = self.connection.prepare("SELECT c.id,c.name,c.type,s.rule_json FROM collections c LEFT JOIN smart_collection_rules s ON s.collection_id=c.id ORDER BY lower(c.name),c.id").map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(sql_error)?;
        let mut result = Vec::new();
        for row in rows {
            let (id, name, kind, rule) = row.map_err(sql_error)?;
            result.push(CollectionRecord {
                id,
                name,
                kind: if kind == "normal" {
                    CollectionKind::Normal
                } else {
                    CollectionKind::Smart
                },
                rule: rule
                    .map(|value| serde_json::from_str(&value))
                    .transpose()
                    .map_err(|error| LibraryError::CorruptDatabase(error.to_string()))?,
            });
        }
        Ok(result)
    }

    pub fn refresh_missing(&self) -> Result<usize, LibraryError> {
        let mut statement = self
            .connection
            .prepare("SELECT id,source_path FROM assets")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        let mut changed = 0;
        for (id, path) in rows {
            let missing = !Path::new(&path).is_file();
            changed += self
                .connection
                .execute(
                    "UPDATE assets SET missing=? WHERE id=? AND missing<>?",
                    params![missing, id, missing],
                )
                .map_err(sql_error)?;
        }
        Ok(changed)
    }

    pub fn relink(&self, asset_id: i64, new_path: impl AsRef<Path>) -> Result<(), LibraryError> {
        let asset = self
            .asset(asset_id)?
            .ok_or(LibraryError::MissingSource(asset_id))?;
        let fingerprint = fingerprint_file(new_path.as_ref())?;
        if fingerprint.digest != asset.content_fingerprint
            || fingerprint.byte_length != asset.file_size
        {
            return Err(LibraryError::RelinkMismatch(asset_id));
        }
        let normalized = normalize_path(new_path.as_ref())?;
        self.connection.execute("UPDATE assets SET source_path=?,source_path_normalized=?,modified_time=?,missing=0,updated_at=? WHERE id=?", params![new_path.as_ref().to_string_lossy(), normalized, file_modified(new_path.as_ref())?, now(), asset_id]).map_err(sql_error)?;
        Ok(())
    }

    pub fn set_project_reference(
        &self,
        asset_id: i64,
        reference: Option<&str>,
    ) -> Result<(), LibraryError> {
        self.connection
            .execute(
                "UPDATE assets SET project_reference=?,updated_at=? WHERE id=?",
                params![reference, now(), asset_id],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    pub fn thumbnail_identity(asset: &AssetRecord, size: ThumbnailSize) -> String {
        let mut hash = Sha256::new();
        hash.update(b"starroom-thumbnail-v1\0");
        hash.update(asset.content_fingerprint.as_bytes());
        hash.update(asset.metadata.orientation.unwrap_or(1).to_le_bytes());
        hash.update(size.pixels().to_le_bytes());
        hex(hash.finalize())
    }

    pub fn generate_thumbnail(
        &self,
        asset_id: i64,
        cache_root: impl AsRef<Path>,
        size: ThumbnailSize,
    ) -> Result<PathBuf, LibraryError> {
        let asset = self
            .asset(asset_id)?
            .ok_or(LibraryError::MissingSource(asset_id))?;
        if asset.missing || !asset.source_path.is_file() {
            return Err(LibraryError::MissingSource(asset_id));
        }
        let identity = Self::thumbnail_identity(&asset, size);
        let directory = cache_root.as_ref().join(size.pixels().to_string());
        fs::create_dir_all(&directory)
            .map_err(|error| LibraryError::ThumbnailFailed(error.to_string()))?;
        let destination = directory.join(format!("{identity}.jpg"));
        if destination.is_file() {
            return Ok(destination);
        }
        let decoded = decode_source_preview(&asset.source_path, size.pixels())
            .map_err(|error| LibraryError::ThumbnailFailed(error.to_string()))?;
        let rendered = render_source_preview_to_srgb8(&decoded, &RenderSettings::default())
            .map_err(|error| LibraryError::ThumbnailFailed(error.to_string()))?;
        let jpeg = encode_jpeg_rgb8(&rendered.data, rendered.width, rendered.height, 88, None)
            .map_err(|error| LibraryError::ThumbnailFailed(error.to_string()))?;
        let temporary = destination.with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&temporary, jpeg)
            .map_err(|error| LibraryError::ThumbnailFailed(error.to_string()))?;
        fs::rename(&temporary, &destination).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            LibraryError::ThumbnailFailed(error.to_string())
        })?;
        Ok(destination)
    }
}

enum ImportDisposition {
    Imported(i64),
    AlreadyPresent,
    Duplicate,
    RelinkCandidate(i64),
}

fn import_one(
    transaction: &Transaction<'_>,
    path: &Path,
) -> Result<ImportDisposition, LibraryError> {
    if !supported(path) {
        return Err(LibraryError::UnsupportedFile(path.to_owned()));
    }
    let normalized = normalize_path(path)?;
    if transaction
        .query_row(
            "SELECT 1 FROM assets WHERE source_path_normalized=?",
            [&normalized],
            |_| Ok(()),
        )
        .optional()
        .map_err(sql_error)?
        .is_some()
    {
        return Ok(ImportDisposition::AlreadyPresent);
    }
    let fingerprint = fingerprint_file(path)?;
    let existing: Option<(i64,bool)> = transaction.query_row("SELECT id,missing FROM assets WHERE fingerprint_version=? AND content_fingerprint=? AND file_size=? ORDER BY id LIMIT 1", params![FINGERPRINT_VERSION,fingerprint.digest,fingerprint.byte_length], |row| Ok((row.get(0)?,row.get(1)?))).optional().map_err(sql_error)?;
    if let Some((id, missing)) = existing {
        return Ok(if missing {
            ImportDisposition::RelinkCandidate(id)
        } else {
            ImportDisposition::Duplicate
        });
    }
    let metadata = read_metadata(path)?;
    let modified = file_modified(path)?;
    let timestamp = now();
    let identity = format!("v1:{}:{}", fingerprint.byte_length, fingerprint.digest);
    transaction.execute("INSERT INTO assets(source_path,source_path_normalized,source_identity,fingerprint_version,content_fingerprint,file_size,modified_time,file_type,width,height,orientation,capture_time,import_time,camera_make,camera_model,lens_make,lens_model,focal_length,aperture,shutter_speed,iso,rating,flag,color_label,missing,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,0,'unflagged','none',0,?,?)", params![path.to_string_lossy(),normalized,identity,FINGERPRINT_VERSION,fingerprint.digest,fingerprint.byte_length,modified,metadata.file_type,metadata.width,metadata.height,metadata.orientation,metadata.capture_time,timestamp,metadata.camera_make,metadata.camera_model,metadata.lens_make,metadata.lens_model,metadata.focal_length,metadata.aperture,metadata.shutter_speed,metadata.iso,timestamp,timestamp]).map_err(sql_error)?;
    Ok(ImportDisposition::Imported(transaction.last_insert_rowid()))
}

pub fn fingerprint_file(path: &Path) -> Result<AssetFingerprintV1, LibraryError> {
    let mut file = File::open(path).map_err(|error| LibraryError::FingerprintFailed {
        path: path.to_owned(),
        reason: error.to_string(),
    })?;
    let length = file
        .metadata()
        .map_err(|error| LibraryError::FingerprintFailed {
            path: path.to_owned(),
            reason: error.to_string(),
        })?
        .len();
    let mut hash = Sha256::new();
    hash.update(FINGERPRINT_VERSION.as_bytes());
    hash.update(length.to_le_bytes());
    let mut offsets = BTreeSet::new();
    if length > 0 {
        offsets.insert(0);
        offsets.insert(length / 4);
        offsets.insert(length / 2);
        offsets.insert(length.saturating_mul(3) / 4);
        offsets.insert(length.saturating_sub(SAMPLE_BYTES));
    }
    for offset in offsets {
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| LibraryError::FingerprintFailed {
                path: path.to_owned(),
                reason: error.to_string(),
            })?;
        let mut buffer =
            vec![0; usize::try_from(SAMPLE_BYTES.min(length.saturating_sub(offset))).unwrap_or(0)];
        file.read_exact(&mut buffer)
            .map_err(|error| LibraryError::FingerprintFailed {
                path: path.to_owned(),
                reason: error.to_string(),
            })?;
        hash.update(offset.to_le_bytes());
        hash.update(&buffer);
    }
    Ok(AssetFingerprintV1 {
        byte_length: length,
        digest: hex(hash.finalize()),
    })
}

pub fn full_sha256(path: &Path) -> Result<String, LibraryError> {
    let mut file = File::open(path).map_err(|error| LibraryError::FingerprintFailed {
        path: path.to_owned(),
        reason: error.to_string(),
    })?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| LibraryError::FingerprintFailed {
                path: path.to_owned(),
                reason: error.to_string(),
            })?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(hex(hash.finalize()))
}

fn read_metadata(path: &Path) -> Result<AssetMetadata, LibraryError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let decoded =
        decode_source_preview(path, 32).map_err(|error| LibraryError::MetadataReadFailed {
            path: path.to_owned(),
            reason: error.to_string(),
        })?;
    Ok(match decoded {
        DecodedSourceImage::Rendered(image) => {
            let lens = lens_metadata(&image);
            let dimensions = image::image_dimensions(path).ok();
            AssetMetadata {
                file_type: extension,
                width: dimensions.map(|value| value.0).or(Some(image.width)),
                height: dimensions.map(|value| value.1).or(Some(image.height)),
                orientation: None,
                capture_time: None,
                camera_make: some(lens.camera_make),
                camera_model: some(lens.camera_model),
                lens_make: some(lens.lens_make),
                lens_model: some(lens.lens_model),
                focal_length: lens.focal_length_mm,
                aperture: lens.aperture,
                shutter_speed: None,
                iso: None,
            }
        }
        DecodedSourceImage::Raw(image) => {
            let raw = &image.metadata;
            AssetMetadata {
                file_type: extension,
                width: Some(image.width),
                height: Some(image.height),
                orientation: Some(raw.orientation),
                capture_time: None,
                camera_make: some(raw.make.clone()),
                camera_model: some(raw.model.clone()),
                lens_make: some(raw.lens_make.clone()),
                lens_model: some(raw.lens_model.clone()),
                focal_length: finite_positive(raw.focal_length_mm),
                aperture: finite_positive(raw.aperture),
                shutter_speed: None,
                iso: None,
            }
        }
    })
}

fn finite_positive(value: f32) -> Option<f32> {
    if value.is_finite() && value > 0.0 {
        Some(value)
    } else {
        None
    }
}
fn some(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}
fn supported(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|v| v.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some(
            "jpg" | "jpeg" | "png" | "tif" | "tiff" | "nef" | "arw" | "cr2" | "cr3" | "dng" | "raf"
        )
    )
}
fn normalize_path(path: &Path) -> Result<String, LibraryError> {
    let absolute = path
        .canonicalize()
        .map_err(|error| LibraryError::MetadataReadFailed {
            path: path.to_owned(),
            reason: error.to_string(),
        })?;
    Ok(absolute.to_string_lossy().replace('\\', "/").to_lowercase())
}
fn normalize_keyword(value: &str) -> String {
    value.trim().to_lowercase()
}
fn file_modified(path: &Path) -> Result<i64, LibraryError> {
    let value = fs::metadata(path)
        .and_then(|m| m.modified())
        .map_err(|error| LibraryError::MetadataReadFailed {
            path: path.to_owned(),
            reason: error.to_string(),
        })?;
    Ok(system_time(value))
}
fn now() -> i64 {
    system_time(SystemTime::now())
}
fn system_time(value: SystemTime) -> i64 {
    value
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

fn build_where(
    query: &LibraryQuery,
) -> Result<(String, Vec<rusqlite::types::Value>), LibraryError> {
    use rusqlite::types::Value;
    let mut clauses = Vec::new();
    let mut values = Vec::new();
    let mut like = |column: &str, value: &Option<String>| {
        if let Some(value) = value.as_ref().map(|v| v.trim()).filter(|v| !v.is_empty()) {
            clauses.push(format!("LOWER({column}) LIKE ? ESCAPE '\\'"));
            values.push(Value::Text(format!(
                "%{}%",
                escape_like(&value.to_lowercase())
            )));
        }
    };
    like("a.source_path_normalized", &query.filename);
    like(
        "COALESCE(a.camera_make,'') || ' ' || COALESCE(a.camera_model,'')",
        &query.camera,
    );
    like(
        "COALESCE(a.lens_make,'') || ' ' || COALESCE(a.lens_model,'')",
        &query.lens,
    );
    if let Some(text) = query
        .text
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
    {
        clauses.push("(a.source_path_normalized LIKE ? ESCAPE '\\' OR LOWER(COALESCE(a.camera_make,'') || ' ' || COALESCE(a.camera_model,'')) LIKE ? ESCAPE '\\' OR EXISTS(SELECT 1 FROM asset_keywords ak JOIN keywords k ON k.id=ak.keyword_id WHERE ak.asset_id=a.id AND k.normalized_name LIKE ? ESCAPE '\\'))".into());
        let value = Value::Text(format!("%{}%", escape_like(&text.to_lowercase())));
        values.extend([value.clone(), value.clone(), value]);
    }
    if let Some(keyword) = query
        .keyword
        .as_ref()
        .map(|v| normalize_keyword(v))
        .filter(|v| !v.is_empty())
    {
        clauses.push("EXISTS(SELECT 1 FROM asset_keywords ak JOIN keywords k ON k.id=ak.keyword_id WHERE ak.asset_id=a.id AND k.normalized_name=?)".into());
        values.push(Value::Text(keyword));
    }
    if let Some(value) = query.minimum_rating {
        clauses.push("a.rating>=?".into());
        values.push(Value::Integer(i64::from(value)));
    }
    if let Some(value) = query.flag {
        clauses.push("a.flag=?".into());
        values.push(Value::Text(value.as_str().into()));
    }
    if let Some(value) = query.color_label {
        clauses.push("a.color_label=?".into());
        values.push(Value::Text(value.as_str().into()));
    }
    if !query.file_types.is_empty() {
        clauses.push(format!(
            "a.file_type IN ({})",
            vec!["?"; query.file_types.len()].join(",")
        ));
        values.extend(
            query
                .file_types
                .iter()
                .map(|v| Value::Text(v.to_lowercase())),
        );
    }
    for (column, value, op) in [
        ("a.iso", query.minimum_iso, ">="),
        ("a.iso", query.maximum_iso, "<="),
    ] {
        if let Some(value) = value {
            if !value.is_finite() {
                return Err(LibraryError::InvalidQuery("ISO must be finite".into()));
            }
            clauses.push(format!("{column}{op}?"));
            values.push(Value::Real(f64::from(value)));
        }
    }
    for (column, value, op) in [
        ("a.capture_time", query.capture_from, ">="),
        ("a.capture_time", query.capture_to, "<="),
    ] {
        if let Some(value) = value {
            clauses.push(format!("{column}{op}?"));
            values.push(Value::Integer(value));
        }
    }
    if let Some(value) = query.missing {
        clauses.push("a.missing=?".into());
        values.push(Value::Integer(i64::from(value)));
    }
    Ok((
        if clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", clauses.join(" AND "))
        },
        values,
    ))
}
fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn rule_to_query(
    rule: &SmartCollectionRuleV1,
    limit: u32,
    offset: u32,
) -> Result<LibraryQuery, LibraryError> {
    let mut query = LibraryQuery {
        limit,
        offset,
        ..Default::default()
    };
    for predicate in &rule.all {
        match predicate {
            SmartPredicate::Rating { minimum } => query.minimum_rating = Some(*minimum),
            SmartPredicate::Flag { value } => query.flag = Some(*value),
            SmartPredicate::ColorLabel { value } => query.color_label = Some(*value),
            SmartPredicate::Camera { value } => query.camera = Some(value.clone()),
            SmartPredicate::Lens { value } => query.lens = Some(value.clone()),
            SmartPredicate::FileType { value } => query.file_types.push(value.clone()),
            SmartPredicate::IsoRange { minimum, maximum } => {
                query.minimum_iso = *minimum;
                query.maximum_iso = *maximum
            }
            SmartPredicate::CaptureDate { from, to } => {
                query.capture_from = *from;
                query.capture_to = *to
            }
            SmartPredicate::Keyword { value } => query.keyword = Some(value.clone()),
        }
    }
    Ok(query)
}

fn row_to_asset(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetRecord> {
    Ok(AssetRecord {
        id: row.get(0)?,
        source_path: PathBuf::from(row.get::<_, String>(1)?),
        source_identity: row.get(2)?,
        content_fingerprint: row.get(3)?,
        file_size: row.get(4)?,
        modified_time: row.get(5)?,
        metadata: AssetMetadata {
            file_type: row.get(6)?,
            width: row.get(7)?,
            height: row.get(8)?,
            orientation: row.get(9)?,
            capture_time: row.get(10)?,
            camera_make: row.get(11)?,
            camera_model: row.get(12)?,
            lens_make: row.get(13)?,
            lens_model: row.get(14)?,
            focal_length: row.get(15)?,
            aperture: row.get(16)?,
            shutter_speed: row.get(17)?,
            iso: row.get(18)?,
        },
        rating: row.get(19)?,
        flag: AssetFlag::parse(&row.get::<_, String>(20)?),
        color_label: ColorLabel::parse(&row.get::<_, String>(21)?),
        missing: row.get(22)?,
        project_reference: row.get(23)?,
        thumbnail_cache_key: row.get(24)?,
        keywords: Vec::new(),
    })
}

const MIGRATION_V1: &str = r#"
CREATE TABLE library_settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
INSERT INTO library_settings(key,value) VALUES('schema_name','starroom-library'),('schema_version','1');
CREATE TABLE assets(id INTEGER PRIMARY KEY,source_path TEXT NOT NULL,source_path_normalized TEXT NOT NULL UNIQUE,source_identity TEXT NOT NULL,fingerprint_version TEXT NOT NULL,content_fingerprint TEXT NOT NULL,file_size INTEGER NOT NULL,modified_time INTEGER NOT NULL,file_type TEXT NOT NULL,width INTEGER,height INTEGER,orientation INTEGER,capture_time INTEGER,import_time INTEGER NOT NULL,camera_make TEXT,camera_model TEXT,lens_make TEXT,lens_model TEXT,focal_length REAL,aperture REAL,shutter_speed REAL,iso REAL,rating INTEGER NOT NULL CHECK(rating BETWEEN 0 AND 5),flag TEXT NOT NULL CHECK(flag IN ('unflagged','pick','reject')),color_label TEXT NOT NULL CHECK(color_label IN ('none','red','yellow','green','blue','purple')),missing INTEGER NOT NULL CHECK(missing IN (0,1)),project_reference TEXT,thumbnail_cache_key TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);
CREATE INDEX assets_fingerprint ON assets(fingerprint_version,content_fingerprint,file_size);CREATE INDEX assets_workflow ON assets(rating,flag,color_label);CREATE INDEX assets_capture ON assets(capture_time,id);CREATE INDEX assets_camera ON assets(camera_make,camera_model);CREATE INDEX assets_lens ON assets(lens_make,lens_model);
CREATE TABLE keywords(id INTEGER PRIMARY KEY,name TEXT NOT NULL,normalized_name TEXT NOT NULL UNIQUE);
CREATE TABLE asset_keywords(asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,keyword_id INTEGER NOT NULL REFERENCES keywords(id) ON DELETE CASCADE,UNIQUE(asset_id,keyword_id));
CREATE TABLE collections(id INTEGER PRIMARY KEY,name TEXT NOT NULL,type TEXT NOT NULL CHECK(type IN ('normal','smart')),created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);
CREATE TABLE collection_assets(id INTEGER PRIMARY KEY,collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,position INTEGER NOT NULL,UNIQUE(collection_id,asset_id));
CREATE TABLE smart_collection_rules(collection_id INTEGER PRIMARY KEY REFERENCES collections(id) ON DELETE CASCADE,schema_version INTEGER NOT NULL,rule_json TEXT NOT NULL CHECK(json_valid(rule_json)));
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, time::Instant};
    fn temp(name: &str) -> PathBuf {
        let path = env::temp_dir().join(format!("starroom-library-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn png(path: &Path, color: [u8; 3]) {
        let image = image::RgbImage::from_pixel(4, 3, image::Rgb(color));
        image.save(path).unwrap();
    }

    #[test]
    fn init_reopen_migration_and_pragmas() {
        let root = temp("init");
        let db = root.join("library.sqlite");
        {
            let library = Library::open(&db).unwrap();
            assert_eq!(library.schema_version().unwrap(), 1);
            assert_eq!(
                library
                    .connection
                    .query_row::<i64, _, _>("PRAGMA foreign_keys", [], |r| r.get(0))
                    .unwrap(),
                1
            );
        }
        let library = Library::open(&db).unwrap();
        assert_eq!(library.schema_version().unwrap(), 1);
    }

    #[test]
    fn import_duplicate_workflow_keyword_query_and_collections() {
        let root = temp("workflow");
        let a = root.join("A.png");
        let b = root.join("B.png");
        png(&a, [30, 40, 50]);
        fs::copy(&a, &b).unwrap();
        let mut library = Library::open(root.join("db.sqlite")).unwrap();
        let first = library
            .import_paths(std::slice::from_ref(&a), &AtomicBool::new(false))
            .unwrap();
        let id = first.imported[0];
        let same = library
            .import_paths(std::slice::from_ref(&a), &AtomicBool::new(false))
            .unwrap();
        assert_eq!(same.already_present.len(), 1);
        let duplicate = library
            .import_paths(std::slice::from_ref(&b), &AtomicBool::new(false))
            .unwrap();
        assert_eq!(duplicate.duplicates.len(), 1);
        library
            .set_workflow(&[id], Some(5), Some(AssetFlag::Pick), Some(ColorLabel::Red))
            .unwrap();
        library
            .add_keywords(&[id], &[" Japan ".into(), "japan".into(), "JAPAN".into()])
            .unwrap();
        let found = library
            .query(&LibraryQuery {
                minimum_rating: Some(5),
                flag: Some(AssetFlag::Pick),
                color_label: Some(ColorLabel::Red),
                keyword: Some("jApAn".into()),
                sort: SortField::Filename,
                direction: SortDirection::Ascending,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].keywords, vec!["Japan"]);
        let normal = library
            .create_collection("Trip", CollectionKind::Normal, None)
            .unwrap();
        library.add_collection_assets(normal, &[id]).unwrap();
        assert_eq!(library.collection_assets(normal, 10, 0).unwrap().len(), 1);
        let smart = library
            .create_collection(
                "Best",
                CollectionKind::Smart,
                Some(&SmartCollectionRuleV1 {
                    all: vec![
                        SmartPredicate::Rating { minimum: 5 },
                        SmartPredicate::Keyword {
                            value: "japan".into(),
                        },
                    ],
                }),
            )
            .unwrap();
        assert_eq!(library.collection_assets(smart, 10, 0).unwrap().len(), 1);
        library.set_workflow(&[id], Some(2), None, None).unwrap();
        assert!(library.collection_assets(smart, 10, 0).unwrap().is_empty());
    }

    #[test]
    fn missing_relink_identity_and_project_relationship() {
        let root = temp("relink");
        let source = root.join("source.png");
        png(&source, [1, 2, 3]);
        let moved = root.join("moved.png");
        let wrong = root.join("wrong.png");
        png(&wrong, [4, 5, 6]);
        let mut library = Library::open(root.join("db.sqlite")).unwrap();
        let id = library
            .import_paths(std::slice::from_ref(&source), &AtomicBool::new(false))
            .unwrap()
            .imported[0];
        library
            .set_project_reference(id, Some("projects/edit.starroom.json"))
            .unwrap();
        fs::rename(&source, &moved).unwrap();
        assert_eq!(library.refresh_missing().unwrap(), 1);
        assert!(library.asset(id).unwrap().unwrap().missing);
        assert!(matches!(
            library.relink(id, &wrong),
            Err(LibraryError::RelinkMismatch(_))
        ));
        library.relink(id, &moved).unwrap();
        let asset = library.asset(id).unwrap().unwrap();
        assert!(!asset.missing);
        assert_eq!(
            asset.project_reference.as_deref(),
            Some("projects/edit.starroom.json")
        );
    }

    #[test]
    fn cancellation_rollback_thumbnail_identity_and_stable_pagination() {
        let root = temp("page");
        let mut paths = Vec::new();
        for index in 0..40 {
            let path = root.join(format!("{index:03}.png"));
            png(&path, [index, 0, 0]);
            paths.push(path);
        }
        let mut library = Library::open(root.join("db.sqlite")).unwrap();
        let cancelled = AtomicBool::new(true);
        let result = library.import_paths(&paths, &cancelled).unwrap();
        assert!(result.cancelled);
        assert!(library.query(&LibraryQuery::default()).unwrap().is_empty());
        let result = library
            .import_paths(&paths, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(result.imported.len(), 40);
        let first = library
            .query(&LibraryQuery {
                sort: SortField::Filename,
                direction: SortDirection::Ascending,
                limit: 10,
                offset: 0,
                ..Default::default()
            })
            .unwrap();
        let second = library
            .query(&LibraryQuery {
                sort: SortField::Filename,
                direction: SortDirection::Ascending,
                limit: 10,
                offset: 10,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(first.len(), 10);
        assert_eq!(second.len(), 10);
        assert!(first.iter().all(|a| !second.iter().any(|b| a.id == b.id)));
        let key = Library::thumbnail_identity(&first[0], ThumbnailSize::Medium512);
        assert_eq!(
            key,
            Library::thumbnail_identity(&first[0], ThumbnailSize::Medium512)
        );
        assert_ne!(
            key,
            Library::thumbnail_identity(&first[0], ThumbnailSize::Small256)
        );
    }

    #[test]
    fn fingerprint_is_rename_invariant_and_content_sensitive() {
        let root = temp("fingerprint");
        let first = root.join("a.bin");
        let second = root.join("b.bin");
        fs::write(&first, vec![7u8; 400_000]).unwrap();
        let before = fingerprint_file(&first).unwrap();
        fs::rename(&first, &second).unwrap();
        assert_eq!(before, fingerprint_file(&second).unwrap());
        let mut bytes = fs::read(&second).unwrap();
        bytes[200_000] = 8;
        fs::write(&second, bytes).unwrap();
        assert_ne!(before, fingerprint_file(&second).unwrap());
    }

    #[test]
    fn ten_thousand_metadata_assets_search_sort_and_page() {
        let root = temp("ten-thousand");
        let mut library = Library::open(root.join("db.sqlite")).unwrap();
        let started = Instant::now();
        let transaction = library.connection.transaction().unwrap();
        {
            let mut insert = transaction.prepare("INSERT INTO assets(source_path,source_path_normalized,source_identity,fingerprint_version,content_fingerprint,file_size,modified_time,file_type,width,height,orientation,capture_time,import_time,camera_make,camera_model,lens_make,lens_model,focal_length,aperture,shutter_speed,iso,rating,flag,color_label,missing,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").unwrap();
            for index in 0..10_000_i64 {
                let path = format!("C:/synthetic/{index:05}.jpg");
                insert
                    .execute(params![
                        &path,
                        &path,
                        format!("asset-{index}"),
                        FINGERPRINT_VERSION,
                        format!("fingerprint-{index}"),
                        1_000_i64,
                        index,
                        "jpeg",
                        6000_i64,
                        4000_i64,
                        1_i64,
                        index,
                        index,
                        "Synthetic",
                        "Camera",
                        "Synthetic",
                        "Lens",
                        50.0_f64,
                        4.0_f64,
                        0.01_f64,
                        100.0_f64,
                        (index % 6),
                        "unflagged",
                        "none",
                        0_i64,
                        index,
                        index
                    ])
                    .unwrap();
            }
        }
        transaction.commit().unwrap();
        let page = library
            .query(&LibraryQuery {
                text: Some("synthetic".into()),
                sort: SortField::Filename,
                direction: SortDirection::Descending,
                limit: 200,
                offset: 9_800,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.len(), 200);
        assert!(page.windows(2).all(|pair| pair[0].id > pair[1].id));
        eprintln!(
            "M24 10,000 metadata rows insert+query: {:.3} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}
