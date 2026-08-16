//! PR #14 沙箱隔离（M1 门禁 2/3）+ M2-PR1 L-Raise 失败模型：guest 崩溃
//! 不伤宿主。
//!
//! - 恶意 guest（`wasm-plugin-rust-panic`）在 `task.step()` 中 panic
//!   （trap）——wasmtime 将 trap 转换为宿主侧 `Result` 错误；
//! - M2-PR1（L-Raise，处置⑤/⑧ 落地）：step 错误 / 越界 set / 绑定冲突
//!   不再以宿主 panic 传播——桥接层以 [`FiberError`] 载荷 raise，核心
//!   `reload` 捕获后记录为 fiber 失败 outcome（`Inactive(Some(ζ))`），
//!   已完成步骤恢复、`is_quiet` 的 ζ 析取成立；
//! - 宿主进程**存活**：后续可继续实例化其他组件、驱动正常 guest——
//!   沙箱隔离验证（论文 §6.3）。

use cordis_core::FiberState;
use cordis_core::fiber::FiberError;
use cordis_core::runtime::Runtime;
use cordis_core::symbol::Symbol;
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

/// 断言 fiber 处于失败终态且错误消息包含 `needle`；宿主存活可继续使用。
fn assert_failed(fiber: &cordis_core::Fiber, needle: &str, runtime: &Runtime, what: &str) {
    match &*fiber.state() {
        FiberState::Inactive(Some(err)) => {
            assert!(
                err.to_string().contains(needle),
                "{what}：错误消息应含 {needle:?}，实际：{err}"
            );
        }
        other => panic!("{what}：期望失败终态 Inactive(Some(ζ))，实际：{other:?}"),
    }
    // L-Raise 后 registry 无卡死 fiber：失败亦静止（Def 49 式 (45) ζ 析取）。
    assert!(runtime.is_quiet(), "{what}：失败 fiber 静止");
}

/// 宿主存活断言（两测试共用）：还能正常实例化并驱动正常组件。
fn host_survives(runtime: &Rc<Runtime>) {
    let root = runtime.context();
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
    fiber.retire();
    assert!(
        runtime.store().symbols().next().is_none(),
        "正常组件退役后绑定全清"
    );
    assert!(runtime.is_quiet(), "回到静止");
}

#[test]
fn guest_trap_becomes_error_outcome_and_host_survives() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();

    // 1. 恶意组件（step 时 panic）：L-Raise——trap 转为 fiber 失败 outcome，
    //    不再是宿主 panic。
    let boom = load_guest("wasm-plugin-rust-panic");
    let fiber = root
        .use_component(boom, Rc::new(()))
        .expect("use_component 不 panic（失败以 fiber 状态承载）");
    assert_failed(&fiber, "step 失败", &runtime, "guest trap → 失败 outcome");

    // 2. 宿主进程存活。
    host_survives(&runtime);
}

/// 走查记录⑧（PR #15）升级（M2-PR1）：恶意 guest 越界
/// `context::set` 写未声明供给键 → 核心 `set_dyn` 纪律违反——从"宿主
/// panic 依赖 catch_unwind 兜底"转为 **L-Raise 失败 outcome**（fiber
/// 进入失败态、宿主存活），处置⑤/⑧ 收官。
#[test]
fn guest_undeclared_set_becomes_error_outcome_and_host_survives() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();

    // 1. 违规组件（step 时写未声明键）：L-Raise——失败 outcome 而非宿主 panic。
    let misbehave = load_guest("wasm-plugin-rust-misbehave");
    let fiber = root
        .use_component(misbehave, Rc::new(()))
        .expect("use_component 不 panic（失败以 fiber 状态承载）");
    assert_failed(
        &fiber,
        "越界写入未声明的键",
        &runtime,
        "越界 set → 失败 outcome",
    );

    // 2. 宿主进程存活。
    host_survives(&runtime);
}

/// FiberError 载荷可被宿主侧显式识别（桥接 raise 与核心 catch 的约定）。
#[test]
fn fiber_error_is_a_typed_payload() {
    let err = FiberError::new("测试错误");
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| err.raise()));
    let payload = caught.expect_err("raise 以 panic 载荷抛出");
    let downcast = payload
        .downcast::<FiberError>()
        .expect("载荷类型为 FiberError");
    assert_eq!(downcast.to_string(), "测试错误");
}
