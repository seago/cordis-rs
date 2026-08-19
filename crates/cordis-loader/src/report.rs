//! 条目级错误与报告面（错误策略草案 v0.2；三级分类的 OrchestrationError
//! 载荷与逐条目报告）。
//!
//! 判定公理：panic 保留 ⟺ 该错误不可能由用户配置或第三方插件合法触发；
//! 凡用户输入可达的错误走本模块（`Result` + 逐条目报告，不中断 apply）。
//! 依据：`docs/cordis-rs-error-strategy-draft.md` v0.2（冻结判定）；
//! 执行计划 `docs/cordis-loader-error-strategy-PLAN.md`。

use cordis_core::Symbol;
use std::fmt;

/// 条目级错误（OrchestrationError 的载荷；用户输入可达，不 panic）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryError {
    /// 条目 id（诊断三要素之一）。
    pub entry_id: String,
    /// 错误种类。
    pub kind: EntryErrorKind,
}

/// 条目级错误种类。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryErrorKind {
    /// 组件未注册（先 `register_component`）。
    UnknownComponent { component: String },
    /// 配置校验失败（挂载前提不满足；v0.2 决议归 OrchestrationError，未挂载）。
    ConfigValidation { message: String },
    /// 供给冲突：与 owner 条目的供给键相交（first-wins 的后到者）。
    /// `keys` = 当前条目 provide 与既有注册提供键的**全部相交键**（可能多条）；
    /// `owner` = 反查既有绑定提供者所属条目 id（按首个相交键定位）。
    ProvisionClash { keys: Vec<Symbol>, owner: String },
    /// 父条目不存在/已移除。
    UnknownParent { parent: String },
}

impl fmt::Display for EntryError {
    // 诊断契约（草案 §6）：entry id + 冲突键/组件名 + 原因，一行可读。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            EntryErrorKind::UnknownComponent { component } => write!(
                f,
                "条目 \"{}\" 未知组件 \"{}\"（先 register_component）",
                self.entry_id, component
            ),
            EntryErrorKind::ConfigValidation { message } => {
                write!(f, "条目 \"{}\" 配置校验失败：{message}", self.entry_id)
            }
            EntryErrorKind::ProvisionClash { keys, owner } => write!(
                f,
                "条目 \"{}\" 供给冲突：键 {} 已由条目 \"{}\" 提供（first-wins）",
                self.entry_id,
                fmt_keys(keys),
                owner
            ),
            EntryErrorKind::UnknownParent { parent } => write!(
                f,
                "条目 \"{}\" 父条目 \"{}\" 不存在/已移除",
                self.entry_id, parent
            ),
        }
    }
}

/// 键列表 Display（`[a,b]`；Symbol 的 Display 为驻留名）。
fn fmt_keys(keys: &[Symbol]) -> String {
    let names: Vec<String> = keys.iter().map(|s| s.to_string()).collect();
    format!("[{}]", names.join(","))
}

/// 单条目协调结果（草案 §3；按协调序收集于 [`ApplyReport`]）。
#[derive(Clone, Debug)]
pub enum EntryOutcome {
    /// 未变更（幂等：desired 与树一致，未重跑）。
    Unchanged,
    /// 已激活/重载成功。
    Activated,
    /// 失败（OrchestrationError 类：本条目**未挂载**；每次 apply 重试）。
    Failed(EntryError),
    /// 组件运行时失败（ComponentFailure 类：已挂载但 `Inactive(ζ)`；既有
    /// 语义，本错误策略不改其行为）。
    FailedFiber { error: String },
}

/// apply 的完整报告（逐条目，按协调序）。
#[derive(Clone, Debug)]
pub struct ApplyReport {
    /// 协调序 outcomes（组条目先于其子条目，子条目按 keyed diff 序）。
    pub outcomes: Vec<EntryOutcome>,
}

impl ApplyReport {
    /// 失败条目流（`Failed` / `FailedFiber`）。
    pub fn failed(&self) -> impl Iterator<Item = &EntryOutcome> {
        self.outcomes.iter().filter(|o| o.is_failure())
    }

    /// 是否全部成功（无任何失败条目）。
    pub fn ok(&self) -> bool {
        !self.outcomes.iter().any(EntryOutcome::is_failure)
    }
}

impl EntryOutcome {
    fn is_failure(&self) -> bool {
        matches!(
            self,
            EntryOutcome::Failed(_) | EntryOutcome::FailedFiber { .. }
        )
    }
}

impl fmt::Display for ApplyReport {
    // 每行一条状态；`Failed` 行直接输出 e（已含条目 id + 三要素，避免
    // 三重嵌套——REVIEW-1f9d5e8 nit-1）。注：`Unchanged/Activated/FailedFiber`
    // 未携带条目 id（§3 `EntryOutcome` 无 id 字段；§6.2 逐条目显示的精化需
    // E1 装配或 outcome 扩展——观察项 nit-2）。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for o in &self.outcomes {
            match o {
                EntryOutcome::Failed(e) => writeln!(f, "{e}")?,
                EntryOutcome::FailedFiber { error } => writeln!(f, "组件失败：{error}")?,
                EntryOutcome::Activated => writeln!(f, "已激活")?,
                EntryOutcome::Unchanged => writeln!(f, "未变更")?,
            }
        }
        Ok(())
    }
}

/// 条目失败观察者（E2）：apply 后对每个 `EntryError`（OrchestrationError）
/// 回调。loader 本身零依赖（不经 events）——由 app/测试把本 hook 接到
/// cordis-events（发射 `loader/entry-failed`），保持 loader run 依赖只
/// `cordis-core`（错误策略草案 §7 integration 点）。
///
/// **回调纪律（REVIEW-c0fb7c1 nit-2）**：回调内不得重入 `Loader::apply`
///（协调器锁已释放但应避免递归协调）；如需重 apply，延迟到回调外。
pub type EntryFailedHook = dyn Fn(&EntryError) + 'static;

/// loader 条目状态查询（报告面）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryState {
    /// 已加载（激活或组件运行中）。
    Loaded,
    /// 已禁用（coordinated disabled）。
    Disabled,
    /// 失败（OrchestrationError 类：未挂载）。
    Failed(EntryError),
    /// 组件失败（ComponentFailure 类：`Inactive(ζ)`）。
    FailedFiber(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(n: &str) -> Symbol {
        Symbol::intern(n)
    }

    #[test]
    fn display_carries_three_elements() {
        let e = EntryError {
            entry_id: "web-search".into(),
            kind: EntryErrorKind::ProvisionClash {
                keys: vec![sym("web")],
                owner: "web-core".into(),
            },
        };
        let s = e.to_string();
        assert!(s.contains("web-search"), "条目 id");
        assert!(s.contains("web"), "冲突键");
        assert!(s.contains("web-core"), "owner id");
        assert!(s.contains("first-wins"), "原因");

        let c = EntryError {
            entry_id: "x".into(),
            kind: EntryErrorKind::ConfigValidation {
                message: "缺 url".into(),
            },
        };
        assert!(c.to_string().contains("配置校验失败：缺 url"));
    }

    #[test]
    fn apply_report_failed_and_ok() {
        let r = ApplyReport {
            outcomes: vec![
                EntryOutcome::Activated,
                EntryOutcome::Failed(EntryError {
                    entry_id: "x".into(),
                    kind: EntryErrorKind::UnknownComponent {
                        component: "y".into(),
                    },
                }),
            ],
        };
        assert_eq!(r.failed().count(), 1);
        assert!(!r.ok());
        let ok = ApplyReport {
            outcomes: vec![EntryOutcome::Activated, EntryOutcome::Unchanged],
        };
        assert!(ok.ok());
    }

    #[test]
    fn entry_state_holds_failed() {
        let s = EntryState::Failed(EntryError {
            entry_id: "a".into(),
            kind: EntryErrorKind::ConfigValidation {
                message: "m".into(),
            },
        });
        assert!(matches!(s, EntryState::Failed(_)));
    }
}
