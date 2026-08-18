# 代码审查报告：commit `fa44fd6`（P1.2 H1：AsyncFiberHandle 门面收口）

- **审查对象**：`fa44fd6314448ab64d37bd6df82fcc088268ff08` — `feat(async): P1.2 H1 AsyncFiberHandle 门面收口——use_component/retire/update 迁至弱引句柄（Weak<Fiber>+generation 审计）+ 回归适配 + Handle 语义测试`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`git show fa44fd6`（`src/lib.rs` +70：`AsyncFiberHandle` 类型 + `use_component`/`retire`/`update` 签名收口；`tests/protocol.rs` +86：m03/m05/m07 回归适配 + 新测试 `m05::async_fiber_handle_generation_and_id`），对照 P1.2 计划 `docs/cordis-async-PHASE1-P2-PLAN.md` §2 Step 0（H1）与草案 v1.4 §5。
- **验证手段**：静态阅读 + 实际运行 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0 test -p cordis-async`（协议 19 + spikes 3 = **22/22 全绿**）。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：1（计划 D-1「代次校验防串代」措辞 vs 实现「审计元数据、换代不失效」的 traceability 缺口——需回写计划）
- **nit**：2（`fiber()` 返回强引可被调用方长期持有，doc 可加警示；`generation` 暂无功能消费点）

核心目标「**仅签名形态迁移、无语义变化**」核实成立：`settle`/`shutdown`/`is_quiet`/`entry`/条目自登记均未触碰；`retire`/`update` 经 Handle upgrade 透传 core（等价于原 `Rc<Fiber>` 直接调用）；测试适配全部为形态迁移、断言语义不变、无残留直接 fiber 调用。Handle 弱引防环（评审点 B 延续）正确：句柄本身无任何强引用路径。可放行进入 H2。

---

## 发现

### Major：无

### Minor-1：计划 D-1 措辞与实现不一致（generation 的「代次校验」vs「审计元数据」）——需回写计划

- **位置**：`src/lib.rs` `AsyncFiberHandle.generation` doc（「审计元数据；换代不失效——防串代由条目内部代次承担」）；对照计划 `docs/cordis-async-PHASE1-P2-PLAN.md` §1 D-1（「`{ Weak<Fiber>, generation }`（弱化防环 + **代次校验，防串代**）」）。
- **问题**：计划 D-1 以「generation 参与防串代校验」措辞；实现将其定为**审计元数据**且**换代不失效**（`retire`/`update` 操作 fiber 本体，防串代由条目 `AsyncFiberEntry` 内部代次机制——`mark_running`/`on_failed` 代次核对——承担）。实现设计**更合理**：core `update_fiber` 修为 fiber 身份保留的换代，编排方换代后继续用同一 handle 操作同一 fiber 是正当的——若按计划字面「generation 校验防串代」会误拒 update 后续操作。故这是实现对计划措辞的**合理细化**，非语义缺陷；但**计划未回写对齐**，后续按计划字面核验会误判（类似 async 草案「旧尾巴先 settle」措辞 vs 实现的先例）。
- **草案/计划依据**：P1.2 计划 §1 D-1；草案 v1.4 §5 门面句柄语义（句柄 = 组件身份，非代次绑定）。
- **建议**：回写计划 D-1 行为「`generation` 为**审计元数据**（use_component 时捕获；换代不使句柄失效——防串代由条目内部代次机制承担）」，与实现注记对齐；若仍要代次参与防串代（如 Handle 复活场景），记入 P1.2 后续观察。

### Nit-1：`fiber()` 返回强引可被调用方长期持有（弱引封装自控边界）

- **位置**：`AsyncFiberHandle::fiber()`（pub，返回 `Option<Rc<Fiber>>`）。
- **问题**：方法体临时强引结束即释放 ✓（doc 已明示），但**返回的 `Rc` 由调用方决定持有时长**——若编排方 `let f = h.fiber().unwrap();` 长期持有，会实际延长 fiber 生命周期（调用方选择，非句柄主动）。弱引封装的承诺（句柄不强持）保持，此暴露是编排手段，可接受；但 `fiber()` 作为 pub 入口，建议 doc 补一句「返回的强引仅限临时读取，避免长期持有（会延长 fiber 生命周期）」。
- **建议**：doc 补警示；不改变签名。

### Nit-2：`generation` 目前无功能消费点（纯审计字段）

- **位置**：`AsyncFiberHandle.generation` + 新测试断言 `handle.generation() == 1`。
- **问题**：`generation()` 仅被测试消费（断言首激活代次 = 1），无运行时功能用途。符合计划 D-1「审计元数据」定位（调试/可观测价值），接受；观察后续（如复活场景是否需要代次参与判断）再定是否富化。

### 未发现问题的核查点（逐条确认）

- **Handle 弱引防环（评审点 B）⑪**：`AsyncFiberHandle { fiber: Weak<Fiber>, generation: u64 }`——无强引用路径；`fiber()` upgrade 返回临时强引（方法结束即释放）；`new(&fiber, gen)` 由 `ctx.use_component` 返回的 `Rc<Fiber>` downgrade，之后该 Rc 随调用方 drop（fiber 仍被 core registry + ctx 回调强持，存活正确）——Handle 不引入回边、无泄漏。
- **use_component 时序**：`ctx.use_component(registrar, config)?`（内部同步激活 → apply `begin_activation` 换代）→ `entry.generation.get()` 捕获审计代次（首激活 = 1）——捕获时机正确（同步激活已完成）；`let _ = entry;` 删除后 entry 仍在 `generation.get()` 使用，无 dead_code。
- **retire/update 语义等价**：`handle.fiber().expect("…已释放…").retire()` 与原 `fiber.retire()` 等价（同一 fiber 本体）；`core.update_fiber(&fiber, config)` 等价；失效肽柄 panic=bug 诊断（草案纪律）✓。
- **仅形态迁移无语义变化**：`settle`/`shutdown`/`is_quiet`/`entry`/条目自登记/Remote 注入——均未触碰；回归测试（m03/m05/m07）断言语义不变。
- **测试适配端到端**：`provider.retire()`→`rt.retire(&provider)`（走门面，符合 C-4）、`b.retire()`/`a.retire()`/`next.retire()`/`first.retire()`（m03，Reentry/SelfRegen 闭包内 `rt.retire`，rt 已被 capture）→ 全适配；`fiber.id()`→`fiber.id().expect()`（Handle.id() 返回 Option<FiberId>）、`*b.state()`→`*b.fiber().expect().state()`——断言语义不变；`cargo grep` 无残留直接 `.retire()`（全走门面或 loader）。
- **新 Handle 语义测试**：`m05::async_fiber_handle_generation_and_id` 直证创建代次审计（=1）、`id()` 存活期查询（`rt.entry(fid)` 命中）、`fiber()` 临时强引读 Active 状态、`rt.retire(&handle)` + settle 完整收账（is_quiet）——覆盖 Handle 主要公开语义。
- **工程门禁**：`test -p cordis-async` = protocol 19 + spikes 3 = 22/22 全绿；clippy/fmt/doc 由委派方本地已验证（缺省信任）；无 `unsafe`；`deny(missing_docs)` 下新增 pub 项（Handle/fiber/generation/id）均有 doc。

---

## 结论

P1.2 H1 与计划 Step 0 目标一致，Handle 弱引收口 + retire/update 门面迁移**仅签名形态迁移、无语义变化**成立；回归适配完整、无残留；22/22 测试绿。唯一 Minor 为实现对计划 D-1 措辞的合理细化未回写计划（traceability），建议回写计划 D-1 行实现注记措辞对齐。

**建议放行进入 H2**（门面纪律 C-4 文档 + O-2/O-3/O-4 决策落地）。
