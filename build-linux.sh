#!/usr/bin/env bash
set -euo pipefail

echo "==================================="
echo "  Building CUTTOOL (Cryss) for Linux"
echo "==================================="

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$PROJECT_DIR"

mkdir -p build/dist build/temp

if command -v nix &>/dev/null && [ -f "flake.nix" ]; then
    toolchain=(nix develop --command)
else
    toolchain=()
fi

echo
echo "[*] Building Rust timing library..."
"${toolchain[@]}" cargo build --release --locked \
    --manifest-path native/timing/Cargo.toml --target-dir build/native

echo
echo "[*] Building Linux standalone binary via PyInstaller..."
"${toolchain[@]}" pyinstaller \
    --clean \
    --noupx \
    --onefile \
    --name "Cryss" \
    --distpath "build/dist" \
    --workpath "build/temp" \
    --specpath "build/temp" \
    --add-binary "$PROJECT_DIR/build/native/release/libcryss_timing.so:." \
    src-linux/main.py

echo
echo "==================================="
echo "  BUILD SUCCESSFUL!"
echo "  Binary location: build/dist/Cryss"
echo "==================================="
