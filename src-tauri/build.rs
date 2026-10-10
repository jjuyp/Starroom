fn main() {
    // Cargo's restored target cache must never make Tauri reuse legal/resources from an older
    // release candidate. Explicit fingerprints force resource staging to follow the current HEAD.
    for resource in [
        "../LICENSE",
        "../THIRD_PARTY_NOTICES.md",
        "../THIRD_PARTY_LICENSES.txt",
        "../NOTICE.md",
        "../MODEL_PROVENANCE.md",
        "../licenses/models/BiRefNet-LICENSE.txt",
        "../docs/17_THIRD_PARTY_PROVENANCE.md",
        "../docs/36_M30_DEPENDENCY_LICENSE_REPORT.json",
        "../release-models/BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx",
    ] {
        println!("cargo:rerun-if-changed={resource}");
    }
    tauri_build::build()
}
