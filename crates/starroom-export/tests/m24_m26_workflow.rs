use image::{Rgb, RgbImage};
use serde_json::json;
use starroom_export::{
    ExportFormat, ExportRequest, ExportSettings, NativeSharedGraphRenderer, export_one,
};
use starroom_history::{EditCommand, EditHistory};
use starroom_library::{CollectionKind, Library, SmartCollectionRuleV1, SmartPredicate};
use starroom_pipeline::RenderSettings;
use starroom_session::{SessionState, autosave, mark_clean, open};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

fn root(name: &str) -> PathBuf {
    let value =
        std::env::temp_dir().join(format!("starroom-m24-m26-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&value);
    fs::create_dir_all(&value).expect("create test root");
    value
}

fn source(path: &Path) {
    let mut image = RgbImage::new(48, 32);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = Rgb([(x * 5) as u8, (y * 7) as u8, ((x + y) * 3) as u8]);
    }
    image.save(path).expect("save source");
}

fn request(root: &Path, source: &Path, asset_id: i64, state: &str) -> ExportRequest {
    ExportRequest {
        asset_id,
        source_path: source.to_owned(),
        destination_directory: root.join("exports"),
        original_name: "kyoto.png".into(),
        capture_date: Some("2026-08-24".into()),
        rating: 5,
        keywords: vec!["workflow".into()],
        camera: None,
        look: Some("Warm".into()),
        sequence: 1,
        source_fingerprint: "fixture-fingerprint".into(),
        edit_state_identity: state.into(),
        settings: ExportSettings {
            format: ExportFormat::Jpeg,
            filename_template: "Kyoto_{date}_{sequence}_{rating}_{look}".into(),
            ..ExportSettings::default()
        },
    }
}

#[test]
fn scenario_a_library_history_snapshot_export_return() {
    let root = root("scenario-a");
    let source_path = root.join("kyoto.png");
    source(&source_path);
    let mut library = Library::open(root.join("library.sqlite")).expect("library");
    let asset_id = library
        .import_paths(std::slice::from_ref(&source_path), &AtomicBool::new(false))
        .expect("import")
        .imported[0];
    library
        .set_workflow(&[asset_id], Some(5), None, None)
        .expect("rating");
    library
        .add_keywords(&[asset_id], &["Kyoto".into()])
        .expect("keyword");
    let smart = library
        .create_collection(
            "Kyoto Five",
            CollectionKind::Smart,
            Some(&SmartCollectionRuleV1 {
                all: vec![
                    SmartPredicate::Rating { minimum: 5 },
                    SmartPredicate::Keyword {
                        value: "kyoto".into(),
                    },
                ],
            }),
        )
        .expect("smart collection");
    assert_eq!(library.collection_assets(smart, 20, 0).unwrap().len(), 1);

    let mut history = EditHistory::new(json!({"exposure": 0.0, "look": null})).unwrap();
    let before_look = history.state().clone();
    history
        .commit(
            "Apply Warm Look",
            "look",
            EditCommand::ReplaceState {
                before: before_look,
                after: json!({"exposure": 0.5, "look": "Warm"}),
            },
        )
        .unwrap();
    let snapshot = history.create_snapshot("Warm").unwrap();
    library
        .set_project_reference(asset_id, Some("projects/kyoto.starroom.json"))
        .unwrap();
    let mut render_settings = RenderSettings::default();
    render_settings.tone.exposure_ev = 0.5;
    let result = export_one(
        &NativeSharedGraphRenderer,
        &request(&root, &source_path, asset_id, &history.state_version().0),
        &render_settings,
        &AtomicBool::new(false),
    )
    .expect("full native export");
    let first_export = result.destination.unwrap();
    assert!(first_export.is_file());
    assert_eq!(history.snapshot_state(&snapshot).unwrap()["look"], "Warm");
    assert_eq!(
        library
            .asset(asset_id)
            .unwrap()
            .unwrap()
            .project_reference
            .as_deref(),
        Some("projects/kyoto.starroom.json")
    );

    let session_path = root.join("session.json");
    let session = SessionState {
        version: 1,
        workspace: "edit".into(),
        selected_asset_id: Some(asset_id),
        selected_source_path: Some(source_path.clone()),
        active_tool: "masks".into(),
        library_panel_open: true,
        filmstrip_open: true,
        zoom_mode: "fit".into(),
        zoom_scale: 1.0,
        library_context: "collection:Kyoto Five".into(),
    };
    autosave(&session_path, &session).unwrap();
    assert!(open(&session_path).unwrap().recovery_available);
    mark_clean(&session_path, &session).unwrap();
    assert_eq!(open(&session_path).unwrap().state, Some(session));

    let second = export_one(
        &NativeSharedGraphRenderer,
        &request(&root, &source_path, asset_id, &history.state_version().0),
        &render_settings,
        &AtomicBool::new(false),
    )
    .expect("deterministic export after normal session reopen")
    .destination
    .unwrap();
    assert_eq!(fs::read(first_export).unwrap(), fs::read(second).unwrap());
}

#[test]
fn scenario_b_missing_relink_preserves_history_and_exports() {
    let root = root("scenario-b");
    let original = root.join("original.png");
    let moved = root.join("moved.png");
    source(&original);
    let mut library = Library::open(root.join("library.sqlite")).unwrap();
    let asset_id = library
        .import_paths(std::slice::from_ref(&original), &AtomicBool::new(false))
        .unwrap()
        .imported[0];
    let mut history = EditHistory::new(json!({"layers": [], "masks": []})).unwrap();
    history.create_snapshot("Before move").unwrap();
    history.persist(root.join("history.json")).unwrap();
    fs::rename(&original, &moved).unwrap();
    assert_eq!(library.refresh_missing().unwrap(), 1);
    library.relink(asset_id, &moved).unwrap();
    let loaded = EditHistory::load(root.join("history.json")).unwrap();
    assert_eq!(loaded.snapshots().len(), 1);
    assert!(
        export_one(
            &NativeSharedGraphRenderer,
            &request(&root, &moved, asset_id, &loaded.state_version().0),
            &RenderSettings::default(),
            &AtomicBool::new(false)
        )
        .is_ok()
    );
}

#[test]
fn scenario_c_undo_redo_changes_recipe_deterministically() {
    let root = root("scenario-c");
    let source_path = root.join("source.png");
    source(&source_path);
    let mut history = EditHistory::new(json!({"exposure": 0.0})).unwrap();
    history
        .commit(
            "Edit B",
            "tone",
            EditCommand::ReplaceState {
                before: json!({"exposure": 0.0}),
                after: json!({"exposure": 1.0}),
            },
        )
        .unwrap();
    let b = history.state_version().0;
    history.undo().unwrap();
    let a = history.state_version().0;
    assert_ne!(a, b);
    history.redo().unwrap();
    assert_eq!(history.state_version().0, b);
    let b_request = request(&root, &source_path, 1, &b);
    assert_eq!(
        starroom_export::export_recipe_identity(&b_request).unwrap(),
        starroom_export::export_recipe_identity(&b_request).unwrap()
    );
}
