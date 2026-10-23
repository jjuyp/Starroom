fn main() {
    let root = std::path::Path::new("../../vendor/lcms-fast-float-2.19");
    let include = std::env::var("DEP_LCMS2_INCLUDE")
        .expect("pinned static lcms2-sys must expose its matching headers");
    let mut build = cc::Build::new();
    build.include(include).include(root.join("include"));
    for name in [
        "fast_16_tethra",
        "fast_8_curves",
        "fast_8_matsh",
        "fast_8_matsh_sse",
        "fast_8_tethra",
        "fast_float_15bits",
        "fast_float_15mats",
        "fast_float_cmyk",
        "fast_float_curves",
        "fast_float_lab",
        "fast_float_matsh",
        "fast_float_separate",
        "fast_float_sup",
        "fast_float_tethra",
    ] {
        let path = root.join("src").join(format!("{name}.c"));
        println!("cargo:rerun-if-changed={}", path.display());
        build.file(path);
    }
    println!("cargo:rerun-if-changed={}", root.join("include").display());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("src/fast_float_internal.h").display()
    );
    build.compile("starroom_lcms_fast_float");
}
