# 代码审查报告：commit `c0fb7c1` + `2086e27`（错误策略线 E2：报告面 hook + events 衔接）

- **审查对象**：`c0fb7c1`（feat: E2 EntryFailedHook 注入 + events 桥接 + 验收 #3/#6）+ `2086e27`（fix: clippy slice::from_ref）
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`crates/cordis-loader/src/{lib.rs, report.rs}` + `crates/cordis-events/tests/error_bridge.rs`（新增），对照草案 v0.2（§7 报告面 + events 衔接、§9 验收 #3/#6）与计划（E2）。
- **验证手段**：静态阅读 + 实测 `cargo +1.97.0 test -p cordis-loader -p cordis-events -p cordis-hmr`（57 / 14 / 9 全绿）、`clippy --all-targets -- -D warnings`、`fmt --check`、`doc -p cordis-loader --no-deps`（0 告警）、`git diff 9b61d82..HEAD -- crates/cordis-core`（空 = core 零改动）。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **Major**：0
- **Minor**：0
- **Nit**：2（其一为 `error_bridge.rs` 的 unused import——**实际破坏 `clippy --all-targets -- -D warnings` 门禁**，与委派方「已验绿」声明不符，须修一行）

E2 报告面 hook + events 衔接 + 验收 #3/#6 与草案 v0.2 §7/§9 一致；`EntryFailedHook` 注入保持 loader 零依赖 events（run-deps 仅 core，E-2 决策微调合理、吻合草案 §7 integration 点）；桥接回环真实直证；core 零改动成立；测试全绿。

---

## 发现

### Major：无

### Minor：无

### Nit

### Nit-1（应修）：`error_bridge.rs` unused import——实际触犯 `-D warnings` 门禁

- **位置**：`crates/cordis-events/tests/error_bridge.rs:5-6`——`use cordis_events::{..., EventBus, ...}` 与 `use cordis_loader::{..., EntryErrorKind, ...}` 未使用。
- **问题**：实测 `cargo +1.97.0 clippy -p cordis-events --all-targets -- -D warnings` 报 2 个 unused import（`EventBus`、`EntryErrorKind`）——在 `-D warnings` 下为编译错误，**Gate A 的 workspace clippy 会红**。「委派方已验绿」与实测不符，须修正一行后方可合入/放行。
- **修复**：从两行 import 中移除 `EventBus` 与 `EntryErrorKind`（测试均未直接引用其名；`EventBus` 类型经 `ctx.get::<EventsKey>` 推导、`EntryErrorKind` 未用）。

### Nit-2（可选）：hook 回调不设防 loader 重入（应用层纪律注记）

- **位置**：`lib.rs` `apply` 尾部 `for Failed in report { hook(e) }`；`EntryFailedHook` doc。
- **问题**：hook 回调内（如订阅者收到事件后）若反调 `Loader::apply`（重入）会递归协调——events 订阅者通常只记录，理论上存在重入递归路径（应用层纪律，非本层缺陷）。
- **建议**：`EntryFailedHook` doc 加一句「回调内不得重入 `Loader::apply`（重入未定义）；如需在失败后修订配置请经事件侧异步/外部队列」——一行 doc 收口。可选。

---

## 核查通过项

- **hook 注入 + 零依赖（E-2 决策微调）**：`EntryFailedHook = dyn Fn(&EntryError)` + `register_entry_failed_hook(Option<Rc<..>>)`——loader 不直接依赖 events，发射由注入的 hook 完成；`cargo tree -p cordis-loader` run 分支不含 events（dev 才见），**保持 loader run-deps 仅 `cordis-core`**；与草案 §7「events 衔接（可选 integration 点）」吻合，偏离计划默认（loader 直接发射）合理且已注记。
- **回调覆盖面**：`apply` 尾部仅对 `EntryOutcome::Failed`（OrchestrationError）回调——**不含 `FailedFiber`**（ComponentFailure/L-Raise 通道保持既有观察（`fiber` 状态/retire hook），不与本 hook 重复）——正确互补。
- **events 桥接回环**（error_bridge）：Loader + EventsProvider 同 core ctx、`subscribe(loader/entry-failed)`、hook→`bus.emit`（sync 派发）→ 订阅者收到含「组件名 + 原因」——回环真实直证；`EntryError` 作 `Event::Payload` 合规。
- **验收 #3（重试复活）**：失败不写回不挂载（`fiber("p").is_none()`）、desired 未变下次 apply 重试并重报 `Failed`（且断言**非** `Unchanged`）、修配置 + revision bump → 复活 `Activated`——v0.2 决议直证完整。
- **验收 #6（Display）**：`ApplyReport` Display 快照含 `条目 "x" 未知组件 "y"`（id + 组件名）+ `已激活`——契约（草案 §6）成立。
- **clippy fix（2086e27）**：`slice::from_ref(&bad)` 避免 `bad.clone()`——合法。
- **core 零改动**：`git diff 9b61d82..HEAD -- crates/cordis-core` 为空 ✅。
- **门禁实测**：loader **57/57**、events **14/14**、hmr **9/9** 全绿；`fmt --check` 干净；`doc -p cordis-loader --no-deps` 0 告警。（clippy 见 Nit-1。）

---

## 结论

E2（报告面 hook + events 衔接 + 验收 #3/#6）语义与草案 v0.2/计划一致，核心机制正确、桥接回环真实、core 零改动、测试全绿。**建议放行进入 E3**（既有测试迁移收尾 + 验收 #8 panic 边界护栏 + 出口走查/EXIT）。

通过前须处理 1 项 Nit-1（`error_bridge.rs` 移除两个 unused import——否则 workspace clippy `-D warnings` 门禁红）；Nit-2 可选（hook doc 一句话）。
