#!/usr/bin/env bash
# 构建 Go guest（M1 门禁 3/3 双语言验收的 Go 侧）。
#
# 管线：标准 go（wasip1，reactor 模式 c-shared）→ 核心模块
#       → 预览1 适配器组件化（tools/componentize）→ guest.wasm
#
# 前置：go（>= 1.24，go:wasmexport / wasmimport 括号语法）、
#       cargo（workspace 内 tools/componentize）。
set -euo pipefail
cd "$(dirname "$0")"
ROOT="$(cd ../.. && pwd)"

export GOFLAGS=-buildvcs=false
export GOCACHE="${GOCACHE:-$ROOT/target/gocache}"

# 1. 核心模块：reactor 模式（-buildmode=c-shared 导出 _initialize，
#    Go 运行时在组件实例化时初始化；普通 exe 模式只有 _start，宿主
#    调用导出前运行时会 panic notInitialized）。
GOOS=wasip1 GOARCH=wasm go build -buildmode=c-shared -o guest-core.wasm .

# 2. 组件化：嵌入 wit 世界元数据 + 预览1 适配器 → 组件二进制。
cargo run --quiet -p componentize -- \
    guest-core.wasm \
    "$ROOT/third_party/wasi-preview1-adapter/wasi_snapshot_preview1.reactor.wasm" \
    "$ROOT/crates/cordis-wasm/wit" cordis \
    guest.wasm

rm -f guest-core.wasm
echo "Go guest 构建完成: examples/wasm-plugin-go/guest.wasm"
