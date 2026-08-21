# P-4 审查 + 出口走查（go ABI 同步自动化）

- **审查对象**：commits `951709d`（build.sh 第 0 步自动化 + go.mod 恢复）+ `c374ff1`（README 维护说明）+ `7e56fea`（EXIT）
- **审查日期**：2026-08-22
- **范围**：`examples/wasm-plugin-go/build.sh` + `README.md` + `docs/cordis-PRODUCTVAL-P4-EXIT.md`
- **验证**：静态阅读 + 实跑 `bash build.sh`（幂等）+ `cargo +1.97.0 test -p cordis-wasm --test go_guest`

---

## 总体结论

✅ **PASS（0 Major / 0 Minor / 1 Nit）** —— go ABI 同步自动化达成，**P-4 出口成立**；REVIEW-6a714ca M-1 维护债闭环。

## 核查

### build.sh 自动化正确性
- **第 0 步**：`wit-bindgen go --out-dir . "$ROOT/crates/cordis-wasm/wit"`——wit 结构变更时自动重生成全部 go 绑定（含 `wit_exports.go` 的 variant 判别 0/1/2），位置在 go build 前（正确时序：绑定先行）；
- **go.mod 恢复**：`git checkout -- go.mod`（wit-bindgen 重写覆盖 vendored replace → 恢复 third_party/go-pkg fork）——与 A2b 处置一致，标准 go 构建必需；
- 注释完整（P-4 引用 + M-1 依据 + 恢复理由）。

### 幂等（实跑验证）
- `bash build.sh` 重跑成功（wit-bindgen 重生成 + componentize 产出 guest.wasm）；
- **git status**：已跟踪文件（build.sh/README/wit 绑定/go.mod/guest.wasm等）**无未提交改动**——wit 未变时生成物与提交一致，幂等 ✓（未跟踪仅草案/requirements 文档，无关）。

### 链路完整
- 重生成 → git checkout go.mod → go build（reactor c-shared）→ tools/componentize → guest.wasm——第 0 步插入后全链跑通。

### 回归（实跑）
- `go_guest` **2/2 通过**（52.89s，含 go 工具链）——ABI 自动同步后双向互通不破坏。

### EXIT / README 诚实
- EXIT 交付与验收（自动化/恢复/幂等/回归/文档）逐条与实测一致；回归数字 46.6s → 实测 52.89s（环境差异，非矛盾）；README 维护流程（wit 变更 → ./build.sh → go_guest 验证）可用。

### Nit
- build.sh 无 `which wit-bindgen` 前置检查——wit-bindgen 不在 PATH 时 step 0 报"command not found"（明确失败，非静默）；建议后续加友好提示（可选，非阻塞）。

## 出口判定

**P-4 达成**：go ABI 同步自动化（wit 变更自动重生成 + go.mod 恢复 + 幂等 + go_guest 回归）——REVIEW-6a714ca M-1 维护债闭环。→ 下一线 P-5（产品假设 spike：agent 插件），计划按纪律起草待用户确认。
