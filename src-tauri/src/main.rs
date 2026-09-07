fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let command = arguments.next();
    if command.as_deref() == Some(std::ffi::OsStr::new("--release-ai-self-test")) {
        match starroom_desktop_lib::release_ai_self_test() {
            Ok(report) => {
                println!(
                    "{}",
                    serde_json::to_string(&report).expect("serialize AI self-test report")
                );
                return;
            }
            Err(error) => {
                eprintln!("Starroom release AI self-test failed: {error}");
                std::process::exit(1);
            }
        }
    }
    if command.as_deref() == Some(std::ffi::OsStr::new("--release-self-test")) {
        let Some(root) = arguments.next() else {
            eprintln!("--release-self-test requires an empty output directory");
            std::process::exit(2);
        };
        match starroom_desktop_lib::release_self_test(std::path::Path::new(&root)) {
            Ok(report) => {
                println!(
                    "{}",
                    serde_json::to_string(&report).expect("serialize self-test report")
                );
                return;
            }
            Err(error) => {
                eprintln!("Starroom release self-test failed: {error}");
                std::process::exit(1);
            }
        }
    }
    starroom_desktop_lib::run();
}
