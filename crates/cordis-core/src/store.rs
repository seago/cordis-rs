//! 依赖表（论文 Def 22 的共效应上下文 `Σ`；Def 28 起按 realm 键控：
//! `σ: (r: R) ⇀ 𝒱 r`，键经 `ρ` 解析到 realm）。

use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;

use crate::key::Key;
use crate::keyset::KeySet;
use crate::symbol::Symbol;

/// 依赖表操作错误：违反前置条件或类型不匹配。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StoreError {
    /// `set` 时 `realm ∈ dom(σ)`（Def 23 前置条件沿 `ρ` 转译，Def 29）。
    AlreadyBound(Symbol),
    /// `get` / 撤销时 `realm ∉ dom(σ)`（Def 23 前置条件沿 `ρ` 转译，Def 29）。
    NotBound(Symbol),
    /// realm 相同但值类型不一致（两个键类型声明了同一符号）。
    TypeMismatch(Symbol),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::AlreadyBound(k) => write!(f, "realm `{k}` is already bound"),
            StoreError::NotBound(k) => write!(f, "realm `{k}` is not bound"),
            StoreError::TypeMismatch(k) => {
                write!(f, "realm `{k}` is bound under a different value type")
            }
        }
    }
}

impl std::error::Error for StoreError {}

/// 依赖表 `σ: (r: R) ⇀ 𝒱 r`：realm 符号 → 类型擦除的值（Def 28）。
///
/// 所有操作都带前置条件（Def 23 沿 `ρ` 转译，Def 29）：`bind` 要求
/// `realm ∉ dom(σ)`、`unbind` 要求 `realm ∈ dom(σ)`；违反则报错且
/// **不产生状态变更**。
///
/// 底层用 `BTreeMap`：迭代序进程内确定（`Symbol` 的 `Ord` 为进程内分配序，
/// 见 THEORY-MAP 已知偏差）。
#[derive(Default)]
pub struct Store {
    bindings: BTreeMap<Symbol, Binding>,
}

struct Binding {
    value: Box<dyn Any + Send + Sync>,
}

impl Store {
    /// 空依赖表。
    pub fn new() -> Self {
        Self::default()
    }

    /// `set(k, v)` 的表层：在 `realm` 处绑定类型化值。前置条件 `realm ∉ dom(σ)`。
    pub fn bind<K: Key>(&mut self, realm: Symbol, value: K::Value) -> Result<(), StoreError> {
        if self.bindings.contains_key(&realm) {
            return Err(StoreError::AlreadyBound(realm));
        }
        self.bindings.insert(
            realm,
            Binding {
                value: Box::new(value),
            },
        );
        Ok(())
    }

    /// `get(k)` 的表层：读取 `realm` 处的绑定。前置条件 `realm ∈ dom(σ)`；
    /// 类型不匹配报错。
    pub fn get<K: Key>(&self, realm: Symbol) -> Result<&K::Value, StoreError> {
        let binding = self
            .bindings
            .get(&realm)
            .ok_or(StoreError::NotBound(realm))?;
        binding
            .value
            .downcast_ref::<K::Value>()
            .ok_or(StoreError::TypeMismatch(realm))
    }

    /// `σ ∖ r`：撤销 `realm` 处的绑定并返回原值。前置条件 `realm ∈ dom(σ)`。
    ///
    /// 类型检查先于移除：类型不匹配时绑定保持不变。
    pub fn unbind<K: Key>(&mut self, realm: Symbol) -> Result<K::Value, StoreError> {
        let binding = self
            .bindings
            .get(&realm)
            .ok_or(StoreError::NotBound(realm))?;
        if !binding.value.is::<K::Value>() {
            return Err(StoreError::TypeMismatch(realm));
        }
        let binding = self.bindings.remove(&realm).expect("checked above");
        let value = binding
            .value
            .downcast::<K::Value>()
            .expect("type checked above");
        Ok(*value)
    }

    /// 符号级查询：`r ∈ dom(σ)`。
    pub fn contains(&self, realm: Symbol) -> bool {
        self.bindings.contains_key(&realm)
    }

    /// 已绑定的 realm 符号集合（确定性序）。
    pub fn symbols(&self) -> impl Iterator<Item = Symbol> + '_ {
        self.bindings.keys().copied()
    }

    /// 满足谓词（无 `ρ` 解析的表层形态）：`∀k ∈ d. k ∈ dom(σ)`（Def 24）。
    ///
    /// 带 `ρ` 解析的上下文级满足谓词见 [`crate::context::Context::satisfies`]。
    pub fn satisfies(&self, spec: &KeySet) -> bool {
        spec.iter().all(|k| self.bindings.contains_key(&k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DbKey;
    impl Key for DbKey {
        type Value = String;
        const SYMBOL: &'static str = "db";
    }

    struct CacheKey;
    impl Key for CacheKey {
        type Value = u64;
        const SYMBOL: &'static str = "cache";
    }

    /// 与 `DbKey` 声明同一符号但值类型不同的键（符号冲突场景）。
    struct OtherDbKey;
    impl Key for OtherDbKey {
        type Value = u32;
        const SYMBOL: &'static str = "db";
    }

    fn db() -> Symbol {
        Symbol::intern(DbKey::SYMBOL)
    }

    #[test]
    fn bind_get_unbind_roundtrip() {
        let mut store = Store::new();
        // get 前置条件：realm ∉ dom(σ) 时 NotBound
        assert_eq!(store.get::<DbKey>(db()), Err(StoreError::NotBound(db())));
        assert!(!store.contains(db()));

        store.bind::<DbKey>(db(), String::from("pg")).unwrap();
        assert!(store.contains(db()));
        assert_eq!(store.get::<DbKey>(db()).unwrap(), "pg");

        // unbind 前置条件满足：返回原值并移除
        assert_eq!(store.unbind::<DbKey>(db()).unwrap(), String::from("pg"));
        assert!(!store.contains(db()));
        assert_eq!(store.get::<DbKey>(db()), Err(StoreError::NotBound(db())));
    }

    #[test]
    fn bindings_are_keyed_by_realm() {
        // Def 28：σ 按 realm 键控——同一键类型可在不同 realm 独立绑定。
        let mut store = Store::new();
        let r1 = Symbol::intern("r1");
        let r2 = Symbol::intern("r2");
        store.bind::<DbKey>(r1, String::from("pg")).unwrap();
        store.bind::<DbKey>(r2, String::from("mysql")).unwrap();
        assert_eq!(store.get::<DbKey>(r1).unwrap(), "pg");
        assert_eq!(store.get::<DbKey>(r2).unwrap(), "mysql");

        store.unbind::<DbKey>(r1).unwrap();
        assert!(matches!(
            store.get::<DbKey>(r1),
            Err(StoreError::NotBound(_))
        ));
        assert_eq!(
            store.get::<DbKey>(r2).unwrap(),
            "mysql",
            "其他 realm 不受影响"
        );
    }

    #[test]
    fn duplicate_bind_rejected_without_mutation() {
        let mut store = Store::new();
        store.bind::<DbKey>(db(), String::from("pg")).unwrap();
        // 违反前置条件 realm ∉ dom(σ)：报错且不改变现有绑定
        assert_eq!(
            store.bind::<DbKey>(db(), String::from("mysql")),
            Err(StoreError::AlreadyBound(db()))
        );
        assert_eq!(store.get::<DbKey>(db()).unwrap(), "pg", "原绑定保持");
    }

    #[test]
    fn type_mismatch_on_symbol_collision() {
        let mut store = Store::new();
        store.bind::<DbKey>(db(), String::from("pg")).unwrap();
        // 同一 realm 下不同值类型：访问点报 TypeMismatch
        assert!(matches!(
            store.get::<OtherDbKey>(db()),
            Err(StoreError::TypeMismatch(s)) if s == db()
        ));

        // unbind 的类型检查先于移除：类型不匹配时绑定保持不变
        assert!(matches!(
            store.unbind::<OtherDbKey>(db()),
            Err(StoreError::TypeMismatch(s)) if s == db()
        ));
        assert!(store.contains(db()), "类型不匹配时绑定不得被移除");
        assert_eq!(store.get::<DbKey>(db()).unwrap(), "pg");
    }

    #[test]
    fn unbind_on_missing_key_is_not_bound() {
        let mut store = Store::new();
        assert_eq!(store.unbind::<DbKey>(db()), Err(StoreError::NotBound(db())));
    }

    #[test]
    fn satisfies_is_conjunctive() {
        // Def 24：σ ⊧ d ⟺ ∀k ∈ d. k ∈ dom(σ)
        let mut store = Store::new();
        store.bind::<DbKey>(db(), String::from("pg")).unwrap();

        let empty: KeySet = KeySet::new();
        assert!(store.satisfies(&empty), "空规格恒满足");

        let both: KeySet = [db(), Symbol::intern(CacheKey::SYMBOL)]
            .into_iter()
            .collect();
        assert!(!store.satisfies(&both), "缺任一键即不满足");

        store
            .bind::<CacheKey>(Symbol::intern(CacheKey::SYMBOL), 3)
            .unwrap();
        assert!(store.satisfies(&both));
        assert_eq!(store.symbols().count(), 2);
    }
}
