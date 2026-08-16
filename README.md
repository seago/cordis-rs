# cordis-rs

论文《A Programming Paradigm for Spatiotemporal Composability》的 Rust + Wasm 参考实现（实施中）。

- 实施规划：[docs/PLAN.md](docs/PLAN.md)
- 论文符号 ↔ 代码映射与偏差记录：[docs/THEORY-MAP.md](docs/THEORY-MAP.md)
- 论文原文：`paper/paper.pdf`

## 示例

```sh
cargo run -p hello-plugin   # server + auth：激活 → 级联卸载 → 重连（M0 验收）
```
