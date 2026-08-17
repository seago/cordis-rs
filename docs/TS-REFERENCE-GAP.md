# TS 参考实现对照：缺口分析（cordiverse/cordis v4 ↔ cordis-rs ↔ 论文）

**日期**：2026-08-17 ｜ **参照对象**：<https://github.com/cordiverse/cordis>（TypeScript 版 Cordis v4，论文所述实现；浅克隆于 `main`）
**方法**：通读 TS `packages/core`（fiber/reflect/registry/context/events/service）、`packages/loader`（index/internal/config/*）、`packages/hmr`、`packages/include`、`packages/group`，逐特性对照论文（§3.1/§5.1/§5.2.1/§5.2.2/§6.2）与本仓库（cordis-core / cordis-loader / cordis-hmr / cordis-wasm / examples/im-bot）。

**一句话架构差异**：TS v4 的「loader」是核心之上的薄层——深层语义全部在 core（plugin/fiber/reflect/registry/events + Service 基类）；条目/组/isolate/HMR 以 `internal/*` 事件钩子挂在 core 上。我们的 loader 是自足的声明式协调器（无事件层），core 语义等价但 API 形态不同。**多数差异是形态/载体差异，少数是真实功能缺口**——下表按优先级排列。

---

## 一、真实功能缺口（按优先级）

### G1【关键】双向绑定：`fiber.update(config)` → 条目写回（§5.2.1 "the binding runs in both directions"；处置⑩）

- **TS 实现**：`Fiber.update(config, noSave)`（fiber.ts:476）→ config 校验 + `restart()`（就地重跑效应，fiber 保留）；loader 经 `internal/update` 钩子把新 config **写回条目**（`entry.options.config = config; tree.write()`，loader/index.ts:74-80）并持久化；fiber 自销毁（`ctx.fiber.dispose()`）经 `internal/plugin` 钩子写回 `entry.options.disabled = true`（loader/index.ts:88-124）。
- **我们**：无 `update` API；`Fiber::retire` 是运行时退役（粘滞、不改条目）；⑩ 已"评估收口"为编排方责任（M3-PR3）。
- **对照结论**：**TS 参考实现证明该方向可实现且是 §5.2.1 双向绑定的组成部分**——我们此前收口过于保守。`retire` 粘滞测试钉死的语义仍正确（我们无写回），但"缺席"应从公开差异升级为**待实施缺口**。
- **建议**：① `Fiber::update(config)`（校验 → 可逆重跑，fiber 保留——等价于我们 revision 重建的依赖者行为，内部可走 `retire + reload`）；② loader 观察者回调（`Loader::on_update`/`on_self_dispose` 注册表，Rust 无事件系统，用 trait 对象替代 `internal/*` 钩子）→ 条目写回 + 持久化留给编排方。测试：`fiber.update` 后依赖者级联重激活（同 revision 重建语义）、写回回调收到新 config。

### G2【关键】插件注入的**每键配置**（Def 30/31 的 ι(k) 实用形态；TS `inject: { [key]: config }`）

- **TS 实现**：`Plugin.inject` 可为 `{ [key]: config }` 或 `@Inject('db', {...})` 装饰器；fiber 激活时把每键 config **写入 `ctx[Context.intercept]` 链**（fiber.ts:139-144）；被注入的 Service 经 `resolveConfig` 沿 intercept 原型链合并出自身配置（service.ts:51-67，`Config.merge` 或浅合并）。功能插件"声明依赖并携带参数"。
- **我们**：`#[component(inject = [K])]` 只声明键（KeySet）；`Entry.config` 是整组件 config；拦截元数据（M2-PR2 的 `get_meta`/`declared_metadata`/`intercept_set_boxed`）是键→`Box<dyn Any>` 元数据，**不是**"注入者给被注入服务传参"的通道。
- **对照结论**：论文 Def 30/31 的 `ι(k)` 合并（`get(k,μ)=σ(k)(μ⊕ₖι(k))`）我们实现了读路径合并；但"依赖声明携带配置"（Koishi 生态的常态：`inject: { database: { type: 'sqlite' } }`）未实现。**缺口**。
- **建议**：`#[component(inject = [K => config 形态])]` 扩展或 `Entry.inject` 字段（TS EntryOptions.inject 同款）：注入键 → config，激活时写入本 fiber ctx 的拦截表，被注入方经 `get_meta` 读取并合并（我们已有读取基元）。

### G3【中】per-key isolate 粒度（§5.2.1 Algorithm 7；TS `EntryOptions.isolate: { name: true | label }`）

- **TS 实现**：条目 isolate 是**每键映射**（`isolate: { db: true }` 本地 realm、`{ db: 'label' }` 全局 realm，isolate.ts:73-85）；`loader/patch-context` 逐键换 realm + delimiter 通知（isolate.ts:92-149）；全局 realm 无引用即 GC（isolate.ts:151-168）。
- **我们**：`Entry.isolate: Option<IsolateAnnotation>` 对**条目全部键**生效（Local = 所有键 `local:{id}:{key}`）；Global 同理。无"只隔离某键/混合粒度"。
- **对照结论**：论文 ρ 是 per-key 映射（`ρ: k ↦ σ`）——我们 per-entry 注解是 ρ 全键等值的退化；真实配置树常需混合（`{ db: 'data', cache: true }`）。**缺口（粒度）**；Algorithm 7 的 realm 重指派机制（patch_isolation）与 delimiter 等价适应（M2-PR4 记录）保持成立。
- **建议**：`Entry.isolate` 改 `Dict<Symbol, IsolateAnnotation>`（键 → 注解），`annotated_ctx` 逐键应用；`patch_isolation` 逐键 diff（现有 Δ 键机制天然支持）。

### G4【中】hooks/事件最小集（TS `events` + `internal/*` 生命周期；支撑 G1/G5/G6 的工程形态）

- **TS 实现**：5 种派发（emit/parallel/serial/bail/waterfall）+ `internal/plugin|status|service|update|get|set|listener|dispatch` 事件；loader 写回、isolate patch、HMR、日志全部挂钩其上；hook 注册是**可逆效应**（`register` 经 `ctx.fiber.effect`，卸载自动移除）。
- **我们**：无事件/钩子系统；loader 协调、HMR 为直接调用。
- **对照结论**：论文未建模事件系统（框架工程层），但 TS 的双向绑定（G1）、条目生命周期观察、HMR 触发都建于此。**缺口（工程形态）**——最小实现即可解锁 G1/G5。
- **建议**：`cordis-core` 增最小 hook 注册（`on/off`，可逆：注册经 fiber 效应追踪）+ 瀑布式调用（`waterfall`）即可；先支持 `internal/update`、`internal/plugin`（fiber 生灭）、`internal/service`（绑定变更）三个事件。

### G5【中】config 表达式插值（TS `interpolate`/`__jsExpr`，config/utils.ts）

- **TS 实现**：配置值可为 `{ __jsExpr: 'ctx.db.url' }`，加载时以 ctx 为作用域 `eval`，递归插值整棵 config；`Entry._resolveConfig` 每次（重）加载时求值——**同一配置随 ctx 状态变化得到不同值**。
- **我们**：config 是 `Rc<dyn Any>` 不透明值，无求值层。
- **对照结论**：论文未要求（工程 DX），但它是 Koishi 配置生态的常态（引用环境/其他服务）。**缺口（DX，低-中）**。
- **建议**：编排工具层提供"配置求值"（串模板 → 值），或 `Config` trait 加 `resolve(&Context)` 可选实现；不进核心。

### G6【中】include 文件树完整形态（TS `@cordisjs/plugin-include`：文件即配置树）

- **TS 实现**：`Include extends EntryTree`——yaml/json 文件**就是**一棵配置树；文件 watch 变更 → `refresh()` 热更新；`patches` 运行时补丁层（insert/name/config/disabled 覆盖）；双向写回持久化到文件（write()）；'internal/update' 钩子联动（loader/index.ts 与 include 同时监听）。
- **我们**：`Entry::include(id, children)` = 与 group 同构的结构嫁接；"外部配置文件的解析由编排方承担"（M2-PR3 记录）。
- **对照结论**：论文 §5.2.1 的 include 语义（外部配置嫁接）我们已对应；TS 的**文件生命周期**（watch/refresh/持久化/patch）未实现。**缺口（中，工具层）**。
- **建议**：编排工具 crate（允许 serde_yaml）实现文件树适配器：文件 → `Vec<Entry>` → loader.apply；watch 变更 → 重新解析 + apply。核心零改动。

### G7【中】config 校验与值级 diff（TS StandardSchema `Config.validate` + `deepEqual` diff）

- **TS 实现**：插件 `Config` 声明 schema，激活前 `validate`（ValidationError 含字段路径）；`Entry.update` 用 `deepEqual` 做**值级 diff**（config 内嵌字段变化 → `fiber.update` 精确触发）。
- **我们**：config 无校验（`Rc<dyn Any>` downcast 失败即 expect panic）；config 变更依赖调用方递增 `revision`（M0 记录：配置值不可比较）。
- **对照结论**：论文未要求 schema（typed world 是我们的已记录方向）；值级 diff 是"最小扰动协调"的精确形态。**缺口（中，typed world 路径）**。
- **建议**：`Config` trait 增可选 `fn validate(&self) -> Result<(), String>` 与 `fn same(&self, other: &dyn Any) -> bool`；loader 在 `same == true` 时免重建（revision 仍作兜底）。

### G8【低-中】`ctx.set` 就地改值语义（TS reflect.set 变异现有绑定；论文 "overwriting its own binding in place is therefore not observed"）

- **TS 实现**：`ctx.set(name, value)` 要求 props 已声明且属本 fiber，**变异 `impl.value` 就地**（不 notify、不换绑定）。
- **我们**：`set` 前置 `ρ(k) ∉ dom(σ)`——已绑定则 `AlreadyBound` 错误；"覆盖"必须 withdraw + 重装（触发 notify，被观察）。
- **对照结论**：论文原文明示 in-place 覆盖**不应被观察**——TS 以静默变异实现；我们以错误拒绝。语义方向上我们更严格（防静默丢值），但缺"确认过的就地更新"通道。**差异（低-中）**。
- **建议**：文档化 + 可选 `set_in_place`（仅本 fiber、值替换、不 notify）；im-bot/bench 不依赖。

### G9【低】Service `check` 可用性谓词（TS `Service[check]`：provider 在册但"不可用"→ 消费者不可见）

- **TS 实现**：`provide(name, value, check)` 携带谓词；`_checkImpl` 谓词为假 → 消费者 epoch INACTIVE（fiber.ts:371-383；REVIEW-431fcf6 nit 修正行号）。loader 的 `[Service.check]` 用其实现"有任务在跑时 loader 服务不可用"。
- **我们**：绑定存在即可用；无谓词。
- **对照结论**：论文 §3.2 的满足谓词是"键在册"；逻辑可用性是额外能力。**缺口（低）**，可用于优雅降级/门控。
- **建议**：`Key::Value` 绑定附加可选 `check`（值携带谓词或 provider 声明），`resolve` 时求值；先记录，按需实现。

---

## 二、已记录为公开差异、TS 有可参照实现的条目（建议随上表优先级重估）

| 条目 | 我们的记录 | TS 参照实现 | 重估建议 |
|---|---|---|---|
| 处置⑩ 双向写回 | "编排方责任，公开差异关闭"（M3-PR3） | `Fiber.update` + `internal/update` 写回 + self-dispose 写回 disabled（loader/index.ts:74-124） | **重新打开为 G1**——TS 证明 §5.2.1 "runs in both directions" 是 loader 契约的一部分 |
| 处置⑫ 模块图生产化适配器 | "零依赖 → 构建工具 crate" | HMR 用 Node 内建 ModuleJob.linked 真模块图（hmr/index.ts:31-42,160-165） | **发现零依赖可行路径**：编排工具生成**依赖清单文件**（文本格式，每模块一行依赖列表）→ `HashMapGraph` 直接消费——无需 TOML/JSON 解析器 |
| 处置⑪ 组条目 isolate | "继承至子树，随 typed world" | TS 组 = 普通 EntryGroup（条目数组），isolate 逐键注解天然 per-entry | TS 无"组级 isolate"概念——我们 ⑪ 的"组 isolate 不应用"是**我们扩展字段的语义未定义**，非 TS 缺失；配合 G3 per-key isolate 后，组的 isolate 可落到子条目 isolate 上（无继承语义） |
| config 变更 = revision 重建（新 fiber） | M0 协调键记录 | TS `fiber.update` 就地 restart（fiber 保留、uid 不变） | 依赖者行为等价（provide 撤销/重装 → 级联重激活）；差异在条目自身 fiber 身份与写回通道——随 G1 一并解决 |
| 异步效应 | ADR-0003：同步引擎，async 后行 | TS effect = 函数/Promise/迭代器/异步迭代器（fiber.ts:54-64,229-273） | 维持记录；TS 是 async 语义的参照（epoch 守卫停止在途异步迭代 = 我们的 armed/步界中断的异步版） |

---

## 三、已对应（核验通过，无缺口）

- **Alg 6 Proxy 访问链**：TS Proxy handler get（reflect.ts:62-98）链上行 + `cannot get required service ... in inactive context`（INACTIVE_ACCESS）↔ 我们 `Context::resolve`（access.rs 4 测试）。✓
- **Alg 2 set/provide 可逆注册 + notify + 排水**：TS `provide` 逆 = 删绑定 + notify + `await allSettled`（reflect.ts:175-203）↔ 我们 `ctx.set` 逆 + 级联（同步）。✓
- **Alg 3 notify 逐 fiber 扫描**：TS `notify(names, filter)` 遍历全部 runtime.fibers（reflect.ts:205-227）↔ `notify_fibers`（bench 直证 O(F) 线性）。✓
- **目标 digest = 提供者 uid 元组**：TS `_refresh` epoch 串接 `impl.fiber.uid`（fiber.ts:385-397）↔ `compute_target`。✓
- **Alg 5 惯性状态机**：TS `inertia` + `_setEpoch` 防重入、转换完成自链 ↔ 我们 inertia/自链。✓
- **失败模型**：TS FAILED（_error）+ await() 抛错 + 可重试 ↔ 我们 L-Raise `Inactive(ζ)` + 失败后可重试（failure_model 测试）。✓（形态：TS 独立 FAILED 态，我们 ζ 折损进 Inactive——M2-PR1 记录）
- **Alg 8/9/10 HMR 三阶段 + 回滚**：TS classify（accepted/declined 不动点）/ detect（依赖树 ∩ accepted）/ reload（缓存备份 + 重导入 + 失败回滚，ESM+CJS 双缓存）↔ 我们 cordis-hmr（HashMapGraph/WasmLeafGraph + 事务回滚 + 双回滚测试）。✓（机制等价；模块图来源不同，见 ⑫）
- **Alg 7 重指派**：TS patch-context（逐键新 realm + delimiter δk 通知 + 绑定迁移 + 依赖者谓词通知）↔ 我们 patch_isolation（树成员等价适应，M2-PR4）。✓（机制：TS 字面 delimiter，我们等价替代）
- **isolate realm 语义**：LocalRealm（'#'+id）/ GlobalRealm（'@'+label）↔ IsolateAnnotation::Local/Global。✓
- **group 配置树**：TS EntryGroup（子列表 keyed diff、move 支持）↔ Entry::group + keyed diff 递归。✓
- **disabled 祖先传播**：TS Entry.disabled 上溯祖先 ↔ 我们组禁用级联拆除。✓
- **`cordis:` builtin** ↔ register_component 名称表。✓
- **plugin 三种形态**（函数/类/对象 apply）↔ Component trait（宏生成 apply_impl）。✓
- **可逆 hook 注册**（TS register 经 fiber.effect）↔ 我们无 hook 但效应可逆性一致。✓（G4 时复用）
- **loader.await / getTasks 静止等待** ↔ `runtime.is_quiet()`。✓

---

## 四、反向对照（我们领先 TS 参考实现的）

- **wasm 沙箱 + 能力面**（import 面 = inject 键、trap 捕获、宿主存活）：TS 进程内无沙箱概念——我们 M1 是超集。
- **双语言 guest**（Rust + Go 同 loader 互通）：TS 无。
- **形式化指标直接断言**（Thm 59/61/66、progress bound、oracle×引擎对照）：TS 无元理论测试面。
- **类型化键 + 符号跨边界**（TypeId + SYMBOL intern）：TS 是字符串键。
- **同步确定性引擎**：TS 异步（调度非确定）；我们同步（静止可判定）——论文元理论更贴近我们。

---

## 五、行动建议（排序）

> **进度**：G1、G2 已于 PR #29 落地（2026-08-17）——见「G1/G2 落地记录」；G3–G9 待办。

1. **G1 双向绑定**（重新打开处置⑩）：`Fiber::update` + loader 写回观察者——§5.2.1 明示语义，TS 有完整参照。✅ **已落地（PR #29 + PR #30）**：core `Fiber::update`（就地重跑、fiber 身份保留、依赖者级联）+ `Runtime::set_update_hook`/`set_retire_hook`；loader `update_entry`/`entry_config`/`register_update_hook`/`register_retire_hook`（fiber→条目反查写回）。**self-dispose → 条目 `disabled` 写回（TS `internal/plugin` 半段）已落地（PR #30）**：`Fiber::retire` 触发退役观察者，loader 过滤「条目仍在且未 disabled」（= 组件自退役）写回书签 `disabled=true`；apply 期间 teardown 的 retire 走 pending 队列延迟排空（防 entries 重借）；`loader.fiber(id)` 对退役 fiber 返回 None（已卸载语义）；desired 显式 `disabled=false` 重新启用（disabled 为协调字段）。G4 hooks 最小集 = `update_hook` + `retire_hook` 两观察者。
2. **G2 每键注入配置**：`Entry.inject` / 宏扩展——Koishi 生态常态，论文 Def 30/31 的 ι 实用化。✅ **已落地（PR #29）**：`Entry.inject`（键 → 拦截元数据）+ `with_inject`，实例化应用（遮蔽同键 intercept、组条目经派生继承），读取方 `get_meta` 右偏合并；变更纪律同 config（revision 代行）。
3. **G3 per-key isolate**：`Entry.isolate` 粒度化（顺带重估处置⑪：组的 isolate 落子条目）。✅ **已落地（PR #31）**：`Entry.isolate` 改 `BTreeMap<Symbol, IsolateAnnotation>`（TS `Dict<true|string>` 同型，混合粒度 `{val: Local, sum: Global("x")}`）；`realm_of` 逐键查表、`patch_isolation` Δ 键域扩展为声明键 ∪ 新旧 isolate 映射键（Algorithm 7 重指派保持逐键）；**组条目 per-key isolate 经派生链拷贝继承给子条目、子条目注解覆盖（最近注解优先）——处置⑪ 顺势收口**；组 isolate 变更仍走整棵重建（保守路径，测试直证）。测试 loader +3（混合粒度 / 组继承与覆盖 / 组 isolate 变更重建）。
4. **G4 hooks 最小集**（internal/update、internal/plugin、internal/service）——G1/G6 的工程底座。✅ **已落地（PR #30）**：`update_hook` + `retire_hook` 两观察者（`internal/update` + `internal/plugin` 半段）；`internal/service`（绑定变更观察）留待按需。
5. **G7 config 校验 + 值级 diff**（typed world 前置）。✅ **已落地（PR #33，opt-in）**：`Config` trait（可选 `validate` + 值级 `same`）+ `Loader::register_config::<C>()` 类型注册表（`&dyn Any` 无法 downcast 到 unsized `dyn Config`，按类型注册 cast）；`validate` 失败 = 配置错误 panic（公开差异：TS → 失败态可重试）；`same` 为真 → revision 递增免重建（TS `deepEqual` 同型）；**HMR 兼容纪律**：`String` 等常用类型不实现 `same`（cordis-hmr 依赖 revision 递增触发重载）。**模型差异（REVIEW-1c86b5f nit-4）**：组条目 config 变更走整棵重建（保守），TS 为 holder fiber 就地 `update`（fiber 保留）——结构性差异记录在案。
6. **G5 配置插值 / G6 include 文件树 / ⑫ 依赖清单文件**（工具层，零依赖可行路径已找到）。✅ **G5/G6 已落地（PR #32，安全收窄）**：`cordis-loader::interpolate`（`{{name}}` 受控占位符替换，resolve 回调编排方提供；TS `with(ctx) eval` 任意表达式不支持、未解析保留原样 = 公开差异）；`Patch` + `apply_patches`（desired 树纯变换：`id` 递归匹配的 `name`/`config`/`revision`/`disabled` 覆盖 + `insert` 向组插入；TS `PatchOptions` loader 侧子集）。**剩余（编排工具层，零依赖纪律）**：yaml/json 文件读取、文件 watch、写回持久化（G6 后半）、依赖清单文件（⑫）。
7. **G8/G9** 文档化或按需。✅ **均已落地（PR #34）**：G8 `Context::set_in_place`（本 fiber 已声明供给键的就地改值——不 notify、不追踪（idΓ 式，论文 "overwritten in place is therefore not observed"）；未绑定/非安装者 → `Err`；越界写纪律同 `set`）；G9 `Context::set_with_check`（TS `provide(name, value, check)` 参照——绑定携带可用性谓词，`provider_of` 每次求值、为假视为未提供（依赖者 Inactive）；谓词须纯、翻转不触发 notify（依赖者经 refresh 感知））。**公开差异（REVIEW-54814d0 nit-5）**：`get`/`resolve` 裸读不经 check——TS `_checkImpl` 删除 `_store[name]` 使直读也 respect check；Rust 侧 check 只影响依赖解析（`provider_of`/`σγ`），裸读仍见绑定值。测试 core `check_in_place.rs` 5。
8. **异步效应**：维持 ADR-0003 记录，typed world/async 阶段以 TS 为语义参照。
