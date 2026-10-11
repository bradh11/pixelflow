use std::env;
use std::path::Path;

fn main() {
    // Windows only starts a program that uses the system's dialogs if the program carries a
    // manifest asking for version 6 of the common controls. tauri-build gives the app's own
    // executable that manifest, but not the executable `cargo test` builds from the library,
    // which then never starts (STATUS_ENTRYPOINT_NOT_FOUND). So in debug builds, which the tests
    // are, the linker puts the same manifest into every executable of this crate instead, and
    // tauri-build leaves it out. Release builds, the ones that ship, are tauri-build's own.
    let target = |name: &str| env::var(format!("CARGO_CFG_TARGET_{name}")).unwrap_or_default();
    let debug = env::var("PROFILE").is_ok_and(|profile| profile == "debug");
    if target("OS") == "windows" && target("ENV") == "msvc" && debug {
        let manifest = Path::new(&env::var("CARGO_MANIFEST_DIR").expect("set by cargo"))
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        let attributes = tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        tauri_build::try_build(attributes).expect("failed to run tauri-build");
    } else {
        tauri_build::build();
    }
}
