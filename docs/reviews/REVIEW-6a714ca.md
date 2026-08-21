# 代码审查报告：commit `6a714ca`（A2b G3 · go guest variant 适配 + go_guest 恢复）

- **审查对象**：`6a714ca951a6e2678a2e161e0477c9892a089453` — `feat(wasm): A2b G3——go 绑定重生成（variant 形态）+ plugin.go 适配 MakeEffectStepDone + go_guest 2 测试恢复绿（M1 双语言门禁恢复，root cause 消除）（A2b）`
- **审查人**：independent-review-agent（对 `A2b-PLAN.md` G3 与 A2 出口约束）
- **日期**：2026-08-22
- **范围**：`examples/wasm-plugin-go/`（wit_bindings.go 重生成、`cordis_core_remote/` 新绑定、plugin.go、wit_exports.go）+ `crates/cordis-wasm/tests/go_guest.rs`（去 ignore）；对照 A2b 计划 G3 与根因（`invalid option discriminant`）。
- **验证手段**：`git show 6a714ca` 静态阅读 + `cargo +1.97.0 test -p cordis-wasm`（全套，含 go_guest）实测。

---

## 总体结论

✅ **PASS（PASS WITH NITS）** —— M1 双语言门禁恢复、root cause 消除论证成立。

- major：0
- minor：1（`wit_exports.go` 用手写 ABI 判别——绑定再生成流程的文档化/自动化缺口，见 M-1）
- nit：2（go 侧 `wait` case 未实测；`cordis_core_remote` 绑定为新增能力面但无 go 消费测试）
- 放行 G4（全套门禁 + EXIT/README 更新 + 出口走查）。

## 核查

### 核心（全部通过）

1. **root cause 消除（关键）**：wit `effect-step` 已 variant 化（`step(inverse)/done(inverse)/wait`，wit:40-47）；`wit_exports.go` 按 variant 写判别（`step→0 / done→1 / wait→2` + 载荷 handle/wait 空）；**实测 `go_guest` 2/2 通过（12.7s，native/rust provider 双路）**——variant（tagged union）编码下 go 绑定与宿主 wasmtime 组件模型一致，`option<resource>` 判别坑不再触发。根因（`invalid option discriminant`）**实证消除**。
2. **plugin.go 适配正确**：`MakeEffectStepDone(res.Ok())`（`done(inverse)` 收尾步）——与 rust guest 的收尾语义一致；go 消费者**不 await 远端**（无 `wait` 使用），符合其"读注入值 → 提供 derived"职责。
3. **双语言回归**：全套 `cargo test -p cordis-wasm` 全绿（lib 7 + 集成 13，含恢复的 go_guest 2，无 ignore）——rust 系 + sandbox/dual_backend 不受 G3 破坏。
4. **不改 core**：commit 触碰文件全在 `examples/wasm-plugin-go` + `go_guest.rs`，无 `crates/cordis-core`。
5. **新绑定目录**：`cordis_core_remote/`（187 行）为 remote 接口的 go 绑定（能力面新增，guest 可用可不用）——入库正确。

## 发现

### Minor

**M-1（建议）**：`wit_exports.go` 的 ABI 判别是**手写**（`0/1/2` 常量 + 内存布局）——若 wit 结构再变（新增 case/调整顺序），此文件需手动同步且易错；且它藏在 `examples/`（非生成管线自动产）。建议在 A2b 出口或 README 记录：「go 侧 ABI 编解码为手动维护（wit 变更时须同步 `wit_exports.go` + 重生成绑定 + 重跑 go_guest）」，或纳入 build.sh 自动化检查（若 wit-bindgen 能生成该骨架则改自动）。

### Nit

- **n-1**：go 侧 `wait` case（EffectStepWait）**未实测**（go 消费者不用 wait）。因 A2b 范围是"恢复既有 go_guest"，wait 语义已由 rust 侧 `a2_e2e` 覆盖——记录即可，不阻塞。
- **n-2**：`cordis_core_remote` 绑定新增但无 go 消费测试——能力面预留，按需再测。

## 判定

**G3 达成**：go guest 适配 variant + `go_guest` 恢复（M1 双语言门禁闭环）+ root cause 消除实证 + 全套回归绿 + 不改 core。**放行 G4**（全套门禁 + EXIT §4 A2b 遗留→已闭环 + README 已知边界更新 + 出口走查）；M-1/n-1/n-2 记入 G4 文档项。
