# PR #28 缺口分析审查报告（431fcf6 + docs）

> 审查员：独立 PR 审查（subagent）
> 范围：commit `431fcf6`（docs，`docs/TS-REFERENCE-GAP.md` 新增 + `docs/THEORY-MAP.md` PR #28 行）
> 对照物：cordiverse/cordis v4 浅克隆（`/tmp/cordis-ts`）、本仓库当前代码
> 方法：逐条抽查报告对 TS 源码的引用（行号/语义）、核对我侧状态描述（G1-G9/18 项对应/5 项领先）、核实结论诚实性。禁 wasm/全 workspace；未跑测试（纯 docs 审查）。

## 一、审查范围

- `docs/TS-REFERENCE-GAP.md`（新增 128 行）：§1 真实功能缺口 G1-G9、§2 公开差异重估表、§3 已对应 18 项、§4 反向领先 5 项、§5 行动建议。
- `docs/THEORY-MAP.md` PR #28 行（处置⑩ 重开、⑫ 零依赖路径、⑪ 澄清）。
- 逐条核对报告引用的 TS 源码位置：`packages/core/src/{fiber,reflect,service}.ts`、`packages/loader/src/{index,config/entry,config/isolate}.ts`、`packages/include/src/index.ts`、`packages/hmr/src/index.ts`。

## 二、逐条发现

### 2.1 TS 源码引用核对（关键断言逐一验证）

| 报告断言 | 引用位置 | 核实结果 |
|---|---|---|
| `Fiber.update(config, noSave)` → 校验 + restart（fiber 保留） | fiber.ts:476-485 | ✅ **完全一致**：477 `assertActive()` → 479 `resolveConfig` → 480-483 `waterfall('internal/update', config, noSave, () => { fiber.config = config; fiber._error = undefined; return fiber.restart() })`。观察者先触发、后换 config、再 restart 的序描述准确。 |
| `internal/update` 写回 `entry.options.config = config; tree.write()` | loader/index.ts:74-80 | ✅ **一致**：74-80 恰为 `internal/update` 钩子（第 77 行写 config、第 78 行 write，报告写 74-80） |
| 自销毁经 `internal/plugin` 写回 `disabled = true` | loader/index.ts:88-124 | ✅ **一致**：88-124 是 `internal/plugin` 的 6-case 过滤 + 122-123 `disabled = true; write()`。报告对「自销毁（ctx.fiber.dispose()）→ disabled 写回」的半段归属（G1 剩余项、⑩ 剩余项）准确。 |
| per-key inject 写入 `ctx[Context.intercept]` 链 | fiber.ts:139-144 | ✅ **一致**：137 `Object.entries(this.inject)` → 139 `Object.create(parent intercept)` → 140-143 逐键覆写。报告写「139-144」，实际写循环在 140-143，属可接受的范围略宽，非误导。 |
| `EntryOptions.inject` | config/entry.ts:8-15 | ✅ **一致**：第 14 行 `inject?: Inject | null`。 |
| isolate per-key `{ db: true | 'label' }` | config/isolate.ts:73-85 | ✅ **一致**：75 `entry.options.isolate?.[name]`、77-78 `true`→LocalRealm、80 `label`→GlobalRealm。报告对 Local/Global 二分的描述准确。 |
| include 文件树（watch/refresh/patch/write） | include/src/index.ts | ✅ **一致**：`Include extends EntryTree`（48）、refresh（187-190）、applyPatches（101）、write/_writeFile（192-216）。 |
| HMR 真模块图（`ModuleJob.linked` 递归） | hmr/index.ts:31-42 | ✅ **一致**：31-42 `loadDependencies` 用 `job.linked` 递归、跳过 node: 与 node_modules。 |
| G5 插值（`interpolate`/`__jsExpr`） | config/utils.ts | ✅ **一致**：10-27 `interpolate`/`isJsExpr`/`__jsExpr` 均在。 |
| 效应四形态（函数/Promise/迭代器/异步迭代器） | fiber.ts:54-64 | ✅ **一致**：`Effect<T> = SyncEffect | AsyncEffect`，四形态齐全。 |
| G8 `reflect.set` 就地变异不 notify | reflect.ts:162-173 | ✅ **一致**：`impl.value = value; return true`，无 notify。报告对「我们 `AlreadyBound` 拒绝、更严格」的自我刻画与我侧 context.rs:252/306 一致。 |
| G9 `check` 谓词（provider 在册但不可用 → 消费者不可见） | 报告写 reflect.ts:371-383、fiber.ts | ⚠️ **nit-1：行号张冠李戴**。`_checkImpl` 实际在 **fiber.ts:371-383**（`if (impl.check && !impl.check.call(...)) return delete this._store[name]`）——reflect.ts 全文仅 281 行，不存在 371-383。语义描述（`provide(name, value, check)` 携带谓词；谓词假 → 消费者 store 删除 → `_refresh` epoch INACTIVE）正确，但 `reflect.ts:371-383` 这一归属是错误的；`provide` 携带 `check` 的实插在 reflect.ts:175-186（报告未错引此处，仅 371-383 悬空）。 |
| Alg 2 `provide` 逆 = 删绑定 + notify + `allSettled` 排水 | reflect.ts:175-203 | ✅ **一致**：195-198 `delete + notify + Promise.allSettled`。 |
| Alg 3 notify 逐 fiber 扫描 | reflect.ts:205-227 | ✅ **一致**：`for (runtime) for (fiber)` 全量遍历（O(F)）。 |
| 目标 digest = provider uid 串接 | fiber.ts:385-397 | ✅ **一致**：`epoch += ':' + impl.fiber.uid`。 |
| proxy 访问链 INACTIVE_ACCESS | reflect.ts:62-98 | ✅ **一致**：`cannot get required service ... in inactive context`（第 87 行附近）。 |

**结论：除 G9 一行号归属错误（nit-1）外，报告对 TS 参考实现的引用无张冠李戴、无夸大。** 所有「带完整参照/可参照实现」的定性均有源码依据。

### 2.2 我侧状态描述核对（G1-G9 / 18 项对应 / 5 项领先）

- **G1/G2 已落地**：核对当前代码确证 `Fiber::update`（fiber.rs:204）、`Entry.inject`（lib.rs:151 `pub inject: Intercepts`）已存在，runtime.rs:407-415 已落地失败复活路径。报告在 §2 处置⑩ 行写「重新打开为 G1」、在 §3 末尾写「hook 注册可逆性一致（G4 时复用）」，与「G4 未实现」自洽，无前后矛盾。
- **G3/G8 我侧刻画准确**：`Entry.isolate: Option<IsolateAnnotation>`（lib.rs:143，per-entry 而非 per-key）与报告「ρ 全键等值的退化」一致；`set` 走 `AlreadyBound` 错误（context.rs:252）与报告「更严格」一致。
- **18 项已对应 / 5 项领先**：抽查 §3/§4 各条目（Alg 6 Proxy、Alg 2/3/5/8/9/10、失败模型、group、disabled 传播、wasm 沙箱、双语言、类型化键），均与我侧文档脉络（THEORY-MAP 既有记录）相符，无虚列。

### 2.3 结论诚实性（处置⑩/⑪/⑫）

- **⑩ 重开**：报告把 M3-PR3「编排方责任、公开差异关闭」的收口结论**主动推翻**，理由是「TS 参考实现证明该方向可实现且是 §5.2.1 双向绑定的组成部分」。属实——TS loader 的 `internal/update`/`internal/plugin` 写回确为 loader 契约的一部分，报告据此把「公开差异」升级为「待实施缺口」是诚实而非美化。
- **⑫ 依赖清单文件路径**：报告 §2 与 THEORY-MAP 行均写「编排工具生成依赖清单文件（文本格式）→ `HashMapGraph` 消费，无需 TOML/JSON 解析器」。此为**零依赖可行性论证**（规避 serde_json/toml），与 hmr 现有「算法数据驱动、仅换数据源」的判断一致，措辞审慎（「发现零依赖可行路径」而非「已实现」），无夸大。
- **⑪ 澄清**：报告区分「TS 无组级 isolate 概念」与「我们扩展字段语义未定义」，把归属从「TS 缺失」纠正为「我们扩展语义未定义，随 G3 落在子条目上」。诚实且厘清了责任边界。

## 三、总体结论

**通过（仅 1 项 nit，无 major）。**

`docs/TS-REFERENCE-GAP.md` 是一份**高保真**的缺口分析：所有关键 TS 源码断言逐条核对均属实，无张冠李戴、无夸大（G1-G9 的「缺口/差异/领先」定性均有源码或我侧代码依据）；我侧状态描述（G1/G2 已落地、G3/G8 更严格、18 项对应、5 项领先）与仓库当前代码一致；结论诚实——⑩ 主动推翻自身早先收口、⑫ 以零依赖论证替代「暂缺」、⑪ 厘清扩展语义归属。

唯一缺陷为 nit-1：G9 的 `reflect.ts:371-383` 行号归属错误（正确位置是 `fiber.ts:371-383`；reflect.ts 仅 281 行）。纯引用笔误，不影响结论正确性，建议后续顺手订正。

**major：0 ｜ nit：1**
