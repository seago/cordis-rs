# 代码审查报告：commit `d1263fa`（M3-PR1——IM-bot 三层拓扑案例 + broker 示例，处置⑦ 落地）

- **审查对象**：`d1263fa1549f2b5a96a68fc08e9da9850c4f778e`（`examples/im-bot/`（main.rs +289、bin/broker.rs +242、Cargo.toml +17）、`Cargo.toml` +1、`Cargo.lock` +9、`.github/workflows/ci.yml` +6）及配套 docs 提交 `8339f27d4a9518633008ed63b5a9c919f777f6e5`（`docs/PLAN.md` +1/−1、`docs/THEORY-MAP.md` +3/−2）
- **审查日期**：2026-08-17（仓库时区）
- **验证手段**：`git show d1263fa` / `git show 8339f27` 逐行核对 diff；读 `crates/cordis-loader/src/lib.rs`（`apply`/`apply_into`/`reconcile_into`/`Entry.revision`/`unload_from`/`teardown`）、`crates/cordis-core/src/context.rs`（`get`/`set`/`resolve`/供给纪律）、`crates/cordis-core/src/fiber.rs`（`state`/`retire`/`id`/`FiberState`）、`crates/cordis-core/src/effect.rs`（`once`）、`crates/cordis-macro/src/lib.rs`（`#[component]`）、`Cargo.toml`/`ci.yml`；从 `paper/paper.pdf` 以 ghostscript txtwrite 提取 §5.3 与 §6.2 原文逐行对照；实跑 `cargo run --quiet -p im-bot`（exit 0，全部断言通过）、`cargo run --quiet -p im-bot --bin broker`（exit 0）、`cargo clippy -p im-bot --all-targets -- -D warnings`（exit 0，0 警告）、`cargo fmt --check -p im-bot`（exit 0）。

---

## 结论：**需修复（major）**

§5.3 三层拓扑（`main.rs`）语义与断言**正确**——adapter/database/bot 三层供给键、切换后端=revision 触发 database 重建而 bot 级联重激活（fiber 不变）、重连 adapter、依赖不可用→bot 保持 Inactive 不报错，均与 §5.3 原文（"reactivates only the dependents whose resolved dependency changed" / "stays inactive until it appears, without erroring"）逐行吻合，实跑通过。

§6.2 broker 示例（`broker.rs`）存在 **1 项 major**：依赖方向与论文**相反**——§6.2 的"中央服务"（broker）**被后备提供者与消费者共同注入**（"a central service … is injected by both the backing providers and the consumers"），后备提供者"**向 broker 注册**（registers with the broker through a revertible effect）"；而实现让后备提供者 **provide** 各自注册键、broker **inject**（硬依赖）两注册键。后果是卸载任一后备会级联 broker→service→消费者停用——这正是 §6.2 所述 broker 设计用来**避免**的扰动（"updating a backing provider leaves the broker in place, so consumers see no change … and no reload is triggered"）。示例场景 3 的断言**明文断言了与论文相反的行为**（broker 停用、消费者级联停用）。另 3 项 nit。详见下文。

---

## 🔴 需修复（major）

### major1. broker 依赖方向与 §6.2 相反：后备提供者被建模为 broker 的**供给者**（broker inject 两注册键），而论文是后备提供者**向 broker 注入/注册**——卸载后备本应"broker 及其服务保持原位、仅路由集移除该后备"，实现却级联 broker→service→消费者整体停用

**位置**：`examples/im-bot/src/bin/broker.rs:51-64`（`BackingA`/`BackingB` 的 `provide = [RegAKey]`/`[RegBKey]` 及 `apply_impl` 内 `ctx.set::<RegAKey>`）、`:72`（`Broker` 的 `inject = [RegAKey, RegBKey], provide = [ServiceKey]`）、`:196-216`（场景 3 断言：broker 停用 / 消费者级联停用 / service 解除）、顶层模块注释 `:9-12`（引文）。

**事实**：§6.2 原文（`paper/paper.pdf` §6.2）：

> "Service broker: a central service that acts as the entrypoint for the interface **is injected by both** the backing providers and the consumers, so that multiple providers coexist and the broker dispatches each request among them. Compared to exclusive binding, the broker absorbs this perturbation: **updating a backing provider leaves the broker in place**, so consumers see no change to their dependency and no reload is triggered."
>
> "…each provider **registers with the broker** through a revertible effect, so unloading it **reverts the registration and drops it from the broker's routing set automatically**."

即：**中央服务（broker）本身是被（后备提供者与消费者）共同注入的依赖**；后备提供者通过**可逆效应向 broker 注册**；broker 维护的是"路由集"（内部数据，由各提供者的可逆注册维护），broker 自身的服务绑定**不因任何单一后备的增删而改变**。

实现（`broker.rs`）把这条边**反向**了：

- `BackingA`/`BackingB` **provide** `RegAKey`/`RegBKey`（`:51-64`），在自身 `apply_impl` 里 `ctx.set::<RegAKey>(id)`（`:59-61`、`:75-77`）——这是后备提供者把**自己的注册键**绑进全局 store，而非"向 broker 注册"；注册过程不经由 broker 提供的任何可逆通道。
- `Broker` **inject** `[RegAKey, RegBKey]`（`:72`）——broker 成了两后备的**硬依赖者**，两注册键任一消失即触发 broker 目标不可满足 → 停用 → `ServiceKey` 解除 → 消费者级联停用。
- 场景 3（`:196-216`）因此**明文断言并庆祝了与论文相反的行为**：`"后备 a 卸载 → 注册撤销 → broker 停用"`、`"消费者级联停用（service 解除）"`、`"service 已解除（broker 停用）"`。

这正是 §6.2 用 broker 形态试图**避免**的扰动——与"exclusive binding"里"switching … requires unloading one provider …, momentarily perturbing every consumer's dependency"同构。

**影响（major，需修复）**：可表达性案例在**招牌语义（服务代理吸收扰动）上失真**。论文的"卸载后备 = 从 broker 路由集撤销注册、broker 与消费者无感"被实现成"卸载后备 = broker 停用、service 解除、消费者级联停用"。示例把 §6.2 broker 的**目的**（避免 consumer 扰动）做成了它的**反面**，且以断言固化为"直证"。这不满足处置⑦"§6.2 broker 示例（可表达性演示）"的落地语义，也与提交信息 / THEORY-MAP PR #24 行 / PLAN M3 行所声称的"更新后备不扰动消费者（无 reload）"自相矛盾（场景 3 已证明并非"不扰动"）。

**建议修法**：按 §6.2 重排方向——
1. 增加一个"注册句柄"键（如 `RegKey`，`Value = Box<dyn Fn(String)>` 或一个 broker 持有的注册表接口）；`Broker` **provide** `RegKey` **与** `ServiceKey`，其 `apply_impl` 内部维护一个 `RefCell<Vec<String>>`（路由集），`RegKey` 的绑定值为"向路由集插入/移除"的闭包。
2. `BackingA`/`BackingB` 改为 **inject** `[RegKey]`（不再 provide 各自注册键），在 `apply_impl` 里经 `ctx.get::<RegKey>()` 取得注册句柄，并用**可逆效应**（`ctx.effect` 或在 `once` 内 `set`/`disposer`）把自己注册进 broker 路由集——卸载时逆自动撤销注册。
3. 断言改为直证论文语义：更新/卸载任一后备 → broker fiber **保持 Active**、`ServiceKey` 绑定**不变**（消费者无感、无 reload）、仅 broker 路由集成员变化；重注册 → 路由集恢复。可选再证一个消费者 fiber id 在全程不变的强断言（无重建）。
4. 同步更正 `broker.rs` 顶部模块注释、THEORY-MAP PR #24 行、PLAN M3 行中"卸载后备 = 可逆注册自动撤销 → broker 停用、service 解除"的相反表述。

### major1 的连带：场景 2 "更新后备不扰动消费者（无 reload）"实际被抢跑为 Active→Inactive→Active 抖振，断言只验证了终值与 fiber id（重激活≠重建），并未验证"消费者全程未被扰动"

**位置**：`examples/im-bot/src/bin/broker.rs:168-195`（场景 2 断言，尤其 `:185-190` 的 `fiber("c").id() == consumer_first`）、`:24-27` 模块注释引文 "updating a backing provider leaves the broker in place, so consumers see no change to their dependency and no reload is triggered"。

**事实**：场景 2 把 `b1` 的 revision 0→1（`:170-175`），`loader.apply` 的 `apply_into` 阶段一会因 `l.revision != entry.revision` 卸载（`teardown` → `fiber.retire()`）`b1`，回收 `RegBKey` 绑定；因 broker `inject = [RegAKey, RegBKey]`，broker 目标瞬时不可满足 → 停用 → `ServiceKey` 解除 → Consumer（inject `ServiceKey`）**瞬态转入 `Inactive`**；阶段二重实例化 `b1` 后逐级重激活。消费者全程发生了 **Active → Inactive → Active** 的抖振（其依赖 `ServiceKey` 被瞬时解除再重建），并非"sees no change to their dependency"。

示例的断言（`:176-190`）只查了三点：终值 `service_of == "via(impl-1)"`（reg-a 未动，天然成立）、`fiber("c").id() == consumer_first`（fiber **身份**跨重激活不变——这是"重激活≠重建"的正确断言，但**不等于**"无扰动"）、`is_quiet()`（终态静止）。**没有任何断言证明消费者在更新过程中保持 Active / 其 service 依赖未被解除**——即纸面宣称的"不扰动消费者"并未被行使，反而是被场景 3 的级联断言证伪了。

**影响（major，与 major1 同根）**：一旦按 major1 重排（broker 不再硬依赖两后备），此抖振自然消失，场景 2 的"不扰动"才真正成立并可直证。当前形态下该断言"名不副实"（只证"无重建"，非"无扰动"）。

**建议修法**：随 major1 重排后，场景 2 改为对**消费者 fiber 全程状态**的强断言（例如在 update 前后各取一次 state，或提供一个可观测点断言消费者从未离开 `Active`），并将注释中"无 reload"精确化为"无重建（fiber id 不变）且无扰动（service 依赖全程保持）"。

---

## ⚪ 细节（nit）

### nit1. `main.rs` 场景 1 所谓"同供给键替换"实为**同一 `database` 条目的 config 变更（revision 递增）**，措辞与既有 `same_supply_replacement_in_single_apply`（跨条目、两不同组件名共享同一供给键）语义混用

**位置**：`examples/im-bot/src/main.rs:100-103`（场景 1 注释"切换存储后端（sqlite → postgres，同供给键单次 apply）"）与提交信息 / THEORY-MAP PR #24 行 / PLAN M3 行的"同供给键替换"表述。

**事实**：切换存储后端是**同一条目 `id = "database"`** 的 `config` 从 `"sqlite"` 换成 `"postgres"` 且 `revision` 0→1（`Entry::new("database", "database", "postgres", 1, false)`），触发的是 `reconcile_into` 的 **revision 变更 → 重建** 分支（`loader/src/lib.rs:348`、`:430`）。而仓库既有的"同供给键替换"专指**两个不同条目 X、Y 提供同一键**在单次 `apply` 用 Y 替换 X（`same_supply_replacement_in_single_apply`，`loader/src/lib.rs:1031-1068`）。二者机制不同（前者是单条目重建 + 依赖者级联重激活，后者是跨条目的两阶段卸载/实例化）。行为断言本身**正确**，仅术语把"config 变更重建"说成了"同供给键替换"。

**影响（nit）**：语义无误，措辞易使读者（尤其对照 loader 测试者）误以为走的是两阶段替换路径。建议措辞改为"切换存储后端（同一条目 revision 递增 → 重建）"。

### nit2. `broker.rs` 的 `Consumer` 组件 `apply_impl` 返回一个"无效应"迭代器，但无注释说明其角色（纯粹靠 `inject = [ServiceKey]` 被服务可用性 gate 的"激活观察者"，不产生任何绑定/效应）

**位置**：`examples/im-bot/src/bin/broker.rs:112-123`

**事实**：`Consumer::apply_impl` 返回 `once(Box::new(|| Box::new(|| {}) as cordis::Disposer))`——即一个"产生零绑定、逆为 no-op"的单步效应。该写法与 `GroupHolder`（`loader/src/lib.rs:792-802`）一致，本身合法且 `clippy` 无警告；但消费者"为何要在此形态下存在"（它只是用 `inject = [ServiceKey]` 让自身激活状态被 service 可用性 gate，供断言观测"消费者是否被扰动"）未在注释中点明，读者易误以为遗漏了读取 service 的逻辑。

**影响（nit）**：可读性。建议补一行注释："消费者不读不写任何值，仅凭 `inject = [ServiceKey]` 把自身激活态作为'服务是否可用'的探针，供断言观测扰动"。

### nit3. docs 两提交与代码**字面一致**（通过审查要点 5），但承载了 major1 的同一语义错误，须随修复同步更正

**位置**：`docs/THEORY-MAP.md:151`（PR #24 行）、`docs/THEORY-MAP.md` 处置⑦ 行（"已落地（M3-PR1，PR #24）…卸载后备 = 可逆注册自动撤销（broker 停用、service 解除）"）、`docs/PLAN.md:314`（M3 行）

**事实**：THEORY-MAP PR #24 行与处置⑦ 行、PLAN M3 行均忠实复刻了 `broker.rs` 的断言行为（"卸载后备 = 可逆注册自动撤销（broker 停用、service 解除）"、"更新后备不扰动消费者（无 reload）"），与代码**一致**——审查要点 5（docs↔代码一致性）**通过**。但"broker 停用、service 解除"及"不扰动"两项表述与 §6.2 相反（同 major1），docs 只是把这一语义错误一并记录了下来。

**影响（nit，随 major1 收口）**：docs 本身无独立错误，但 major1 修复后 THEORY-MAP PR #24 行 / 处置⑦ 行 / PLAN M3 行的结论句必须连同更正（"卸载后备 → broker 与消费者保持 Active、仅路由集移除；重注册恢复"），否则将形成"代码修复、文档仍记录相反语义"的漂移。

---

## 非发现项（已核对无误）

- **§5.3 三层拓扑正确性**：`main.rs` 的 adapter（provide `PlatformKey`）/ database（provide `DbKey`）/ bot（inject `[PlatformKey, DbKey]`, provide `ReplyKey`）逐层对应 §5.3 "IM adapters provide access to each messaging platform, database drivers provide persistent storage, and functional plugins declare these as coeffects and access them"。切换后端（revision 0→1 重建 database、bot 级联重激活且 fiber id 不变、adapter 不受影响）、重连 adapter（退役→移除→重装→bot 级联停用再自动重连、database 不受影响）、依赖不可用（bot 保持 `Inactive` 不报错、重现后自动激活）三个场景的断言全部正确且已实跑通过。
- **供给纪律**：各组件 `ctx.set` 的键与其 `provide` 声明一致（`Adapter`/`Database`/`Bot`、`BackingA`/`BackingB`/`Broker`），不触发越界写 panic；`#[component]` 宏生成的 `inject`/`provide` 与手写声明语义一致。
- **config→revision 纪律**：三处 `revision` 递增（`main.rs` 数据库 0→1、`broker.rs` b1 0→1）均正确触发重建；未变条目 revision 保持不变，幂等。
- **CI 门禁**：`ci.yml` 新增 step（`cargo run --quiet -p im-bot` + `--bin broker`）合理——`cargo test --workspace` 只编译不运行 bin，双 `run` 使端到端断言纳入门禁；`default-run = "im-bot"` 使 `-p im-bot` 精准指向 `src/main.rs`，broker 经 `--bin broker` 显式运行。已实跑两个 bin 均 exit 0。
- **workspace / lints / 零依赖**：`examples/im-bot` 正确加入 `members`；依赖**仅** `cordis`/`cordis-core`/`cordis-loader`（零第三方依赖）；`[lints] workspace = true` 继承 `unsafe_code = "deny"` + `clippy all = "warn"`，`-D warnings` 由 CI 的 `RUSTFLAGS="-D warnings"` 与 `clippy -- -D warnings` 施加。`cargo clippy -p im-bot --all-targets -- -D warnings`（exit 0）、`cargo fmt --check`（exit 0）均干净。
- **`Symbol` 一致性**：`service_of` 用 `Symbol::intern("service")` 与 `ServiceKey::SYMBOL = "service"`、`main.rs` 用 `Symbol::intern("reply")` 与 `ReplyKey::SYMBOL = "reply"` 均经全局 intern 恒等，读取正确。
- **docs 处置⑦ 状态流转**：处置⑦ 由"本次新增 / M3 案例素材"→"已落地（M3-PR1，PR #24）"，PR #24 行、PLAN M3 行加列，状态流转与提交事实一致（未提交 uncommitted 状态）。

---

## 修复提示（仓库惯例）

- 按惯例 split 为 **code + docs 两个 commit**：code（`examples/im-bot/` + 若需 `Cargo.toml`/`ci.yml`）先行，docs（`docs/THEORY-MAP.md`、`docs/PLAN.md` 及本报告的入库）随后。
- major1/major2 修复后建议**补一个"消费者全程不离开 Active"的强断言**（而非仅终态 + fiber id），使 §6.2 "no reload / no change to dependency" 真正被直证，避免再次落入"只证无重建"的浅断言。
