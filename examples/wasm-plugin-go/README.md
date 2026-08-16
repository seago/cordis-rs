# wasm-plugin-go — Cordis Go guest 示例（M1 门禁 3/3 双语言验收）

**db 消费者**：与 `../wasm-plugin-rust-consumer` 语义一致（Def 43 的
(d, p, e) 跨边界形态）——注入 `db`，激活时经宿主 `context::get` 读取
注入值，提供 `derived = "derived(<db>)"`。验证**标准 go**（非 tinygo）
实现的组件与 Rust 组件在同一宿主上可互换。

## 布局

- `wit_exports.go`、`cordis_core_context/`、`cordis_core_plugin/`、
  `export_cordis_core_plugin/wit_bindings.go`：`wit-bindgen 0.60.0`
  生成代码（`wit-bindgen go --out-dir . ../../crates/cordis-wasm/wit`），
  勿手改。
- `export_cordis_core_plugin/plugin.go`：应用代码——生成代码引用
  `Component` / `Task` 类型（字段 `handle` / `pinner` 与方法是生成
  代码的既定契约），由本文件提供实现。
- `go.mod`：`replace go.bytecodealliance.org/pkg => ../../third_party/go-pkg`
  （cordis-rs fork，见下）。
- `guest.wasm`：构建产物（组件二进制，不入库）。
- `build.sh`：完整构建管线。

## 构建

```sh
./build.sh
# 等价手写：
# GOOS=wasip1 GOARCH=wasm go build -buildmode=c-shared -o guest-core.wasm .
# cargo run -q -p componentize -- guest-core.wasm \
#    ../../../third_party/wasi-preview1-adapter/wasi_snapshot_preview1.reactor.wasm \
#    ../../../crates/cordis-wasm/wit cordis guest.wasm
```

要点（踩坑记录，PR #14）：

1. **reactor 模式**：必须 `-buildmode=c-shared`——导出 `_initialize`
   而非 `_start`，Go 运行时在组件实例化时初始化；否则宿主调用导出时
   运行时 panic `notInitialized`。
2. **预览1 适配器**：标准 go 只能产出 wasip1 核心模块，其
   `wasi_snapshot_preview1.*` 导入由组件化时的 reactor 适配器映射为
   wasip2（`tools/componentize` + `third_party/wasi-preview1-adapter/`）。
3. **cabi_realloc 预初始化窗口**：适配器首次被调用（Go 运行时
   schedinit 期间，包 init 之前）会经 `cabi_realloc` 分配影子栈
   （64KB）与 State（64KB）。此窗口内既不能用 GC 也不能回调适配器
   ——fork 的 `wit/runtime/runtime.go` 以静态缓冲 bump 分配实现
   上游 tinygo `runtime.sbrk` 的语义（上游符号标准 go 不存在）。
4. **`runtime.Handle.TakeHandle`**：wit-bindgen 0.60 生成代码需要，
   上游 pkg v0.2.3 尚无——fork 补齐（语义同 `Take`）。

## third_party/go-pkg（fork）

`../../third_party/go-pkg` 是 `go.bytecodealliance.org/pkg@v0.2.2`
的 vendored 拷贝，改动：`wit/runtime/runtime.go`（去掉 tinygo 专有
`runtime.sbrk`，补预初始化 bump 分配器与 `TakeHandle`）。其余
`wit/types`、`wit/async` 与上游一致。
