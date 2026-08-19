# 代码审查报告：commit `63ff297`（C 探针 C2 两阶段 guest 端到端）

- **审查对象**：`63ff2971494fa96fed29f9e07ebe918bb02586fd` — `feat(wasm): C 探针 C2 两端guest 端到端……（C 探针）`
- **审查日期**：2026-08-20
- **审查人**：review-agent（按审查书静态阅读 63ff297 提交 diff + 对照 C 探针计划 C2 核查点）
- **审查范围**：`crates/cordis-wasm/src/lib.rs`（preseed_mirror）、`tests/c_probe.rs`（c1+c2）、`tests/bridge_core.rs`、`tests/load_guest.rs`（provide 断言适配）、`examples/wasm-plugin-rust/src/lib.rs`（两步分支）
- **验证**：`cargo +1.97.0 test -p cordis-wasm` 全套绿（含 c_probe 2/2）；`git diff` 63ff297 不触 `crates/cordis-core`。

---

## 总体结论

✅ **通过（PASS WITH NITS）** — 放行 C3（评估报告 + EXIT）

- **major**：0
- **minor**：2（失败"路径"只覆盖"回注载荷形态不合法"，非"远端操作失败"：worker Err 无法经 preseed 传达；preseed 值跨卸载存活为隐含契约）
- **nit**：3（阶段判定二值 —— 镜像有/无值；`rep=0` 硬编码；`get` 读的是镜像快照而非真实 await）

两阶段形态与 C2 核查点全部达成：阶段 1 提交（echo(7)）→ worker 回填 Count(14) → `preseed_mirror` 回注镜像 → revision bump 重建阶段 2 → guest 经镜像 `get` 消费（14+1）→ `probe_out=Count(15)` 断言 + 失败 path（Text→`probe_err`）→ 退役静止。**零 core 改动**、既有提供键断言适配合理、全套回归绿、读取通道（REVIEW-C1 minor-1）落地方案成立。

---

## 发现

### Major：无

### Minor

### M-1（诚实性）：失败 path 覆盖的是「回注载荷形态不合法」，非「远端操作失败」

- `c2` 失败路径：`preseed_mirror("probe_in", Value::Text("boom"))` → 阶段 2 读 Text 走 `v => probe_err` 分支 → 断言 `probe_err == "boom"`。
- 这是"输入类型不符合预期 → 阶段 2 兜底"，**不是"worker 执行失败（回填 Err）"**：`preseed_mirror` 只存 `Value`（成功形态），真实 worker `Err`（`Result<Value,String>` 的 Err）没有通道写进镜像 → 远端失败无法经 C 探针的两阶段模拟传达。
- 影响：C3 评估如要验证"远端失败 → 阶段 2 消费失败"的完整形态，C 探针不足（需 err 通道达镜像或回填直入）——**如实记录为探针覆盖边界**。
- 建议：C3 EXIT 明确标注失败 path 的覆盖范围（形态失败 ✓ / 远端失败 ✗，后者留 B 或待 err 通道）。

### M-2（隐含契约）：`preseed_mirror` 值跨卸载存活（手动插入、无逆、unload 不清）

- 测试时序：preseed（rev0 apply 之后、rev1 apply 之前）→ loader apply(rev1)（阶段一 unload rev0 → 阶段二重建）→ 阶段 2 激活 step0 读镜像 `probe_in` 仍命中（实测 `probe_out=15` 通过）。
- 依理：`set` 产物的逆在卸载/逆执行时清镜像；preseed 是**手动插入**（无逆），unload 清理路径只处理 set 产物 → preseed 值**潜在地跨卸载/生命周期存活**，清理责任悬空。
- 影响：探针内可接受（测试后 runtime/drop 即弃）；正式通道（如 B 的 Await / sync_injected 化）须明确生命周期。C3 评估记录。
- 建议：C3 EXIT 注明 preseed 为探针专用、生命周期未管辖；不视为最终 API。

### Nit

- **N-1（低）**：阶段判定为「镜像有/无 `probe_in`」二值——若阶段 2 需表达"合法空/None 输入"则无法与"未回注"区分；探针足够。
- **N-2（低）**：`await_remote_value(&comp, 0)` 硬编码 rep=0（echo 为首个句柄）；测试语义明确，可持续。
- **N-3（低）**：`context::get` 读的是**镜像快照**（宿主回注后预置），非真实 await 语义——C3 评估时应归类为"数据流等价的模拟"而非完整 await（与 W2 时序边界一致）。

---

## 通过项（逐条确认）

- **两阶段形态**：C2 核查点全部满足——阶段 1 提交+完成；宿主 poll 回填（`await_remote_value`，不依赖 guest take）；`preseed_mirror` 回注（REVIEW-C1 minor-1 的读取通道落地：写镜像、不触核心依赖解析、不新 crate）；revision bump 重建（同 loader 条目 rev+1）→ 阶段 2 step 判定（镜像有 `probe_in`）→ 消费（`n+1`）→ `probe_out=Count(15)` 断言 ✓；失败 path `probe_err` 断言 ✓；退役 `is_quiet` ✓。
- **读取通道诚实性**：preseed 写 `Host.bindings`（镜像），guest `get` 读取；与注入同步（inject 键）通道不同且在注释中如实标注"探针形态（评估后由正式通道替代）"；`sync_injected` 仅处理 inject 键（本组件 `inject=[]`）→ 不清 probe_in ✓（实测通过佐证）。
- **零 core 改动**：`git show 63ff297 --stat` 仅 `cordis-wasm/{src, tests}` + `examples/wasm-plugin-rust`，无 `crates/cordis-core` ✓。
- **既有适配**：bridge_core / load_guest 提供键断言改为**排序后**包含 `db, probe_err, probe_out`（C2 扩展提供键是 guest 语义变化，断言跟进合理且稳定）。
- **回归**：`cargo +1.97.0 test -p cordis-wasm` 全套绿（lib 8 + 集成 15 = 23，含 c_probe 2、bridge_core 2、load_guest、driver 等——无 FAILED）；clippy/fmt/doc 父会话已验绿。
- **注入依赖同步延续**：阶段 2 无需新 provider（probe_in 镜像预注不参与核心依赖解析）——避免破坏 bridge_core 等依赖拓扑 ✓。

---

## 结论

C2（两阶段 guest 端到端）达成：真实 worker 回填 → 回注 → 阶段 2 消费的链路直证（`echo(7)→14→probe_out=15`），失败 path 覆盖"载荷形态不合法"，既有回归绿、零 core 改动、读取通道方案成立且诚实标注。2 项 Minor（失败 path 覆盖边界、preseed 生命周期隐含契约）+ 3 Nit 建议在 **C3 评估报告如记录**，不阻塞放行。

→ **建议放行 C3**：评估报告（guest-await 需求强度结论：两阶段拆分的作者体验 / 状态显式化 / 远端失败不可达 → B 开工或降级建议）+ `docs/cordis-wasm-C-PROBE-EXIT.md`。
