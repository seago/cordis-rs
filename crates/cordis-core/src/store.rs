//! 依赖表（论文 Def 22 的共效应上下文 `Σ`）。

use std::any::Any;
use std::collections::HashMap;
use std::fmt;

use crate::key::Key;
use crate::keyset::KeySet;
use crate::symbol::Symbol;

/// 依赖表操作错误：违反前置条件或类型不匹配。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StoreError {
    /// `set(k, ·)` 时 `k ∈ dom(σ)`（Def 23 前置条件）。
    AlreadyBound(Symbol),
    /// `get(k)` / 撤销时 `k ∉ dom(σ)`（Def 23 前置条件）。
    NotBound(Symbol),
    /// 符号相同但值类型不一致（两个键类型声明了同一 `SYMBOL`）。
    TypeMismatch(Symbol),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::AlreadyBound(k) => write!(f, "key `{k}` is already bound"),
            StoreError::NotBound(k) => write!(f, "key `{k}` is not bound"),
            StoreError::TypeMismatch(k) => {
                write!(f, "key `{k}` is bound under a different value type")
            }
        }
    }
}

impl std::error::Error for StoreError {}

/// 依赖表 `σ: (k: K) ⇀ 𝒱 k`：键（realm 符号）→ 类型擦除的值。
///
/// 所有操作都带 Def 23 的前置条件：`bind` 要求 `k ∉ dom(σ)`、`unbind`
/// 要求 `k ∈ dom(σ)`；违反则报错且**不产生状态变更**。
#[derive(Default)]
pub struct Store {
    bindings: HashMap<Symbol, Binding>,
}

struct Binding {
    value: Box<dyn Any + Send + Sync>,
}

impl Store {
    /// 空依赖表。
    pub fn new() -> Self {
        Self::default()
    }

    /// `set(k, v)`：绑定类型化键。前置条件 `k ∉ dom(σ)`（Def 23）。
    pub fn bind<K: Key>(&mut self, value: K::Value) -> Result<(), StoreError> {
        let symbol = Symbol::intern(K::SYMBOL);
        if self.bindings.contains_key(&symbol) {
            return Err(StoreError::AlreadyBound(symbol));
        }
        self.bindings.insert(
            symbol,
            Binding {
                value: Box::new(value),
            },
        );
        Ok(())
    }

    /// `get(k)`：读取绑定值。前置条件 `k ∈ dom(σ)`；类型不匹配报错。
    pub fn get<K: Key>(&self) -> Result<&K::Value, StoreError> {
        let symbol = Symbol::intern(K::SYMBOL);
        let binding = self
            .bindings
            .get(&symbol)
            .ok_or(StoreError::NotBound(symbol))?;
        binding
            .value
            .downcast_ref::<K::Value>()
            .ok_or(StoreError::TypeMismatch(symbol))
    }

    /// `σ ∖ k`：撤销绑定并返回原值。前置条件 `k ∈ dom(σ)`。
    ///
    /// 类型检查先于移除：类型不匹配时绑定保持不变。
    pub fn unbind<K: Key>(&mut self) -> Result<K::Value, StoreError> {
        let symbol = Symbol::intern(K::SYMBOL);
        let binding = self
            .bindings
            .get(&symbol)
            .ok_or(StoreError::NotBound(symbol))?;
        if !binding.value.is::<K::Value>() {
            return Err(StoreError::TypeMismatch(symbol));
        }
        let binding = self.bindings.remove(&symbol).expect("checked above");
        let value = binding
            .value
            .downcast::<K::Value>()
            .expect("type checked above");
        Ok(*value)
    }

    /// 符号级查询：`k ∈ dom(σ)`。
    pub fn contains(&self, symbol: Symbol) -> bool {
        self.bindings.contains_key(&symbol)
    }

    /// 已绑定的符号集合（确定性序）。
    pub fn symbols(&self) -> impl Iterator<Item = Symbol> + '_ {
        self.bindings.keys().copied()
    }

    /// 满足谓词 `σ ⊧ d`（Def 24）：`∀k ∈ d. k ∈ dom(σ)`。
    pub fn satisfies(&self, spec: &KeySet) -> bool {
        spec.iter().all(|k| self.bindings.contains_key(&k))
    }
}
