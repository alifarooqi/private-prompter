#!/usr/bin/env bash
#
# Build the llama.cpp `llama-server` binary as a Tauri sidecar, for both
# Apple Silicon and Intel macOS. Output is dropped into src-tauri/binaries/
# with the naming convention Tauri's sidecar resolver expects:
#   llama-server-aarch64-apple-darwin
#   llama-server-x86_64-apple-darwin
#
# Requires: cmake, a C/C++ toolchain, ninja (or make), git.
#
# Usage: ./scripts/build-sidecar.sh [native|all]
#
#   native  (default) — build only the host arch (arm64 on M-series, x86_64
#                       on Intel). Fast; what you need for local dev.
#   all               — cross-compile both arm64 and x86_64. Used by CI.
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LLAMA_SRC_DIR="${LLAMA_SRC_DIR:-$REPO_ROOT/.build/llama.cpp}"
LLAMA_REPO="${LLAMA_REPO:-https://github.com/ggerganov/llama.cpp.git}"
LLAMA_BRANCH="${LLAMA_BRANCH:-master}"

MODE="${1:-native}"

OUT_DIR="$REPO_ROOT/src-tauri/binaries"
mkdir -p "$OUT_DIR"

echo "==> Cloning llama.cpp ($LLAMA_BRANCH) into $LLAMA_SRC_DIR"
if [ ! -d "$LLAMA_SRC_DIR" ]; then
  git clone --depth 1 --branch "$LLAMA_BRANCH" "$LLAMA_REPO" "$LLAMA_SRC_DIR"
else
  echo "    (already cloned; reusing)"
fi

build_for() {
  local arch="$1"
  local cargo_target="$2"
  local build_dir="$LLAMA_SRC_DIR/build-$arch"

  echo "==> Building llama-server for $arch"
  cmake \
    -S "$LLAMA_SRC_DIR" \
    -B "$build_dir" \
    -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_OSX_ARCHITECTURES="$arch" \
    -DGGML_NATIVE=OFF \
    -DLLAMA_BUILD_SERVER=ON \
    -DLLAMA_BUILD_TESTS=OFF \
    -DLLAMA_BUILD_EXAMPLES=OFF \
    -DLLAMA_BUILD_BENCHMARK=OFF \
    -DLLAMA_CURL=OFF \
    >/dev/null

  cmake --build "$build_dir" --config Release --target llama-server -j

  cp "$build_dir/bin/llama-server" \
     "$OUT_DIR/llama-server-${cargo_target}"

  echo "    wrote $OUT_DIR/llama-server-${cargo_target}"
}

NATIVE_ARCH="$(uname -m)"
case "$NATIVE_ARCH" in
  arm64)
    build_for arm64    aarch64-apple-darwin
    if [ "$MODE" = "all" ]; then
      build_for x86_64 x86_64-apple-darwin
    fi
    ;;
  x86_64)
    build_for x86_64   x86_64-apple-darwin
    if [ "$MODE" = "all" ]; then
      build_for arm64  aarch64-apple-darwin
    fi
    ;;
  *)
    echo "unknown native arch: $NATIVE_ARCH" >&2
    exit 1
    ;;
esac

echo "==> Done. Sidecar binaries:"
ls -lh "$OUT_DIR"/llama-server-* 2>/dev/null || echo "(none)"