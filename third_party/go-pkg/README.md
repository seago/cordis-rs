# third_party/go-pkg

`go.bytecodealliance.org/pkg@v0.2.2` 的 vendored 拷贝（仅保留 go guest
构建所需子集），供 `examples/wasm-plugin-go` 经 `replace` 指令使用。

改动（vs 上游 v0.2.2）：

- `wit/runtime/runtime.go`
  - 移除 tinygo 专有符号 `runtime.sbrk`（标准 go 链接失败）；
  - `cabi_realloc` 预初始化窗口（Go 运行时 schedinit 期间、包 init 之前，
    适配器首次调用分配影子栈 64KB + State 64KB）改由静态缓冲 512KB +
    bump 指针实现（`preinitAlloc`，`//go:nosplit`，不触碰 GC/导入）；
  - 补 `Handle.TakeHandle`（wit-bindgen 0.60 生成代码所需，上游尚无）。

许可：Apache-2.0（见 LICENSE）。

注：`wit/types` 的 `future/stream/tuple/unit` 与 `wit/async` 为上游完整
拷贝——当前 cordis 世界（`crates/cordis-wasm/wit/cordis.wit`）仅用
`option`/`result`，其余留待更丰富类型的未来世界（未引用即不编译）。
