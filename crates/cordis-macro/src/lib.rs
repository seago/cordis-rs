//! Cordis 过程宏 DX 层（PLAN §4.3）。
//!
//! [`component`]：把结构体声明为组件（Def 43），从属性参数生成
//! `Component::inject` / `Component::provide`，并把 `apply` 委托给用户的
//! `apply_impl` 方法——声明部分（`d`、`p`）由宏生成，效应函数（`e`）
//! 保持命令式实现。
//!
//! 生成的代码引用 `::cordis` 路径：使用本宏的 crate 需依赖
//! `cordis` 门面 crate（其 re-export 了全部所需类型）。

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Error, ItemStruct, Token, Type};

/// `#[component(inject = [K1, K2], provide = [K3])]` 的参数。
///
/// - `inject`：依赖键类型列表（`d`，Def 43）——每个类型须实现 [`Key`]；
/// - `provide`：供给键类型列表（`p`，Def 43）。
struct ComponentArgs {
    inject: Vec<Type>,
    provide: Vec<Type>,
}

impl Parse for ComponentArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut inject = Vec::new();
        let mut provide = Vec::new();
        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let content;
            syn::bracketed!(content in input);
            let mut types = Vec::new();
            while !content.is_empty() {
                types.push(content.parse::<Type>()?);
                if content.is_empty() {
                    break;
                }
                content.parse::<Token![,]>()?;
            }
            match ident.to_string().as_str() {
                "inject" => inject = types,
                "provide" => provide = types,
                other => {
                    return Err(Error::new(
                        ident.span(),
                        format!("未知参数 `{other}`（支持 `inject` / `provide`）"),
                    ));
                }
            }
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        Ok(ComponentArgs { inject, provide })
    }
}

/// 组件声明宏（Def 43 的 `(d, p, e)` 之声明部分）。
///
/// ```ignore
/// #[component(inject = [DbKey], provide = [DbKey])]
/// struct DatabasePlugin { url: String }
///
/// impl DatabasePlugin {
///     fn apply_impl(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter> {
///         // 效应函数（e）：经 ctx.set / ctx.effect 执行
///     }
/// }
/// ```
///
/// 生成：
/// - `inject()`：`[DbKey::SYMBOL, ...]` 的驻留键集合；
/// - `provide()`：同上的供给键集合；
/// - `apply()`：委托 `self.apply_impl(ctx, config)`（用户必须实现）。
///
/// 用户实现 `apply_impl` 时的义务（与手写 `Component` 相同）：只写入
/// `provide` 声明的键（Def 43/48 纪律，运行时检查）、迭代器有限终止。
#[proc_macro_attribute]
pub fn component(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = syn::parse_macro_input!(args as ComponentArgs);
    let input = syn::parse_macro_input!(item as ItemStruct);
    match expand(args, input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand(args: ComponentArgs, input: ItemStruct) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let inject = &args.inject;
    let provide = &args.provide;

    Ok(quote! {
        #input

        impl #impl_generics ::cordis::Component for #name #ty_generics #where_clause {
            fn inject(&self) -> ::cordis::KeySet {
                [
                    #(::cordis::Symbol::intern(<#inject as ::cordis::Key>::SYMBOL)),*
                ]
                .into_iter()
                .collect()
            }

            fn provide(&self) -> ::cordis::KeySet {
                [
                    #(::cordis::Symbol::intern(<#provide as ::cordis::Key>::SYMBOL)),*
                ]
                .into_iter()
                .collect()
            }

            fn apply(
                &self,
                ctx: ::std::rc::Rc<::cordis::Context>,
                config: &dyn ::std::any::Any,
            ) -> ::std::boxed::Box<dyn ::cordis::EffectIter> {
                self.apply_impl(ctx, config)
            }
        }
    })
}
