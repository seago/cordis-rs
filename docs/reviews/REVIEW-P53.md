# 代码审查报告：commit `eb11c7e` + `e812009`（P5-3 全栈串联，产品验证线 P-5）

- **审查对象**：`eb11c7e`（full_stack.rs 四层协作回路 + lib.rs sync_injected 保留 preseed + guest 非注入依赖多轮 + cordis-events dev-dep）+ `e812009`（clippy 修：去 unused import / guard 收窄 / 引用直传）
- **审查日期**：2026-08-22
- **审查人**：independent-review-agent
- **验证手段**：静态阅读 + `cargo +1.97.0 test -p cordis-wasm`（全套绿：lib 8 + 集成 13，含 full_stack 1/1、a2_e2e 3/3、wasm_agent 2/2、go 2/2）+ 关键单测实跑

---

## 总体结论

✅ **PASS WITH NITS**（Major 0 / Minor 1 / Nit 2）——P5-3 达成，放行 P5-4。

**四层协作回路直证**（`full_stack_events_async_wasm_remote_await`）：
`user/msg` 事件 →（Arc 队列，listener Send+Sync 合规）→ async LLM 组件（`LlmLoop` 长驻，`spawn_remote` LLM worker 执行）→ `bot/reply` 事件 →（订阅记录）→（main 桥接）`preseed_mirror("wasm/in")` → 延迟挂载 wasm agent（协作序：输入就绪后激活，guest 首轮即用输入参数）→ `poll_and_advance`（P-3 底座）多轮 Await → `probe` 落盘。**协作序断言**（`llm:reply:你好` / `bot:reply:你好` / `wasm-probe:` + `injected`）全部通过——四层（events + async + wasm + Remote + Await）单一场景收口。

## 核查通过（核心）

- **通道设计**：guest 协作输入经 **preseed 镜像**（`wasm/in` 非注入键）——`inject` 回空（避免无提供者时 Inactive，`wasm_plugin_rust` 改动注记）→ `sync_injected` 循环只遍历注入键（空 list，B 循环不执行）→ preseed 非注入键不被 sync 清理；guest 经 `context::get("wasm/in")` 读协作输入（多轮 submit 参数）。
- **多轮 take-await**：guest `submit("llm", [param])` → Await → take 回复 → 再 submit（round 递增）→ 落盘 `probe`；`poll_and_advance` 驱动（Await 回路）——P-3 底座在真实多轮形态下使用。
- **不改 core/wit**；`sync_injected` 分支为桥接语义（P-2 统一类型下镜像同步）；cordis-events dev-dep 仅为测试；回归全套绿。

## 发现

### Minor

### M-1（语义观察）：`sync_injected`「核心无值 → 保留 preseed」分支对注入依赖键的残留偏差
- **位置**：lib.rs `sync_injected`——原 `None => mirror.remove(key)` 改为「核心无值 → 保留（host preseed）」。该分支只遍历**注入键**。
- **关联**：当前 guest `inject` 回空（非注入依赖）→ 循环不执行 → 分支不触发，P5-3 无影响（preseed 的 wasm/in 非注入键）。
- **潜在偏差**：若未来组件用**真注入依赖**（inject 非空）——提供者消失（核心无值）时，该注入键镜像**未被清除**（此前 `mirror.remove` 会清）→ guest `get` 读到残留旧值（依赖已消失却读到旧绑定）。与「依赖不满足 → get none」的既有语义有偏差。
- **处理建议**：M1 已知边界——记录（当前形态无影响；真注入组件场景需复核）；或此分支明确只服务"host 预注协作输入"（非注入键），对注入键保持原 remove——两用途分离。

### Nit

- n-1：多轮 take 轮询预算 1024×1ms（main 循环）与既有时序模式一致（实测 0.65s 无 flaky）；
- n-2：`full_stack.rs` 用 `cordis_wasm::Value as WValue` 与 `cordis_wasm::Value`（pub use = cordis_value::Value）——统一类型别名，可读性可精简（无碍）。

## 结论

P5-3（全栈串联四层协作回路）**达成**：events + async + wasm + Remote + Await 单场景收口、协作序断言直证、全套回归绿、不改 core/wit。M-1 为**注入依赖键的语义观察**（当前非注入形态无影响，记录为已知边界）；放行 **P5-4**（评估报告 + `docs/cordis-PRODUCTVAL-P5-EXIT.md`），M-1 建议并入 EXIT 记录。
