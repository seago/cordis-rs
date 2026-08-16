# wasi_snapshot_preview1.reactor.wasm

wasi preview1 → preview2 的 **reactor** 适配器，用于把标准 go 编译的
wasip1 核心模块编码为组件模型组件（`tools/componentize`）。

- 来源：`wasi-preview1-component-adapter-provider` crate **47.0.3**
  （与 wasmtime 47.0.3 同版，随 wasmtime 仓库发布）。
- 获取方式：`https://static.crates.io/crates/wasi-preview1-component-adapter-provider/wasi-preview1-component-adapter-provider-47.0.3.crate`，
  解包取 `artefacts/wasi_snapshot_preview1.reactor.wasm`。
- 许可：Apache-2.0 WITH LLVM-exception（上游同）。
- 用途：go guest（`examples/wasm-plugin-go`）构建管线的一部分；不是运行期
  依赖——组件化后适配器逻辑已并入生成的组件。
