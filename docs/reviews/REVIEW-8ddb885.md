# 代码审查报告：commit `8ddb885`（PR #2 feat(core): Symbol/Key/KeySet/Store + 参考解释器）

- **审查对象**：`8ddb885f8fdc869f280b796c76ad19a74d42afdf`（相对于 `b304714`），10 个文件，+873/-35 行
- **审查日期**：2026-08-15（仓库时区）
- **验证手段**：本地 clone 完整阅读 diff；`cargo test --all` 全绿（cordis-core 7 个测试）；`cargo clippy --workspace --all-targets` 零警告（rustc 1.95 / edition 2024）
- **范围说明**：GitHub MCP 无法访问该私有仓库，审查基于本地 clone，与远端 commit 一一对应

---

## 🔴 必须修复（major）

### M1. THEORY-MAP.md 声称的"单元"测试不存在——Store/Symbol/Key 核心契约零直接覆盖
**位置**：`docs/THEORY-MAP.md`（Symbol / `𝒱 k` / KeySet / Store 四行均标注"单元"测试）+ `crates/cordis-core/src/{symbol,key,keyset,store,fiber}.rs`

**事实**：本 commit 全部 7 个测试都在 `interp.rs` 的 `tests` 模块中；`store.rs`、`symbol.rs`、`key.rs`、`keyset.rs`、`fiber.rs` **没有任何 `#[cfg(test)]`**。尤其是 `Store`（Def 22–24 的核心契约：`AlreadyBound`/`NotBound`/`TypeMismatch` 三分支、`unbind` 的"类型检查先于移除、失败不改变状态"、`satisfies` 的 ∀ 语义）——这些前置条件语义是后续 PR #3 生产引擎直接依赖的，目前只被 `interp.rs` 间接使用 `KeySet`/`Symbol`，`Store` 本身零覆盖。

**为什么是问题**：文档宣称有测试（误导审查与走查记录），且最容易回归的边界行为（如 `unbind` 类型不匹配时绑定保持、同符号双键的 `TypeMismatch`）没有任何护栏。

**建议**：为 `store.rs` 补一组直接单元测试（bind/get/unbind 三分支 + `satisfies` + `symbols` 往返），`symbol.rs` 补 intern 同一性/`as_str` 往返测试，并同步修正 THEORY-MAP 中与实际不符的标注。

### M2. `Symbol` 的 `Ord`/`Hash` 基于进程内分配序——"跨运行可复现"声明不成立
**位置**：`crates/cordis-core/src/symbol.rs:27-28`（`Symbol(u32)`，id 由 `intern` 调用序分配）；`crates/cordis-core/src/keyset.rs:8-9`（"`BTreeSet` 保证确定性的迭代顺序（跨线程、跨运行可复现）"）；`interp.rs:15`（`View = BTreeMap<Symbol, FiberId>`）

**事实**：`Symbol` 的 `u32` id 取决于**进程内首次 intern 的顺序**。同一名称 "db" 在两个进程（或 wasm 宿主与原生引擎）中可能得到不同 id。因此：

- `KeySet`/`View` 的 `Eq`/`Hash`/`Ord` 只在本进程内自洽，跨进程比较会得出错误结论；
- `KeySet::iter()` 与 `View` 的迭代顺序**跨运行不保证一致**（取决于谁先 intern），与 `keyset.rs:8` 的"跨运行可复现"声明矛盾；
- `interp.rs:15` 的 `View` 文档序同理。

**为什么是问题**：oracle 的设计目的是给真实引擎（PR #3 起，包括 wasm 后端）做基准对比。若任何跨进程对比发生（哪怕只是 Debug 输出 diff），id 序差异会产生假阳性/假阴性。当前单进程场景功能无误，但文档声明过头，且这是未来最容易踩的坑。

**建议**：二选一——(a) 修正文档，明确"进程内确定性"；或 (b) 若跨进程顺序/相等性会成为需求，将 `Symbol` 的序改为按驻留名称排序（如存 `&'static str` 并 `BTreeMap<&'static str, ...>`），或在 THEORY-MAP 已知偏差中记录此限制。

---

## 🟡 建议修复

### m1. `reload` 对未知 fiber 报 `NoTarget` 而非 `UnknownFiber`——与 `unload` 不一致
**位置**：`crates/cordis-core/src/interp.rs:240-243`（对比 `:256-257`）；`Violation::UnknownFiber` 文档（`interp.rs` 约 70 行）声称 "O-Retire / O-Remove / **L-\***：`n ∉ dom(Fγ)`"

**事实**：`reload` 先执行 `self.target(n)`——对不存在的 `n`，`target` 内部 `self.fibers.get(&n)?` 返回 `None`，于是 `reload` 走 `Err(NoTarget)` 分支；而同类的 `unload` 先 `get_mut` 再算 target，正确返回 `UnknownFiber`。同一类前提违反（`n` 不存在）在两个 L- 规则中产生**不同错误码**。同理，`reload` 对**已退役** fiber 报 `NoTarget` 倒是符合 target 定义（式 41），无问题。

**建议**：`reload` 中先 `self.fibers.get(&n).ok_or(Violation::UnknownFiber)?` 校验存在性，再计算 target。同时补一个断言测试固定该行为。

### m2. `Store::symbols()` 文档声称"确定性序"，底层是 `HashMap`
**位置**：`crates/cordis-core/src/store.rs`（`symbols()` 的 doc 注释"已绑定的符号集合（确定性序）"），实现为 `self.bindings.keys().copied()`，`bindings: HashMap<Symbol, Binding>`（store.rs 约 33 行）

**事实**：`HashMap`（默认 `RandomState`）的迭代序跨运行不稳定，与 doc 注释矛盾；也与同文件 `KeySet` 刻意用 `BTreeSet` 保证确定性的设计意图不一致。`Symbol` 已实现 `Ord`，无阻碍。

**建议**：`bindings` 改用 `BTreeMap<Symbol, Binding>`（或删掉"确定性"字样）。这直接关系到 M2 的 oracle 对比需求。

### m3. CI 删除 `RUSTFLAGS: "-D warnings"` 无理由
**位置**：`.github/workflows/ci.yml`（-1 行）

**事实**：当前 workspace `clippy --workspace --all-targets` **零警告**，删除该门禁没有技术必要性；且与 `Cargo.toml` 中 `workspace.lints` 的注释策略（"骨架阶段全量开启；个别 crate 需要局部放宽时在该 crate 内覆盖**并注明理由**"）相矛盾——在 `ci.yml` 里删门禁正是"无理由放宽"。

**建议**：恢复 `-D warnings`；若某 crate 确有无法消除的警告，按既定策略在该 crate 内局部覆盖并注明理由。

### m4. `insert` 的 `ProvisionClash` 检查包含已退役 fiber——未在已知偏差中记录
**位置**：`crates/cordis-core/src/interp.rs`（`insert` 中 `self.fibers.values().any(|f| f.provide.intersects(...))`）

**事实**：检查对 `dom(Fγ)` 中**所有** fiber 生效，包括 `retired = true` 且已停用（`table = ∅`，实际不再提供任何键）的 fiber。若论文 O-Insert 前提 `∀m ∈ dom(Fγ). p ∩ p_m = ∅` 的 `dom` 含退役成员，则实现正确——但这意味着"退役组件的供给名被占用直到 remove"，是一个值得明示的语义决策。

**建议**：与论文核对后在 THEORY-MAP 已知偏差表补一行（当前未记录），或补测试固化该行为（如"退役未移除的 fiber 仍阻挡同名供给"）。

---

## ⚪ 细节（nit）

1. **`symbol.rs:38-41` 每符号两份字符串拷贝**：`by_name` 持有 `Box<str>`，同时 `Box::leak(boxed.clone())` 再存一份。可改为 `by_name: HashMap<&'static str, u32>`（借用已泄漏的字符串，`HashMap<&'static str, _>::get(&str)` 可用 `Borrow` 查找），省一份拷贝。
2. **`symbol.rs:16-24,33,47` 全局互斥锁热点**：`as_str`（`Display`/`Debug`/迭代热路径）每次锁全局 `Mutex`；且锁内若触发 `expect`（如 37 行 u32 溢出），整个进程的 Symbol 系统永久毒化。当前锁内代码简单、风险低，oracle 场景可接受；建议在文档注明或后续换无锁方案（如 `dashmap`/`phf`）。
3. **`interp.rs` `Violation::NameExists` 为死代码**（文档已说明"名字恒新鲜，不会发生"）。保留论文规则映射完整性可接受，无需处理。
4. **`interp.rs:255-256` `unload` 先计算 `target` 再检查 state**：对 `NotActive` 场景白算一次 `target`（O(|F|·|d|)），先查 state 更清晰且省算。
5. **`fiber.rs` `FiberId::fresh` 为 `pub(crate)`**：测试（`interp.rs` 的 `unknown_parent_rejected`）直接以 `fresh(&mut 100)` 造"幽灵 id"，绕过了"名字只能由系统分配"的 Def 45 纪律。可接受，但可提供 `#[cfg(test)]` 专用构造器。
6. **`symbol.rs:58-62` `Debug` 不转义**：名称含换行/引号时破坏日志格式。nit 级别。
7. **`interp.rs` `drive_to_quiescence` 步数上界 `8·|F|+8` 为经验值**：已有断言 panic 兜底（oracle 自检语义），可接受；建议在注释中写明推导依据，防止未来规则变化导致误伤。

---

## 整体评价

**优点**：规则即代码的直译质量很高——O-/L- 规则的前提检查、`target`/`quiet`/`support_set` 派生量与 Def 45/46/67-70 对应清晰；`unbind` 的"类型检查先于移除"、"违反前置条件不产生状态变更"等细节处理正确；测试覆盖了依赖激活序、撤退级联、confluence 全交错枚举，且全部通过；`unsafe` deny、`missing_docs` deny 下编译干净；`interp` 类型刻意不 root 导出的命名冲突规避也是好决策。

**必须修复**：M1（补 Store/Symbol 直接测试并修正文档标注）、M2（修正"跨运行可复现"声明或改为名称排序）。
**建议修复**：m1–m4（reload 错误分类、`symbols()` 确定性、恢复 `-D warnings`、ProvisionClash 退役语义记录）。
**可忽略**：nit 1–7 均不阻塞合入。

**置信度**：高——所有"事实"类结论均经本地实测（测试/编译/lint）或代码直接核验；唯一无法完全确认的是 m4 与论文 O-Insert 前提的精确对应关系（论文文本不在本地，已如实标注为"需与论文核对"）。
