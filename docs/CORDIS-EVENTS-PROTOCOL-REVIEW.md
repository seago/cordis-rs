# cordis-events 协议草案评审记录

- **评审对象**：`docs/cordis-events-protocol-draft.md` v0.1（初稿，待评审）
- **评审日期**：2026-08-18
- **评审人**：review-agent（对照 `cordis-core` 既有约束与 `cordis-async-protocol-draft.md` v1.4 §8 关系约定）
- **范围**：§0–§7 + 附录（类型化事件 / 派发协议 / 订阅即效应 / 衔接 / 推迟 / 验收 / 开放问题）

---

## 总体结论

**方向正确、结构清晰，但存在 1 项设计阻塞（M-1）与 1 项 API 缺口（M-2），需修订到 v0.2 后方可动工（按执行纪律，动工另由用户下达）。**

- **Major**：2（结构 ↔ core 约束冲突；serial/bail 订阅入口与返回类型约束缺口）
- **Minor**：3（跨文档派发清单不一致；EventsProvider 的 native 依赖偏离零依赖定位；E-1 对"派发中退订"语义未言明）
- **Nit**：3（命名对齐 / 引用非拷贝对照 / O-5 确认）

订阅即效应（S1 已验证语义固化）、符号冲突检测（同名异型 panic + 类型名诊断）、重入快照（core notify 同款）、E-2 空集四断言、scope 用 realm 隔离实例、与 async 的快照/活变化互补——均与既有协议一致，通过。

---

## 发现

### Major

### M-1（阻塞）：`EventsKey::Value = Arc<EventBus>` 与 core `Key` 的 `Send+Sync` 约束冲突——`RefCell` 结构编译不可能

- **位置**：§3.1 `EventsKey`（`type Value = Arc<EventBus>`）、§3.2 `EventBus` 结构（`names: RefCell<HashMap<...>>`、`listeners: RefCell<HashMap<...>>`）。
- **问题**：core `Key`（crates/cordis-core/src/key.rs:10-12）要求 `Key: Send+Sync` 且 `Value: Send+Sync`。`Arc<EventBus> is Send+Sync ⟺ EventBus: Send+Sync`；而 `RefCell` 非 `Sync` → `EventsKey::Value = Arc<EventBus>` **无法通过编译**。这与 ADR-0002（单线程 Rc/RefCell）的潮流直觉相反，但 `Key::Value` 的 `Send+Sync` 约束（值可能跨 wasm 边界/其他桥）是 core 的既有强制。
- **草案/依据**：core key.rs 约束；草案 §3.1/§3.2；草案 §0「只依赖 cordis-core」。
- **建议修法（任选其一）**：
  1. **内部同步化**：`EventBus` 字段改 `Mutex`/`RwLock`（单线程下临界区短、无竞争；ADR-0002 不受影响，`Mutex` 仅用作 `Sync` 包装）。订阅/派发都短借用；E-1 快照迭代需先 `collect` 再释放锁（避免锁内调用闭包——闭包可能重入订阅）。**推荐**：`RwLock` + 快照 release-then-invoke。
  2. **不把总线作 `Key::Value`**：`EventsKey` 值类型改为句柄通道（如 `Arc<EventBusCloneable>`？或 store 存 `Arc<Mutex<...>>` 的同构）——本质与 1 相同；或改为全局注册表 + store 存 id（破坏 realm 键控设计，不推荐）。
  - 建议修订版在 §3.2 明示锁的类型与释放时机，并加一条验收测试（实例化 `EventsKey` 值并满足 `Send+Sync` 编译断言——如 `fn assert_send_sync<T: Send+Sync>()`）。

### M-2（阻塞）：`serial`/`bail` 派发的听众注册入口缺失，且返回类型 `R` 的单一性未约束

- **位置**：§2.1 仅暴露 `on`（`Fn(&P::Payload)`，无返回）与 `on_waterfall`（`Fn(&mut, next)`）；§2.2 却定义 `serial<P, R> -> Vec<R>` 与 `bail<P, R> -> Option<R>` 派发。§3.2 `ListenerEntry` 含 `Serial`/`Bail` 变体（暗示应有对应订阅入口），但 §2.1 未给。
- **问题**：
  1. **无听众来源**：`on` 注册的监听器返回 `()`，无法产出 `R`——`serial`/`bail` 派发对谁收集？需要 `on_serial<P, R>` / `on_bail<P, R>`（`Fn(&P) -> R`）类订阅方法（`ListenerEntry::Serial`/`Bail` 已预设，API 补全即可）。§2.1 的 `on` 注释「emit/serial/bail 通用」有歧义：若指三者共用 `Fn(&P)` 则 serial 无 R 可收；若指另有入口则须写明。
  2. **R 单一性未约束**：同一事件名下多个 serial 听众若返回不同 `R`（`R1`/`R2`），派发 `serial<P, R>` 时如何 downcast？（TS 语义是「事件名 → ret 类型」单一对应。）需明确：同事件 serial/bail 的 `R` 必须单一（订阅时 TypeId 冲突检测扩展到 `(模式, R)`），或派发 `serial<P, R>` 对不匹配听众的处理策略。
- **建议修法**：§2.1 补 `on_serial<P, R>` / `on_bail<P, R>`（或合并为 `on_reply<P, R>` 供 serial/bail 共用注册）；§3.2 冲突检测表扩展为按 `(Symbol, 模式)` 记录 `TypeId(载荷)` + `TypeId(R)`，同名同模式异 `R` 亦 panic（类型名诊断，与 §3.2 载荷冲突同款）；验收测试 #5/#6 补「同名同模式异 R → 订阅 panic」。

### Minor

### m-1（建议）：跨文档派发模式清单不一致——async 草案 §8「emit/waterfall/parallel/serial」vs 本草案「emit/waterfall/serial/bail + parallel 推迟」

- **位置**：本草案 §0（四种 sync 派发：emit/waterfall/serial/bail）、§5 推迟表 vs `cordis-async-protocol-draft.md` v1.4 §8「四种派发（emit/waterfall/parallel/serial，sync 闭包）」（**无 bail、含 parallel**）。
- **问题**：两文档的「四种」对不上（parallel vs bail）。本草案的决议（bail 入 v1、parallel 推迟到 async 层以 `spawn_remote` 扇出 + join 实现）**合理且与 O-6 政策一致**，但引用方 async §8 的措辞未同步——读者按 §8 核对会误判。
- **建议**：本草案 §0 或 §5 明示「相对 async §8 派发清单的修订：v1 以 bail 取代 parallel，parallel 推迟到 async 层（§5）」；并建议 async 草案 §8 同步措辞（或本草案标注版本差）。

### m-2（建议）：`EventsProvider` 引用 `cordis-nativive::with_ctx` 偏离「只依赖 cordis-core」定位

- **位置**：§3.1 `EventsProvider::apply` 用 `cordis_native::with_ctx`。
- **问题**：`with_ctx` 仅是 `cordis_core::once(Box::new(move || step(&ctx)))` 的一行包装（crates/cordis-native/src/lib.rs:24-32）——events 本体引 native 引入一个 run-dep，破坏草案 §0「纯 sync、零依赖（只依赖 cordis-core）」。若 EventsProvider 进 crate 本体，应直接用 core 原生 `once(Box::new(move || ctx.set::<EventsKey>(...)))`；若引 native 仅作示例方便，应把 EventsProvider 放 example/dev。
- **建议**：本体用 core 原生 `once`；`EventsProvider` 作为示例进 dev-dependency 版示例。

### m-3（建议）：E-1 快照对「派发中**退订**的监听器」本轮是否仍触发的语义未言明

- **位置**：§2.2 E-1「派发中注册/退订的监听器本轮不触发」。core `notify` 同款快照纪律只覆盖「注册」（新 reactor 本轮不触发）——core reactor 无退订路径，故「退订者」是 events 自己的语义。
- **问题**：快照若持有闭包引用，已退订（disposer 已跑）的监听器本轮是否仍被调用？两种语义（「快照内照调」vs「检查 alive 后跳过」）都自洽，但必须二选一言明——否则与「不触发」字面打架。
- **建议**：明确「派发开始即快照监听器集合 + 各自 armed/alive 标志；派发中被退订者本轮**不再调用**（与 '不触发' 字面一致）」，并在验收 #7 加「派发中退订 → 本轮跳过、后续不再触发」断言。

### Nit

### n-1（可选）：事件身份命名对齐——`Event::NAME` vs core `Key::SYMBOL`

- §1 `const NAME`；core `Key::SYMBOL`。术语不统一，读者可能困惑二者关系。建议 `Event::SYMBOL`（完全镜像 Key）或 doc 明示「NAME 为语义化别名，内部经 `Symbol::intern` 与 Key 同驻留」。

### n-2（可选）：对照速查表补「引用 vs 值传递」

- TS `emit` 值传递（浅拷贝）；本层 `emit(&P::Payload)` / `waterfall(&mut)` 为零拷贝引用观察。附录对照表可补一行，避免语义误解（尤其 waterfall 的 `&mut` 改写模型已明示，emit 的引用语义可同步注明）。

### n-3（确认）：O-5 的 `R: 'static` 取舍

- AGREED：ADR-0002 下单线程 sync 层 `R: 'static` 足够，不必 `Send`。可作为未来 scope/并行化的扩展点记录。

---

## 通过项（逐条确认）

- **订阅即效应（E-3）**：任何订阅先 `ctx.effect` 落账（逆入 fiber ctx 累加器 → 卸载自动退订），与 spike S1 已固化语义一致；「裸订阅（非 fiber 上下文）不自动退订、责任自负」的边界划分清晰 ✓。
- **符号冲突检测**：首订阅写定 `TypeId`、后续同名必须一致否则 panic（含类型名诊断）——与 core「两键不得同 SYMBOL（访问点报 TypeMismatch）」义务纪律同构 ✓；建议扩展至 R（M-2）。
- **重入快照（E-1）**：= core `notify` 快照纪律 ✓（「注册者本轮不触发」与 core 审查 m1 同款）；仅需澄清退订者（m-3）。
- **E-2 空听众集四断言**：emit=no-op / waterfall=仅 terminal / serial=空 vec / bail=None——明确且可测 ✓。
- **disposer 幂等**：「StepGuard armed 同款」自研 armed Cell（core `StepGuard` 为 `pub(crate)` 不可直接复用，与 async `CancelFlag` 同路线——建议 §2.1 明示「自研 armed 机制，同款语义」以免误读为可复用核心项）✓。
- **与 async 衔接（§4.1）**：async 监听器 = sync 闭包内 `spawn_local`（遵循 C-5 可追溯、不裸 spawn）；活变化通道与 C-1' 快照纪律互补（快照=稳定视图，事件=活变化流）——与 async §8 约定一致 ✓（派发清单措辞除外，见 m-1）。
- **scope 模式（§4.3）**：per-agent 隔离 = realm 隔离的独立 `EventsKey` 实例（core `ρ` 解析天然路由），scope-filtered dispatch 留 app 层——符合核心 realm 语义、边界清晰 ✓。
- **明确推迟项 + 开放问题**（§5/§7）：parallel 推迟（async 层 `spawn_remote` 扇出 + join，符合 O-6）、O-1 prepend / O-2 once / O-3 terminal 缺省 / O-4 panic 传播（=core panic=bug）、O-5——结构良好、不阻塞 ✓。

---

## 结论

v0.1 的**架构决策正确**（类型化事件=Key 镜像、订阅即效应、realm 隔离 scope、async 边界），但 **M-1（`RefCell` × `Send+Sync` 冲突）是设计阻塞、M-2（serial/bail 订阅与 R 单一性）是 API 缺口**——两处都需修订到 **v0.2** 后方可进入实现（按执行纪律，动工另由用户下达）。Minor/Nit 可在 v0.2 一并处理。建议修订要点：① §3.2 总线内部同步化（`RwLock` + 快照 release-then-invoke）+ 加 `Send+Sync` 编译断言验收；② §2.1 补 serial/bail 订阅入口 + 冲突检测扩展至 `(模式, R)`；③ 首段标注「相对 async §8 的派发清单修订」；④ EventsProvider 改 core 原生 `once`（或移示例）。

---

## Addendum v0.2（复核 2026-08-18）

对 `docs/cordis-events-protocol-draft.md` v0.2（采纳 v0.1 评审的修订稿）逐条复核：M-2 的 `on_reply` + R 单一性、m-1/m-2/m-3/n-1/n-2/n-3 全部落地到位 ✓；**M-1 的修订不彻底——锁化未解决「监听器闭包的 Send+Sync」这一真正阻塞；且 M-2 修订引入 bail 返回形态的语义矛盾**。结论：**仍需 v0.3**。

### 复核结论：已确认修订（通过）

- **M-2 主体**：补 `on_reply<P, R>`（serial/bail 共用注册）、modes 表 `(Symbol, Mode) → (载荷 TypeId, R TypeId)`、同名同模式异 R panic、验收 #5/#6 增补——✅ 采纳正确。
- **m-1**：§0 标注「相对 async §8 派发清单修订」、§5 parallel 处置更新、async 保持冻结——✅。
- **m-2**：EventsProvider 改 core 原生 `once`（保零依赖）——✅。
- **m-3**：E-1 明确 alive 检查「派发中退订者本轮不再调用」、验收 #7 增补——✅。
- **n-1**：`Event::NAME`→`Event::SYMBOL`（完全镜像 Key）——✅。
- **n-2**：对照表补「引用 vs 值传递」行——✅。
- **n-3**：O-5 决议（`R:'static` 足够）——✅。

### v0.2 残留/新增发现

### M-1'（阻塞）：`EventBus` 的 `Send+Sync` 仍未成立——锁改 `RwLock` 未解决「监听器闭包」的 Send/Sync

- **位置**：§3.2 `EventBus` 字段改 `RwLock<HashMap<Symbol, Vec<ListenerEntry>>>`，但 `ListenerEntry` 内 `f: EmitListener<dyn Any>` 等仍是**无 `Send+Sync` 上界的 `Box<dyn Fn … + 'static>`**。
- **问题**（Rust 语义事实）：① `Box<dyn Fn(…) + 'static>` **默认非 `Send`/`Sync`**（trait object 不自动携带 auto-trait，须显式 `+ Send + Sync`）；② `RwLock<T>: Send+Sync ⟺ T: Send+Sync`。因此 `RwLock<HashMap<…Vec<ListenerEntry>>>` 要求 `ListenerEntry: Send+Sync` → 监听器闭包必须 `Box<dyn Fn(…) + Send + Sync + 'static>`。v0.2 只把 `RefCell` 换成 `RwLock`，**监听器存储仍非 Send+Sync → `EventBus` 仍不满足 `Key::Value: Send+Sync`，编译依旧不可能**。验收 #9 的 `assert_send_sync::<EventBus>()` 在当前结构下会**编译失败**（而非通过）。
- **建议**：订阅 API 增加 `Send + Sync` 上界并明示理由——`on/on_waterfall/on_reply` 的 `listener: impl Fn(…) + Send + Sync + 'static`（对应 `ListenerEntry` 内 `Box<dyn Fn(…) + Send + Sync>`）。事件总线是 **store 内全局服务**（跨 wasm 边界/桥可访问），监听器闭包约束与 `Key::Value: Send+Sync` 纪律一致；单线程组合线程内捕获 `Rc` 的监听器由此**编译失败**——这是有意取舍，草案须在最显眼处（§0 或 §1 义务）明示：「事件总线监听器闭包须 `Send+Sync`（store 值纪律）；纯线程私有总线如需 Rc 捕获可另行设计非 Send-sync 变体，不属本层」。若不想牺牲 Rc 捕获，改「store 存 `Arc<dyn EventSink + Send+Sync>` 句柄 + 组合线程本地总线」的间接层（实现更重）。
- **验收 #9 同步**：`assert_send_sync::<EventBus>()` 待上述修复合法的编译断言。

### M-2'（阻塞）：bail 语义与 `on_reply` 的返回类型 `R` 矛盾——「首个返回 Some(r) 即停」无法从 `Fn(&P)->R` 判定

- **位置**：§2.1 `ReplyListener<P, R> = Fn(&P) -> R`（on_reply 共用 serial/bail）；§2.2 表 bail 行「首个返回 `Some(r)` 的听众即停」。
- **问题**：bail 的判定依赖「听众**是否返回**」（TS：返回 `any` / `void`；void = 继续）。而 `on_reply` 的听众返回纯 `R`——**每次调用总有返回**，bail 将恒停于首听众（`bail -> Option<R>` 中的 `None` 永不出现），退化为「只跑第一个听众」，与 TS bail 语义相悖。serial（收集全部 `Vec<R>`）与 bail（取首个"非空"）需要不同的返回形态，**不能共用一个 `Fn(&P) -> R`**。
- **建议三选一（文档需选明）**：
  1. **on_reply 返回 `Option<R>`**（`Fn(&P) -> Option<R>`）：None = 继续（bail 语义天然成立）；serial 收集 `Vec<Option<R>>`（“串行”变成收集全部回复，类型变 `Vec<Option<R>>`——需接受串行语义调整）——改动小但 serial 类型变化；
  2. **拆分 `on_serial<P,R>`（`Fn(&P)->R`）+ `on_bail<P,R>`（`Fn(&P)->Option<R>`）**：各自类型贴合 TS（serial=收集全部 R；bail=首个 Some 停），ListenerEntry 相应分 `Serial`/`Bail` 变体——类型最严谨，API 多一个入口；
  3. **bail 复用 `Fn(&P)->R` + 「R 的默认值 = 空」判定**（如 `R: Default` + `== Default` 视为空）——不推荐（引入魔数 Eq 语义）。
  - 推荐 **方案 2**（与 TS 语义最贴合、无类型怪异），若求简则方案 1。评审 v0.1 建议的「共用 on_reply」在本复核中判断为**不可行**（上述矛盾），此为 M-2 修订的落地修正。

### Minor

### m-3'（建议）：派发侧 R 一致性校验未明示

- `serial<P, R>` / `bail<P, R>` 应对照 modes 表校验 `R TypeId == TypeId::of::<R>()`，不匹配 panic（诊断「派发 R 与订阅写定的 R 不符」），否则运行时 downcast 失败（`Box<dyn Any>` → Vec<R>）。建议 §3.2 冲突检测段补一句话。

### m-4（建议）：§3.2 的 `names` 表与 `modes` 表功能重叠

- `names: HashMap<Symbol, TypeId>` 记录的载荷 TypeId 在 `modes: HashMap<(Symbol, Mode), (TypeId, TypeId)>` 已冗余包含（Emit/Waterfall 的 R 位为空即可）——可合并为单表（或明示 names 仅作「是否有同名事件」的存在性质询缓存）。非阻塞，结构简化建议。

### Nit

### n-1'（可选）：`Mode` 枚举定义与 serial/bail 的「共用 vs 分表」需明示

- §2.1 说 on_reply「注册一次、两种派发各自解释」——若走方案 1（共用 `Option<R>`），Mode 应为单 `Reply`（serial/bail 同槽）；若走方案 2（拆分），Mode 分 `Serial`/`Bail`。§3.2 未定义 `Mode` 枚举——补一行 `enum Mode { Emit, Waterfall, Reply }`（或含 Serial/Bail），消除歧义。

### n-2'（可选）：附录对照表补 serial/bail 订阅入口行

- 对照表有 `ctx.on`/`ctx.waterfall` 行，缺 serial/bail 的订阅行（TS 的 `ctx.serial`/`ctx.bail` listener）——补 `on_reply`（或拆分后的 on_serial/on_bail）对应行。

---

### Addendum v0.2 结论

v0.2 各项 Minor/Nit（m-1/m-2/m-3/n-1/n-2/n-3）与 M-2 主体（on_reply + R 单一性）修订到位；**但 M-1 只修了外层锁、未解决监听器闭包的 `Send+Sync` 上界（编译仍不可能），M-2 的「共用 on_reply」与 bail 语义自相矛盾**——两处仍阻塞。建议 ✓ **v0.3**：
1. 订阅/存储闭包统一加 `Send + Sync` 上界（§3.2 + §2.1 同步），§0 明示「监听器闭包须 Send+Sync」取舍；
2. bail 返回形态定案（推荐拆 `on_serial`/`on_bail`，或 `on_reply` 返回 `Option<R>`）；
3. 补派发侧 R 校验（m-3'）、Mode 枚举（n-1'）；合并 names/modes 或注明（m-4）。

---

## Addendum v0.3（复核 2026-08-18）

对 v0.3（采纳 v0.2 复核的修订稿）逐条复核：**M-1'（闭包 Send+Sync + 成立链 + §0 核心义务 + O-6' + 验收 #9）与 M-2'（拆 on_serial/on_bail + Mode 枚举 + 验收 #5）修订到位，m-3'/m-4/n-1'/n-2' 全部落地**。无阻塞。发现 2 项 Minor（其一为 O-5 决议与存储擦除的约束回卷、其一为 m-4 合并带来的跨模式载荷漏检），建议微修后冻结。

### 复核结论：已确认修订（通过）

- **M-1' 闭环成立**：`Send+Sync 成立链`（§3.2）——闭包 `+ Send + Sync` → `ListenerEntry` → `HashMap/Vec` → `RwLock` → `EventBus` → `Arc<EventBus>` ✅ 正确；订阅 API 与 §3.2 存储上界一致；§0「核心义务」与对照表「监听器闭包捕获」行明示取舍；O-6'（非 Send 变体）记录合理；验收 #9 在带界闭包下**合法**（此前 v0.2 会编译失败的问题已消除）。
- **M-2' bail 语义闭合**：`on_serial`（`Fn(&P)->R`）+ `on_bail`（`Fn(&P)->Option<R>`）拆分，bail 表「逐个询问、首个 Some 即停、全 None 得 None」——与 `Option` 判定自洽，v0.2 的「恒停首听众」矛盾消除；`Mode { Emit, Waterfall, Serial, Bail }` 显式化（n-1'）消除共用/分表歧义。
- **m-3'**：派发侧 R 一致性校验（`serial<P,R>`/`bail<P,R>` 对照 modes 表 R TypeId，不符 panic + 诊断）✅；**m-4**：names/modes 合并单表（Emit/Waterfall R 位 = ()）✅（但引入跨模式载荷漏检，见下方 Minor-2）；**n-1'** Mode 枚举 ✅；**n-2'** 对照表补 serial/bail 订阅行与闭包约束行 ✅。

### v0.3 发现

### Minor-1（建议）：O-5 决议「R: 'static 即可，不必 Send」与 serial/bail 的存储擦除矛盾——R 实需 `Send + Sync`

- **位置**：§2.1 `on_serial<P, R: 'static>` / `on_bail<P, R: 'static>`（§7 O-5 决议「R 不必 Send」）；§3.2 `ListenerEntry::{Serial,Bail}` 存 `...<dyn Any, Box<dyn Any + Send + Sync>>`。
- **问题**（Rust 语义事实）：`Box::new(r) as Box<dyn Any + Send + Sync>` 的 unsize 强转要求 `R: Send + Sync`。若仅 `R: 'static`（O-5 决议），serial/bail 订阅**无法装箱进 `Send+Sync` 的 `ListenerEntry`** → 编译失败。O-5（继承自 v0.1 的 n-3 确认）与 M-1' 的存储擦除自相矛盾。
- **建议**：把订阅上界统一为 `R: Send + Sync + 'static`（O-5 决议修订：serial/bail 的返回值进制 `Send+Sync` 总线，故 R 须 `Send+Sync`——实际代价极小：R 多为 bool/u32/枚举/String，天然 Send+Sync，无需 Arc）；§2.1 三处签名 + §7 O-5 决议 + 对照表一并同步。这是**必要小修**，否则动工即编译失败。

### Minor-2（建议）：m-4 合并单表后丢失「同一 SYMBOL 跨模式载荷一致性」检测

- **位置**：§3.2 单一 `modes: RwLock<HashMap<(Symbol, Mode), (TypeId, TypeId)>>`；§1 义务「同一 SYMBOL 事件唯一载荷」。
- **问题**：冲突检测按 `(Symbol, Mode)` 分键——同一事件名（SYMBOL `"x"`）若分别以事件类型 `P1`（Emit 订阅）与 `P2`（Serial 订阅）注册，落在不同 Mode 键（`(x,Emit)` vs `(x,Serial)`），**载荷 TypeId 互不见面 → 检测不到 P1≠P2**，违背 §1「两事件类型不得同 SYMBOL」（此场景恰是违规：两个不同事件类型声明同 SYMBOL，订阅点应 panic）。v0.2 的 `names: HashMap<Symbol, TypeId>` 恰好承担跨模式载荷统一，m-4 合并时该职责丢失。
- **建议**：订阅时除 (Symbol, Mode) 冲突检测外，追加「同 Symbol 任意既有模式载荷 TypeId 一致」检查（查询同 Symbol 任一既有项的载荷 TypeId 比对；跨模式载荷不一致 → panic + 双方类型名诊断）——或 modes 表保留全键但载荷列置于 Symbol 级。验收 #6 增补此断言（同名跨模式异载 → 订阅 panic）。

### Nit（顺带）

- **n-3'**：Minor-1 修订后，`serial<P,R>`/`bail<P,R>` 派发签名（§2.2）与 `subscribe_serial/subscribe_bail`（§3.1）的 `R` 上界同样须带 `Send+Sync`——四处签名同步即可，避免文档内上界不一。

---

## Addendum v0.3 结论

**无 Major 阻塞。** M-1'/M-2' 两处 v0.2 阻塞已彻底修订并有闭环验证。仅剩 **Minor-1（R 的 Send+Sync 上界，必要小修）与 Minor-2（跨模式载荷一致性检测，m-4 回归点）**——建议极轻微修订到 **v0.3.1**（或就地采纳以下一字之改：订阅 API/O-5 的 `R: 'static` → `R: Send+Sync+'static`；§3.2 补跨模式载荷检查 + 验收 #6 增补），即可**冻结**进入实现动工（按执行纪律，动工由用户下达）。

---

## Addendum v0.3.1（复核 2026-08-18 · 冻结判定）

对 v0.3.1（采纳 v0.3 复核的微修稿）逐条复核：**Minor-1 与 Minor-2 均修订到位并闭环，无残留阻塞**。

### 复核确认

- **Minor-1（R 上界）✅**：`R: Send + Sync + 'static` 已统一至六处签名——`on_serial`/`on_bail`/`serial`/`bail`（§2.1/§2.2）+ `subscribe_serial`/`subscribe_bail`（§3.1）；O-5 决议修订（v0.3 前「`R:'static` 即可」作废并注明原因：`Box<dyn Any + Send + Sync>` unsize 强转要求）。与 §3.2 存储擦除自洽。
- **Minor-2（跨模式载荷一致性）✅**：§3.2 冲突检测追加「同 Symbol 任一既有模式载荷 TypeId 一致」检查（不一致 panic + 双方类型名诊断），保证「一个 SYMBOL = 一个载荷类型」全局成立（Emit(P1) + Serial(P2) 于同一 SYMBOL 亦被拦截）；验收 #6 增补对应断言。
- **n-3' 四处签名同步** ✅（实际六处全同步）。

### 末次一致性扫尾（无新发现）

派发 downcast 链（`Box<dyn Any+Send+Sync>::downcast::<R>` 合法，`R: Any`）、emit 的 `&P::Payload → &dyn Any` 擦除一致、waterfall terminal 即时传入无需 Send+Sync（不进存储）、serial/bail 同事件下 R 各自独立（不同 Mode 键记各自的 R，语义自洽）、release-then-invoke + alive 快照与 E-1 组合无死锁、验收 #1–#9 与各修订点一一对应——全部核验通过。

### Nit（可选，实现期记录即可，不阻塞）

- **n-3''**：可在一行 doc 明示「同一事件名的 serial R 与 bail R 相互独立（不同模式各自记 R）」——当前 modes 表按 `(Symbol, Mode)` 记 R 已隐含此意，仅建议显式化以免读者误以为 serial/bail 的 R 必须相同。

---

## 冻结判定

**`docs/cordis-events-protocol-draft.md` v0.3.1 具备冻结条件**：评审闭环 v0.1 → v0.2 → v0.3 → v0.3.1，历次发现（M-1/M-1'/M-2/M-2' × 4 阻塞 + Minor 若干）全部采纳并复核通过，0 Major / 0 Minor 未决，仅 1 项可选 Nit。

按执行纪律：Phase 1 开工（`cordis-events` 首个里程碑：crate 骨架 + 订阅/派发核心 + 验收 #1–#9 首批直证）由用户另行下达；未下达前本草案保持冻结、不写实现代码。
