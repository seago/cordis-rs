# 产品验证线 P-5 · P5-1+P5-2 审查报告（原生 + wasm agent 插件）

- **审查对象**：`36df78e`（examples/agent-plugin 原生 agent）+ `bbc3ad2`（wasm-plugin-rust 多轮 guest + wasm_agent 测试 + a2_e2e 多轮升级 + c_probe/remote_e2e 适配）
- **审查日期**：2026-08-22
- **验证**：`cargo run -p agent-plugin` 通过（完整回路 log）+ `test -p cordis-wasm --test wasm_agent`（2/2）+ `--test a2_e2e`（3/3）；全套 wasm 由父会话已验绿；clippy/fmt 本地已验。

## 总体结论

✅ **PASS（0 Major / 0 Minor / 3 Nit）** —— P5-1/P5-2 达成，放行 P5-3（全栈串联）。

## 核查

### P5-1 原生 agent 插件（examples/agent-plugin）
- **回路完整**：`user/msg` 事件（sync 订阅→Arc 队列，listener Send+Sync 合规通道）→ agent loop（cancel 检查点）→ `spawn_remote` LLM（worker 执行）→ join 回灌 → `bot/reply` 事件发布（async 段 emit）→ 出口订阅收到；卸载 retire→cancel→检查点退出→flush。
- **双重装箱修复正确**：`boxed(move || format!(...))`（T=String）——`RemoteRequest::boxed` 内部一次 `Box::new(f())`（cordis-async lib.rs 确认）；未误用 `T=RemoteValue`（那会双重装箱）。实测自检（直接 submit→await 回灌 "probe"）。
- **示例可运行**：`cargo run -p agent-plugin` 通过，log 全链（in→agent:start→llm:req/post/reply→bot:reply→exit@cancel→flush）。

### P5-2 wasm agent 多轮 take-await
- **guest 状态机连贯**：`ROUNDS=3`；每轮 submit("llm",[round])（首轮带 `Step(db)` 有逆，后续 `Wait`）→ Await（宿主暂停）→ take 累积（`r0|r1|r2`）→ 下一轮；收尾 `Done(probe)`；失败轮（op panic）→ take Err → `Done(probe_err)` 终止。
- **Wait 语义正确**：take 就绪后产 `Wait`（**无在途 join**）——`WasmTaskIter` 的 Await 判定为 `matches!(Wait) && !remote_joins.is_empty()` → 无 join 时 `Wait` 作 `Yielded` 空步 → 宿主继续 advance 进入下一轮提交 ✓；take 未就绪（有 join）→ Await ✓。两态区分正确。
- **poll_and_advance 驱动**：多轮（≥2 拍/轮）+ 4000×1ms 回路；3 轮累积 + 失败轮（第 2 轮 panic）终止 + probe/probe_err 断言 + O-6（a2_e2e 多轮每段 tid 隔离升级）。
- **适配诚实**：echo→llm 语义随多轮演进（a2_e2e/c_probe/remote_e2e 同步）；不改 core/wit。

## 发现（Nit，不阻塞）

- **n-1**：take 就绪后产 `Wait` 依赖宿主**多拍** advance（一拍进入提交）；单次 advance 场景可能停在空步 Wait（无 join → Yielded 一次，需再 advance）——`poll_and_advance` 循环已覆盖，语义可作 doc 注记。
- **n-2**：agent-plugin 的直接链路自检（submit→await）在正式回路前独立进行——无碍。
- **n-3**：guest 首轮 `Step(db)` 提供 db 键（realm 绑定），与 wasm_agent 的 loader 挂载兼容。

## 结论

P5-1（原生 agent 全栈回路 + boxed 单次装箱修复）与 P5-2（wasm 多轮 take-await + 失败轮 + O-6）达成，示例可运行、测试直证、全回归绿——**放行 P5-3（全栈串联）**。
