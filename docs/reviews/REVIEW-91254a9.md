# 代码审查报告：commit `91254a9`（M0.2 drive 引擎 + I-1/I-2 测试，Phase 0）

- **审查对象**：`91254a9d535a746a1a05cb50607c9b8377774bb2`
- **审查范围（静态）**：仅 `git show 91254a9` / `git show 91254a9 --stat`，对照 `docs/cordis-async-protocol-draft.md` v1.4 §1（drive 伪码）与 §9（测试 1/2：I-1 复合逆 LIFO、I-2 步界 guard）
- **审查日期**：仓库时区（2026-08-18）
- **验证手段**：静态阅读（按要求不运行 cargo/测试，只跑 git show）

**改动统计**：2 文件，+168/-1。
- `crates/cordis-async/src/lib.rs` +42：删除骨架 `#![allow(dead_code)]`，新增 `drive` 引擎
- `crates/cordis-async/tests/protocol.rs` +127：新建 3 个单测（I-1 / I-2 / I-2 立即退场变体）

---

## 逐条发现

### 1. drive 实现与草案 §1 一致性（lib.rs:59-97）✅ 通过

与 `docs/cordis-async-protocol-draft.md` §1 伪码（行 56-77）逐行一致：

- **逐步 await + 步界 guard**：`loop { if !guard() { break; } match iter.next().await { ... } }` —— guard 在每次 `next().await` **之前**检查，与草案及 core `execute`（effect.rs:48-67）同款「每个步界检查」语义。✓
- **Failed → LIFO 恢复 + Err**：`for d in acc.into_iter().rev() { d().await; } return Err(e);` —— 与草案 §1 行 66-69 一致；先以应用逆序 await 收回已完成步骤，再上报失败。✓
- **Ok → 复合逆（应用逆序）**：闭包 `acc.into_iter().rev()` 逐个 await——与草案行 72-76、不变量 I-1 一致。✓
- **`Finished` 提前 break 后不再 guard**：`Finished(d)` 分支 `acc.push(d); break;` 直接跳出 loop，落在 Ok 分支。后续不再调用 guard——因为已无下一步，无需步界检查，与「guard 只在步界检查」语义自洽（I-2 的 guard 计数只计实际起点步）。✓

`drive` 是 core `execute`（Algorithm 1）的忠实 async 转写，`Failed` 臂为 async 层新增、与草案完全一致。

### 2. I-1 测试（tests/protocol.rs，`i1_composite_disposer_runs_lifo`）✅ 通过

`SeqIter::finished(&["a","b","c"])`：`next` 每步 `yield_now().await` 后按序产出 `Yielded(a) → Yielded(b) → Finished(c)`，`acc=[a,b,c]`。复合逆执行自然得 `rev:c → rev:b → rev:a`，断言 `vec!["rev:c","rev:b","rev:a"]`。

- **直证 LIFO**：三分量断言明确 `c,b,a` 应用逆序——直证 Thm 16 的 async 版。✓
- **drive 期不误执行逆**：`disposer()` 调用前先断言 `log.borrow().is_empty()`，证明 drive 只折叠、不执行逆。✓

### 3. I-2 测试（`i2_guard_false_at_step_boundary_keeps_inflight_step`）✅ 通过（见 nit-2）

guard 闭包计数并返回 `c < 3`。驱动轨迹：
1. guard#1(true) → await → `Yielded(a)`，`acc=[a]`
2. guard#2(true) → await → `Yielded(b)`，`acc=[a,b]`
3. guard#3(false) → break（c 的 `next()` 从未被调用）

断言 `checks.get()==3`（a/b/c 三个步界各一次）与 `log==["rev:b","rev:a"]`（c 未触及、a·b 已入账）。**验证了「guard 只在步界检查、guard 假时不执行逆、已完成步照常入账」**。✓

- **立即退场变体**（`i2_guard_false_immediately_yields_empty_composite`）：`guard=||false` → 首次即断 → 空复合逆，`disposer()` 后 log 为空——验证零步场景。✓

### 4. 潜在问题核验（所有权 / guard 形态 / 文档链接）✅ 通过

- **guard 为 `Fn`（多次调用）正确**：`guard: impl Fn() -> bool`，loop 内多次 `guard()` 调用要求 `Fn`（非 `FnOnce`/`FnMut`）；测试闭包用 `Rc<Cell>` 内部可变 + move 捕获，满足 `Fn`。与草案签名一致。✓
- **`acc` 在 Failed 分支消费后 Err 返回**：`acc.into_iter().rev()` 在 Failed 分支 move 消费 `acc` 后 `return Err(e)`——控制流不再进入 Ok 分支，故复合逆**不产生**（语义正确：Failed 不产复合逆），无二次 use 问题。✓
- **Ok 分支复合逆 move `acc`**：loop 结束后 `Box::new(move || { ... acc.into_iter().rev() ... }) as AsyncDisposer`——`acc` 被 move 进 `FnOnce` 闭包，`FnOnce` 恰调用一次，所有权/借用正确。`AsyncDisposer = Box<dyn FnOnce() -> LocalBoxFuture<()> + 'static>`，内部 `Pin<Box<dyn Future + 'static>>`，`acc` 中含 'static disposer → future 'static，类型成立。✓
- **文档链接**：`AsyncDisposer`/`AsyncStep`/`AsyncFiberError` 的 intra-doc 链接已用**全限定路径**（`` `cordis_core::effect::Disposer` `` / `` `cordis_core::effect::Step` `` / `` `cordis_core::fiber::FiberError` ``），即 M0.1 nit-1 的落地修复。核验三个目标在 cordis-core 均存在并公开（lib.rs:35-36 重导出；effect.rs:24/36、fiber.rs:38）→ 链接可解析，无 broken links。✓

### 5. 门禁 A（静态自述）✅ 静态合理

提交自述「fmt/clippy -D warnings 干净，cordis-async protocol 3/3，workspace 38 摘要行 0 FAILED」。静态核验：删除 crate 根 `#![allow(dead_code)]` 合理（类型已被 drive/测试使用）；测试内 `#[allow(dead_code)]`（`failed_at`）带注释理由（M0.4 失败通道用），为 scoped、非 blanket allow，不违 clippy `all=warn`+`-D warning`。语义 self-consistent，静态可信（无法运行工具复核，按指令）。

---

## 问题清单

### nit-1（低）：I-1 文档链接触发点
（说明性）`AsyncFiberError` 的 doc 注释未列于本次 diff，但 `AsyncStep` 的 `Failed` 变体 doc 及 `AsyncDisposer`/`AsyncStep` 注释本次已全限定——延续修复，无新增 broken link。非问题，仅记录。

### nit-2（低）：I-2 命名/注释对「在途步」叙事略过度
测试注释与函数名称「长 await 中途 guard 翻假 / 在途步 b 完成入账」，但实际轨迹中 **guard 在第 3 次检查（c 步界前）翻假，b 早已作为一个被完整 await 的常规步入账**，并非「b 的 await 挂起期间 guard 翻假、b 仍完成」。所验证语义（guard 只在步界、假时不执行逆、已完成步 a·b 入账、未起点 c 未触及）**正确且有效**；但「await 挂起期间 guard 翻假 → 在途步仍完成」这一 I-2 不变量的**直接要件**并未被该测试真实触发（guard 是同步计数的，翻假发生在边界而非挂起中）。由于 guard 只按步界求值、`next().await` 挂起期间绝不重查 guard，该要件由构造保证成立，故不构成语义错误——仅建议注释与函数名措辞收敛，或补一个「guard 在 await 内翻假」的用例以直证 I-2 的在途要件。

> 此项不阻塞合入：非逻辑缺陷，属测试覆盖面/命名精确性。

---

## 总体结论

✅ **通过**（无 major；2 项 nit 均为非阻塞小项）

- **major**：0
- **nit**：2（nit-1 仅记录；nit-2 I-2 测试对「在途步」的命名/注释略过度，非语义错误）

drive 引擎与草案 §1 逐行一致，Failed→LIFO 恢复+Err、Ok→复合逆应用逆序、Finished 提前 break 且不再 guard、guard 只步界检查等语义全部核验通过；I-1 直证 LIFO 且 drive 期不误执行逆；I-2 及其立即退场变体正确验证「guard 假时已完成步入账、未起点未触及、零步空复合逆」；acc 所有权（Failed 消费后 Err、Ok move 进 FnOnce）正确；guard 为 Fn 多次调用成立；intra-doc 链接已全限定且三目标均可解析（M0.1 nit-1 落地）；移除骨架 allow(dead_code) 合理、门禁 A 自述静态自洽。结论：**可合入**。
