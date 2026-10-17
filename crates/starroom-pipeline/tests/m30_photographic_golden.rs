use serde_json::Value;
use starroom_imageio::decode_source_preview;
use starroom_pipeline::{
    RenderSettings, render_source_export_to_srgb_f32, render_source_export_to_srgb8,
    render_source_preview_to_srgb8,
};
use std::{collections::BTreeSet, fs, path::PathBuf};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("pipeline crate lives under the repository crates directory")
        .to_path_buf()
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn m30_active_photographic_golden_assets_use_the_shared_native_graph() {
    let root = repository_root();
    let manifest: Value = serde_json::from_slice(
        &fs::read(root.join("fixtures/golden/manifest.json")).expect("read Golden manifest"),
    )
    .expect("parse Golden manifest");
    let cases = manifest["cases"].as_array().expect("Golden cases");
    let assets = manifest["assets"].as_array().expect("Golden assets");
    let active_assets: BTreeSet<&str> = cases
        .iter()
        .filter(|case| case["status"] == "active" && case["fixtureKind"] == "photographic")
        .map(|case| case["assetId"].as_str().expect("photographic asset id"))
        .collect();
    assert!(
        active_assets.len() >= 5,
        "M30 requires a real photographic corpus"
    );

    for asset_id in active_assets {
        let asset = assets
            .iter()
            .find(|asset| asset["id"] == asset_id)
            .expect("case asset exists");
        let source = root.join("fixtures/golden").join(
            asset["path"]
                .as_str()
                .expect("Golden asset has a repository path"),
        );
        let immutable_source = fs::read(&source).expect("read immutable Golden source");
        let decoded = decode_source_preview(&source, 1024).expect("decode Golden source");

        let identity = RenderSettings {
            image_identity: asset_id.to_owned(),
            ..RenderSettings::default()
        };
        let preview = render_source_preview_to_srgb8(&decoded, &identity)
            .expect("identity Native preview succeeds");
        let export = render_source_export_to_srgb8(&decoded, &identity)
            .expect("identity Native export succeeds");
        assert_eq!(preview.width, export.width, "{asset_id} width parity");
        assert_eq!(preview.height, export.height, "{asset_id} height parity");
        assert_eq!(
            preview.data, export.data,
            "{asset_id} preview/export parity"
        );

        let float_export = render_source_export_to_srgb_f32(&decoded, &identity)
            .expect("high-precision shared graph succeeds");
        assert!(
            float_export.data.iter().all(|channel| channel.is_finite()),
            "{asset_id} produced NaN/Inf"
        );

        let mut edited = identity.clone();
        edited.tone.exposure_ev = 0.75;
        edited.tone.highlights = -0.7;
        edited.tone.shadows = 0.65;
        edited.tone.whites = 0.3;
        edited.tone.blacks = -0.25;
        edited.relative_color.temperature = 0.4;
        edited.relative_color.tint = -0.25;
        edited.local_detail.texture = 0.25;
        edited.sharpen.amount = 0.55;
        let edited_a = render_source_export_to_srgb8(&decoded, &edited)
            .expect("extreme-control Native render succeeds");
        let edited_b = render_source_export_to_srgb8(&decoded, &edited)
            .expect("repeated Native render succeeds");
        assert_eq!(
            edited_a.data, edited_b.data,
            "{asset_id} is not deterministic"
        );
        assert_eq!(
            fnv1a64(&edited_a.data),
            fnv1a64(&edited_b.data),
            "{asset_id} deterministic fingerprint"
        );
        let changed = preview
            .data
            .iter()
            .zip(&edited_a.data)
            .filter(|(before, after)| before != after)
            .count();
        assert!(
            changed > preview.data.len() / 100,
            "{asset_id} extreme controls did not materially affect the photograph"
        );
        assert_eq!(
            immutable_source,
            fs::read(&source).expect("reread immutable Golden source"),
            "{asset_id} source was modified"
        );
    }
}
