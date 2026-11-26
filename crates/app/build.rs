// build.rs — auto-copy WebView2Loader.dll next to the app binary on every build.
//
// Cargo sets OUT_DIR to something like:
//   target/{profile}/build/app-<hash>/out
// The binary lands in target/{profile}/, so we walk up 3 levels from OUT_DIR.
//
// We look for the x64 DLL produced by the webview2-com-sys build script,
// which unpacks it under:
//   target/{profile}/build/webview2-com-sys-<hash>/out/x64/WebView2Loader.dll

use std::{env, fs, path::PathBuf};

fn main() {
    // Only meaningful on Windows.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));

    // out_dir = …/target/<profile>/build/app-<hash>/out
    // go up 3 → …/target/<profile>/build
    // go up 1 more → …/target/<profile>
    let build_dir = out_dir
        .ancestors()
        .nth(3) // .../target/<profile>/build
        .expect("unexpected OUT_DIR depth");
    let profile_dir = build_dir.parent().expect("build_dir has no parent"); // .../target/<profile>

    let dst = profile_dir.join("WebView2Loader.dll");

    // If already copied, nothing to do.
    if dst.exists() {
        println!("cargo:rerun-if-changed=build.rs");
        return;
    }

    // Find the first x64 WebView2Loader.dll under the build dir.
    if let Some(src) = find_webview2_dll(build_dir) {
        fs::copy(&src, &dst).unwrap_or_else(|e| {
            eprintln!("cargo:warning=Failed to copy WebView2Loader.dll: {e}");
            0
        });
        println!("cargo:rerun-if-changed={}", src.display());
    } else {
        // The webview2-com-sys build script hasn't run yet (first build).
        // Cargo guarantees dependencies build before dependents, but build
        // scripts within the same package's dependency graph may race.
        // Emit a warning; the NEXT incremental build will succeed because
        // webview2-com-sys will have already unpacked the DLL by then.
        // The justfile `run` recipe also copies it explicitly as a safety net.
        println!(
            "cargo:warning=WebView2Loader.dll not found yet — \
             it will be copied on the next build once webview2-com-sys unpacks it. \
             Use `just run` to launch the app safely."
        );
    }

    println!("cargo:rerun-if-changed=build.rs");
}

/// Walk `build_dir` looking for `…/x64/WebView2Loader.dll`.
fn find_webview2_dll(build_dir: &std::path::Path) -> Option<PathBuf> {
    let read_dir = fs::read_dir(build_dir).ok()?;
    for entry in read_dir.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with("webview2-com-sys") {
            continue;
        }
        let candidate = entry
            .path()
            .join("out")
            .join("x64")
            .join("WebView2Loader.dll");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}
