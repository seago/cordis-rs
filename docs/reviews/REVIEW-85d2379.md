# 代码审查报告：commit `85d2379`（M1.1 crate 骨架，Phase 1）

- **审查对象**：`85d2379570c4ee953b227daa533d7bc94d69f305` — `feat(events): M1.1 crate 骨架——Event trait + 监听器类型 + EventBus/EventsProvider 占位，零依赖（Phase 1）`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`git show 85d2379`（`crates/cordis-events/{Cargo.toml,src/lib.rs}` 新增 138 行 + 根 `Cargo.toml` members 增补），对照受冻草案 `docs/cordis-events-protocol-draft.md` **v0.3.1** §1/§2.1/§3.1 与执行计划 `docs/cordis-events-PHASE1-PLAN.md` M1.1。
- **验证手段**：静态阅读审查（门禁由委派方本地实测：test 1/1、clippy `-D warnings`、fmt、doc 0 告警、workspace 无回归、`cargo tree -p cordis-events` 仅 cordis-core）。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：0
- **nit**：2（`Default` 骨架期略多余但无害；M1.2 建议验收测试移至 `tests/` integration 测 pub API）

M1.1 骨架与草案 v0.3.1 逐条对齐，范围为纯协议类型占位、无行为泄漏（订阅/派发行为正确留待 M1.2）；`Send+Sync` 上界（M-1' 闭环）、零第三方（m-2）、`_private:()` 防外部构造均成立。可放行进入 M1.2。

---

## 证据核验

| 检查点 | 核验 |
|---|---|
| §1 `trait Event { type Payload: 'static; const SYMBOL: &'static str }` | ✅ 逐字一致（v0.3.1 的 `SYMBOL` 命名，n-1 落地；doc 注明与 `Key::SYMBOL` 同构、`Symbol::intern` 同驻留） |
| §2.1 监听器别名 ×4 带 `Send + Sync + 'static` | ✅ 全部带上界（M-1' 闭环）；`EmitListener`（`Fn(&P)`）/`WaterfallListener`（`&mut + next`）/`SerialListener`（`->R`）/`BailListener`（`->Option<R>`）与草案逐字一致（M-2' 方案 2） |
| §3.1 `EventsKey`（`Value = Arc<EventBus>`，SYMBOL `"events"`） | ✅ C-1 Arc 惯例；`EventsKey` 为 unit struct，`Arc<EventBus>` 满足 `Key::Value: Send+Sync`（当前 `EventBus` 无字段天然 Send+Sync） |
| §3.1 `EventsProvider`（core 原生 `once` 绑定） | ✅ 仅 `cordis_core::once`，不引 cordis-native（m-2 零依赖定位）；inject 空 / provide `[events]`；apply 绑定 `EventsKey`，逆经 ctx 累加器登记 |
| M1.1 范围克制（无行为泄漏） | ✅ 仅类型层（trait/别名/结构/组件声明）——`on`/`on_waterfall`/`on_serial`/`on_bail` 与四派发**均未出现**，行为正确留待 M1.2 |
| `_private: ()` 防外部构造 | ✅ 私有字段阻止外部 `EventBus {}` 字面构造（只能经 `new()`）；`_` 前缀不触发 dead_code |
| crate doc §0 核心义务 | ✅ 首段最显眼处明示「监听器闭包须 `Send+Sync`、不得捕获 Rc、服务经 Arc、O-6' 变体」 |
| 零第三方 | ✅ `Cargo.toml` 仅 `cordis-core` path；workspace members 增补正确 |
| deny(missing_docs) / 测试 | ✅ 全部 pub 项有 doc；sanity 测试直证构造/义务/符号/d-p |

---

## 发现

### Major：无

### Minor：无

### Nit

### Nit-1：`impl Default for EventBus` 骨架期略多余（无害）

- **位置**：`src/lib.rs` `impl Default for EventBus`（`default()` 即 `new()`）。
- **问题**：M1.1 骨架无任何 `Default` 消费点，属前瞻便利；保留无副作用，故仅 Nit。
- **修法**：可选。保留（M1.2 后总线内部有字段，`Default` 语义仍需定义）；或 M1.1 期暂不留待 M1.2 一并给。不阻塞。

### Nit-2：M1.2 起验收测试建议移至 `tests/`（integration）以测 pub API

- **位置**：`src/lib.rs` `#[cfg(test)] mod tests`（sanity 用 lib 内测）。
- **问题**：lib 内测试可访问私有项，测不到「外部 crate 视角」的 pub API 面；M1.2 的验收 #1–#9 多为对 `EventBus` 公开方法的黑盒断言，integration 形式更能固化外部契约。
- **修法**：M1.2 起验收测试放 `crates/cordis-events/tests/`（同仓库 cordis-async 的 `tests/protocol.rs` 惯例）。M1.1 的 sanity 保留 lib 内亦可。

---

## 结论

M1.1 骨架与草案 v0.3.1 完全对齐，类型声明、`Send+Sync` 上界、零依赖定位、范围克制全部成立，无逻辑缺陷。**建议放行进入 M1.2**（订阅/派发核心：on/on_waterfall/on_serial/on_bail + emit/waterfall/serial/bail + 冲突检测四规则 + 快照/release-then-invoke + 幂等 disposer，验收 #1/#2/#5/#6/#8）。2 项 Nit 记录在案，不阻塞。
