//! PR #14 沙箱隔离（M1 门禁 2/3）：guest 崩溃不伤宿主。
//!
//! - 恶意 guest（`wasm-plugin-rust-panic`）在 `task.step()` 中 panic
//!   （trap）——wasmtime 将 trap 转换为宿主侧 `Result` 错误；
//! - 宿主捕获错误后**进程存活**：后续可继续实例化其他组件、注册
//!   新 fiber、驱动正常 guest——沙箱隔离验证（论文 §6.3）。

use cordis_core::runtime::Runtime;
use cordis_core::symbol::Symbol;
use cordis_core::{FiberState};
use cordis_wasm::WasmComponent;
use std::rc::Rc;

fn guest_wasm(example: &str) -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest).join(format!(
        "../../examples/{example}/target/wasm32-wasip2/debug/{}.wasm",
        example.replace('-', "_")
    ))
}

fn load_guest(example: &str) -> Rc<WasmComponent> {
    let engine = wasmtime::Engine::default();
    let bytes =
        std::fs::read(guest_wasm(example)).expect("先构建 guest（CI 步骤 build wasm guest）");
    WasmComponent::load(&engine, &bytes).expect("组件加载")
}

#[test]
fn guest_trap_is_caught_and_host_survives() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();

    // 1. 恶意组件（step 时 panic）注册：激活时 reload 驱动 step → trap。
    let boom = load_guest("wasm-plugin-rust-panic");
    // trap 发生在 use_component（同步 reload）内——wasmtime 抛 Trap。
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        root.use_component(boom, Rc::new(())).map(|_| ())
    }));
    assert!(result.is_err(), "trap 以 panic（Trap）形式从宿主侧可见");

    // 2. 宿主进程存活：还能正常实例化并驱动正常组件。
    let provider = load_guest("wasm-plugin-rust");
    let fiber = root
        .use_component(provider, Rc::new(()))
        .expect("宿主存活：正常组件可实例化");
    assert!(
        matches!(&*fiber.state(), FiberState::Active { .. }),
        "正常组件激活"
    );
    assert!(
        runtime.store().contains(Symbol::intern("db")),
        "正常组件绑定生效"
    );

    // 3. 正常组件退役清理。
    fiber.retire();
    assert!(
        runtime.store().symbols().next().is_none(),
        "正常组件退役后绑定全清"
    );
    // 注：trap 组件在 registry 留有未完成注册的 fiber（卡在激活转换中）——
    // L-Raise（处置清单⑤）未落地的已知边界（THEORY-MAP PR #14 行），
    // `is_quiet` 因此为 false；该边界随失败模型实现后处置。
}
