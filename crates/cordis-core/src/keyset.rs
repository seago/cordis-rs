//! 键集合（论文 Def 25 的规格 `𝔇Σ` 与 Def 43 的供给 `𝔓Γ`）。

use std::collections::BTreeSet;

use crate::symbol::Symbol;

/// 键集合，同时充当：
///
/// - 共效应规格 `d ∈ 𝔇Σ`（Def 25：组件从环境声明的依赖）；
/// - 供给 `p ∈ 𝔓Γ`（Def 43：组件可提供的键）。
///
/// `BTreeSet` 保证确定性的迭代顺序（跨线程、跨运行可复现）。
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
