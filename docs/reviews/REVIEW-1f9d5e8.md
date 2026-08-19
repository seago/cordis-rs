# 代码审查报告：commit `1f9d5e8`（loader 错误策略线 E0 类型面）

- **审查对象**：`1f9d5e83152f6eff83453318a3ff033115c1e61c` — `feat(loader): E0 错误策略类型面——EntryError/EntryErrorKind 四变体 + Display 三要素契约 + EntryOutcome/ApplyReport/EntryState + smoke 测试（错误策略线）`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`git show 1f9d5e8`（`crates/cordis-loader/src/report.rs` 新增 206 行 + `src/lib.rs` 挂 mod + pub use +3 行），对照冻结草案 `docs/cordis-rs-error-strategy-draft.md` **v0.2** §3/§6 与执行计划 `docs/cordis-loader-error-strategy-PLAN.md` E0。
- **验证手段**：静态阅读 + 实际运行 `cargo +1.97.0 test -p cordis-loader`（52/52 绿）。

---

## 总体结论

✅ **通过（PASS）**（Major 0 / Minor 0 / Nit 3）

E0 类型面与草案 v0.2 §3/§6 逐条一致，`core` **零改动**（`git show --stat` 无 `crates/cordis-core` 触碰）——统一护栏达成；纯类型面（无行为迁移，行为正确留待 E1）。

---

## 核查（逐条）

| 检查点 | 核验 |
|---|---|
| `EntryError { entry_id, kind }` + Display 三要素 | ✅ 与草案 §3/§6 一致；抽查 `ProvisionClash` 输出含 条目 id（web-search）+ 冲突键（web）+ owner（web-core）+ 原因（first-wins）；`ConfigValidation` 含 message（校验失败无键/组件概念，message 即原因——合理变通） |
| `EntryErrorKind` 四变体 | ✅ `UnknownComponent { component }` / `ConfigValidation { message }` / `ProvisionClash { keys: Vec<Symbol>, owner }`（keys 全列） / `UnknownParent { parent }`——与草案逐字一致 |
| `EntryOutcome`（Unchanged/Activated/Failed/FailedFiber{error}） | ✅ 与草案 §3 一致；`Failed` = 未挂载（OrchestrationError）、`FailedFiber` = 已挂载 Inactive(ζ)（ComponentFailure，既有语义不改）——注释如实 |
| `ApplyReport`（failed()/ok() + Display） | ✅ `failed()` 返回 `impl Iterator<Item=&EntryOutcome>`、`ok()` 无失败即真（空报告 ok 合理）；Display 每行一条 |
| `EntryState`（Loaded/Disabled/Failed/FailedFiber） | ✅ 与草案 §3 一致 |
| lib 集成 | ✅ `mod report;` + `pub use report::{ApplyReport, EntryError, EntryErrorKind, EntryOutcome, EntryState};`（loader 新 pub API 面——评审 n-3 记于 loader 里程碑，符合计划） |
| core 零改动 | ✅ 本 commit 不触碰 `crates/cordis-core`（`git show --stat` grep 0 处） |
| 纯类型面 | ✅ 无 loader 行为改动依赖（E1 前置面）；smoke 测试直证 Display/报告/状态 |

## 发现

### Major：无

### Minor：无

### Nit

### Nit-1（可选）：`ApplyReport` Display 行的前缀冗余

- **位置**：`report.rs` `impl Display for ApplyReport`——`writeln!(f, "条目：{line}")` 且 `Failed(e)` 行 `line = format!("失败：{e}")`、`e` 自身又以 `条目 "id" ...` 开头——失败行输出为 `条目：失败：条目 "web-search" 供给冲突：...`（三重语义前缀嵌套）。
- **建议**：草案 §6.2「每行一条：`条目 "x"：<状态>`」——可对 `Unchanged`/`Activated`/`FailedFiber` 用 `条目：<状态>`、`Failed(e)` 直接用 `{e}`（其已含 `条目 "id"` 前缀），消除冗余；或统一「`<id>`：<状态>」形态。非阻塞。

### Nit-2（可选）：`EntryOutcome` 未 derive `PartialEq/Eq`

- 草案 §3 示例 `EntryOutcome` 亦仅 `Clone, Debug`（同稿一致，未偏离）；但测试/报告面断言 `outcome == Failed(...)` 时缺 `PartialEq` 不便。建议 E1 起按需补 derive（草案未要求，符合）。

### Nit-3（确认）：`fmt_keys` 与 `Symbol Display`

- `fmt_keys` 用 `s.to_string()`（Symbol 的 Display 为驻留名）——输出 `[web]` 形态与草案示例一致；`Vec<Symbol>` 全列满足 m-3（keys 多条全列）✅。仅确认。

---

## 验证记录

- `GOCACHE=.../gocache cargo +1.97.0 test -p cordis-loader` — **PASS**，52/52（含 E0 新增 smoke 测试 3 条；既有 49 条回归绿）。
- clippy/fmt/doc 由委派方本地验证绿（本审查重点为静态核对 + 测试，未复跑命令验证——如发现涉及编译行为可补跑）。
- `git show --stat 1f9d5e8` — 仅 `crates/cordis-loader/src/{report.rs(+,lib.rs+3)}`，无 `cordis-core`。

---

## 结论

E0（类型面）与草案 v0.2 §3/§6 完全对齐，`core` 零改动、纯类型无行为泄漏，Display 三要素契约与四变体载荷准确无偏差，entry 状态面完整——**建议放行进入 E1**（OrchestrationError 迁移：`validate_config` 改 `Result`、未知组件/`use_component` 失败（ProvisionClash/UnknownParent）不 panic → 报告 + 跳过 + 每次 apply 重试；`apply` 返回 `ApplyReport` + 协调序汇聚）。3 项 Nit 记录在案，不阻塞。
