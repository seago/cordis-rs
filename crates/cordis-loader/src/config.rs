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
