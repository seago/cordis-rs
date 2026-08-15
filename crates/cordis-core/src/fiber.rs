//! Fiber 身份（论文 Def 44 的 `n: 𝔑`）。

/// fiber 名：原子，只比较相等、从不检查结构（Def 45 的名称纪律）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct FiberId(u64);

impl FiberId {
    /// 分配一个全新的名字（绝不复用）。
    pub(crate) fn fresh(next: &mut u64) -> FiberId {
        let id = FiberId(*next);
        *next += 1;
        id
    }
}
