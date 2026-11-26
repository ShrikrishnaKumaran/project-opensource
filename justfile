set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

default: check

# Run formatting check, clippy with warnings denied, and tests across all workspace crates
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

# Build debug Android APK for the app (targets ARM64 physical phones)
apk:
    cd crates/app && dx build --android --target aarch64-linux-android

# Build Windows Desktop .exe (optimized release)
desktop:
    cargo build -p app --release
    $dll = Get-ChildItem -Path target/release/build/webview2-com-sys* -Recurse -Filter 'WebView2Loader.dll' -ErrorAction SilentlyContinue | Where-Object { $_.FullName -like '*x64*' } | Select-Object -First 1; if ($dll) { Copy-Item $dll.FullName target/release/WebView2Loader.dll -Force }

# Run Windows Desktop app locally
run:
    cargo build -p app
    $dll = Get-ChildItem -Path target/debug/build/webview2-com-sys* -Recurse -Filter 'WebView2Loader.dll' -ErrorAction SilentlyContinue | Where-Object { $_.FullName -like '*x64*' } | Select-Object -First 1; if ($dll) { Copy-Item $dll.FullName target/debug/WebView2Loader.dll -Force }
    cargo run -p app

# Check dependencies with cargo-deny
deny:
    cargo deny check
