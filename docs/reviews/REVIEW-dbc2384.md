# 代码审查报告：A2a（B 计划 wasm 桥 Await 接线）

- **审查对象**：`dbc2384e4fb075a8f56b6796f709ebfef6f0c545` — `feat(core,wasm): B A2a——wit effect-step inverse→option + WasmTaskIter 在途 join 判 Await + Fiber::is_suspended + rust 4 guest ABI 同步重编 + a2_e2e（guest 完整 take-await）+ 断言宽松化 + go_guest 标 ignore`
- **审查日期**：2026-08-20
- **审查人**：independent-review-agent（被委派审查子代理）
- **范围**：`crates/cordis-wasm/{wit/cordis.wit, src/lib.rs, tests/{a2_e2e,bridge_core,c_probe,go_guest,load_guest}.rs}` + `crates/cordis-core/src/fiber.rs`（`Fiber::is_suspended`）+ `examples/wasm-plugin-{rust,rust-consumer}`
- **验证**：静态阅读 + 实测（见 §4）

---

## 总体结论

✅ **PASS WITH NITS**（Major 0 / Minor 2 / Nit 2）→ **放行**，A2a 达成核心目标：guest 完整 take-await（submit→Await 挂起→回填→advance→自取→O-6 隔离）真实解锁，pre-existing 的 debug/release 差异与 A2b（go ABI）如实划界。

- **major**：0
- **minor**：2（① 根 `Cargo.lock` 未提交（A2a 后遗留）；② release 下 `spike_s2` 失败已溯源为 pre-existing——建议父会话登记为既有 release 已知边界）
- **nit**：2（go_guest ignore 理由诚实；`is_suspended` 访问器语义清晰但为测试便利——合理）

---

## 深度核查（全部通过）

### 主链路真实闭环（a2_e2e::guest_awaits_remote_join_and_continues）
- **wit**：`effect-step.inverse → option<inverse>`（A2a 前提，等待步空逆）；bindgen 重生成、宿主兼容（本步逆来自 pending 转发的 reps，非 effect-step.inverse 字段）✓
- **宿主 Await 判定**：`!done && !remote_joins.is_empty() → Step::Await`——core `try_execute_with` 挂起（resumable + Active + 逆保留）→ 测试断言 `fiber.is_suspended()` + 挂起时 db 已绑定（step0 副作用已发生）✓
- **恢复**：`poll_remotes` 轮询回填 → `runtime.advance(fid)` → guest take 读到 → `probe` 落盘（worker tid）→ **`assert_ne!(probe_tid, combo_tid)` O-6 隔离实测**（guest 自取结果，非宿主断言——相对 W2 的实质升级）✓；恢复后 `is_suspended() == false` ✓；退役 LIFO 静止 ✓
- **错误通道**（`guest_take_receives_remote_err`）：op panic → worker 内 catch → err 回填 → guest take `Err` → `probe_err` 落盘（含 "boom"）、`probe` 不落 ✓——real err 达 guest（C 探针覆盖不了的通道现在通了）

### ABI 波及（wit 全局变化）
- 主 guest 重写为 A2 多步 take；`rust-consumer/misbehave/panic` 三款同步适配 `Some(inverse)` → 全编译过；宿主 `bridge_core`/`load_guest` provide 断言**宽松化**（含核心键即可，探针键演进不再碎断言）✓
- `c_probe` 移除（两阶段 c2 被 A2a 取代——C 定位为轻量捷径，评估结论保留在 `docs/cordis-wasm-C-PROBE-EXIT.md`）✓
- `go_guest` 标 `#[ignore = "A2b：..."]`（rust 系已对齐；go ABI 待 A2b 收尾）——**ignore 理由诚实、范围划界透明** ✓

### core 改动额度
- A2a 对 core 仅新增 `Fiber::is_suspended` 访问器（读 `resumable`）——属 A1 授权范围（机制面），**无新语义改动** ✓

## 4. 实测记录

| 项 | 结果 |
|---|---|
| `a2_e2e` | **2/2 过**（0.64s：主链路 + 错误通道） |
| `cordis-core --lib` | **58/58**（A1 机制未回归） |
| `cordis-wasm`（release） | 全套绿：lib 7 + 集成 15（**go_guest 2 ignored**；remote_e2e/c_probe(c1)/load_guest/bridge 等全绿） |
| `cordis-async --test spikes --release spike_s2` | **FAILED（0.00s，复现 2 次）**——但 **A1 父提交（`52461ae`）同测同败** → **pre-existing release 时序问题，非 A2a 引入**（debug 通过） |

## 5. 发现

### Minor
- **m-1（应处理后提交）**：根 `Cargo.lock` 有未提交改动（`git status` 显示 `M Cargo.lock`；A2a 提交未含）——guest 重编相关 lock 变动未入库；建议父会话把 lock 变更入库（保持 lock 一致性）。
- **m-2（记录，非本 commit）**：`spike_s2` release 下稳定失败为 **pre-existing**（A1 态同败）——建议父会话登记为 cordis-async 既有 release 已知边界（debug 绿；与 B/A2 无关；后续可查 release 优化时序预算）。

### Nit
- **n-1**：`Fiber::is_suspended` 目前仅测试/探针消费（挂起态断言）；生产语义（宿主轮询挂起集）留 A3 评估——合理。
- **n-2**：go_guest ignore 依赖 go ABI 收尾（A2b）不落下——建议 A3 或 A2b 出口确认 go 重编回归，若 go 长期无法适配需显式定位（降级/归档记录）。

## 6. 结论

**A2a 达成**：guest 完整 take-await（真实 O-6 隔离 + 错误通道 err 达 guest + 挂起/恢复/退役断言直证）——B 计划核心收益落地；rust 系 ABI 全对齐、断言稳固、core 额度合规、实测全绿（go 2 ignored）。

**次序建议**：先 **A3**（C 归位 + 文档：WASMREMOTE-EXIT 时序边界解锁记录 + 占位 doc 引用 Await，轻量、低依赖），再 **A2b**（go ABI 收尾，独立、需 go 工具链投入）；m-1（Cargo.lock 入库）随 A3 提交一并处理。release spike_s2 记为既有边界单独立项（不影响本线与既定 debug 门禁）。
