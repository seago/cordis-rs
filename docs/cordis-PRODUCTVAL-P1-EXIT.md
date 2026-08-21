# 产品验证线 P-1 出口判定 —— wasm 逆表回收

**依据**：计划 `docs/cordis-PRODUCTVAL-P1-PLAN.md`；REVIEW-2a7a686 m3（已知边界）；审查 REVIEW-f57faad（PASS，0 Major/1 Minor/3 Nit 已处置）。
**判定日期**：2026-08-22。

## 1. 交付与验收

- **P1-1 free list 机制**：`Host.inverse_free` 复用池——`set` 分配优先复用；`run_inverse`（逆已执行、槽位空、句柄已撤销）后入池；`drop` 保持 no-op（句柄销毁 ≠ 逆执行——语义边界注释）；借用无冲突（core_inverses/store 独立 RefCell）；防重复入池（take 幂等）。
- **P1-2 有界性验收**：`host_inverse_free_reuse_bounds_rep_allocation`——循环 1000 次 set→释放→set，断言 rep 复用 + `next_rep` 恒定（=1），**分配量 ≈ 峰值并发逆数（非操作次数）**。
- **门禁**：wasm 全套绿（lib 8 + 集成 14 含 go 20s）+ clippy/fmt/doc 0 + 不改 core；逆撤销路径（退役级联）回归不破坏。

## 2. 边界处置记录

- **REVIEW-2a7a686 m3 已知边界 → 已回收**（组件生命周期内分配量有界）；生命周期外（实例释放）整表随 InstanceState 释放不变。
- **m-1（记录）**：真实 `run_inverse` 入池链路的集成级直证未单测（复用机制宿主层已直证；真实路径静态核查 + 逆撤销回归覆盖）——待 P-5 产品 spike 的长驻组件场景自然覆盖，或后续补集成测试。
- **n-2**：`run_inverse` 的 `task()` panic 时 rep 不入池（保守留白，注释已注——核心逆不应 panic）。

## 3. 出口判定

**P-1 完成**：逆表回收机制 + 有界性直证 + 全回归绿 + 审查闭环（0 Major 未决）。→ 下一线 **P-2（双后端值类型下沉）**，计划按纪律起草待用户确认。
