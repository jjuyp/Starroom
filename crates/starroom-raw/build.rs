use std::{env, fs, path::PathBuf};

fn collect_cpp(directory: PathBuf, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("read vendored LibRaw source directory") {
        let path = entry.expect("read vendored LibRaw source entry").path();
        if path.is_dir() {
            collect_cpp(path, files);
        } else if path.extension().and_then(|value| value.to_str()) == Some("cpp")
            && !path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.ends_with("_ph.cpp"))
        {
            files.push(path);
        }
    }
}

fn main() {
    let crate_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let vendor = crate_dir.join("../../vendor/libraw-0.22.2");
    let mut sources = Vec::new();
    collect_cpp(vendor.join("src"), &mut sources);
    sources.sort();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .include(&vendor)
        .define("LIBRAW_NODLL", None)
        .define("LIBRAW_BUILDLIB", None)
        .define("_CRT_SECURE_NO_WARNINGS", None)
        .warnings(false)
        .extra_warnings(false)
        .files(sources)
        .file(crate_dir.join("src/libraw_bridge.cpp"));

    let compiler = build.get_compiler();
    if compiler.is_like_msvc() {
        build.flag("/EHsc").flag("/std:c++17");
    } else {
        build
            .flag_if_supported("-std=c++17")
            .flag_if_supported("-Wno-unused-result")
            .flag_if_supported("-Wno-format-truncation")
            .flag_if_supported("-pthread");
    }
    build.compile("starroom_libraw");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=ws2_32");
    }
    println!("cargo:rerun-if-changed=src/libraw_bridge.cpp");
    println!("cargo:rerun-if-changed={}", vendor.display());
}
