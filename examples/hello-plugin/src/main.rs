//! hello-plugin：原生组件的端到端示例（PLAN M0 验收：server + auth，
//! 运行时卸载与重连）。
//!
//! 演示：
//! 1. `#[component]` 宏：声明式依赖（`inject`）与供给（`provide`），
//!    用户只实现 `apply_impl`（效应函数 `e`）；
//! 2. 依赖激活顺序：`auth` 在 `server` 激活后才激活（§3.2.2 前半）；
//! 3. 运行时卸载：退役 `server` → `auth` 级联停用、绑定恢复（Thm 63）；
//! 4. 重连：移除旧 `server`（释放供给名）→ 新 `server` 激活 →
//!    `auth` 自动重连到新实例。
//!
//! 运行：`cargo run -p hello-plugin`（全部断言通过即成功）。

use cordis::{Context, EffectIter, Fiber, FiberState, Key, Runtime, component};
use std::any::Any;
use std::rc::Rc;

/// server 服务（共效应值）。
#[derive(Clone, Debug)]
struct Server {
    name: String,
    port: u16,
}

/// 键：server 服务。
struct ServerKey;
impl Key for ServerKey {
    type Value = Server;
    const SYMBOL: &'static str = "server";
}

/// 键：auth 结果。
struct AuthKey;
impl Key for AuthKey {
    type Value = String;
    const SYMBOL: &'static str = "auth";
}

/// server 组件：无依赖，提供 `server`。
#[component(inject = [], provide = [ServerKey])]
struct ServerPlugin {
    name: String,
    port: u16,
}

impl ServerPlugin {
    fn apply_impl(&self, ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
        let server = Server {
            name: self.name.clone(),
            port: self.port,
        };
        Box::new(cordis_native::with_ctx(ctx, move |ctx| {
            ctx.set::<ServerKey>(server).expect("绑定 server")
        }))
    }
}

/// auth 组件：注入 `server`，读取后绑定 `auth`。
#[component(inject = [ServerKey], provide = [AuthKey])]
struct AuthPlugin;

impl AuthPlugin {
    fn apply_impl(&self, ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
        Box::new(cordis_native::with_ctx(ctx, |ctx| {
            // 读取依赖（借用须在 set 前释放）。
            let label = {
                let server = ctx.get::<ServerKey>().expect("注入的 server 可用");
                format!("auth@{}:{}", server.name, server.port)
            };
            ctx.set::<AuthKey>(label).expect("绑定 auth")
        }))
    }
}

fn assert_active(fiber: &Rc<Fiber>, what: &str) {
    assert!(
        matches!(&*fiber.state(), FiberState::Active { .. }),
        "{what} 应处于 Active"
    );
}

fn assert_inactive(fiber: &Rc<Fiber>, what: &str) {
    assert!(
        matches!(&*fiber.state(), FiberState::Inactive(_)),
        "{what} 应处于 Inactive"
    );
}

fn main() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();

    // 1. 加载 server（无依赖 → 立即激活）。
    println!("> 加载 server...");
    let server = root
        .use_component(
            Rc::new(ServerPlugin {
                name: "primary".into(),
                port: 8080,
            }),
            Box::new(()),
        )
        .expect("实例化 server");
    assert_active(&server, "server");
    println!("  server 激活（提供 server 键）");

    // 2. 加载 auth（依赖 server 已满足 → 立即激活）。
    println!("> 加载 auth...");
    let auth = root
        .use_component(Rc::new(AuthPlugin), Box::new(()))
        .expect("实例化 auth");
    assert_active(&auth, "auth");
    assert_eq!(
        root.get::<AuthKey>().unwrap().as_str(),
        "auth@primary:8080",
        "auth 读取到注入的 server"
    );
    println!("  auth 激活（注入 server@primary:8080）");

    // 3. 运行时卸载：退役 server → auth 级联停用、绑定恢复。
    println!("> 退役 server...");
    server.retire();
    assert_inactive(&server, "server");
    assert_inactive(&auth, "auth");
    assert!(root.get::<AuthKey>().is_err(), "auth 绑定已恢复");
    println!("  server 卸载 → auth 级联停用（Thm 63 顺序）");

    // 4. 重连：移除旧 server（释放供给名）→ 新 server 激活 → auth 重连。
    println!("> 移除旧 server 并加载新 server...");
    runtime.remove_fiber(server.id()).expect("移除旧 server");
    let server2 = root
        .use_component(
            Rc::new(ServerPlugin {
                name: "secondary".into(),
                port: 9090,
            }),
            Box::new(()),
        )
        .expect("实例化新 server");
    assert_active(&server2, "server（重连）");
    assert_active(&auth, "auth（重连）");
    assert_eq!(
        root.get::<AuthKey>().unwrap().as_str(),
        "auth@secondary:9090",
        "auth 自动重连到新 server"
    );
    println!("  auth 自动重连到 server@secondary:9090");

    println!("✓ hello-plugin：全部断言通过（激活 → 级联卸载 → 重连）");
}
