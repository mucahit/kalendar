use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=KALENDAR_SKIP_SWIFT_BUILD");
    println!("cargo:rerun-if-changed=../../native/macos-calendar-bridge/Package.swift");
    println!("cargo:rerun-if-changed=../../native/macos-calendar-bridge/Sources");
    println!(
        "cargo:rerun-if-changed=../../native/macos-calendar-bridge/Sources/KalendarBridge/Info.plist"
    );
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos")
        || env::var_os("KALENDAR_SKIP_SWIFT_BUILD").is_some()
    {
        return;
    }
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let package = manifest.join("../../native/macos-calendar-bridge");
    let module_cache = package.join(".build/module-cache");
    fs::create_dir_all(&module_cache).expect("creating Swift module cache");
    let target = env::var("TARGET").expect("Cargo target triple");
    let (swift_triple, swift_directory) = match target.as_str() {
        "aarch64-apple-darwin" => ("arm64-apple-macosx13.0", "arm64-apple-macosx"),
        "x86_64-apple-darwin" => ("x86_64-apple-macosx13.0", "x86_64-apple-macosx"),
        _ => panic!("unsupported macOS target for EventKit bridge: {target}"),
    };
    let status = Command::new("swift")
        .args([
            "build",
            "--disable-sandbox",
            "-c",
            "release",
            "--triple",
            swift_triple,
            "--package-path",
        ])
        .arg(&package)
        .env("CLANG_MODULE_CACHE_PATH", &module_cache)
        .env("SWIFTPM_MODULECACHE_OVERRIDE", &module_cache)
        .status()
        .expect("Swift is required to build the EventKit bridge on macOS");
    assert!(status.success(), "building the EventKit bridge failed");
    let source = package
        .join(".build")
        .join(swift_directory)
        .join("release/kalendar-eventkit");
    let destination = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"))
        .join("kalendar-eventkit");
    fs::copy(&source, &destination).expect("copying the EventKit bridge into Cargo output");
    println!(
        "cargo:rustc-env=KALENDAR_EVENTKIT_BUILD_PATH={}",
        destination.display()
    );
}
