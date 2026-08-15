//! 键集合（论文 Def 25 的规格 `𝔇Σ` 与 Def 43 的供给 `𝔓Γ`）。

use std::collections::BTreeSet;

use crate::symbol::Symbol;

/// 键集合，同时充当：
///
/// - 共效应规格 `d ∈ 𝔇Σ`（Def 25：组件从环境声明的依赖）；
/// - 供给 `p ∈ 𝔓Γ`（Def 43：组件可提供的键）。
///
/// `BTreeSet` 保证进程内确定的迭代顺序（`Symbol` 的 `Ord` 为进程内分配序，
/// 跨运行顺序不保证；见 THEORY-MAP「已知偏差」）。
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct KeySet(BTreeSet<Symbol>);

impl KeySet {
    /// 空集合。
    pub fn new() -> Self {
        Self(BTreeSet::new())
    }

    /// 插入一个键。
    pub fn insert(&mut self, key: Symbol) -> bool {
        self.0.insert(key)
    }

    /// 是否包含 `key`。
    pub fn contains(&self, key: Symbol) -> bool {
        self.0.contains(&key)
    }

    /// 迭代键（BTree 序，确定性）。
    pub fn iter(&self) -> impl Iterator<Item = Symbol> + '_ {
        self.0.iter().copied()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 键数。
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 是否与另一集合相交（O-Insert 的供给不相交检查，Def 43/§4.2）。
    pub fn intersects(&self, other: &KeySet) -> bool {
        self.0.intersection(&other.0).next().is_some()
    }
}

impl FromIterator<Symbol> for KeySet {
    fn from_iter<T: IntoIterator<Item = Symbol>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl Extend<Symbol> for KeySet {
    fn extend<T: IntoIterator<Item = Symbol>>(&mut self, iter: T) {
        self.0.extend(iter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(name: &str) -> Symbol {
        Symbol::intern(name)
    }

    #[test]
    fn insert_contains_iter() {
        let mut set = KeySet::new();
        assert!(set.is_empty());
        assert!(set.insert(sym("a")));
        assert!(!set.insert(sym("a")), "重复插入返回 false");
        assert!(set.contains(sym("a")));
        assert!(!set.contains(sym("b")));
        assert_eq!(set.len(), 1);
        let collected: Vec<_> = set.iter().collect();
        assert_eq!(collected, vec![sym("a")]);
    }

    #[test]
    fn intersects_and_disjoint() {
        let a: KeySet = [sym("x"), sym("y")].into_iter().collect();
        let b: KeySet = [sym("y"), sym("z")].into_iter().collect();
        let c: KeySet = [sym("z")].into_iter().collect();
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
        assert!(a.intersects(&a), "与自身相交");
        assert!(!KeySet::new().intersects(&a));
    }

    #[test]
    fn extend_accumulates() {
        let mut set = KeySet::new();
        set.extend([sym("a"), sym("b")]);
        set.extend([sym("b"), sym("c")]);
        assert_eq!(set.len(), 3);
    }
}
