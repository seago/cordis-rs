# A2b 开发计划 —— go guest ABI 收尾（修法①：wit 显式 variant）

**依据**：`docs/cordis-core-AWAIT-EXIT.md` §4（A2b 遗留 + 修法建议①）；用户指定修法①。
**目标**：恢复 `go_guest` 2 测试（去 `#[ignore]`）+ M1 双语言门禁；同时消除 `option<resource>` 编码坑（root cause：go 绑定对 `option<inverse>` 的判别编码与宿主组件模型未对齐，host 解码 `invalid option discriminant`）。
**状态**：**草案——待开工指令**（含 2 项设计决策，见 §1）。
**保证**：Gate A/B 同前；commit 分 code/docs；workspace 全回归绿（本计划将再次波及全部 guest——需全量重编，含 CI 构建步骤确认）。

---

## 0. 为什么选修法①（variant）

- 根因在 `option<resource>`（effect-step.inverse 可选）的 go 侧编码——修 go 绑定（方案②）需深入 wit-bindgen go 生成层（当前 go 侧编解码骨架位置不透明，诊断已确认该方向成本高）。
- 修法①把"等待步"显式化为 wit 结构（variant），**消除 option<resource> 本身**——宿主/全部 guest 的语义都变显式；虽全局波及（rust 4 guest + go 重编），但路径确定、不依赖工具链内部。

## 1. 设计决策（开工前确认）

### D-1：variant 形态（推荐 B）
- **A（精简）**：`variant effect-step { step(inverse), wait }` + 终止 = 外层 `option` 的 `None`——语义最简，但**改动既有协议**（guest 原 `done:true` 收尾步须改 None；宿主 Finished 判定改 None）。
- **B（保留 done，推荐）**：`variant effect-step { step(inverse), done, wait }`——`Some(done)` = 终止（宿主 Finished）、`Some(step(inv))` = 有逆步继续、`Some(wait)` = 等待远端（宿主 Await）、`None` = 无步（原语义）。改动面更小、与既有 guest/宿主逻辑同构。

### D-2：波及范围确认
- wit `effect-step` 变 variant → 宿主 `WasmTaskIter`（done 分支改 variant 分支）+ **全部 guest 重编**：rust 主 guest（多步 take）、rust-consumer / misbehave / panic、go guest（plugin.go + 绑定重生成）。
- CI 的 guest 构建步骤（rust target build + go build.sh）需跑通新版。

## 2. 分步计划

### Step G1：wit variant + 宿主适配（G1）
- **目标**：wit 改 variant（按 D-1）；宿主 `WasmTaskIter` 处理 `step/done/wait` 三分支（wait → `Step::Await` 判定沿用"在途 join"）。
- **任务**：
  1. `wit/cordis.wit`：`effect-step` → variant（D-1 定稿）；
  2. `WasmTaskIter::next`：match variant（step→Yielded / done→Finished / wait→Await 判定）+ 既有在途 join 逻辑保持；
  3. rust 主 guest（`wasm-plugin-rust`）：适配 variant（submit 步 → `Some(step(inv))`；等待步 → `Some(wait)`；收尾 → `Some(done)`）。
- **验证**：宿主 + 主 guest 重编；`a2_e2e` 2 测试恢复绿（guest 完整 take-await 不变）。

### Step G2：rust 其余 guest 适配 + 全 rust 回归（G2）
- **目标**：rust-consumer / misbehave / panic 适配 variant（收尾步 `Some(done)`）+ 全套 wasm 测试绿（含 load_guest/dependency/sandbox/dual_backend）。
- **任务**：
  1. 3 guest 改收尾形态 + 重编；
  2. 宿主断言如有 done 相关调整一并适配；
  3. 全套回归（go_guest 仍 ignore，等 G3）。
- **验证**：`cargo test -p cordis-wasm`（除 go_guest）全绿。

### Step G3：go guest 适配 + go_guest 恢复（G3）
- **目标**：go guest 按新 wit 重生成绑定 + plugin.go 适配 variant（`witTypes.Some(EffectStep{...})` → 新形态）+ `go_guest` 2 测试去 ignore 转绿。
- **任务**：
  1. 重跑 go 绑定生成（build.sh 管线含 wit 嵌入——确认绑定重生成路径）；
  2. plugin.go：Step 返回 variant（step/done/wait 三态映射：有逆 → Some(step(inv))；done → Some(done)）；
  3. `go_guest` 2 测试去 `#[ignore]` → 断言恢复（native/rust provider 双路）。
- **验证**：go_guest 2/2 绿（M1 双语言门禁恢复）。

### Step G4：出口（G4）
- **目标**：专项出口走查 + EXIT 更新（A2b 闭环）。
- **任务**：
  1. 全套门禁：fmt/clippy/doc/workspace 全回归（含 go）；
  2. `docs/cordis-core-AWAIT-EXIT.md` §4 A2b 遗留 → 已闭环记录；README 已知边界更新；
  3. 出口走查（Gate B）。
- **验证**：全绿 + 走查 PASS。

## 3. 里程碑与量级

| 里程碑 | 内容 | 依赖 | 量级 |
|---|---|---|---|
| G1 | wit variant + 宿主 + 主 guest | 开工指令 + D-1 | 1 天 |
| G2 | rust 3 guest + 全 rust 回归 | G1 | 0.5–1 天 |
| G3 | go 适配 + go_guest 恢复 | G2 | 1–2 天（go 工具链为主风险） |
| G4 | 出口走查 + EXIT | G3 | 0.5 天 |

全程约 3–5 天（含审查门禁）。

## 4. 风险

- **全局重编**：wit 结构变 → 全部 guest（rust 4 + go）重编；CI 构建步骤同步更新。
- **go 绑定重生成**：build.sh 的绑定生成路径需确认（若 go 绑定非自动重生成，需显式跑 wit-bindgen go——G3 前置确认）。
- **协议语义漂移**：`done` 显式化后，`None`（无步）与 `Some(done)`（终止）区分——宿主/guest 一致（G1 起即锁定语义）。

## 5. 纪律

- 同前：Gate A/B、commit 分 code/docs、workspace 全回归；**本计划不改 core**（纯 wit/guest/宿主层）。
- 出口判定：go_guest 2/2 + 双语言回归 + 全套门禁绿 + EXIT 更新。
