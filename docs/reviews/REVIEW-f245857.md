# 代码审查报告：commit `f245857`（PR #3 可逆效应引擎）

- **审查对象**：`f24585734172c445da88a1507946d47888cbb571`（相对 `8ddb885`），6 个文件，+526/-8 行
- **审查日期**：2026-08-15（仓库时区）
- **核心代码**：`effect.rs`（220 行：execute/once/EffectHandle）、`context.rs`（202 行：Context::effect/dispose_all）、lib.rs 导出、PLAN/THEORY-MAP 文档
- **验证手段**：`cargo test -p cordis-core` **28/28 全绿**（本 commit 新增 effect 5 + context 4 共 9 个测试）；`RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings` 零警告（工作树 HEAD 含本 commit 全部代码）

---

## 🔴 必须修复 / 需核对（major）

### M-A. 嵌套效应的撤销顺序与"应用逆序"冲突——且零测试覆盖
**位置**：`context.rs:52-59`（push 时机）+ `context.rs:67-72`（dispose_all）

**事实**：`Context::effect` 的入栈（`self.dispose.push`，context.rs:59）发生在 `execute(callback(), guard)` **同步完成之后**（:52）。因此当外层迭代器的某一步内部再调用 `ctx.effect`（嵌套效应）时：

- 时间线：外层 step1 效应 E1 应用（t1）→ 内层效应 E3 应用（t4）→ 外层 step2 效应 E2 应用（t5，内层在其内部）→ 内层句柄入栈（t4）→ **外层句柄入栈（t6，更晚）**
- `dispose_all` drain + rev → 外层句柄先运行：`E2逆 → E1逆 → E3逆`

而 Thm 16(1)（本 commit 测试注释自己的表述"逆序撤销——最后产出的逆最先运行"）以**应用顺序**为基准：应用序 E1 < E3 < E2，期望撤销序 E2 → E3 → E1。**实现把 E3（内层）排到了 E1 之后**——若内层效应的逆依赖其先撤销（事务嵌套语义），顺序即被破坏。

**为什么是问题**：这是 Thm 16 核心语义路径上的真实分歧点。若论文 Algorithm 1 第 17 行 `ctx.dispose ← dispose ∘ ctx.dispose` 以**注册序**组合（后注册在左、先执行），则实现与论文一致（外层后注册 → 先撤销）；若以应用序为准，则实现违反。本地无论文文本，无法判定。且**嵌套场景没有任何测试**（9 个新测试全部是单层）。

**建议**：① 用论文 Algorithm 1 第 17 行核对嵌套撤销顺序语义；② 无论结论如何，补一个嵌套测试（外层迭代器多步 + 中间步内部嵌套内层效应，断言 dispose_all 后的完整撤销序），固化行为；③ 若确认应以应用序为准，需调整结构（如嵌套深度感知的入栈，或 execute 期间内层句柄插入到外层句柄之前）。

### M-B. `Context::effect` 对非终止迭代器：execute 永不返回 + 无界内存 + guard 中断路径实际不可达
**位置**：`effect.rs:36-48`（guard 循环）、`context.rs:46-52`（guard = armed）

**事实**：
- `execute` 只能被两件事中断：guard 失效或迭代器 `Finished`（effect.rs:38-45）。
- `Context::effect` 内部 guard 绑定 `handle.is_armed()`（context.rs:48-51）；armed 只在 `dispose()` 置 false（effect.rs:104-116）；而 disposer 在 `execute` **返回之后**才创建（context.rs:52-58）。因此 execute 运行期间**无人能 disarm**——guard 中断路径在 effect() 内部是不可达的理论路径。
- 后果：若迭代器不终止（`EffectIter` 协议未要求终止），`execute` 同步阻塞永不返回，且每次 `Yielded` 都 push 一个逆进 `acc`（effect.rs:42）——**内存无界增长**。

**为什么是问题**：模块文档（effect.rs:1-7）说"驱动 `iter` 直至 `guard` 失效或迭代终止"，未明示"经 `Context::effect` 注册的迭代器必须终止"；公开的 `EffectIter` trait 无终止义务。这是 API 陷阱：订阅型/无限效应（论文 notify 类）当前阶段必然挂死。

**建议**：在 `effect` 与 `execute` 的文档中明示"当前同步核心要求迭代器终止（PR #5 async 提供无限迭代支持）"，或对 `Context::effect` 加步数上限断言（oracle 风格，超限 panic 提示协议违反）。

---

## 🟡 建议修复（minor）

### m-A. `Context::store()` 借用与 `effect` 内部 `borrow_mut` 的运行时 panic 未文档化
**位置**：`context.rs:31-34`（`store()` 返回 `Ref<'_, Store>`）

调用者若持有 `ctx.store()` 的 `Ref` 期间再调用 `ctx.effect(...)`（内部 `store_cell().borrow_mut()`）→ `RefCell` 双重借用运行时 panic。单线程 RefCell 的正常代价，但作为公开 API 应文档提示（"调用 effect 时不得持有 store() 借用"），或 `store()` 改用 `try_borrow` 返回 Result。

### m-B. `EffectHandle::install` 可覆盖旧任务（静默丢弃）
**位置**：`effect.rs:97-100`

`install` 无条件覆盖 `task`——若被调用两次，第一个 Disposer 被丢弃**永不运行**（撤销丢失）。当前仅 `Context::effect` 单调用点，安全；但 `pub(crate)` 接口无防护，建议 `debug_assert!(self.task.borrow().is_none())` 或文档注明"至多一次"。

### m-C. panic 策略未记录：单步逆 panic 会中止剩余撤销
**位置**：`effect.rs:49-53`（LIFO 折叠无 unwind 保护）、`context.rs:68-71`（dispose_all 已 drain，首个 panic 后其余 disposer 永久丢失）

单线程宿主下这是"panic 即不一致"的静默路径。若项目策略是"panic = bug"（oracle 阶段成立），建议在模块文档明确；否则需 `catch_unwind` 保护或逐项 try。当前阶段可接受，但应记录。

### m-D. 提交卫生：`REVIEW-8ddb885.md`（88 行审查报告）混入 feat 提交
**位置**：commit 文件列表

审查报告是文档/chore 内容，混入 "feat(core): 可逆效应引擎" 提交，提交信息与内容不符（且该文件在 `1440fce` 中才被移入 `docs/reviews/`）。建议此类文档独立提交或在 commit message 说明。

---

## ⚪ 细节（nit）

1. **`context.rs` `disposer_is_idempotent` 测试注释**："`Box<dyn FnOnce>` 自带 must_use"不准确——`Box<dyn FnOnce()>` 并无 `#[must_use]`，`drop()` 仅是表意。无害。
2. **`effect` 签名 `self: &Rc<Self>` 的原因未文档化**（回调需把 ctx 克隆进迭代器闭包）——建议在 doc 注明调用方需持有 `Rc<Context>`。
3. **`Once::next` 二次调用 panic**（effect.rs:69-72）：协议说明充分（"恰好产出一步（协议违反）"），行为正确，无需改。

---

## 正面确认（与文档一致、实现正确的点）

- **armed 幂等**（effect.rs:104-116）：`dispose` 先置 false 再取任务，跨"返回副本 + 累加器副本"双路径只撤销一次，且 `dispose_all` 中重入调用安全（无借用冲突：`take()` 后运行任务）——测试 `disposer_is_idempotent` 覆盖到位。
- **guard 步界中断语义**：guard 在迭代边界检查（effect.rs:37-40），step 内失效不影响已执行步的逆收集——与文档"§4.3.2 步界中断"一致，测试 `execute_interrupts_at_step_boundary` 验证正确。
- **`dispose_all` 先 drain 再运行**（context.rs:68）：运行期间允许注册新效应且不会误恢复——正确的重入设计。
- **dead guard 恒等逆**、**Finished 后不再驱动**：均有测试。
- **文档纪律**：THEORY-MAP 已知偏差新增 2 条（同步迭代器、组合时机）、Thm 7/16 标注完成、Cor 21 诚实推迟——与仓库风格一致。

---

## 总结

- **必须处理**：M-A（嵌套撤销顺序——需论文核对 + 补嵌套测试，这是 Thm 16 核心语义的盲区）、M-B（非终止迭代器挂死/无界内存——补文档或上限断言）。
- **建议处理**：m-A ~ m-D（借用 panic 文档化、install 覆盖防护、panic 策略记录、提交卫生）。
- **nit**：1–3 可忽略。

**置信度**：高——所有代码事实与行为推演均经实测/直接核验；M-A 中"实现是否违反论文"的最终判定依赖论文 Algorithm 1 第 17 行原文（本地无论文，已如实标注为需作者核对）。


