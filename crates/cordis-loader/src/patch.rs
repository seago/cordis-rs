//! G6 include patches（TS `Include.Config.patches` 参照，PR #32）。
//!
//! TS 实现（`/tmp/cordis-ts/packages/include/src/index.ts`）的 `patches`
//! 是配置文件的**运行时补丁层**：`PatchOptions { id, insert, name,
//! config, group, disabled, inject, intercept, isolate, ... }`——按 `id`
//! 定位条目并覆盖字段、`insert` 向组插入子条目。
//!
//! 本模块提供 loader 侧等价：**对 desired 树的纯变换**（`apply_patches`，
//! 原树不动、返回新树），随后照常 `apply`。**公开差异（收窄）**：文件
//! 读取/文件 watch/写回持久化/yaml 解析属编排工具层（仓库零第三方依赖
//! 纪律，无 serde_yaml/chokidar 等价物）；`inject`/`intercept`/`isolate`
//! 字段补丁按需扩展（当前覆盖 `name`/`config`/`revision`/`disabled`/
//! `insert`）。

use crate::Entry;
use std::any::Any;
use std::rc::Rc;

/// 单条补丁（TS `PatchOptions` 的 loader 侧子集）。
#[derive(Clone, Debug)]
pub struct Patch {
    /// 目标条目 id（整棵树递归匹配；None = 顶层插入，见 [`Patch::insert`]）。
    pub id: Option<String>,
    /// 向 `id` 组条目插入的子条目（非组条目 → 忽略，TS warn 同型）。
    pub insert: Option<Vec<Entry>>,
    /// 组件名替换（`name` 变更 → 重建）。
    pub name: Option<String>,
    /// config 覆盖（`revision` 应随变更递增，同 config 纪律）。
    pub config: Option<Rc<dyn Any>>,
    /// revision 覆盖。
    pub revision: Option<u64>,
    /// disabled 覆盖。
    pub disabled: Option<bool>,
}

impl Patch {
    /// 顶层/组插入补丁（`id` = 目标组条目 id；None = 根层插入）。
    pub fn insert_into(id: Option<String>, children: Vec<Entry>) -> Self {
        Self {
            id,
            insert: Some(children),
            name: None,
            config: None,
            revision: None,
            disabled: None,
        }
    }

    /// 字段覆盖补丁。
    pub fn override_fields(id: impl Into<String>) -> Self {
        Self {
            id: Some(id.into()),
            insert: None,
            name: None,
            config: None,
            revision: None,
            disabled: None,
        }
    }
}

/// 对 desired 树应用补丁（纯变换；原树不动，返回新树）。
///
/// 递归匹配 `id`（组 children 内亦匹配）；`id` 未命中 → 忽略（TS warn
/// 同型）。`insert` 的目标必须是组条目（`is_group`），否则忽略。
pub fn apply_patches(entries: &[Entry], patches: &[Patch]) -> Vec<Entry> {
    entries.iter().map(|e| patch_entry(e, patches)).collect()
}

fn patch_entry(entry: &Entry, patches: &[Patch]) -> Entry {
    let mut out = entry.clone();
    for patch in patches {
        let id_match = patch.id.as_deref().is_some_and(|id| id == entry.id);
        if id_match {
            if let Some(name) = &patch.name {
                out.component = name.clone();
            }
            if let Some(config) = &patch.config {
                out.config = Rc::clone(config);
            }
            if let Some(revision) = patch.revision {
                out.revision = revision;
            }
            if let Some(disabled) = patch.disabled {
                out.disabled = disabled;
            }
            if let Some(children) = &patch.insert
                && out.is_group()
            {
                out.children.extend(children.iter().cloned());
            }
        }
        // 递归子条目（组内 patch 也作用于嵌套条目）。
        out.children = out
            .children
            .iter()
            .map(|c| patch_entry(c, patches))
            .collect();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Patch, apply_patches};
    use crate::Entry;
    use std::any::Any;
    use std::rc::Rc;

    fn entry(id: &str, component: &str, config: &str, revision: u64, disabled: bool) -> Entry {
        Entry::new(
            id,
            component,
            Rc::new(config.to_string()),
            revision,
            disabled,
        )
    }

    #[test]
    fn patch_overrides_fields_and_preserves_rest() {
        let tree = vec![
            entry("a", "comp", "v1", 1, false),
            entry("b", "comp", "v2", 1, false),
        ];
        let mut p = Patch::override_fields("a");
        p.config = Some(Rc::new("v9".to_string()) as Rc<dyn Any>);
        p.revision = Some(2);
        p.disabled = Some(true);
        let out = apply_patches(&tree, &[p]);
        assert_eq!(out.len(), 2, "树大小不变");
        assert_eq!(
            out[0].config.downcast_ref::<String>().unwrap(),
            "v9",
            "config 覆盖"
        );
        assert_eq!(out[0].revision, 2, "revision 覆盖");
        assert!(out[0].disabled, "disabled 覆盖");
        assert_eq!(
            out[1].config.downcast_ref::<String>().unwrap(),
            "v2",
            "未命中条目不动"
        );
    }

    #[test]
    fn patch_inserts_into_group_and_ignores_non_group() {
        let tree = vec![
            Entry::group("g", vec![entry("c1", "comp", "v", 1, false)]),
            entry("leaf", "comp", "v", 1, false),
        ];
        let out = apply_patches(
            &tree,
            &[Patch::insert_into(
                Some("g".into()),
                vec![entry("c2", "comp", "v2", 1, false)],
            )],
        );
        let g = &out[0];
        assert!(g.is_group(), "组保留");
        assert_eq!(g.children.len(), 2, "插入子条目");
        assert_eq!(g.children[1].id, "c2");

        // 非组目标 → 忽略（TS warn 同型）。
        let out2 = apply_patches(
            &tree,
            &[Patch::insert_into(
                Some("leaf".into()),
                vec![entry("c3", "comp", "v", 1, false)],
            )],
        );
        assert_eq!(out2[1].children.len(), 0, "非组条目 insert 忽略");
    }

    #[test]
    fn patch_matches_nested_ids_and_unknown_ids_ignored() {
        let tree = vec![Entry::group(
            "g",
            vec![entry("deep", "comp", "v", 1, false)],
        )];
        let mut p = Patch::override_fields("deep");
        p.name = Some("other".into());
        let out = apply_patches(&tree, &[p]);
        assert_eq!(out[0].children[0].component, "other", "嵌套 id 命中");

        let out2 = apply_patches(&tree, &[Patch::override_fields("ghost")]);
        assert_eq!(out2[0].children[0].component, "comp", "未知 id 忽略");
    }
}
