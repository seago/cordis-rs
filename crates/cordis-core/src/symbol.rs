//! 共效应键符号（论文 Def 22 的键空间 `K` 与 Def 28 的 realm 空间 `R`）。
//!
//! 符号是原子：只比较相等、从不检查结构（Def 45 对名称的纪律）。
//! 采用全局驻留（intern）：同一字符串恒对应同一 [`Symbol`]。

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};

/// 驻留表：名称 → id、id → 泄漏的 `'static` 字符串。
struct InternerData {
    by_name: HashMap<Box<str>, u32>,
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
        let boxed: Box<str> = name.into();
        let leaked: &'static str = Box::leak(boxed.clone());
        data.by_name.insert(boxed, id);
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
