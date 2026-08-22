# 产品验证线 P-6 出口判定 —— §6.6 版本化依赖 + 小项②③（产品验证线收官）

**依据**：计划 `docs/cordis-PRODUCTVAL-P6-PLAN.md`（§6.6 版本化依赖 + Progress 上界 + loader 索引）；论文 §6.6（drift/collision 两问题）+ Thm 66 + bench M3-BENCH 已知边界①。
**判定日期**：2026-08-22。

## 1. 交付与验收

- **P6-1 版本化链接（§6.6，v1 键内编码 `key@version`）**：版本隔离（`db@1`/`db@2` 共存不冲突）、升级（依赖迁移）、冲突（同版本双提供 → ProvisionClash 报告）——`versioned_keys_isolate_upgrade_and_conflict` 直证；`cordis-PLUGIN-GUIDE.md` §8bis 版本化章节（接口漂移防护：消费者声明版本键、升级后旧声明不再满足显式 Inactive；约束区间留 v2）。
- **P6-2 Progress 定量上界补测（Thm 66）**：`progress_quantitative_upper_bound`——K=2 链深 2 拓扑（A≺B≺C），驱动到静止总步数 6 ≤ ΣB(n)=612 直证——**THEORY-MAP 46 行覆盖缺口关闭**。
- **P6-3 loader desired-diff 索引化**：阶段一 `rev().find()` 逆扫 O(N²) → HashMap last-wins 索引 O(N)（bench 已知边界①）；`desired_duplicate_id_last_wins` 直证（与 rev().find() 等值）。
- **门禁**：core 61、loader 60、workspace 无回归、clippy/fmt/doc 0、零第三方（P6-1/P6-3 仅 loader；P6-2 仅测试——core 零改动）。

## 2. 产品验证线收官（P-1..P-7）

| 线 | 内容 | EXIT |
|---|---|---|
| P-1 | wasm 逆表回收（REVIEW-2a7a686 m3 闭环） | P1-EXIT |
| P-2 | 双后端值类型下沉（cordis-value，THEORY-MAP PR#13 闭环） | P2-EXIT |
| P-3 | Await 生产化（挂起集/批量恢复/统一驱动回路） | P3-EXIT |
| P-4 | go ABI 同步自动化（M-1 闭环） | P4-EXIT |
| P-5 | agent 插件产品 spike（原生+wasm 多轮+全栈串联） | P5-EXIT |
| P-7 | 错误策略 O-1/O-4 联动 | P7-EXIT |
| **P-6** | **§6.6 版本化依赖 + Progress 上界 + loader 索引** | **本 EXIT** |

**产品验证线（Phase 2 决策：C 组合全做）全部收官**——既有边界/债项清零 + 产品价值落地。

## 3. 出口判定

**P-6 完成 + 产品验证线全线收官**：§6.6 版本化链接落地（drift 显式防护）+ Thm 66 定量缺口关闭 + loader O(N²)→O(N) + 门禁全绿。后续取向（其它工作线/论文进一步映射）按纪律由用户下达。

## 4. 走查闭环（REVIEW-P6-EXIT）

Gate B 独立走查 `docs/reviews/REVIEW-P6-EXIT.md`：**PASS（WITH MINORS）**——ΣB(n)=612 复核正确、last-wins 等值确认、`@` 不误伤无版本键、workspace 无回归。3 项 Minor 落地：

- **M-1**（key namespacing 缺口）→ §8bis 补"范围注记"（collision 由 first-wins 报告兜底，命名空间化留后续）；
- **M-2**（doc 1 告警：agent-plugin main.rs:36 `<Context>` 未闭合 rustdoc 标签，P-5 来源）→ 反引号化修复，doc 归 0；
- **M-3**（升级直证覆盖粒度）→ 注记：直证为"双版本共存 + 新增消费者迁移"，单消费者条目级迁移留覆盖粒度说明（v1 语义等价，见 §8bis"升级 = 消费者迁移到新版本键"）。

走查判定 + Minor 全部闭环，EXIT 成立。
