# 代码审查报告：commit `aa346d2`（P1.2 H3 Remote API 冻结标注）

- **审查对象**：`aa346d29849e9d393bf3c66f75b43d46dc98690c` — `docs(async): P1.2 H3 Remote API 冻结标注`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`git show aa346d2`（仅 `crates/cordis-async/src/lib.rs` M0.6 Remote 区域 +11 行 doc），对照 `docs/cordis-async-PHASE1-P2-PLAN.md` §2 Step 2（H3）。
- **验证手段**：静态阅读 diff + `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0 doc -p cordis-async --no-deps`（0 告警）。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：1（计划 H3 任务列表含 `RemoteJoin`，本次未加标注——join 语义不受 P1.3 扩展影响，标注非强必需，属计划举列举过严）
- **nit**：1（`TokioRemote` 本次未显式标“复核通过/冻结”——M0.6 doc 已含生命周期/O-6，且为 v1 唯一实现，不阻塞）

H3（Remote API 冻结标注，纯文档里程碑）达成：标注准确、无行为改动、doc 0 告警。放行 **P1.2 出口走查（Step 3）**。

---

## 核查记录

| 检查点 | 核验 |
|---|---|
| **标注准确性** | ✅ v1 = `spawn_blocking` 闭包形态明确（`RemoteRequest` doc）；P1.3 扩展点 = 「Send-future 分池形态（草案 §4）+ WasmRemote（M1 宿主驱动协议）」标注于 `RemoteRequest`/`Remote`/`RemoteValue`；「扩展以**新增表述变体**进行、不破坏既有签名」与草案 §2/§4 pending-set 泛化语义一致 |
| **API 冻结声明** | ✅ `Remote` trait：`submit(RemoteRequest) -> RemoteJoin<RemoteValue>` 冻结为 P1.3 稳定接入面；`RemoteValue` 冻结注 |
| **无行为改动** | ✅ `git show` 仅 +11 行 doc（`RemoteValue` +3 / `RemoteRequest` +4 / `Remote` +4），零行为变化 |
| **deny(missing_docs) / intra-doc** | ✅ `cargo doc -p cordis-async --no-deps` 0 告警；`[`TokioRemote`]`/`[`Remote`]`（RemoteRequest doc）与 `[`tokio::runtime::Handle`]`（TokioRemote doc）均解析无悬空 |
| **TokioRemote 生命周期 / O-6 复核（H3 任务 2）** | ✅ 既有 doc（M0.6）已含「`worker`（Handle）须比桥存活更久…panic=配置错误=bug」+「O-6 纪律：worker 侧不得触碰组合线程资源」——复核通过、无需新增（本次未改，合理） |
| **P1.3 前置 API 面稳定** | ✅ 冻结标注给出 P1.3 扩展的明确接入面（新增表述变体而非破坏签名），满足计划「P1.3 前置 API 稳定」 |

---

## 发现

### Major：无

### Minor

### Minor-1（建议）：计划 H3 任务列 `RemoteJoin` doc 补全，本次未加标注（覆盖偏差）

- **位置**：计划 `docs/cordis-async-PHASE1-P2-PLAN.md` §2 Step 2 任务 1「`Remote`/`RemoteJoin`/`RemoteValue`/`RemoteRequest` 的 doc 补全」；本次提交仅标注 `RemoteValue`/`RemoteRequest`/`Remote` 三处，`RemoteJoin<T>`（`LocalBoxFuture<T>` 别名）未加 P1.3/冻结注。
- **问题**：`RemoteJoin` 是**纯 join 别名词**——P1.3 的 Send-future 分池扩展以新增请求表述变体进行，`submit` 返回类型不变（仍是 `RemoteJoin<RemoteValue>`），故 join 语义不受扩展影响，标注**非强必需**。属计划举列举过严导致覆盖偏差，非功能缺口。
- **修法（可选）**：在 `RemoteJoin` doc 补一行「join 语义不随 P1.3 扩展变化（`submit` 返回形态不变）」消除计划-实现覆盖差；或同步计划措辞（列举只指三个需补 doc 的类型）。不阻塞。

### Nit

### Nit-1：`TokioRemote` 未显式标注「复核通过 / 冻结」

- `TokioRemote` 本次未新增标注，也未在提交说明显式声明「H3 任务 2 复核通过、既有 doc 已覆盖」。M0.6 的 `TokioRemote` doc 已含生命周期与 O-6（见核查表），复核结论可从 diff 推断，但提交层面未显式留痕。
- **修法（可选）**：下个 commits/milestone 文档可加一行「v1 实现随 P1.3 扩展可能增设（不破坏既有 `submit`）」；或审查报告留痕即可。不阻塞。

---

## 结论

H3（Remote API 冻结标注）**达成**：三处标注准确覆盖 v1 形态与 P1.3 扩展点（Send-future 分池 + WasmRemote）、`submit` 签名冻结为稳定接入面、无行为改动、doc 0 告警；`TokioRemote` 生命周期/O-6 复核通过（M0.6 doc 已足，无需新增）。1 Minor / 1 Nit 均为文档措辞级，不阻塞。

**放行 P1.2 出口走查（Step 3）**。
