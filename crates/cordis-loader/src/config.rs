//! G5 配置插值（TS `interpolate`/`__jsExpr` 的 Rust 安全收窄，PR #32）。
//!
//! TS 实现（`/tmp/cordis-ts/packages/loader/src/config/utils.ts`）以
//! `new Function('ctx', expr)` + `with(ctx) eval` 求值任意 JS 表达式。
//! Rust 无 eval——收窄为**受控占位符替换**：`{{name}}`，`name` 由编排方
//! 提供的解析器解析（ctx 值 / 环境变量等）。
//!
//! **公开差异**：任意表达式求值不支持；未解析的占位符**保留原样**
//!（宽容，与 TS eval 对未定义变量 throw 不同）——调用方可自行决定
//! 严格性。TS 的"每次（重）加载时求值"语义由编排方承担（实例化前调用
//! 本函数）。

/// G7 配置协议（TS `Config` schema + `deepEqual` 参照，PR #33）：
/// **可选实现**（opt-in）——激活前校验 + 值级相等（免重建）。
///
/// **公开差异**：
/// - 校验失败：TS → fiber 失败态（可重试/复活）；Rust 侧配置校验失败 =
///   宿主配置 bug（panic，与 ProvisionClash/未知组件同型）——组件**运行时**
///   失败才走 L-Raise。
/// - `same` 默认 `false`（未实现 = 保守，走 revision 语义）：实现方须
///   承诺"同值 = 无重载需求"——**HMR 兼容纪律**：cordis-hmr 的 `reload`
///   以 revision 递增 + 复用旧 config 触发重建（组件版本变化），若 config
///   类型实现 `same` 且重载时同值会**免重建使 HMR 失效**——因此 `String`
///   等常用类型不实现 `same`（保持 revision 语义）；需要值级幂等的类型
///   自行实现并知悉上述纪律。
pub trait Config: Any {
    /// 校验（TS StandardSchema `validate` 参照）：`Err` = 拒绝激活。
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }

    /// 值级相等（TS `deepEqual` 参照）：revision 递增但同值 → 免重建。
    fn same(&self, other: &dyn Any) -> bool {
        let _ = other;
        false
    }
}

/// `&dyn Any` → `&dyn Config` 的 cast 函数（注册表值：`downcast` 到具体
/// 类型后 upcast——`Any::downcast_ref` 不接受 unsized 的 `dyn Config`，
/// 故需调用方按类型注册，见 [`crate::Loader::register_config`]）。
pub(crate) type ConfigCast = fn(&dyn Any) -> Option<&dyn Config>;

/// 校验配置（G7）：经注册表取回 [`Config`]，`validate` 返回 `Err` →
/// panic（配置错误 = bug，与 ProvisionClash/未知组件同型）。
/// **范围（REVIEW-1c86b5f nit-1）**：panic 中止**整个 apply**（协调期），
/// 与 TS 的单 fiber 失败态（其余条目继续）不同——公开差异；运行时组件
/// 失败仍走 L-Raise（fiber 级）。
/// 校验配置；失败返回 `Err(message)`（错误策略 v0.2：不 panic，由调用方
/// 报告 `ConfigValidation`、不挂载；每次 apply 重试）。
pub(crate) fn validate_config(
    casts: &std::collections::HashMap<std::any::TypeId, ConfigCast>,
    config: &dyn Any,
    _id: &str,
) -> Result<(), String> {
    if let Some(c) = casts.get(&config.type_id()).and_then(|f| f(config))
        && let Err(msg) = c.validate()
    {
        return Err(msg);
    }
    Ok(())
}

/// 值级相等（G7）：双方类型均注册且 `same` 为真 → 免重建；未注册/
/// 默认 `false` → 保守走 revision 语义。
pub(crate) fn configs_same(
    casts: &std::collections::HashMap<std::any::TypeId, ConfigCast>,
    a: &dyn Any,
    b: &dyn Any,
) -> bool {
    // REVIEW-1c86b5f nit-3：异类型直接短路（`same(b)` 依赖 b 与 self
    // 同型，`type_id` 不同则必不等——避免把异类型交给 `same`）。
    if a.type_id() != b.type_id() {
        return false;
    }
    match (
        casts.get(&a.type_id()).and_then(|f| f(a)),
        casts.get(&b.type_id()).and_then(|f| f(b)),
    ) {
        (Some(x), Some(_)) => x.same(b),
        _ => false,
    }
}

/// 注册 `C` 的 [`Config`] cast（G7）：启用该校验与值级 diff。
/// 调用方为每个实现 [`Config`] 的 config 类型注册一次。
pub fn register_config_cast<C: Config + 'static>(
    casts: &mut std::collections::HashMap<std::any::TypeId, ConfigCast>,
) {
    casts.insert(std::any::TypeId::of::<C>(), |any: &dyn Any| {
        any.downcast_ref::<C>().map(|c| c as &dyn Config)
    });
}

use std::any::Any;

/// 模板插值：把 `template` 中的 `{{name}}` 占位符替换为
/// `resolve(name)` 的返回值；未解析的占位符保留原样。
///
/// 语法：`{{` + name（不含 `}`、两侧空白会被 trim）+ `}}`。
/// 同名占位符可多次出现，各自求值。
pub fn interpolate(template: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            // 未闭合：保留原样。
            out.push_str(&rest[start..]);
            return out;
        };
        let name = after[..end].trim();
        match resolve(name) {
            Some(value) => out.push_str(&value),
            None => {
                // 未解析：保留 `{{name}}` 原样（宽容，公开差异）。
                out.push_str(&rest[start..start + 2 + end + 2]);
            }
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::interpolate;

    #[test]
    fn interpolates_known_placeholders() {
        let out = interpolate("postgres://{{host}}:{{port}}/db", &|name| match name {
            "host" => Some("127.0.0.1".into()),
            "port" => Some("5432".into()),
            _ => None,
        });
        assert_eq!(out, "postgres://127.0.0.1:5432/db", "同名/多占位符替换");
    }

    #[test]
    fn keeps_unresolved_and_unclosed_placeholders() {
        let out = interpolate("a={{x}} b={{y}}", &|_| None);
        assert_eq!(out, "a={{x}} b={{y}}", "未解析保留原样（宽容）");
        let out = interpolate("unclosed={{x", &|_| None);
        assert_eq!(out, "unclosed={{x", "未闭合保留原样");
    }

    #[test]
    fn trims_whitespace_and_handles_empty() {
        let out = interpolate("{{ x }}", &|name| {
            assert_eq!(name, "x", "占位符名 trim");
            Some("v".into())
        });
        assert_eq!(out, "v");
        assert_eq!(interpolate("", &|_| None), "", "空模板");
        assert_eq!(interpolate("no placeholders", &|_| None), "no placeholders");
    }
}
