use image::{Rgb, RgbImage};
use serde_json::json;
use starroom_export::{
    ExportFormat, ExportRequest, ExportSettings, NativeSharedGraphRenderer, export_one,
};
use starroom_history::{EditHistory, HistoryError};
use starroom_library::{Library, LibraryError, ThumbnailSize};
use starroom_pipeline::RenderSettings;
use starroom_session::{SessionError, open};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

fn root(name: &str) -> PathBuf {
    let value = std::env::temp_dir().join(format!("starroom-m30-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&value);
    fs::create_dir_all(&value).expect("create M30 test root");
    value
}

fn source(path: &Path) {
    let mut image = RgbImage::new(32, 24);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = Rgb([(x * 7) as u8, (y * 9) as u8, ((x + y) * 4) as u8]);
    }
    image.save(path).expect("save source fixture");
}

fn request(root: &Path, source_path: &Path) -> ExportRequest {
    ExportRequest {
        asset_id: 1,
        source_path: source_path.to_owned(),
        destination_directory: root.join("exports"),
        original_name: "release.png".into(),
        capture_date: None,
        rating: 0,
        keywords: Vec::new(),
        camera: None,
        look: None,
        sequence: 1,
        source_fingerprint: "m30-fixture".into(),
        edit_state_identity: "m30-release-recovery".into(),
        settings: ExportSettings {
            format: ExportFormat::Jpeg,
            filename_template: "release-recovered".into(),
            ..ExportSettings::default()
        },
    }
}

#[test]
fn corrupt_session_and_history_are_typed_without_overwrite() {
    let root = root("corrupt-state");
    let session = root.join("session.json");
    let history = root.join("history.json");
    fs::write(&session, b"{not-json").unwrap();
    fs::write(&history, b"{not-json").unwrap();

    assert!(matches!(open(&session), Err(SessionError::Invalid(_))));
    assert!(matches!(
        EditHistory::load(&history),
        Err(HistoryError::HistoryCorrupt(_))
    ));
    assert_eq!(fs::read(&session).unwrap(), b"{not-json");
    assert_eq!(fs::read(&history).unwrap(), b"{not-json");
}

#[test]
fn missing_source_and_thumbnail_are_typed_and_database_survives_reopen() {
    let root = root("missing-thumbnail");
    let source_path = root.join("source.png");
    source(&source_path);
    let database = root.join("library.sqlite");
    let asset_id = {
        let mut library = Library::open(&database).unwrap();
        library
            .import_paths(std::slice::from_ref(&source_path), &AtomicBool::new(false))
            .unwrap()
            .imported[0]
    };
    fs::remove_file(&source_path).unwrap();

    let library = Library::open(&database).unwrap();
    assert!(matches!(
        library.generate_thumbnail(asset_id, root.join("thumbnails"), ThumbnailSize::Small256),
        Err(LibraryError::MissingSource(id)) if id == asset_id
    ));
    assert!(library.asset(asset_id).unwrap().is_some());
}

#[test]
fn incomplete_export_temp_does_not_replace_or_block_atomic_output() {
    let root = root("incomplete-export");
    let source_path = root.join("source.png");
    source(&source_path);
    let export_directory = root.join("exports");
    fs::create_dir_all(&export_directory).unwrap();
    let stale = export_directory.join("release-recovered.jpg.999999.starroom-tmp");
    fs::write(&stale, b"incomplete previous process output").unwrap();

    let result = export_one(
        &NativeSharedGraphRenderer,
        &request(&root, &source_path),
        &RenderSettings::default(),
        &AtomicBool::new(false),
    )
    .expect("valid export must not consume an incomplete temporary file");
    let destination = result.destination.expect("completed destination");

    assert!(destination.is_file());
    assert_ne!(fs::read(&destination).unwrap(), fs::read(&stale).unwrap());
    assert!(image::open(destination).is_ok());
    assert_eq!(
        fs::read(&stale).unwrap(),
        b"incomplete previous process output"
    );
}

#[test]
fn release_history_round_trip_remains_deterministic() {
    let root = root("history-round-trip");
    let path = root.join("history.json");
    let history = EditHistory::new(json!({"exposure": 0.0, "layers": [], "masks": []})).unwrap();
    let identity = history.state_version();
    history.persist(&path).unwrap();
    let loaded = EditHistory::load(&path).unwrap();
    assert_eq!(loaded.state_version(), identity);
    assert_eq!(loaded.state(), history.state());
}
