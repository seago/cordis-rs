# 产品验证线 P-1 详细计划 —— wasm 逆表回收（P-1）

**依据**：Phase 2 提案（`docs/cordis-PHASE2-PROPOSAL.md`，决策：C 组合 / P-1..P-7 全做 / 顺序 P-1→…）；REVIEW-2a7a686 m3（`InstanceState.core_inverses` 槽位单调增长，已知边界：与组件生命周期内 set 次数同阶；M2 提供回收）。
**命名**：产品验证线（Product Validation Line）。
**状态**：**草案——待开工指令**。
**保证**：Gate A/B 同前；commit 分 code/docs；**不改 core**（纯 cordis-wasm 层）；wasm 全套回归绿（含 go）。

---

## 0. 问题与目标

- **现状**：`InstanceState.core_inverses: Vec<Option<CoreInverse>>` 按 `next_rep` 单调分配；`HostInverse::drop` 为 no-op（REVIEW-2a7a686 m3）——长驻组件生命周期内每 `set` 占一槽，只增不减（内存随操作次数线性增长，与峰值并发逆数无关）。
- **目标**：组件生命周期内**回收已销毁/已执行逆的 rep 槽位**，使分配量有界（≈ 峰值并发逆数）而非操作次数；不破坏跨边界逆句柄语义。

## 1. 设计（回收方案）

### 1.1 free list 复用（核心）
- `InstanceState` 增 `inverse_free: Vec<u32>`（可复用 rep 池）。
- **分配**：`context::set` 的 rep 分配改为"优先从 free list 取，空则 `next_rep` 自增"。
- **回收时机（两个可靠信号）**：
  1. **`HostInverse::drop`（资源句柄销毁）**：wasmtime 在资源引用计数归零（无任何跨边界句柄存活）时调用——把 rep 入 free list（**安全前提**：drop = guest 侧句柄已销毁，旧句柄不可能再引用该 rep；若协议违反（旧句柄调用 run）→ 既有 `HostInverse::run` panic = bug，不静默错）；
  2. **`run_inverse` 执行后**（核心逆已取走/执行）：槽位释放 → rep 入 free list。
- **不重映射**：`core_inverses` 仍按 rep 索引的 `Vec<Option<..>>`（无压缩/无重编号——跨边界句柄 id 稳定性保持）。

### 1.2 语义安全复核
- rep 复用仅发生在"对应句柄已销毁（drop）或逆已执行"——与协议 m4（guest 不得调用 run；drop 由宿主驱动）不冲突；
- 复用后旧 rep 的残留槽位（若有）在复用前清空（take）——无脏数据；
- `next_rep` 单调计数器**不再代表表长**（表长 = 活跃逆数 + free 池）；调试断言改以 `core_inverses.len()` 为准。

## 2. 分步

### Step P1-1：free list 机制（P1-1）
- **任务**：`inverse_free` 字段 + 分配走 free list + `HostInverse::drop` 入池 + `run_inverse` 后入池；调试断言更新。
- **验证**：既有 wasm 全套绿（逆撤销路径不回归）。

### Step P1-2：有界性验收（P1-2）
- **目标**：长驻循环 set/undo 后 rep 分配量有界。
- **任务**：单测（宿主层模拟或 guest 长驻组件）：N 次 set → undo（句柄 drop / 逆执行）循环 → 断言 `next_rep`（或表长峰值）**有界**（≤ 并发峰值 + 常数），而非 O(N)。
  - 宿主层单测（不经 wasm）：直接测 Host/InstanceState 的 rep 分配/回收循环。
- **验证**：新增 1–2 测试 + 全套回归。

### Step P1-3：出口（P1-3）
- **任务**：门禁全绿 + EXIT 文档（`docs/cordis-PRODUCTVAL-P1-EXIT.md`：边界闭环记录——REVIEW-2a7a686 m3 从"已知边界"转"已回收"）+ 出口走查。
- **验证**：Gate B 走查 PASS。

## 3. 里程碑与量级

| 里程碑 | 内容 | 依赖 | 量级 |
|---|---|---|---|
| P1-1 | free list 机制 + drop/run 入池 | 开工指令 | 0.5–1 天 |
| P1-2 | 有界性验收测试 | P1-1 | 0.5 天 |
| P1-3 | 出口 + EXIT | P1-2 | 0.5 天 |

全程约 1.5–2 天（含审查门禁）。

## 4. 风险

- **drop 时机语义**：wasmtime 资源 drop 的确定性（引用计数归零即调）——若实例整体释放（Rc 归零）时 drop 逐个调用（无碍：表随后整体丢弃）；复用只发生在生命周期内。
- **rep 复用与跨边界**：旧句柄引用复用 rep → 违规调用 run = panic（协议已有）——不接受静默错误。
- 不改 core；不触 go（无 go 侧改动）。

## 5. 纪律

Gate A（fmt/clippy -D warnings/wasm 全套/workspace 无回归）+ Gate B（REVIEW-<hash>）；commit 分 code/docs；**本线不改 core**。
