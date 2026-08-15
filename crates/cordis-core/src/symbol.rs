//! 共效应键符号（论文 Def 22 的键空间 `K` 与 Def 28 的 realm 空间 `R`）。
//!
//! 符号是原子：只比较相等、从不检查结构（Def 45 对名称的纪律）。
//! 采用全局驻留（intern）：同一字符串恒对应同一 [`Symbol`]。
//!
//! 注意：`Symbol` 的 `u32` id 是**进程内**首次 intern 的分配序（进程内确定），
//! 跨进程不可比较——跨边界（wasm）互操作以**名称字符串**为媒介，而非 id
//! （THEORY-MAP「已知偏差」）。

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};

/// 驻留表：名称 → id、id → 泄漏的 `'static` 字符串。
///
/// `by_name` 以已泄漏的 `&'static str` 为键（借用 `by_id` 的同一份存储），
/// 每个名称只分配一份字符串（`Borrow<str>` 支持 `&str` 查找）。
struct InternerData {
    by_name: HashMap<&'static str, u32>,
    by_id: Vec<&'static str>,
}

fn interner() -> &'static Mutex<InternerData> {
    static INTERNER: OnceLock<Mutex<InternerData>> = OnceLock::new();
    INTERNER.get_or_init(|| {
        Mutex::new(InternerData {
            by_name: HashMap::new(),
            by_id: Vec::new(),
        })
    })
}

/// 键符号（论文 `k: K`）。`Copy`，相等性即身份。
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Symbol(u32);

impl Symbol {
    /// 驻留一个符号：同一 `name` 恒返回同一 [`Symbol`]。
    pub fn intern(name: &str) -> Symbol {
        let mut data = interner().lock().expect("interner poisoned");
        if let Some(&id) = data.by_name.get(name) {
            return Symbol(id);
        }
        let id = u32::try_from(data.by_id.len()).expect("symbol space exhausted");
        let leaked: &'static str = Box::leak(name.into());
        data.by_name.insert(leaked, id);
        data.by_id.push(leaked);
        Symbol(id)
    }

    /// 该符号的规范名称。
    pub fn as_str(self) -> &'static str {
        let data = interner().lock().expect("interner poisoned");
        data.by_id[self.0 as usize]
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Symbol({})", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_is_identity_by_name() {
        let a = Symbol::intern("db");
        let b = Symbol::intern("db");
        let c = Symbol::intern("cache");
        assert_eq!(a, b, "同一名称恒同一符号");
        assert_ne!(a, c, "不同名称不同符号");
    }

    #[test]
    fn as_str_roundtrip() {
        let s = Symbol::intern("database");
        assert_eq!(s.as_str(), "database");
        assert_eq!(format!("{s}"), "database", "Display 即名称");
        assert_eq!(format!("{s:?}"), "Symbol(database)", "Debug 含名称");
    }

    #[test]
    fn symbol_is_copy_and_hashable() {
        let s = Symbol::intern("k");
        let copy = s;
        assert_eq!(copy, s);
        let mut set = std::collections::BTreeSet::new();
        set.insert(s);
        assert!(set.contains(&copy));
    }
}
