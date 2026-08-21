# 产品验证线 P-4 出口判定 —— go ABI 同步自动化

**依据**：REVIEW-6a714ca M-1（`wit_exports.go` 手写 ABI 判别随 wit 变更需手动同步）；A2B-EXIT §4（go ABI 维护注意）。
**判定日期**：2026-08-22。

## 1. 交付与验收

- **build.sh 自动化（第 0 步）**：`wit-bindgen go --out-dir .` 自动重生成全部 go 绑定（含 `wit_exports.go` 的 variant 判别 0/1/2）——wit 结构变更后直接重跑 `./build.sh` 即同步（消除手动重跑/手写判别同步的维护坑）；
- **go.mod 恢复**：wit-bindgen 重写 go.mod（覆盖 vendored replace）→ `git checkout -- go.mod` 恢复 third_party/go-pkg fork replace（标准 go 构建必需）；
- **幂等验证**：wit 未变时重跑 build.sh → 生成物与提交一致（git status 仅 build.sh/README 变更）；
- **回归**：`go_guest` 2/2 绿（46.6s，含 go 工具链）；构建链路完整（重生成 → 恢复 → go build → componentize）。
- **文档**：go README 维护说明（wit 变更流程）。

## 2. 出口判定

**P-4 完成**：go ABI 同步自动化（wit 变更自动重生成 + go.mod 恢复 + 回归验证）+ 文档 + 幂等确认——REVIEW-6a714ca M-1 维护债闭环。→ 下一线 **P-5（产品假设 spike：agent 插件）**，计划按纪律起草待用户确认。
