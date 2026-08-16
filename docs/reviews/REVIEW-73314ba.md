# 代码审查报告：commit `73314ba`（PR #10 wit 世界 + wasmtime 宿主 + guest 闭环）

- **审查对象**：`73314ba4e5040a74e4563c4dac79eefe556bc885`（M1 起步：`wit/cordis.wit` 世界 v1、wasmtime 宿主加载/驱动原语、Rust guest 示例端到端、CI wasm target/构建步骤）+ docs `77e1f3e`
- **审查日期**：2026-08-16（仓库时区）
- **验证手段**：guest 构建后 `cargo test --workspace` 全绿（含 wasm 测试）；clippy `-D warnings` 干净；**干净环境模拟实测**（移走 guest target）复现 CI 缺陷

---

## 🔴 必须修复（blocker）

### B1. CI 步骤顺序缺陷：`cargo test --workspace` 先于 guest 构建——干净环境必然红
**位置**：`.github/workflows/ci.yml`（本 PR 引入的 `build wasm guest (M1)` 步骤被放在 `cargo test` **之后**）

**事实与实测**：
- cordis-wasm 的测试（`load_guest` 及后续 PR #11 的 `bridge_core`）按约定路径读取 `examples/wasm-plugin-rust/target/wasm32-wasip2/debug/wasm_plugin_rust.wasm`；
- guest 是**独立 crate**（非 workspace 成员，`[workspace]` 空段），`cargo test --workspace` 不会构建它；
- rust-cache 默认只缓存工作目录 `./target`，不缓存 guest 的独立 target 目录；
- **本地实测**：`mv examples/wasm-plugin-rust/target /tmp/` 后跑 `cargo test -p cordis-wasm` → 测试 `FAILED`（panic：文件不存在）；还原后通过。CI 干净环境即"移走后"状态——**`cargo test` 步骤必然失败，且每次运行都失败（cache 不覆盖）**。
- **GitHub Actions 实测确认（2026-08-16）**：
  - run `31948978224`（head `77e1f3e`，本 PR docs）failure，失败步骤 `cargo test`（fmt/clippy success、后续步骤 skipped）；PR #11 docs 的 run `31951129109` 同因 failure；
  - 日志关键行：`panicked at crates/cordis-wasm/tests/bridge_core.rs:19:50: 先构建 guest：cargo build ... No such file or directory`；`Process completed with exit code 101`；
  - 对照：M1 之前的 run `31939890445`（head `705a199`）为 success——CI 由绿转红与本 PR 合入完全吻合。
  - 日志尾部 "Node 20 is being deprecated" 为 runner 环境提示，与失败无关。

**建议**：把 `build wasm guest (M1)` 步骤移到 `cargo test` **之前**。

---

## ⚪ 细节（nit）

1. **guest wasm 路径约定脆弱**（`../../examples/wasm-plugin-rust/target/...`）——expect 消息有构建指引，M1 起步可接受。
2. **wasmtime 无 fuel/epoch 限制**——guest 死循环挂死宿主（PR #12 沙箱计划已声明，风险已知）。

---

## 正面确认（实现正确的点）

- **wit 世界 v1 简洁合理**：`inverse` 资源句柄化（不依赖 destructor）、`task.step → option<effect-step>` 协议与 Def 51 对应；`set` 返回 `result<inverse, string>` 带错误通道。
- **工具链决策文档化**：wasip2 直出组件（免组件化步骤）、guest 独立 crate 规避 workspace `unsafe_code=deny`（wit-bindgen ABI 胶水使用 unsafe，仅 wasm target 编译）——理由充分。
- **端到端测试闭环**：`load_guest.rs`（constructor → inject/provide 核对 → start/step 激活绑定 → 迭代终止）验证宿主加载与驱动原语完整链路。
- **Host 结构**：绑定镜像 + 逆句柄计数 + WASI p2 上下文（`WasiView`），职责清晰。

---

## 总结

- **必须修复**：B1（CI 顺序——`build wasm guest` 移到 `cargo test` 前；本地与 Actions 双重实测确认）。
- **nit**：1–2 可忽略。

**置信度**：高——B1 经干净环境本地复现 + GitHub Actions 日志双重确认。
