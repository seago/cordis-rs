//! 反应式通知（论文 §3.2.2 / Algorithm 3）。
//!
//! 核心是 Def 26 的分类：状态迁移 `σ → σ′` 按共效应规格 `d` 分为
//! activating / deactivating / neutral。分类是反应性的代数基础——
//! 每个共效应变更都经效应函数（可逆）发生，因此变更可观察、可分类
//! （§3.2.2："the effect system guarantees that every coeffect change is observed"）。
//!
//! fiber 级通知（Algorithm 3 的 refresh 调用）依赖 registry（PR #5）；
//! 本模块提供分类原语与上下文级传播机制（[`crate::context::Context::notify`]）。
//!
//! **衔接留白（审查 m1）**：`classify` 需要前/后两个 `Store` 快照，而
//! [`Context::notify`] 只广播受影响**键**（不携带状态快照；`Store` 不可克隆）。
//! 快照/变更日志机制由 **PR #5** 提供（届时确定 `notify` 携带 `prev` 快照或
//! 变更描述的形态，fiber 反应器据此完成分类）；在此之前 `classify` 是
//! 独立验证的纯函数，系统内无调用点（THEORY-MAP 已知偏差）。

use crate::keyset::KeySet;
use crate::store::Store;

/// 通知分类（Def 26 式 (26)）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Classification {
    /// `σ ⊭ d ∧ σ′ ⊧ d`：满足状态由不满足变为满足。
    Activating,
    /// `σ ⊧ d ∧ σ′ ⊭ d`：满足状态由满足变为不满足。
    Deactivating,
    /// 其余情况：满足状态不变。
    Neutral,
}

/// 对迁移 `prev → next` 按规格 `spec` 分类（Def 26）。
///
/// 判定基于满足谓词 `σ ⊧ d`（Def 24），仅依赖绑定存在性——值变更而
/// 满足状态不变时分类为 `Neutral`。
pub fn classify(prev: &Store, next: &Store, spec: &KeySet) -> Classification {
    match (prev.satisfies(spec), next.satisfies(spec)) {
        (false, true) => Classification::Activating,
        (true, false) => Classification::Deactivating,
        _ => Classification::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::Key;
    use crate::symbol::Symbol;

    struct K1I;
    impl Key for K1I {
        type Value = usize;
        const SYMBOL: &'static str = "k1";
    }

    fn store_with(bindings: &[(&str, usize)]) -> Store {
        let mut store = Store::new();
        for (name, v) in bindings {
            let sym = Symbol::intern(name);
            store.bind::<K1I>(sym, *v, None).unwrap();
        }
        store
    }

    #[test]
    fn classification_matrix() {
        // Def 26 的分类矩阵（规格 d = {k1}）。
        let d: KeySet = [Symbol::intern("k1")].into_iter().collect();
        let empty = Store::new();
        let k1 = store_with(&[("k1", 1)]);
        let k1_other_value = store_with(&[("k1", 2)]);
        let k2 = store_with(&[("k2", 1)]);
        let k1k2 = store_with(&[("k1", 1), ("k2", 1)]);

        // 满足状态翻转：activating / deactivating。
        assert_eq!(classify(&empty, &k1, &d), Classification::Activating);
        assert_eq!(classify(&k1, &empty, &d), Classification::Deactivating);
        // 其他键的变化使满足从不满足变为满足：仍按满足状态分类。
        assert_eq!(classify(&k2, &k1, &d), Classification::Activating);
        assert_eq!(classify(&k1k2, &k2, &d), Classification::Deactivating);
        // 满足状态不变（含值变更）：neutral。
        assert_eq!(classify(&empty, &empty, &d), Classification::Neutral);
        assert_eq!(classify(&k1, &k1, &d), Classification::Neutral);
        assert_eq!(classify(&k1, &k1_other_value, &d), Classification::Neutral);
        assert_eq!(classify(&empty, &k2, &d), Classification::Neutral);
        assert_eq!(classify(&k1, &k1k2, &d), Classification::Neutral);
    }
}
