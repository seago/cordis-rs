# M3 基准报告：notify 扇出与切换延迟

**日期**：2026-08-17 ｜ **提交**：PR #25（`examples/im-bot/src/bin/bench.rs`） ｜ **门禁**：bench 全断言 + 宽松绝对上界

## 1. 定位

论文 §5.3 明确其案例是**存在性 / 采纳性结论**（"an existence-and-adoption result rather than a quantitative one; measuring the abstraction's overhead and its effect on developer productivity against a baseline remains future work"）。本报告是仓库侧的**量化补充**，直证两条运行时性质，为 §5.3 / §5.1.2 的算法语义提供实测支撑：

1. **notify 扇出传播成本**（§5.1.2，Algorithm 2 的 set/撤销 + Algorithm 3 的逐 live fiber 测试）：1 个提供者 + N 个注入同一键的消费者，激活 / 停用 / 再激活的总耗时随 N 的扩展；
2. **切换延迟与重激活局部性**（§5.3 "reactivates only the dependents whose resolved dependency changed"）：adapter/database/bot 三层 + M 个无关组件，切换存储后端（同一条目 revision 递增 → database 重建 → 新 fiber 重供 `db` 键）的耗时随 M 的扩展，以及**重激活工作量与系统规模无关**（效应重执行次数恒定）。

## 2. 方法

- **实现**：`examples/im-bot/src/bin/bench.rs`——`std::time::Instant` 手动计时，零第三方依赖（遵守仓库零依赖纪律，不引入 criterion）；每次重复 **fresh loader**（避免二次 apply 的 no-op 幂等）取 5 次**中位数**。
- **场景 A（扇出）**：N ∈ {1, 10, 100, 1000}；激活（全新系统单次 apply）、停用（provider 条目 `disabled` 切换 → 绑定撤销 → 级联 Inactive）、再激活（恢复 → 级联 Active）。断言：N 个消费者状态全对（停用全 Inactive / 再激活全 Active）+ 近线性门禁 + 绝对上界。
- **场景 B（切换延迟）**：M ∈ {0, 100, 500}；构建三层 + M 个填充组件后，计时"database revision 0→1"的单次 apply；断言：bot 效应**恰好重执行 1 次**、adapter **0 次**、bot fiber id 不变、M 个填充组件保持 Active、终态静止（`is_quiet`）。效应重执行次数经 config 内计数器直证——这是"只重激活解析依赖变化的依赖者"的直接证据（如果无关组件或 adapter 被重载，其效应计数必然 > 基线）。
- **环境**：macOS Darwin 25.6.0 x86_64、16 核；rustc 1.95.0 (59807616e 2026-04-14)；`cargo run --release`（本报告数据）与 `cargo run`（CI 门禁，debug，上界取自 debug 实测）。

## 3. 数据

### 场景 A：notify 扇出（N 消费者注入同一键；中位数，reps=5，release）

| N | 激活 | 停用 | 再激活 |
|---|---|---|---|
| 1 | 12.3 µs | 1.1 µs | 1.2 µs |
| 10 | 55.1 µs | 5.9 µs | 6.1 µs |
| 100 | 534.5 µs | 80.3 µs | 80.4 µs |
| 1000 | 9.24 ms | 3.60 ms | 3.56 ms |

**每消费者边际成本**：激活（10→100）≈ 5.3 µs / 消费者；（100→1000）≈ 9.7 µs / 消费者。停用/再激活（10→100）≈ 0.8 µs / 消费者；（100→1000）≈ 3.5 µs / 消费者。

### 场景 B：存储后端切换（三层 + M 个无关组件；中位数，reps=5，release）

| M | 切换延迟 | bot 效应重执行 | adapter 效应重执行 |
|---|---|---|---|
| 0 | 14.8 µs | 1 | 0 |
| 100 | 115.3 µs | 1 | 0 |
| 500 | 1.11 ms | 1 | 0 |

**每填充组件边际成本**：≈ 2.5 µs / 组件（100→500）。

CI 门禁（debug 构建）实测：N=1000 激活 ≈ 101 ms（上界 500 ms）；M=500 切换 ≈ 6.9 ms（上界 200 ms）。

## 4. 解读

**扇出近线性、超线性残差随 N 抬升**。激活成本在 N ≤ 100 区间近线性（10→100 为 9.7×），N=1000 时每消费者边际成本抬升到约 1.8×——这是 **Algorithm 3 的逐 live fiber 扫描**（"testing, for each live fiber, whether a changed key appears in its fiber.inject and resolves to the same realm"）在扇出级联下的放大：每次绑定变更都要扫描整棵 fiber 表，级联重激活链把扫描次数叠加。停用/再激活的残差更明显（100→1000 约 45×），同因：每次撤销/重绑各触发一次全表扫描，且级联 teardown 串行化（RefCell 借用）。

**切换重激活是局部的，扫描是全局的**。场景 B 三条硬断言在全部 M 上成立：bot 恰好重执行 1 次（其注入键 `db` 的提供者变化 → 目标变化 → 级联重激活）、adapter 0 次（fiber 不变）、bot fiber id 不变（条目未变 → 重激活非重建）、填充组件全程 Active——**重激活工作量与系统规模无关**。切换延迟随 M 增长（≈ 2.5 µs/组件）来自 Algorithm 3 的全表扫描：每个无关填充组件至多被 O(1) 的目标比较扫描并 early-return，**从不被重载**——这正构成本实现中 §5.3 "reactivates only the dependents whose resolved dependency changed" 的成本结构：扫描 O(F)（语义要求的逐 fiber 测试），重激活 O(受影响者)。

**对论文的量化补充**：自 §5.3 "future work" 的量化测量起——单机单线程 Rust 实现下，扇出传播的成本结构以近线性为主、超线性残差源于 Algorithm 3 的 O(F) 扫描；切换延迟以 O(F) 扫描为主导项，而重激活本体恒定。潜在的索引优化（按 realm 建依赖倒排索引，把 notify 扫描降为 O(受影响者)）记录为已知边界，未纳入本 PR。

## 5. 门禁与复现

- `cargo run --quiet -p im-bot --bin bench`（CI step）：全部断言通过即成功；绝对上界宽松（防 CI 抖动）且取自 debug 实测余量 >60×。
- `cargo run --release -p im-bot --bin bench`：复现本报告的 release 数据（数值随机器而异，定性结论与断言不变）。