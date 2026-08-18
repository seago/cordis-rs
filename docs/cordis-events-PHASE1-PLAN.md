# cordis-events Phase 1 开发计划

**依据**：
1. `docs/cordis-events-protocol-draft.md` v0.3.1（**已冻结**，2026-08-18；评审闭环 `docs/CORDIS-EVENTS-PROTOCOL-REVIEW.md` v0.1→v0.3.1，0 Major / 0 Minor 未决，仅 1 项可选 Nit）；
2. `docs/cordis-async-PHASE0-PLAN.md` §4.5 Phase 1 预备路线（M1.1–M1.4 提议，出口已固化于 `docs/cordis-async-PHASE0-EXIT.md`）；
3. `docs/cordis-async-protocol-draft.md` v1.4（冻结）§8 关系约定。
**状态**：**待开工指令**（按草案执行纪律：开工由用户另行下达；本计划为执行方案，不含实现代码，未下达前不写实现）。
**保证**：里程碑间独立审查硬门禁（Gate B）同 Phase 0 §3；Gate A（fmt + clippy -D warnings + 单测全绿 + workspace 无回归）；commit 分 code/docs；零第三方依赖。

---

## 0. 目标与交付物

在 fixture `cordis-core`（零语义改动）之上新增一等 `cordis-events` crate：类型化事件 + 四种 sync 派发 + 订阅即效应（fiber 卸载自动退订），并按预备路线推进 async 完善 / Remote 扩展 / DX 文档。

**Phase 1 最终交付**：
1. `crates/cordis-events`（workspace member）——草案 v0.3.1 §0–§7 协议实现，验收 #1–#9 全过；
2. 与 `cordis-async` 的衔接落地（async 监听器投递模式、活变化通道语义对齐）；
3. 与 loader/bundle 的集成（`EventsProvider` 根条目）；
4. P1.2–P1.4 预备路线（AsyncRuntime 完善 / Remote 扩展 / DX 文档）——各自草案/决议先固化、再开工（本计划给出排期与前置）。

**Phase 1 出口标准**：events 验收 #1–#9 全绿 + async 衔接单测 + loader 集成单测 + workspace 无回归 + 出口走查（无未解释偏差）→ 进入 Phase 2 决策。

---

## 1. 前置复核（P1.1 动工前）

- **core 无新依赖**：订阅经既有 `Context::effect`（fiber ctx 累加器 → 卸载自动退订）；`EventsProvider` 用 core 原生 `once`（不引 cordis-native，评审 m-2）；`Symbol`/`KeySet`/`once`/`Step` 均 pub——**无需 core 前置小修**。
- **草案冻结确认**：`cordis-events-protocol-draft.md` v0.3.1 为唯一权威；实现中的新偏差（如 `Mode` 内部枚举细节、冲突检测查询实现）在对应里程碑审查中显式核对草案条款。
- **可选 Nit 处理（n-3''）**：实现期在 doc 明示「同一事件名的 serial R 与 bail R 相互独立」——纳入 Step 2 的 crate doc。

---

## 2. 分步计划（P1.1 cordis-events 主交付线）

**里程碑门禁**：每里程碑完成 → Gate A 全绿 + 独立审查闭环（`docs/reviews/REVIEW-<commit>.md`）→ 才进下一里程碑（同 Phase 0 纪律）。

### Step 0：crate 骨架（M1.1）

- **目标**：`crates/cordis-events` 立项、workspace 接入、编译通过、协议类型占位。
- **任务**：
  1. `Cargo.toml`（workspace 继承；依赖：`cordis-core` path 仅此一项 = 零第三方；dev-deps 空或按需）；
  2. `workspace.members` 增 `crates/cordis-events`；
  3. `src/lib.rs`：`trait Event`（`Payload` + `SYMBOL`）+ 占位 `EventBus` + `EventsKey`/`EventsProvider` 签名 + `#![deny(missing_docs)]` + workspace lints；crate doc 标注草案 v0.3.1 依据。
- **产出**：可编译 crate；fmt/clippy/test 干净。
- **验证**：`cargo build -p cordis-events` ＋ 空测试通过。
- **门禁**：Gate A（fmt + clippy `-D warnings`，`unsafe_code=deny` 继承）。

### Step 1：订阅 + 派发核心（M1.2）——验收 #1/#2/#5/#6/#8

- **目标**：监听器注册（on/on_waterfall/on_serial/on_bail）+ 四种派发（emit/waterfall/serial/bail）+ 冲突检测（载荷/R 单一性 + 跨模式载荷一致性）。
- **任务**：
  1. `EventBus` 内部结构（`modes` 单表 `RwLock<HashMap<(Symbol, Mode), (TypeId, TypeId)>>` + `listeners` 表；Emit/Waterfall R 位 `()`；闭包 `Box<dyn Fn(...) + Send + Sync>` 存储）；
  2. 订阅 API（`on`/`on_waterfall`/`on_serial<P,R:Send+Sync>`/`on_bail<P,R:Send+Sync>`）——冲突检测四规则（同名同模式载荷、同名同模式 R、派发侧 R、**跨模式载荷一致性**）；
  3. 派发（`emit`/`waterfall`/`serial`/`bail`）+ 快照迭代 + release-then-invoke + alive 标志（E-1）+ 空集（E-2）；
  4. 自研幂等 disposer（armed Cell，同款语义）；
  5. 测试 1/2/5/6/8（emit 序、disposer 幂等、serial/bail 语义、四类冲突 panic、E-2 四断言）。
- **产出**：订阅/派发核心 + 5 条验收测试。
- **验证**：`cargo test -p cordis-events`（本步 5 条）。
- **风险**：`Box<dyn Any + Send + Sync>` 的 downcast 链；release-then-invoke 与 alive 快照的组合时序；冲突检测的锁内查询（write 临界区内跨模式比对）。

### Step 2：订阅即效应集成（M1.3）——验收 #3 + DX 入口

- **目标**：`EventsKey`/`EventsProvider`/`subscribe*` + `ctx.effect` 落账（fiber 卸载自动退订）。
- **任务**：
  1. `EventsProvider`（core 原生 `once` 绑定 `EventsKey`）；
  2. 便捷订阅 `subscribe`/`subscribe_waterfall`/`subscribe_serial`/`subscribe_bail`（取总线 → `ctx.effect` 注册 → 幂等 disposer）；
  3. 测试 3（fiber 退役 → 订阅撤销、事件不再到达；spike S1 形态固化）；
  4. crate doc 注记 n-3''（serial 与 bail 的 R 相互独立）。
- **产出**：订阅即效应闭环 + 1 条验收测试。
- **验证**：累计 6 条。

### Step 3：waterfall 语义 + 重入/空集精化（M1.4）——验收 #4/#7

- **目标**：waterfall around/短路/terminal 完整语义；重入快照（派发中注册不触发、退订跳过）+ 空集。
- **任务**：
  1. waterfall 链（A→B→terminal 序、around、短路）+ 终端缺省约定（O-3 观察项）；
  2. 测试 4（waterfall）；测试 7（重入快照两断言）+ 测试 8 复核；
  3. O-2（once）/O-1（prepend）观察项复核（暂不落地，记录使用信号）。
- **产出**：语义精化 + 2 条验收测试。
- **验证**：累计 9 条（#1–#9 中已覆盖 #1–#8）。

### Step 4：Send+Sync 断言 + async 衔接 + loader 集成（M1.5）——验收 #9 + 集成

- **目标**：编译期断言固化 + 与 async 层/loader 的衔接落地。
- **任务**：
  1. 验收 #9（`assert_send_sync::<EventBus>()` / `Arc<EventBus>()`，防回退）；
  2. async 监听器投递模式单测（sync 闭包内 `spawn_local` 投递；不阻塞派发；C-5 可追溯）——与 `cordis-async`（dev-dep 引入做集成测试？——**events 本身零依赖**；async 衔接测试放 events 的 dev-deps 或 async 侧集成测试，二者择一，以不污染 run-deps 为原则）；活变化通道语义对齐（快照 = 稳定视图、事件 = 活变化流）；
  3. `EventsProvider` 经 loader 挂载的集成单测（根条目形态，仿 S1 spike 的 loader 路径）。
- **产出**：验收 #9 + async 衔接 + loader 集成共 2–3 条。
- **验证**：`cargo test -p cordis-events` 全绿（累计 11–12 条）。
- **风险**：events dev-deps 若引 cordis-async/tokio——仅为测试用，run-deps 保持零第三方（与 async 的 tokio 只进自身 run-deps 同款纪律）。

### Step 5：Phase 1 出口（P1.1 线）

- **目标**：events 验收 #1–#9 全过 + 集成全绿 + 出口走查。
- **任务**：
  1. 全 workspace 门禁（fmt/clippy/test 无回归）；
  2. 出口走查：验收映射表逐条核对（§6 清单 ↔ 测试名，无夸大/遗漏）；async 衔接与草案 §4.1 对齐；
  3. 固化 `docs/cordis-events-PHASE1-EXIT.md`（验收 + 集成 + 门禁记录 + 出口判定）；流转入 P1.2–P1.4 决策。
- **验证**：全部测试绿 + 出口文档评审（里程碑审查覆盖）。

---

## 3. 后续工作线（P1.2–P1.4，预备路线——各自先固化草案/决议再开工）

| 线 | 内容 | 草案/决议前置 | 排期 |
|---|---|---|---|
| P1.2 | AsyncRuntime 完善：自动 settle 模式（O-2）、`AsyncFiberHandle` 门面收口（M0.5 临时 `Rc<Fiber>` 定型）、lifecycle observer hook 启用（O-3，基于既有 `update_hook`/`retire_hook`）、Failed 载荷富化（O-4，按 S3 首个真实失败场景） | 各自决议记录（可复用 `CORDIS-EVENTS-PROTOCOL-REVIEW` 风格评审链）；未决项见 `docs/cordis-async-protocol-draft.md` O-2..O-4、`cordis-async-PHASE0-EXIT.md` | P1.1 出口后 |
| P1.3 | Remote 扩展：Send-future 分池形态（当前 v1 为 spawn_blocking 闭包）、WasmRemote 接入 M1 宿主驱动协议 + 双运行时（sync 树 + async 组合线程）共存收口（S2 已验证二分） | async 草案 §2/§4 桥泛化目标；需新草案或决议 | P1.2 后 |
| P1.4 | DX 与文档：插件作者指南（C-1/C-1'/C-2/C-4：Arc 值惯例、绑定 vs 资源、门面纪律）、错误/安静语义文档、示例插件模板（含事件订阅 + async 监听器 + agent loop 的组合示例） | 依赖 P1.1–P1.3 落定的 API | 并行 + 收尾 |

每条线沿用：里程碑门禁（Gate A/B）+ 独立评审闭环 + 开工由用户下达。

---

## 4. 里程碑与时间量级（P1.1 events 线）

| 里程碑 | 内容 | 依赖 | 量级（单人） |
|---|---|---|---|
| M1.1 | Step 0 骨架 | — | 0.5 天 |
| M1.2 | Step 1 订阅/派发核心（验收 #1/2/5/6/8） | M1.1 | 1–2 天 |
| M1.3 | Step 2 订阅即效应集成（#3） | M1.2 | 1 天 |
| M1.4 | Step 3 waterfall/重入/空集（#4/7） | M1.3 | 1 天 |
| M1.5 | Step 4 Send+Sync + async/loader 集成（#9） | M1.4 | 1–2 天 |
| 出口 | Step 5 出口判定（#1–#9 + 集成 + 走查） | M1.5 | 0.5–1 天 |

合计 P1.1 约 5–7 天（含审查门禁开销）；全年 P1.2–P1.4 另行估算。

---

## 5. 依赖与纪律约束

- **零第三方**：`cordis-events` run-deps 仅 `cordis-core`（path）；async/tokio 如需仅入 dev-deps（集成测试），不污染 run-deps（同 async 的 tokio 只进自身 run-deps 纪律）。
- **unsafe_code=deny** + `#![deny(missing_docs)]` + workspace lints 继承。
- **commit split**：实现（feat/fix）+ 文档（docs）分开提交；审查报告入库 `docs/reviews/`。
- **协议纪律**：实现以冻结稿 v0.3.1 为权威；新偏差（如有）在里程碑审查显式记录核验；审查报告级别同 Phase 0（Major 必须修复才合入 / Minor 建议 / Nit 可选）。
- **开工纪律**：本计划为执行方案，**开工指令由用户下达**；未下达不写实现。
