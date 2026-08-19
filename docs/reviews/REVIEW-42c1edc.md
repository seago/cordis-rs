# 代码审查报告：commit `42c1edc`（P1.3 R2 WasmRemote 接入点，P1.2 后续线）

- **审查对象**：`42c1edcedc78b4a5e28120012ffe6ef2fe044e7f` — `docs(async): P1.3 R2 WasmRemote 接入点 + M1 协议接线 doc（按 D-2 默认：实际宿主桥留 M1 wasm 专项）`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`git show 42c1edc`（`crates/cordis-async/src/lib.rs` 追加 `WasmRemote` 占位类型 +17 行），对照 P1.3 计划 `docs/cordis-async-PHASE1-P3-PLAN.md` §2 Step R2 与草案 v1.4（冻结）§2/§4。
- **验证手段**：静态阅读 + `cargo +1.97.0 doc -p cordis-async --no-deps`（0 告警）+ `cargo +1.97.0 clippy -p cordis-async --all-targets -- -D warnings`（0 告警）。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：0
- **nit**：1（占位类型无 pub 构造入口，可作未来 M1 扩展点记录）

R2 接入点 + 协议 doc 里程碑达成：范围严格符合 P1.3 决策 D-2 默认（接入点 + 接线说明，实际宿主桥留 M1 wasm 专项），与草案 v1.4 §2/§4 的 WasmRemote 语义逐点一致，无行为改动、无死代码、doc 干净。

---

## 证据核验

| 检查点 | 核验 |
|---|---|
| WasmRemote 占位类型（§2/§4 接入点） | ✅ 空结构（`_private: ()` 防外部构造、`_` 前缀不触发 dead_code ymw），pub 类型无未用警告 |
| submit 语义 doc（guest 无自发线程；入队 + 宿主 step 边界驱动 + 回填） | ✅ 与草案 §4 逐字一致（join 语义同 [`TokioRemote`]，执行方为宿主驱动协议 PR #11–13 而非本地 worker 池） |
| 范围符合 D-2（不提供 Remote 实现 / 实际桥留 M1 专项） | ✅ doc 明示「接入 host 协议前实现无意义——M1 专项在接入后 `impl Remote for WasmRemote`」——取舍合理，无范围漂移（D-2 默认即接入点 + doc） |
| deny(missing_docs) + intra-doc 链接（[`Remote`]/[`TokioRemote`]） | ✅ `cargo doc` 0 告警，链接全部解析 |
| 无行为改动 / 不破坏既有 API | ✅ diff 仅 +17 行（类型 + doc）；既有 `Remote`/`RemoteRequest`/`TokioRemote` 未触碰 |
| 死代码 | ✅ pub 结构 + `_private: ()`，clippy `-D warnings` 0 告警 |

## 发现

### Major：无

### Minor：无

### Nit

### Nit-1（低，可选）：`WasmRemote` 无 pub 构造入口，也未写「何时提供构造」

- **位置**：`lib.rs` 尾部 `WasmRemote` 占位（`_private: ()` 私有字段，无 `new()`）。
- **问题**：当前占位类型**无法被外部实例化**（无 pub 构造）——M1 wasm 专项接入前本无实例化需求，故属刻意的占位语义，非缺陷。但 doc 未言明「构造入口随 M1 接入一并提供」这一扩展点。
- **建议**：可选——在 doc 追加一句「构造入口（`WasmRemote::new(host_handle)`）随 M1 专项接入时一并落地」；不阻塞。

## 结论

R2（Step R2：WasmRemote 接入点 + 协议接线 doc）与计划 §2 Step R2 对齐、符合 D-2 决策范围，无逻辑缺陷、无行为改动、无死代码、doc 干净（0 告警）。**建议放行进入 R3**（Step R3：双运行时共存收口——拓扑文档 + 组合示例，P1.3 计划 §2）。Nit 记录在案，不阻塞。
