# 代码审查报告：commit `23b75fa`（P1.2 H2 门面纪律 + 决策落档）

- **审查对象**：`23b75fa76c9d29f5ef45aad89504ffc1b18f0e65` — `docs(async): P1.2 H2 门面纪律 C-4 + O-2/O-3/O-4 决策状态落档（crate doc）`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`git show 23b75fa`（仅 `crates/cordis-async/src/lib.rs` crate doc 追加 +22/-1，纯文档、无行为改动），对照 `docs/cordis-async-PHASE1-P2-PLAN.md` §2 Step 1（H2）与 `docs/cordis-async-protocol-draft.md` v1.4 §5 C-4 / §10 O-2/O-3/O-4。
- **验证手段**：静态阅读 + 实测 `GOCACHE=…/gocache cargo +1.97.0 doc -p cordis-async --no-deps`（0 告警）、`cargo +1.97.0 check -p cordis-async`（通过）；前置 H1（`fa44fd6`，AsyncFiberHandle 收口）已存在且审查闭环（REVIEW-fa44fd6 PASS，0 Major/1 Minor 已落地）——H2 文档引用的 `AsyncFiberHandle` 有效。

**改动统计**：1 文件 +22/-1：crate doc 追加「门面纪律（契约 C-4，P1.2 H2 文档化）」段 +「开放项决策状态（P1.2，H2 记录）」段（O-2/O-3/O-4 三项）+ 依据行补 P1.2 计划引用。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：0
- **nit**：2（C-4 落档用 `use_component` vs 草案字面 `apply` 的对应未明示；「逃生口常态化」措辞不贴切）

H2（纯文档里程碑）达成：C-4 门面纪律分界表述准确且与草案 §5 等价；O-2/O-3/O-4 决策值 = 计划 §1 默认（显式 settle / hook 不启用 / Failed 保持 String），与草案开放项语义一致、无扭曲；intra-doc 链接（[`AsyncRuntime`]）有效、`deny(missing_docs)` 下 0 告警。无行为改动（diff 仅 crate doc）。可放行 H3（Remote API 冻结标注）。

## 发现

### Major：无

### Minor：无

### Nit

### Nit-1：C-4 落档用「use_component」、草案字面为「apply」——语义等价，建议明示对应

- **位置**：`src/lib.rs` crate doc「门面纪律」段（`retire` / `update` / `use_component`** 必须走门面**）。
- **问题**：草案 C-4（§5）字面为「生命周期变更（retire/update/**apply**）必须走门面」。落档以 `use_component` 指代挂载入口——本层 `AsyncRuntime::use_component` 即核心「apply/use」的挂载语义，等价成立；但读者按草案字面核对时可能把「apply」误解为组件 `apply` 方法而非挂载操作。
- **建议**：doc 补一句「`use_component` = 本层对核心 `apply`（挂载）的门面入口」即可消除歧义。可选。

### Nit-2：措辞「逃生口常态化」不贴切

- **位置**：`src/lib.rs` crate doc「开放项决策状态」O-3 行（「……逃生口常态化」）。
- **问题**：草案 O-3 语义是「列此备查、默认不做、若 C-4 频繁违反再启用」——「常态化」易读作「正在使用/始终开启」，与「不启用」语义冲突（轻微误导）。
- **建议**：改「逃生口按需启用（备而不用）」或直接删「常态化」。可选。

## 未发现问题（逐条确认）

- **C-4 语义准确**：落档「生命周期变更 retire/update/use_component 必须走门面；绕过门面直接调 core sync API（如 Fiber::retire）对 sync-only 组件允许，但其 async 尾巴不被 settle 记账；（AsyncFiberHandle 解引用出的 fiber 亦应经门面操作）此分界是插件作者文档明示项」——与草案 §5 C-4（分界 + sync-only 例外 + 不记账后果 + 文档明示）逐点等价 ✓（Nit-1 仅措辞对应问题）。
- **O-2 一致**：草案「框架层保持显式；app 层封装决定」→ 落档「保持显式 settle()（框架层不提供自动 settle 封装；模式由 app 层封装）」 ✓；且与上游任何异步层现有 `settle()` 显式语义无冲突。
- **O-3 一致**：草案「默认不做、列此备查、若 C-4 频繁违反再启用」→ 落档「不启用既有 update_hook/retire_hook（草案默认；若 C-4 频繁违反再启用）」 ✓；`update_hook`/`retire_hook`（反引号非链接）指向 core 既有项（G1/G4 落地件）。
- **O-4 一致**：草案「等 app 层第一真实失败场景再定；草案先用 String」→ 落档「保持 String（AsyncFiberError 不变）；等首个真实失败场景再定」 ✓。
- **intra-doc / missing_docs**：`[`AsyncRuntime`]`（lib.rs 内 pub 项）链接有效；crate doc 非 pub 项无需 doc——`cargo doc --no-deps` 实测 **0 告警** ✓；`check` 通过（doc 无语法破坏）✓。
- **行为影响**：diff 仅 crate doc 行——`git show` 确认无行为改动；纯文档里程碑符合计划 H2「仅文档化」定义 ✓。
- **计划 Step 1 任务覆盖**：C-4 文档 ✓、O-2 默认仅文档 ✓、O-3/O-4 默认仅文档 ✓、无新测试（计划注明「若无新增决策实现则无新测试」）✓。

## 验证记录（实际执行）

1. `git show 23b75fa` — 确认 1 文件 +22/-1 纯 doc。
2. `GOCACHE=…/gocache cargo +1.97.0 doc -p cordis-async --no-deps` — **0 告警**（grep -cE "^warning|^error" = 0）。
3. `cargo +1.97.0 check -p cordis-async` — **通过**（`Finished`）。
4. 前置登记：H1（`fa44fd6`）存在且审查闭环；`AsyncFiberHandle` 定义于 `lib.rs:670`、`use_component → Result<AsyncFiberHandle, RegistryError>`（lib.rs:757）——H2 文档引用有效。

## 结论

H2 达成：C-4 门面纪律 + O-2/O-3/O-4 决策落档为纯文档、语义与草案/计划一致、门禁干净、无行为影响。**放行进入 H3**（P1.2 Step 2：Remote trait/AsyncCx::spawn_remote/TokioRemote 签名冻结标注，P1.3 前置 API 面稳定）。2 项 Nit 为措辞/一致性优化，可在 H3 顺手处理，不阻塞。
