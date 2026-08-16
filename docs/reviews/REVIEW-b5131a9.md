# 代码审查报告：commit `b5131a9`（PR #14 沙箱隔离 + Rust/Go 双语言 guest）

- **审查对象**：`b5131a9a1c4336473dec30db6a4e28f1dadf6c0e`（36 文件，+2810/−3）+ docs `317b2f0`（PLAN.md + THEORY-MAP.md 回填）
- **审查日期**：2026-08-17（仓库时区）
- **验证手段**：`cargo test --workspace` **77 测试全绿**（新增 sandbox_isolation 1 + go_guest 2 全通过）；`RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings` 干净；`cargo fmt --all -- --check` **失败（exit 1）**；`examples/wasm-plugin-go/build.sh` 本地端到端构建成功（go1.26.3 + cargo 1.95）

---

## 结论：**有条件通过（1 项 blocker 必须修复）**

门禁功能（沙箱隔离 + Go 双语言 guest）本身已达成并实测验证，但 **CI 的 `cargo fmt --all -- --check` 步骤必然失败**——存在 1 项 blocker。修复后即可通过。

---

## 🔴 必须修复（blocker）

### b1. `sandbox_isolation.rs` 未过 `cargo fmt`——CI fmt 门禁第一步即红
**位置**：`crates/cordis-wasm/tests/sandbox_isolation.rs:10`

**事实**：`use cordis_core::{FiberState};`（单条导入被冗余花括号包裹）被 rustfmt 规范化为 `use cordis_core::FiberState;`。实测 `cargo fmt --all -- --check` 返回 **exit 1**，diff 即此一行：

```rust
+use cordis_core::FiberState;
 use cordis_core::runtime::Runtime;
 use cordis_core::symbol::Symbol;
-use cordis_core::{FiberState};
```

CI（`.github/workflows/ci.yml:37-38`）的 `cargo fmt --all -- --check` 在安装 Rust 工具链后、任何构建/测试前**第一个**执行，因此该步骤在 ubuntu runner 上必然失败，阻塞整个 PR 门禁。对照 `go_guest.rs:18` 的 `use cordis_core::{Component, FiberState, Runtime};`（多条目，合法）可确认只有单条花括号形式违规。修复即 `cargo fmt`（删花括号）或手工改该行。

---

## ⚪ 细节（nit）

### nit1. `runtime.go` 未通过 `gofmt`——`//go:nosplit` 前缺注释分隔空行
**位置**：`third_party/go-pkg/wit/runtime/runtime.go:112` 附近

**事实**：`gofmt -l` 标记该文件；diff 显示 `//go:nosplit` 指令前缺一个 `//` 空注释行（gofmt 要求 doc 注释与编译指令之间以 `//` 分隔）。纯格式问题，不影响编译。仓库 CI 未设 gofmt 门禁（Go 仅为 guest 示例、非 workspace crate），故不构成 blocker；建议后续在 fork 文件上补一条 `//` 使 gofmt 干净。

### nit2. `go_guest.rs` 第二测试缺 `is_quiet()` 收尾断言——与既有惯例不一致
**位置**：`crates/cordis-wasm/tests/go_guest.rs:159`（`go_consumer_reads_native_provider_value` 结尾）

**事实**：第一测试（Rust provider 路，~L128）以 `fiber.retire()` → 级联停用 → `symbols().next().is_none()` → `runtime.is_quiet()` 收束；第二测试（native provider 路）走到 `loader.apply(&[entry("cons", ...)])` 移除 provider → 级联停用 → `symbols().next().is_none()`，**但缺 `is_quiet()` 断言**。既有 `dual_backend.rs` 的同构双路测试（native 路 L187）保留了 `is_quiet()`。虽然 `symbols()` 全清已强断言绑定层干净，`is_quiet` 还额外覆盖"无滞留 fiber/待驱动任务"语义；建议补齐以与门禁级断言链一致。

### nit3. go-pkg fork 带入未被 `cordis` 世界引用的类型模块——与"仅保留子集"表述略有出入
**位置**：`third_party/go-pkg/wit/types/{future,stream,tuple,unit}.go`、`third_party/go-pkg/wit/async/async.go`

**事实**：当前 `cordis` 世界（`crates/cordis-wasm/wit/cordis.wit`）仅用到 `option`/`result`/`string`/`list<string>`/`unit`；生成绑定（`cordis_core_context/wit_bindings.go` 等）只引用 `witTypes.Option`/`Result`，**未引用** `Future`/`Stream`/`Tuple`/`async`。README 声称 fork"仅保留 go guest 构建所需子集"，但实际保留了 5 个当前未使用的类型模块（约 800 行）。这不影响构建（未引用即不编译），属 vendoring 最小化纪律的轻微溢；或保留（若未来世界引入更丰富类型时免于再取），建议在 README 补一句"Future/Stream/Tuple/async 为上游完整拷贝，当前 cordis 世界未用、留待更丰富类型世界"以消除"子集"表述与事实的偏差。

### nit4. commit 消息"唯一新 Rust 依赖"措辞不精确——实为"唯一新 crate 族（多版本并行）"
**位置**：commit `b5131a9` 消息与 `tools/componentize/Cargo.toml`

**事实**：`tools/componentize` 引入 `wit-component`/`wit-parser`（0.256），连带新增 `wasm-metadata`/`wasm-encoder`/`wasmparser` 的 0.256 版本（workspace 既有 0.252/0.254 版本），形成 wit-component 0.254 与 0.256、wasm-metadata 0.254 与 0.256 的**并行双版本**。这是客观约束（adapter provider 47.0.3 对齐 wasmtime 47.0.3 需 0.256，而 wit-bindgen 0.60 内嵌 0.254），非缺陷；仅"唯一"表述略欠准。Cargo.lock 已正确随变，锁定纪律无问题。

---

## 正面确认（实现正确的点）

- **沙箱隔离（门禁 2/3）实质达成**：`sandbox_isolation.rs` 以 `catch_unwind` 捕获恶意 guest（`wasm-plugin-rust-panic`，step panic → trap）在 `use_component` 同步 reload 驱动时以 panic（Trap）形式上抛的宿主侧错误；随后**同一进程**继续 `use_component` 实例化正常组件、断言其 `Active` 且 `db` 绑定生效、退休后绑定全清——"trap 不伤宿主"的断言链完整且可执行（实测 0.66s 通过）。
  - 诚实标注已知边界：测试末尾注释 + THEORY-MAP 行均如实记录"trap 组件在 registry 留有一个卡在激活转换中的 fiber，`is_quiet` 为 false；L-Raise（处置清单⑤）未落地，随失败模型实现后处置"——未夸大为完全静止，纪律良好。
- **Go 双语言（门禁 3/3）实质达成**：`go_guest.rs` 双路断言链与 `dependency_consumption`/`dual_backend` 同构——注册 → 双向 `Active` → `derived("wasm-pg")` / `derived("native-pg")` → 级联停用 → 绑定全清。Rust provider / native provider 双路均实测通过（19.68s）。借用纪律：`derived_value` 的 `runtime.store()` 临时借用在 `fiber.retire()` 前已随函数返回释放，无 `store()` Ref 跨块残留（与 `dependency_consumption.rs:55` 注释揭示的隐患模式一致地得到规避）。
- **CI 顺序正确**：`build wasm guest (M1)`（Rust provider + consumer + panic 三个独立 guest）与 `build wasm guest (Go)` 均在 `cargo test` **之前**执行；`sandbox_isolation`/`go_guest` 的 `env!("CARGO_MANIFEST_DIR")` 相对路径指向的 `target/wasm32-wasip2/debug/` 与 `examples/wasm-plugin-go/guest.wasm` 均先于测试就位（沿袭 PR 审查 B1 的"guest 先行"教训）。
- **`-D warnings` 门禁覆盖新成员**：`tools/componentize` 为 workspace 成员，`cargo clippy --workspace --all-targets` 覆盖之；且 CI 全局 `RUSTFLAGS=-D warnings` 使 `build.sh` 内 `cargo run -p componentize` 也在告警即失败下编译（实测 clippy 全干净，无依赖告警泄漏）。
- **vendored 纪律充分**：`third_party/wasi-preview1-adapter/README.md` 记录来源（provider 47.0.3、crates.io 直接获取路径）与许可（Apache-2.0 WITH LLVM-exception）；`third_party/go-pkg/README.md` 记录上游 v0.2.2、三处改动（去 `runtime.sbrk`、预初始化 bump 分配器、补 `Handle.TakeHandle`）与许可，并附 LICENSE；fork 改动集中在 `runtime.go`，`wit/types`、`wit/async` 与上游一致（头部注释保留上游）。
- **生成代码标记合规**：`wit_exports.go` 与 4 个 `wit_bindings.go` 均以 `// Generated by wit-bindgen 0.60.0. DO NOT EDIT!` 开头，`empty.s` 为 wit-bindgen 标准占位（wasmimport 空体测试用）。
- **产物不入库正确**：`guest.wasm`/`guest-core.wasm` 已 `git ls-files` 确认**未被跟踪**（本地 build 遗留），`.gitignore` 条目正确且 build.sh 的 `rm -f guest-core.wasm` 与防御性 ignore 双向一致。
- **可复现性**：`go.sum` 缺失正确（`replace` 指向本地 `../../third_party/go-pkg`，无网络求和需要）；panic guest 独立 crate（`[workspace]` 空）自带 337 行 `Cargo.lock`，与 consumer 示例同构。
- **理论对齐**：THEORY-MAP 两行新增（沙箱 §6.3、Go 双语言 Def 43/51 + §6.3 + Alg 4/5）准确，且如实记录了**值得记录的三项偏差**——`-buildmode=c-shared` 的 `_initialize` 要求、标准 go 无组件能力须适配器组件化、预初始化窗口的 fork 决策。PLAN.md M1 状态行改为"门禁 1/3、2/3、3/3 全部达成，待走查 §6.2–6.4"与事实一致。

---

## 风险核验（无实质风险）

- **既有 77 测试无回归**：全 workspace 实测通过；`tools/componentize` 引入的 wit-component 0.256 与既有 0.254 并行（`serde`/`indexmap` 等共享依赖版本收敛，无冲突）。
- **ubuntu runner 可复现性**：`setup-go` 固定 `1.26.x`（go1.26.3 已发布，本地同版验证通过）；Go 构建仅依赖本地 replace 与 stdlib，无网络脆弱点；`-buildmode=c-shared` + 预览1 reactor 适配器在 x86-64 linux 与 darwin 行为一致（适配器为纯 wasm 产物，平台无关）。`GOCACHE` 落入 `target/gocache`（gitignored），rust-cache 间接受益。
- **依赖漂移边界已记录**：wit-component 0.254/0.256 双版本并存是适配器版本对齐的客观结果，Cargo.lock 已锁定且无生命周期冲突。

---

## 总结

- **必须修复（blocker）**：b1（`sandbox_isolation.rs:10` 单条花括号导入未过 rustfmt，CI fmt 门禁第一步即红）。
- **建议修复（major）**：无。
- **nit**：nit1（gofmt 空注释行）、nit2（`is_quiet` 收尾断言补齐）、nit3（fork 未用类型模块的"子集"表述澄清）、nit4（"唯一新依赖"措辞）。均可忽略或后续顺手修正，不阻塞合入。

**结论：有条件通过。** 置信度：高——本地完整工具链（go1.26.3 / cargo 1.95 / wasm32-wasip2 target）端到端复现了 CI 的 guest 构建与 77 测试全绿，且独立复现了 fmt 门禁的失败（exit 1 + 单行 diff）；b1 为 rustfmt 版本无关的确定性规则，可直接核验。
