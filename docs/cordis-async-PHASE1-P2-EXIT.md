# cordis-async Phase 1 P1.2 出口判定

**依据**：计划 `docs/cordis-async-PHASE1-P2-PLAN.md`（H1–H3 + 出口）；草案 v1.4（冻结）§5/§10。
**判定日期**：2026-08-19（H1–H3 全部审查闭环后，出口走查）。
**出口标准**（计划 §2 Step 3）：Handle 迁移完整性（无残留 `Rc<Fiber>` 门面签名）+ O-2/O-3/O-4 决策记录 + C-4 文档 + 门禁全绿 + 出口走查。

---

## 1. H1 Handle 迁移对照（草案 §5 门面正式化）

| 项 | 迁移前 | 迁移后（H1） | 依据 |
|---|---|---|---|
| `use_component` 返回 | `Rc<Fiber>`（M0.5 临时） | `AsyncFiberHandle { Weak<Fiber>, generation }` | 草案 §5 门面正式形态；防环（评审点 B） |
| `retire` | `&Rc<Fiber>` | `&AsyncFiberHandle`（upgrade → core） | C-4 门面纪律 |
| `update` | `&Rc<Fiber>` | `&AsyncFiberHandle`（upgrade → core） | C-4 / §3.1 update 闭环 |
| Handle `generation` | — | 审计元数据（use_component 同步激活后捕获 = 1；换代不失效——防串代由条目内部代次承担，REVIEW-fa44fd6 Minor-1 回写） | P1.2 决策 D-1（定稿） |
| Handle `fiber()`/`id()` | — | 临时强引读状态 / FiberId 查询（弱引封装保留，强引警示 doc） | H1 + REVIEW-fa44fd6 |

**语义保证**：仅签名形态迁移、无语义变化——settle/shutdown/is_quiet/entry/条目自登记均未触碰；既有测试适配（`.retire()`→`rt.retire(&h)`、`.id()`→`.id().expect()`、`.state()`→`.fiber().expect().state()`）断言语义不变；新增 `m05::async_fiber_handle_generation_and_id` 直证 Handle 语义。

## 2. H2 门面纪律与开放项决策

- **C-4 门面纪律**（crate doc）：生命周期变更（retire/update/use_component，即草案 C-4 的 `apply` 域对应物）必须走门面；直接 core sync API 允许但 async 尾巴不 settle 记账。
- **O-2**：显式 `settle()`（app 层封装不提供，草案 O-2 决议采纳）。
- **O-3**：不启用 core hook（按需启用逃生口，措辞 REVIEW-23b75fa 对齐）。
- **O-4**：`AsyncFiberError` 保持 `String`（结构化错误等真实场景）。

## 3. H3 Remote API 冻结（P1.3 前置）

- `Remote`/`RemoteJoin`/`RemoteValue`/`RemoteRequest`/`TokioRemote` 标注「API 冻结（P1.2 H3）」+ P1.3 扩展点（Send-future 分池形态 / WasmRemote 接入；扩展以新增表述变体、不破坏既有签名）。
- TokioRemote 生命周期 / O-6 doc 复核通过（M0.6 已完整，REVIEW-aa346d2）。

## 4. 门禁与回归记录

- `cargo +1.97.0 fmt --check` ✅（workspace）；`clippy --workspace --all-targets -- -D warnings` ✅ 0 告警
- `cargo +1.97.0 doc -p cordis-async --no-deps` ✅ 0 告警
- `cargo +1.97.0 test --workspace` ✅ 无回归（cordis-async 22 条 = protocol 19 + spikes 3；既有各 crate 全绿）
- 里程碑审查闭环：H1（REVIEW-fa44fd6）/ H2（REVIEW-23b75fa）/ H3（REVIEW-aa346d2）全部 PASS，0 Major / 0 Minor 未决

## 5. 出口判定

**P1.2 全部完成**：Handle 门面收口（语义等价迁移）+ C-4/O-2/O-3/O-4 决策落地 + Remote API 冻结标注 + 门禁全绿 + 审查闭环。

→ **进入 P1.3 决策**（Remote 扩展 + 双运行时收口）：待决策项——Send-future 分池形态、WasmRemote 接入 M1 协议范围、双运行时共存收口文档化；P1.3 详细计划在开工前起草（按纪律由用户下达）。P1.4（DX 文档）随 P1.3 API 定稿后可并行。
