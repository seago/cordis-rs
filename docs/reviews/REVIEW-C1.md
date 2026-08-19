# 代码审查报告：C 探针 C1（回注机制打通）

- **审查对象**：`d3e07e2` + `9825865` + `2b1d08a`（crates/cordis-wasm/tests/c_probe.rs，C 探针 C1）
- **审查日期**：2026-08-20
- **审查范围**：对照 `docs/cordis-wasm-C-PROBE-PLAN.md` Step C1（回注机制打通；零 core 改动）
- **验证手段**：读 3 commit + 静态阅读 + `cargo +1.97.0 test -p cordis-wasm --test c_probe`（1/1 过）+ `git diff d3e07e2^ HEAD -- crates/cordis-core`（空，core 零改动）

---

## 总体结论

**✅ PASS（探针有效），1 项 C2 衔接前置条件（Minor-1），放行 C2 但需先解读取通道。**

- **Major**：0
- **Minor**：1（C2 前置：阶段 2 guest「经注入读回注键」依赖组件 `inject` 声明——当前 guest `inject=[]` 读不到，需在 C2 先解读取通道并注意不破坏既有注入/依赖测试）
- **Nit**：3

C1 本身范围（宿主侧「结果→回注绑定」链路）正确实现且直证：`echo(Count 7)→worker→Count(14)→set_dyn("probe_in")→核心 store`，**不依赖 guest take**（如 C 方案设计）、**core 零改动**（仅 tests 新文件）。探针机制基础成立。

---

## 发现

### Minor

### M-1（C2 前置，探针方案层面的接缝）：当前 guest 无法经 `get` 读回注键

- **位置**：`examples/wasm-plugin-rust/src/lib.rs:39-41`（`DbProvider::inject` 返回空）+ `crates/cordis-wasm/src/lib.rs::sync_injected`（只同步 `self.inject` 内键进镜像）+ `c_probe.rs:71-74`（回注 `set_dyn("probe_in")`）。
- **问题**：C1 将结果回注到**核心 store** 的 `probe_in`（已直证 store 含该键）✓；但阶段 2 的 guest 经 `get("probe_in")` 只读**镜像**，而镜像仅由「本组件 `inject` 键的注入同步」或「本组件自身 set」填充。当前 `DbProvider::inject` 为 `[]` → **阶段 2 guest 读取不到回注值**。
- **影响**：C 探针的「阶段 2 以远端结果为输入继续」核心环节依赖一条未被 C1 覆盖、且存在约束的读取路径。
- **实现约束（C2 需先解）**：让 guest 声明 `inject: ["probe_in"]` 会改变**依赖解析**（inject 键需有 provider——核心激活规则；当前 bridge_core 等既有测试环境无 `probe_in` 提供者 → 改 DbProvider.inject 会破坏既有回归）；C2 安全路径是**新增独立 guest 组件**（另一 world/组件形态或直接经本组件新的 `get` 通道），但之前曾遇新 wasm crate 依赖下载（`~/.cargo` 写沙箱）问题——需在 C2 计划中明确「读取通道方案 + guest 重建路径」。
- **建议**：C2 计划在开工前明确「阶段 2 读取通道」二选一——(i) 新增独立 guest 组件（inject 含 probe_in + 宿主提供 provider），或 (ii) 把回注值经既有关节（如本组件已 provide 键 / config 通道）送达；并核实 guest 重建的环境条件。C1 无需回改（其回注链表成立）；此点为 C1→C2 接缝。

### Nit

- **n-1**：`await_remote_value` 的 `rep` 硬编码 `0`（依赖 guest 单步 submit 顺序）——c 探针专测可接受，注记即可（远程 e2e 已有 rep 明确性）。
- **n-2**：C1 无 O-6（worker tid ≠ 组合线程）断言——注释已言「聚焦回注，O-6 已由 remote_e2e 直证」，合理。
- **n-3**：`_keep`（回注 Disposer 保留至测试作用域末）语义清晰、clippy must-use 修正到位 ✓。

---

## 通过项（逐条确认）

- **回注辅助 `await_remote_value`**（c_probe.rs:24-36）：轮询 `poll_remotes` + `remote_result(rep)`，Ok 返回 / Err panic / 4000 超时 panic——与 remote_e2e 同构的**宿主侧驱动**语义，不依赖 guest take ✓。
- **`register_remote("echo", params[0]=Count(n)→Count(n*2))`**：7→14 直证 ✓；参数容错（非 Count → 0）→ 返回 0（op 侧防御合理）。
- **激活 guest（单步：db 绑定 + submit echo(Count 7)）→ 回填返回 Count(14)**：真实 worker 链路 ✓。
- **回注**：`root.set_dyn(Symbol::intern("probe_in"), Box::new(result))` → `store().contains(probe_in)` 断言 ✓；Disposer 保持到作用域末（C2 阶段 2 读取期间不撤销）✓。
- **core 零改动**：`d3e07e2^..HEAD` 对 `crates/cordis-core` 零 diff ✓（护栏成立）。
- **范围诚实性**：注释/测试名明确 C1 只验「回注链路」，不预设 C2 结果 ✓；与 C 探针计划的「C1 回注机制打通」一致 ✓。
- **门禁实测**：`c_probe` 1/1 过（0.47s）；3 commit 依次为 feat/style(clippy 修 must-use)/feat——干净。

---

## 结论

C1（宿主侧回注机制）**有效达成**：远端结果已能进入核心绑定体系、链路直证、core 零改动。**放行 C2**，但 M-1 为 C2 前置——「阶段 2 经 `get` 读回注」需先在 C2 计划明确**读取通道**（新 guest 组件 / 既有关节送达）与 **guest 重建路径**（沙箱依赖约束），避免 C2 落空。M-1 不否定 C1；属探针方案层面的接缝注记，按 C 探针「评估诚实性」要求记录。
