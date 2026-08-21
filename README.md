# cordis-rs

**《A Programming Paradigm for Spatiotemporal Composability》**（Shi / Zhang / Cui）的 **Rust + Wasm 参考实现**。

时间上可逆（效应带逆、撤销即恢复）、空间上可组合（realm/键控、依赖即注入）的组件范式：**配置驱动、保存即生效、出错可回滚**。本仓库把论文的核心定理（Thm 16/59/61/63 等）逐一落地为可运行的库与端到端案例。

- 实施规划：[docs/PLAN.md](docs/PLAN.md)
- 论文符号 ↔ 代码映射与偏差记录：[docs/THEORY-MAP.md](docs/THEORY-MAP.md)
- 论文原文：`paper/paper.pdf`

---

## 核心概念（一分钟）

| 概念 | 含义 | 落点 |
|---|---|---|
| **可逆效应** | 每个效应（绑定/订阅）带逆，卸载即按 LIFO 撤销（Thm 16） | `cordis-core` |
| **空间组合** | 键（`Key`）→ realm（隔离）→ 依赖解析（Def 44–46） | `cordis-core` |
| **级联** | 提供者退役 → 依赖者自动停用/恢复（Thm 63） | `cordis-core` |
| **单线程组合** | 进程内唯一组合线程 + `Rc/RefCell`（ADR-0002），跨线程经值/桥 | 全仓 |
| **异步补全** | `Step::Await` 挂起 / `Runtime::advance` 恢复——同步桥等外部异步 | `cordis-core`（B 计划） |
| **事件层** | 类型化事件 + 四种派发（emit/waterfall/serial/bail）+ 订阅即效应 | `cordis-events` |
| **async 层** | 一等 async 效应、两阶段卸载、失败通道、Remote 桥 | `cordis-async` |
| **Wasm 后端** | wit 世界 + wasmtime 组件模型 + 沙箱隔离 + Rust/Go guest | `cordis-wasm` |
| **HMR** | 三阶段事务性热重载（Alg 8/9/10），失败回滚 | `cordis-hmr` |

## Workspace 结构

```text
crates/
├── cordis/          门面 crate：统一 re-export（PLAN §4.1）
├── cordis-core/     理论核心：可逆效应引擎、fiber 生命周期、realm、L-Raise 失败模型
├── cordis-loader/   声明式加载器：配置树、group/include、隔离、错误策略（三级分类）
├── cordis-hmr/      热模块替换：事务性重载 + 双回滚
├── cordis-events/   事件层：类型化事件 + 四种派发 + 订阅即效应（§0 核心义务）
├── cordis-async/    async 层：AsyncFiberHandle、AsyncEffectIter、失败通道、Remote（双形态）
├── cordis-wasm/     Wasm 组件后端：wit 世界 + 宿主驱动 + 沙箱 + remote 桥
├── cordis-macro/    过程宏 DX 层（PLAN §4.3）
└── cordis-native/   进程内组件后端（PLAN §4.3）

examples/
├── hello-plugin/            最小示例：server + auth 级联（M0 验收）
├── im-bot/                  三层依赖拓扑案例 + bench（M3）
├── wasm-plugin-rust/        Rust guest（db 提供者 + 远端消费探针）
├── wasm-plugin-rust-consumer/  注入依赖者 guest
├── wasm-plugin-rust-misbehave/ 越界写 guest（沙箱用例）
├── wasm-plugin-rust-panic/      panic guest（沙箱用例）
└── wasm-plugin-go/          Go guest（wasip1 + 预览1 适配器组件化）
```

## 能力总览（按里程碑）

| 线 | 内容 | 状态 |
|---|---|---|
| **M0–M3**（主线） | 定理核心 → wasm 后端 → loader + HMR → 案例验证 | ✅ 完成（走查闭环，见 PLAN 路线图） |
| **Phase 0**（async 预备） | AsyncRuntime 协议单测 + 三 spike（事件订阅/tokio 服务壳/agent loop） | ✅ 完成（`cordis-async-PHASE0-EXIT.md`） |
| **Phase 1.1**（events） | 事件层 v0.3.1 全部验收（#1–#9） | ✅ 完成（`cordis-events-PHASE1-EXIT.md`） |
| **Phase 1.2**（async 门面） | `AsyncFiberHandle` 收口 + C-4 门面纪律 + O-2/3/4 决策 | ✅ 完成（`cordis-async-PHASE1-P2-EXIT.md`） |
| **Phase 1.3**（Remote） | Remote 双形态（闭包/future）+ WasmRemote 接入点 + 双运行时共存 | ✅ 完成（`cordis-async-PHASE1-P3-EXIT.md`） |
| **Phase 1.4**（DX） | 插件作者指南 + 错误/安静语义 + 示例模板 | ✅ 完成（`cordis-PHASE1-P4-EXIT.md`） |
| **错误策略线** | loader 三级错误分类（Bug/ComponentFailure/OrchestrationError）+ 逐条目报告 | ✅ 完成（`cordis-loader-error-strategy-EXIT.md`） |
| **M1 wasm 桥** | `WasmRemote` 宿主驱动：wit `remote` 接口 + 宿主注入 Remote + 回填 + 沙箱 | ✅ 完成（`cordis-wasm-WASMREMOTE-EXIT.md`） |
| **B 计划（Await）** | core `Step::Await` + `Runtime::advance`——guest 完整 take-await | ✅ 主体完成（`cordis-core-AWAIT-EXIT.md`；遗留见下） |

## 构建与测试

工具链：**Rust 1.97+**（CI 对齐 `cargo +1.97.0`）；Go guest 另需 Go ≥ 1.24。

```sh
# 门禁（每次里程碑都跑）
cargo +1.97.0 fmt --check
cargo +1.97.0 clippy --workspace --all-targets -- -D warnings   # 0 告警
cargo +1.97.0 test --workspace                                  # 全绿
cargo +1.97.0 doc --workspace --no-deps                         # 0 broken links

# Wasm guest（独立 workspace，CI 步骤）
cd examples/wasm-plugin-rust && cargo build --target wasm32-wasip2
cd ../wasm-plugin-go && bash build.sh    # GOOS=wasip1 + tools/componentize
```

## 快速开始

```sh
# 最小示例：server + auth → 激活 → 级联卸载 → 重连（M0 验收）
cargo run -p hello-plugin

# 三层依赖拓扑案例 + bench（M3）
cargo run -p im-bot --bin broker
cargo run -p im-bot --bin bench

# async 组合示例（sync 树 + async 层 + Remote 回路，P1.3）
cargo run -p cordis-async --example async_combo

# 插件模板（事件订阅 + agent-loop + 卸载 flush，P1.4）
cargo run -p cordis-async --example plugin_template
```

## 文档索引（docs/）

- 总规划/路线图：[PLAN.md](docs/PLAN.md) · 论文映射与偏差：[THEORY-MAP.md](docs/THEORY-MAP.md)
- 草案（外部工作文件，冻结后实施）：`cordis-async-protocol-draft.md`（v1.4）· `cordis-events-protocol-draft.md`（v0.3.1）· `cordis-rs-error-strategy-draft.md`（v0.2）
- 评审记录：`CORDIS-EVENTS-PROTOCOL-REVIEW.md` · `CORDIS-ERROR-STRATEGY-REVIEW.md`
- 线程拓扑：[cordis-async-THREADING.md](docs/cordis-async-THREADING.md)
- 插件作者指南：[cordis-PLUGIN-GUIDE.md](docs/cordis-PLUGIN-GUIDE.md) · 错误/安静语义：[cordis-ERRORS-QUIET.md](docs/cordis-ERRORS-QUIET.md)
- 各线出口判定：`cordis-*-EXIT.md`（Phase 0/1、错误策略、WasmRemote、Await）
- 里程碑审查报告：`docs/reviews/REVIEW-<commit>.md`（Gate B 闭环记录）

## 已知边界与遗留

- **A2b（go ABI 收尾）**：wit `effect-step.inverse→option` 后 Go guest 编码未对齐（host 解码 `invalid option discriminant`）——`go_guest` 2 测试暂 `#[ignore]`；修法建议：wit 改显式 `variant effect-step { step(inverse), wait }` 或 go 绑定编码修正。M1 双语言门禁恢复项（`cordis-core-AWAIT-EXIT.md` §4）。
- **C 探针定位**：两阶段 guest（preseed/rev-bump）为"一次性请求-消费"的轻量捷径；正式 take-await 走 B 计划（A2a 已打通，`a2_e2e` 直证）。
- **wasm 逆表回收**：`core_inverses` 槽位单调（REVIEW-2a7a686 m3）——M2 级回收项。
- **双后端值类型**：wit `Value` 与核心值类型的统一下沉（THEORY-MAP PR#13 边界）未做。
- **release/负载敏感测试**：async 时序类测试已统一 512×1ms 轮询模式（spike_s2 / m06）。

## 开发纪律（贡献约定）

- **里程碑门禁**：Gate A（fmt / clippy `-D warnings` / 全测试 / workspace 无回归）+ Gate B（独立审查报告 `docs/reviews/REVIEW-<hash>.md` 通过才进下一步）。
- **commit 纪律**：code/docs 分开提交；不做 rebase/squash（保持 `REVIEW-<hash>` ↔ 历史可追溯）。
- **core 零改动**为默认纪律；唯一例外是**授权专项**（如 B 计划 A1：core 改动额度一次性授权、THEORY-MAP 记录偏离）。
- **零第三方 run-deps**：cordis-async 仅 tokio；events/loader/wasm 等 run 依赖均为仓库内 crate。
- **草案纪律**：外部草案文件（`cordis-*-protocol-draft.md`）只评审、不修改；开工前先有详细计划与决策确认。
