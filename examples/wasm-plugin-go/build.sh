#!/usr/bin/env bash
# 构建 Go guest（M1 门禁 3/3 双语言验收的 Go 侧）。
#
# 管线：标准 go（wasip1，reactor 模式 c-shared）→ 核心模块
#       → 预览1 适配器组件化（tools/componentize）→ guest.wasm
#
# 前置：go（>= 1.24，go:wasmexport / wasmimport 括号语法）、
#       cargo（workspace 内 tools/componentize）、
#       wit-bindgen-cli 0.60.0（第 0 步重生成 go 绑定，与生成代码版本一致）。
set -euo pipefail
cd "$(dirname "$0")"
ROOT="$(cd ../.. && pwd)"

# wit-bindgen PATH 检查（REVIEW-P4 nit 落地）：缺失时给可操作错误——
# 否则 "command not found" 无上下文（CI 曾因此失败：build wasm guest (Go)）。
if ! command -v wit-bindgen >/dev/null 2>&1; then
    echo "错误：wit-bindgen 未安装（build.sh 第 0 步重生成 go 绑定需要）" >&2
    echo "安装（pin 0.60.0）：cargo install wit-bindgen-cli --version 0.60.0" >&2
    exit 127
fi

export GOFLAGS=-buildvcs=false
export GOCACHE="${GOCACHE:-$ROOT/target/gocache}"

# 0. 同步 wit 绑定（P-4 产品验证线，REVIEW-6a714ca M-1）：wit 结构变更时
#    自动重生成 go 绑定（含 wit_exports.go 的 variant 判别）——消除"手动
#    重跑 wit-bindgen + 手写 ABI 判别同步"的维护坑。wit-bindgen 会重写
#    go.mod（覆盖 vendored replace）——重生成后恢复 go.mod（third_party/
#    go-pkg fork 的 replace 是标准 go 构建必需，A2b 处置同款）。
wit-bindgen go --out-dir . "$ROOT/crates/cordis-wasm/wit"
git checkout -- go.mod

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
