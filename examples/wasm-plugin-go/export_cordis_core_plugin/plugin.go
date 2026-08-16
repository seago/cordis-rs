// Cordis Go guest 示例（M1 验收，标准 go + wasip1）：**db 消费者**。
//
// 与 examples/wasm-plugin-rust-consumer 语义一致（Def 43 (d, p, e) 跨边界
// 形态）：注入 `db`，激活时经宿主 context::get 读取注入值，再提供
// `derived = "derived(<db>)"`。验证双语言 guest 在同一宿主上可互换
// （PR #14：Rust + Go 双语言验收）。
//
// 注意：本文件与生成的 wit_bindings.go 同属包 export_cordis_core_plugin——
// wit-bindgen 生成的资源胶水（资源新建/析构、句柄表）引用 `Component` /
// `Task` 类型，由本文件（应用侧）提供实现。字段名（handle / pinner）与
// 方法是生成代码的既定契约，不可改名。

package export_cordis_core_plugin

import (
	"runtime"

	witTypes "go.bytecodealliance.org/pkg/wit/types"

	"wit_component/cordis_core_context"
	"wit_component/cordis_core_plugin"
)

// Component 是 cordis:core/plugin 的 component 资源实现。
type Component struct {
	handle int32
	pinner runtime.Pinner
}

// MakeComponent 对应 wit 的 [constructor]component。
func MakeComponent() *Component {
	return &Component{}
}

// Inject 声明本组件消费的键（Def 43 的 d，共效应）。
func (c *Component) Inject() []string {
	return []string{"db"}
}

// Provide 声明本组件供应的键（Def 43 的 p，效应）。
func (c *Component) Provide() []string {
	return []string{"derived"}
}

// Start 创建效应迭代器（Def 51 𝔈iter 跨边界形态：一次 step 读取 → 提供）。
func (c *Component) Start() *Task {
	return &Task{}
}

// OnDrop 由生成代码在资源析构时调用。
func (c *Component) OnDrop() {}

// Task 是 cordis:core/plugin 的 task 资源实现。
type Task struct {
	handle int32
	pinner runtime.Pinner
	done   bool
}

// Step 读取注入的 db 并提供 derived（宿主在每次 step 前已同步镜像）。
func (t *Task) Step() witTypes.Option[cordis_core_plugin.EffectStep] {
	if t.done {
		return witTypes.None[cordis_core_plugin.EffectStep]()
	}
	t.done = true

	got := cordis_core_context.Get("db")
	if !got.IsSome() {
		return witTypes.None[cordis_core_plugin.EffectStep]()
	}
	db := got.Some().Text()

	derived := "derived(" + db + ")"

	res := cordis_core_context.Set("derived", cordis_core_context.MakeValueText(derived))
	if res.IsErr() {
		return witTypes.None[cordis_core_plugin.EffectStep]()
	}
	return witTypes.Some(cordis_core_plugin.EffectStep{Inverse: res.Ok(), Done: true})
}

// OnDrop 由生成代码在资源析构时调用。
func (t *Task) OnDrop() {}
