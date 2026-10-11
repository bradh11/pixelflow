//! Records what built the benchmark (compiler, profile, target), for the machine details in its
//! reports.

use std::process::Command;

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let version = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|text| text.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=PF_BENCH_RUSTC={version}");
    for (name, var) in [
        ("PF_BENCH_PROFILE", "PROFILE"),
        ("PF_BENCH_OPT_LEVEL", "OPT_LEVEL"),
        ("PF_BENCH_TARGET", "TARGET"),
    ] {
        let value = std::env::var(var).unwrap_or_else(|_| "unknown".to_string());
        println!("cargo:rustc-env={name}={value}");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
