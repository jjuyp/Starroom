use std::{env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let database = manifest.join("data/lensfun-v0.3.4/db");
    println!("cargo:rerun-if-changed={}", database.display());
    let mut paths: Vec<_> = fs::read_dir(&database)
        .expect("Lensfun database directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "xml"))
        .collect();
    paths.sort();
    let mut generated = String::from("pub const LENSFUN_XML: &[(&str, &str)] = &[\n");
    for path in paths {
        let name = path.file_name().expect("filename").to_string_lossy();
        generated.push_str(&format!("    ({name:?}, include_str!({path:?})),\n"));
    }
    generated.push_str("];\n");
    let output = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("lensfun_db.rs");
    fs::write(output, generated).expect("generate embedded Lensfun database");
}
