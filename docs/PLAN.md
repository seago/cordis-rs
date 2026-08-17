# Cordis-rs 实施规划（PLAN）

> 依据论文：`paper/paper.pdf` — *A Programming Paradigm for Spatiotemporal Composability*（Shi / Zhang / Cui，2026）
> 本文档整合了项目前期设计讨论的结论：可行性分析、架构决策、开发模式与验证策略。
> 配套文档：`docs/THEORY-MAP.md`（论文符号 ↔ 代码映射、定理覆盖与已知偏差，随开发持续维护）

## 1. 项目定义

- **目标**：以 Rust + Wasm 实现论文描述的 Cordis 元框架：
  - 宿主运行时核心（`cordis-core`）：可逆效应、反应式共效应、fiber 生命周期演算；
  - 两个组件后端：进程内原生组件（`cordis-native`）、Wasm 组件模型沙箱组件（`cordis-wasm`）；
  - 声明式组件加载器（`cordis-loader`）与热模块替换（`cordis-hmr`）；
  - 过程宏 DX 层（`cordis-macro`）。
- **真源**：论文第 3–5 章。第 4 章演算是唯一真源；实现 = 逐规则翻译，测试 = 逐定理验证。
- **非目标（当前版本不做）**：持久化、跨进程编排、论文 6.6 节的结构兼容统一模型、6.7 节的语言/OS 协同设计。

## 2. 可行性要点（已论证结论）

1. **论文自己点名了这条路线**：§6.4 明确 Rust traits/impl 与 procedural macros 是类型层依赖声明的推荐路径，并点名 Wasmtime 为原生 embedder 的代码回收路径（"released when a native embedder drops it"）。
2. **关键洞察——演算保证全部在宿主侧**：第 4 章元理论（Preservation、时空可组合性、Progress、Confluence）全部作用于 fiber 状态机 / registry / accumulator。Wasm 隔离不触碰任何一条定理，它只改变组件边界的机械细节（论文 §6.3 的桥接模式已覆盖）。
3. **Wasm 与论文机制的正向叠加**：
   - 无共享内存 = Definition 48（confinement）的物理实现；
   - 每 fiber 独立 import surface = §6.3 能力模型（组件只能拿到它声明的依赖）的物理实现；
   - 实例 drop 即回收 = §6.4 的模块回收路径；
   - wit 实例化时结构检查 = §6.6 结构兼容的部分落地。
4. **插件语言开放**：wit 组件模型支持 Rust / C / Go / Python / JS 等（第一梯队为字节码联盟官方维护的绑定）；宿主只维护一份 wit 世界，inject 声明可由 world 的 import 段自动生成。插件作者**不需要会 Rust**。
5. **真正的难点在工程而非理论**：跨边界对象引用（imported resources）、guest 侧逆的句柄化、宿主驱动的效应迭代协议。对策见 §4.4 与 §10。

## 3. 总原则

1. **第 4 章演算是唯一真源**：模块任务清单对应到具体定义/定理/算法编号；不引入论文之外的自创语义，有取舍必须标注并记入 THEORY-MAP.md 偏差栏。
2. **核心与后端严格分层**：`cordis-core` 零依赖 wasmtime；Wasm 只是"效应与逆的供应商"之一，与原生后端并存。
3. **每个里程碑自带验收标准**：验收 = 对应定理的 property test 全绿 **+ 论文走查无未解释偏差**（§7）。
4. **M0 原生闭环先行**：M0 不碰 wasmtime，先把第 4 章演算在纯 Rust 中做对、测透；M1 只换 apply 的调用目标。

## 4. 架构决策

### 4.1 仓库结构（Cargo workspace）

```
cordis-rs/
├── Cargo.toml                  # workspace（+ rust-toolchain、fmt/clippy/test/miri CI）
├── paper/                      # 论文（唯一真源）
├── docs/
│   ├── PLAN.md                 # 本文档
│   ├── THEORY-MAP.md           # 符号映射 / 定理覆盖 / 已知偏差（活文档）
│   └── adr/                    # 轻量 ADR（ADR-0001 起，见 §4.5）
├── crates/
│   ├── cordis-core/            # 宿主运行时核心：Context/Effect/Fiber/Registry（§5.1）
│   ├── cordis-native/          # 进程内组件后端：Component trait
│   ├── cordis-wasm/            # wasmtime + 组件模型后端：wit 世界、桥接、按 fiber 的 Linker
│   ├── cordis-loader/          # 声明式配置、协调、托管 realm（§5.2.1）
│   ├── cordis-hmr/             # 三阶段热替换引擎（§5.2.2）
│   ├── cordis-macro/           # proc macro：#[component]/#[inject]/#[provide]
│   └── cordis/                 # 门面 crate（等价于 npm 的 @cordisjs/core）
├── examples/                   # hello-plugin(native)、db-server、wasm-plugin-rust、wasm-plugin-go、…
├── tests/                      # 元理论 property test（每定理一套）
└── benches/                    # 生命周期切换、notify 扇出
```

依赖基线：`tokio`（单线程 flavor）、`proptest`、`serde`/`serde_yaml`、`interner`、`slotmap`；M1 起加 `wasmtime`、`wit-bindgen`；原生后端用 `libloading`。

### 4.2 核心类型设计（M0 冻结）

以下草图是论文 Definition 44/49 的直接转写：

```rust
// ── 键与值（Def 22–24）──────────────────────────────────────────
/// 类型化共效应键：TypeId 即键身份，Value 即值类型（论文 𝒱 k）
pub trait Key: 'static + Send + Sync {
    type Value: Send + Sync + 'static;
    const SYMBOL: &'static str;      // 跨边界（wasm）与调试用的符号名
}

/// 字符串符号（wasm 插件、动态配置用）；realm 也用它（Def 28 的 R ⊇ K）
pub struct Symbol(u32);             // interned，Copy + Eq + Hash

/// 依赖表 σ：realm → 值（Def 22/28）
pub struct Store(HashMap<Symbol, Box<dyn Any + Send + Sync>>);

// ── 上下文（Def 32 的 Γ∞）───────────────────────────────────────
pub struct Context {
    runtime: Rc<RuntimeState>,                     // registry + store 共享引用
    fiber: Option<FiberId>,                        // 所属 fiber（root 为 None）
    realms: Rc<RefCell<HashMap<Symbol, Symbol>>>,  // ρ：隔离表（派生子上下文时覆盖）
    intercept: Rc<RefCell<HashMap<Symbol, Box<dyn Any>>>>, // ι：拦截元数据（Def 30）
    dispose: RefCell<Vec<Disposer>>,               // 本层 accumulator（Algorithm 1 第 17 行）
}

// ── 可逆效应（Def 8/51，Algorithm 1）─────────────────────────────
pub type Disposer = Box<dyn FnOnce() + Send + 'static>;   // 逆的宿主侧载体

/// 效应迭代器 𝔈iter_Γ：宿主驱动，每步 yield 一个逆（Algorithm 1 的 iter）
pub trait EffectIter: Send + 'static {
    fn step(&mut self, ctx: &Context) -> BoxFuture<'_, (Disposer, bool)>; // (本步逆, done)
}

/// execute 引擎 = Algorithm 1 逐行翻译（guard 在每个迭代边界检查，§4.3.2）
pub async fn execute(iter: &mut dyn EffectIter, guard: impl Fn() -> bool) -> Disposer;

// ── 组件与 Fiber（Def 43/44）─────────────────────────────────────
pub trait Component: Send + Sync + 'static {
    fn inject(&self) -> Spec;                  // d：声明依赖
    fn provide(&self) -> KeySet;               // p：供给键（O-Insert 据此查不相交）
    fn apply(&self, ctx: Context, config: Value) -> Box<dyn EffectIter>; // e
}

pub struct Fiber {
    uid: FiberId, parent: Option<FiberId>,
    inject: Spec, provide: KeySet,
    retired: bool,                             // τ
    state: LifecycleState,                     // θ
    target: Option<View>, committed: Option<View>, // target_n(γ) 与 ω
    inertia: Option<tokio::task::JoinHandle<()>>,  // 在途转换（§4.3.3 惯性）
    ctx: Context,                              // fiber 自己的派生上下文
}

/// Def 49 的逐字转写——非法状态不可表示
pub enum LifecycleState {
    Inactive(Option<Error>),                             // Inactive(ζ)
    Loading { iter: Box<dyn EffectIter>, acc: Vec<Disposer>, view: View }, // Reloading(i,g,ω)
    Active { acc: Vec<Disposer>, view: View },           // Active(g,ω)
    Unloading { acc: Vec<Disposer>, view: View, outcome: Option<Error> }, // Unloading(g,ω,ζ)
}
```

### 4.3 分 crate 任务清单（对应论文章节）

#### cordis-core（§5.1，M0 主体）

| 任务 | 对应 | 交付物 |
|---|---|---|
| Symbol/Key/Spec/Store | Def 22–24 | 键身份、依赖表、满足谓词 `σ⊧d` |
| EffectIter + execute + ctx.effect | Algorithm 1，Thm 7/13/16 | LIFO 恢复、armed 自销毁、父组合 |
| set/get/isolate/intercept + notify | Algorithm 2/3，Def 23–31 | 依赖操作、分类通知（activating/deactivating/neutral） |
| Fiber + refresh/reload/unload | Algorithm 4/5，Def 46–50 | 惯性状态机、committed view、撤离 guard（`¬relied`）、依赖排空等待 |
| Registry + use/retire/remove | O-Insert/O-Retire/O-Remove，Def 47 | 编排原语、父子树、实例化作为可追踪效应 |
| 上下文访问（等价 Proxy） | §5.1.4 | `ctx.inject::<K>()` 沿 fiber 链解析，未声明报错 |

#### cordis-native + cordis-macro

- `#[component]` 宏：从 `#[inject] db: Arc<Database>` 字段生成 `inject()`/`provide()`；`ctx.inject::<Database>()` 编译期类型安全。
- `#[cordis::test]`：带临时 runtime 的测试上下文宏。

#### cordis-loader（§5.2.1）

- Entry 结构（id/url/isolate/intercept/config/disabled，Def 74）、配置树、按字段最小扰动协调（id/url→重建、isolate→Algorithm 7 重指派、intercept→原地、config→组件自决、disabled→卸载）。
- 托管 realm：local（按 entry id 打标、随迁）与 global（命名共享）；delimiter 标签机制。
- group/include 组件（对应 @cordisjs/group/include）。

#### cordis-hmr（§5.2.2）

- Algorithm 8/9/10 直译：模块分类不动点（native 用 cargo metadata 依赖图，wasm 用 wit import 图）→ 过期条目检测 → 事务性重载（wasm：换实例；native：dlopen 换库；失败回滚）。

### 4.4 Wasm 后端设计（M1）

- **wit 世界两份**：
  1. 通用 host 接口（context/get/set/isolate/intercept）；
  2. 每插件家族的 typed world——**import 段即 inject 规格**，实例化时结构检查。
- **桥接层**（论文 §6.3）：host resource handle ↔ `Arc<dyn Any>` 适配。
- **宿主驱动的效应迭代协议**：guest 导出 `task` 资源，`step()` 每步返回 `option<effect-step>`（含 `inverse` 资源句柄）；宿主 execute 引擎驱动，guard 在宿主侧检查（§4.3.2 的 step-boundary interruption）。**guest 无需自己的异步运行时**。
- **逆的句柄化**：宿主 accumulator 持 `Vec<ResourceHandle>`，卸载时 LIFO 调 `run()` 再 drop。**不依赖 destructor 作为逆**（显式 `dispose` 语义更贴合论文的确定性恢复）。
- **按 fiber 的 Linker**：import 集合 = 该 fiber 的 committed view + 拦截裁剪后的能力。

### 4.5 已锁定的 6 个设计决策（ADR 目录，改动必须新立 ADR）

| 编号 | 决策 | 内容 |
|---|---|---|
| ADR-0001 | 键身份 | 进程内 = `TypeId`（类型化）；跨边界 = interned `Symbol` + wit 类型；realm 一律 Symbol |
| ADR-0002 | 调度模型 | 单线程 tokio（`LocalSet + spawn_local`）+ `Rc<RefCell>`；论文的编排模型是单编排器 |
| ADR-0003 | 异步边界 | M1 只用宿主驱动的同步 wasm step；wasmtime 组件模型异步预留为后续增强，不进 M1 关键路径 |
| ADR-0004 | 值语义 | 宿主内 `Arc<dyn Any>` + trait 对象直通；跨边界 resource handle + 桥接翻译 |
| ADR-0005 | 错误模型 | ζ 建模为 `Inactive(Option<Error>)`；加载失败 → 记录错误 + target 置 ⊥（对应 L-Raise） |
| ADR-0006 | 版本策略 | 起步用论文 §6.6 peer-dependencies 路线（key symbol 对应接口 crate 版本），wit 结构检查兜底；K×P 命名空间为后续增强 |

## 5. 里程碑计划（门禁式）

| 里程碑 | 范围 | 验收标准（门禁） | 人力估计 |
|---|---|---|---|
| **M0 原生闭环** | core + native + macro + loader 前半 | Thm 7/16/63/66/73 property test 全绿；示例 server+auth 演示运行时卸载/重连；论文走查（§3.1–3.3、§4.1–4.4、§5.1）无未解释偏差 | 1 人 4 周 |
| **M1 Wasm 后端** | wasm 后端 + 桥接 + 双后端共存 | 同一 loader 加载原生与 wasm 组件；guest 崩溃不伤宿主（沙箱隔离）；Rust + Go 双语言 guest；走查 §6.2–6.4 | 1 人 3–4 周 |
| **M2 加载器 + HMR** | reconciliation 全字段、托管 realm、HMR 三阶段 | 改插件代码保存即生效，in-flight 任务不中断、其他组件状态保留；回滚用例；走查 §5.2 | 1 人 2 周 |
| **M3 案例验证** | IM-bot 迷你案例 + 基准 + 文档 | adapter/database/功能插件三层依赖拓扑案例；bench 报告（notify 扇出、切换延迟）；走查 §5.3 | 1 人 2–3 周 |

- 总计单人约 11–13 周；双人并行（一人 core/理论，一人 wasm/工具链，M1 集成周合流）约 7–8 周。
- **M0 是风险闸门**：M0 定理测试与走查不通过，M1 不开工。

## 6. 验证策略

**每个定理一套测试**：

| 论文结论 | 测试手段 |
|---|---|
| Thm 7/16：LIFO 恢复、声音不变量 | 单元测试（effect 序列 + 逆序恢复） |
| Cor 21：独立效应乱序撤销 | 单元测试（任意排列撤销回到 γ₀；PR #9 已落地） |
| Thm 63：依赖者先于提供者停用、teardown 期间依赖仍可读 | 集成测试（3 组件拓扑，断言撤离顺序） |
| Thm 64：单个转换不跨两次解析 | 集成测试（转换中途改配置） |
| Thm 66：Progress、撤离 guard 不死锁 | proptest + loom（随机拓扑） |
| Thm 73 / Cor 62：Confluence、离场无残留 | proptest，**oracle = 参考解释器** |
| Def 26：通知分类正确性 | 单元测试（activating/deactivating/neutral） |

**参考解释器（oracle）**：在 PR #2 之前，把第 4.2 节基础演算规则原样写成一个约 200 行的迷你解释器（规则即代码，不优化）。用途：(1) 动手前把论文读"实"；(2) 作为 confluence 类 property test 的 oracle。

```rust
proptest! {
  #[test]
  fn confluence_and_progress(actions in arb_orchestration(20)) {
      let rt = Runtime::new();
      let final_cfg = actions.apply_sequentially(&rt).await; // 编排动作与生命周期步任意交错
      rt.drive_to_quiescence().await;                        // Thm 66：必达静止
      let expected = final_cfg.interpret();                  // Thm 73：最终状态只由配置决定
      assert_eq!(rt.installed_components(), expected);
      assert!(rt.no_residue_from_removed());                 // Cor 62
  }
}
```

**CI 门禁**：fmt + clippy（deny warnings）+ 单元测试 + proptest；M1 起加 miri（wasm 桥接层 unsafe 检查）；需要时加 loom。

**论文走查**：里程碑门禁的第二半（测试只证明"跑通了"，走查防"语义悄悄偏了"），见 §7。

## 7. 论文走查（里程碑门禁）

- **定位**：审计，而非日常检查。日常细粒度对照由工作循环承担（§8）；走查只在稳定态（里程碑边界，测试全绿、API 冻结）执行。
- **偏差捕获是连续的**：每 PR 合入时即把对照发现写入 THEORY-MAP.md 的"已知偏差"栏（哪怕一句话），**不允许攒到里程碑**。走查时看到的是已积累的记录，不是第一次暴露。

**程序**（并入门禁，可复制执行）：

```
输入：本里程碑 diff + THEORY-MAP.md 覆盖报告（定理覆盖率）
步骤：
1. 重读对应章节：
   M0 → §3.1–3.3、§4.1–4.4、§5.1
   M1 → §6.2–6.4
   M2 → §5.2（§5.2.1/5.2.2）
   M3 → §5.3
2. 逐条核对"已知偏差"栏：每条给出处置（修正 / ADR 保留 / 公开差异声明）
3. 补查：类型/函数与论文符号的映射是否有未记录偏差
输出：走查记录（追加进 THEORY-MAP.md）+ 处置清单
门禁判定：存在「未解释偏差」→ 门禁不通过，进入修正循环；处置清单成为下个里程碑的首批任务
```

- **时间预算**：每里程碑 0.5–1 天（M0 最大，覆盖演算章节最多；M2 最小）。

## 8. 开发模式

**模式**：定理驱动的规格优先开发（Theorem-Driven Spec-First）+ 垂直切片 + 门禁式里程碑。

**选择依据**：规格是形式化的（验收标准天然可测试）；正确性优先于速度（不变量违约 = 理论失效，P0）；风险分层（M0 是地基）；团队 1–2 人（极简仪式，不做 Scrum 估点/计划会）。

**工作循环（每 PR 一次）**：

```
1. 选一个论文单元（一个 Definition / Theorem / Algorithm）
2. 先把它的测试写成红的（定理 → 断言；算法 → 行为测试）
3. 实现到绿
4. 回填追溯矩阵 docs/THEORY-MAP.md：论文编号 ↔ 类型/函数 ↔ 测试
5. 小 PR 合入主干
```

**三条纪律**：
1. PR 标题带论文编号，如 `feat(core): execute 引擎 [Alg 1] [Thm 7/16]`；合入前对照论文章节逐行核对。
2. THEORY-MAP.md 是活文档；每里程碑结束统计定理覆盖率，缺口即技术债。
3. 参考解释器（§6）先于真实引擎落地，作为 oracle。

**三层检查闭环**：每 PR 连续对照 → 里程碑走查审计（§7）→ 门禁判定。任何一层发现偏差都有明确处置路径（修正 / ADR / 差异声明）。

**流程参数**：

| 层面 | 做法 |
|---|---|
| 节奏 | 无固定迭代；按论文单元滚动，每单元 ≤ 2 天，做不完再切小 |
| 分支 | 主干开发；单人直接主干 + 小 PR，双人 short-lived 分支 |
| 设计变更 | 必须新立 ADR（§4.5） |
| 文档 | THEORY-MAP.md（追溯）+ PLAN.md（里程碑状态）+ ADR；README 只放示例 |

**单人 / 双人**：单人严格 M0→M3 串行，做完一个定理单元即锁死测试；双人一人守 core/理论（关键路径），一人打 wasm/工具链（可并行），M1 集成周合流。

**反模式**（本项目特别易踩）：
1. 水平分层一次性做完（先写完全部 core 再测试再上 backend）——正确姿势是垂直切片：第一个切片 = 一个组件 + 加载/卸载/恢复，从第一天起端到端可跑。
2. 把 wasm 塞进 M0——工具链噪音会混淆状态机问题定位。
3. 过早做 DX 糖（proc macro）——先让裸 API 正确，宏最后做。

## 9. 开工顺序（前 8 个 PR）

1. 工作区骨架 + CI（fmt/clippy/test）+ `docs/THEORY-MAP.md` 空表
2. core：Symbol/Key/Spec/Store + 满足谓词（参考解释器在此之前落地）
3. core：EffectIter/execute/ctx.effect + Thm 7/16 测试
4. core：set/get/isolate/intercept/notify + Def 26 分类测试
5. core：Fiber/registry/use/refresh/reload/unload + 最小状态机
6. tests：Thm 63/66/73 property suites（**在写更多功能前锁死正确性**）
7. native + macro + 示例 `hello-plugin`
8. loader 最小协调（id/config/disabled 三字段）
9. Cor 21 独立效应乱序撤销测试（任意排列回到 γ₀；§3.1 收尾）

## 10. 风险清单与对策

| 风险 | 对策 |
|---|---|
| wit-bindgen async 边缘 case 不成熟 | 宿主驱动同步 step（ADR-0003）；组件模型异步不进 M1 关键路径 |
| 多语言绑定成熟度分梯队 | Rust 第一公民 SDK（宏糖）；其余语言裸 wit 兜底（机制与保证全在宿主，与作者语言无关）；按社区需求逐语言补 SDK |
| 论文 §6.1 系统边界（外部发射不可逆） | 不因 Rust 消失；按 acquisition/emission 两段式设计，记录为已知差异 |
| 实现悄悄偏离演算语义（本项目最大风险） | 三层检查闭环：PR 对照 / 里程碑走查 / 门禁判定 + 追溯矩阵 |
| DX 无 Proxy 糖 | 进程内组件用宏补偿；wasm 插件不受影响（访问中介在宿主 import 边界，不需要语言反射） |
| 单线程 / 单编排器假设的边界 | ADR-0002 记录；超出边界时新立 ADR 扩展 |

## 11. 里程碑进度

| 里程碑 | 状态 | 门禁 |
|---|---|---|
| M0 原生闭环 | **通过（走查完成）**（PR #1–9 + M0 走查：骨架、核心类型 + 参考解释器、可逆效应引擎、共效应操作 + 通知分类、fiber 生命周期状态机、oracle × 引擎元理论验证、DX 层 + hello-plugin 示例、loader 最小协调、Cor 21 测试、Thm 73(1) canonical form 补测） | 定理测试全绿 ✓；走查（§3.1–3.3/§4.1–4.4/§5.1）**通过（含处置清单）**——6 项处置清单成为 M1 首批任务（THEORY-MAP「里程碑走查记录」） |
| M1 Wasm 后端 | **通过（走查完成）**（PR #10–16：wit 世界 v1 + 工具链定型 + 宿主加载/驱动 + WasmComponent 接入 cordis-core + wasm 依赖者消费 + 双后端共存（值类型统一互通）+ 沙箱隔离（恶意 guest trap 捕获、宿主存活）+ Rust + Go 双语言 guest（标准 go + wasip1 + 预览1 适配器组件化）+ **走查 §6.2–6.4（PR #15）无未解释偏差** + 处置③ Thm 59/61 直接测试 + 处置④ Thm 66 定量上界断言） | 同一 loader 加载原生与 wasm 组件 ✓；guest 崩溃不伤宿主 ✓；Rust + Go 双语言 guest ✓；走查 §6.2–6.4 ✓——**4/4 门禁全部达成**；处置清单（③④ 已落地；① ② ⑤ ⑥ ⑧ ⑨ 共 6 项为 M2 首批任务，⑦ 归 M3 案例素材；详见 THEORY-MAP「M1 Wasm 后端走查记录」） |
| M2 加载器 + HMR | **通过（走查完成）**（PR #17-23：L-Raise 失败模型、interception 求值形态、loader 全字段 + group/include + 托管 realm（Algorithm 7）、cordis-hmr（Alg 8/9/10 事务性重载 + 双回滚直证）、Alg 6 Proxy 访问层、处置⑥⑨ 评估收尾 + **走查 §5.2 无未解释偏差**） | 改插件代码保存即生效 ✓（hmr_reload_applies_new_version_keeping_other_components）；in-flight 不中断/其他组件状态保留 ✓（同步引擎原子事务 + 状态保留断言）；回滚用例 ✓（加载失败 / L-Raise 组件失败双回滚）；走查 §5.2 ✓——**4/4 门禁全部达成**；处置清单（⑦ 归 M3；⑩⑪⑫ 新记录）成为 M3 首批任务 | 改插件代码保存即生效，in-flight 任务不中断、其他组件状态保留；回滚用例；走查 §5.2 |
| M3 案例验证 | **进行中**（PR #24 已闭环：IM-bot 三层拓扑案例 + broker 示例（处置⑦ 落地，审查重排依赖方向后全断言）——adapter/database/功能插件拓扑全断言：切换后端/重连/依赖不可用；broker：更新/卸载后备不扰动 broker 与消费者、可逆注册自动撤销、重注册自动恢复；剩余：bench 报告 + 处置⑩⑪⑫ + 走查 §5.3） | adapter/database/功能插件三层依赖拓扑案例；bench 报告（notify 扇出、切换延迟）；走查 §5.3 |
