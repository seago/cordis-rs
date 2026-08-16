package runtime

import (
	"fmt"
	"runtime"
	"unsafe"
)

type Handle struct {
	value int32
}

func (h *Handle) Use() int32 {
	if h.value == 0 {
		panic("nil handle")
	}
	return h.value
}

func (h *Handle) Take() int32 {
	if h.value == 0 {
		panic("nil handle")
	}
	value := h.value
	h.value = 0
	return value
}

func (h *Handle) Set(value int32) {
	if value == 0 {
		panic("nil handle")
	}
	if h.value != 0 {
		panic("handle already set")
	}
	h.value = value
}

func (h *Handle) TakeOrNil() int32 {
	value := h.value
	h.value = 0
	return value
}

// TakeHandle 返回并清零句柄；wit-bindgen 0.60 生成代码所需（上游 pkg v0.2.3
// 尚无此方法，本 fork 补上；语义与 Take 相同）。
func (h *Handle) TakeHandle() int32 {
	return h.Take()
}

func MakeHandle(value int32) *Handle {
	if value == 0 {
		panic("nil handle")
	}
	return &Handle{value}
}

func Allocate(pinner *runtime.Pinner, size, align uintptr) unsafe.Pointer {
	pointer := allocateRaw(size, align)
	pinner.Pin(pointer)
	return pointer
}

func allocateRaw(size, align uintptr) unsafe.Pointer {
	if size == 0 {
		return nil
	}

	if size%align != 0 {
		panic(fmt.Sprintf("size %v is not compatible with alignment %v", size, align))
	}

	switch align {
	case 1:
		return unsafe.Pointer(unsafe.SliceData(make([]uint8, size)))
	case 2:
		return unsafe.Pointer(unsafe.SliceData(make([]uint16, size/align)))
	case 4:
		return unsafe.Pointer(unsafe.SliceData(make([]uint32, size/align)))
	case 8:
		return unsafe.Pointer(unsafe.SliceData(make([]uint64, size/align)))
	default:
		panic(fmt.Sprintf("unsupported alignment: %v", align))
	}
}

// NB: 上游用 `runtime.sbrk`（tinygo 专有符号）处理**运行时初始化前**的
// `cabi_realloc` 调用。标准 go（wasip1）无该符号，本 fork 用静态缓冲 +
// bump 指针实现同一语义（`preinitAlloc`）：
//
// 适配器（wasi_snapshot_preview1.reactor.wasm）在首次被调用时（此时 Go
// 运行时仍在 schedinit 阶段、包 init 尚未执行）会经 `allocate_stack` 调用
// 主模块的 `cabi_realloc` 分配影子栈（64KB），随后 `State::new` 再分配
// 64KB 的 State。此窗口内**不能**用 GC（`make` 需要已初始化的堆），也
// **不能**回调适配器（`adapter_monotonic_clock_set_paused` 需要已分配的
// State——先有鸡还是先有蛋），故必须走不依赖运行时的纯线性内存分配。
// `useGCAllocations` 在包 init 时置位，此后恒走 GC 路径（见 cabiRealloc）。

//nolint:unused
var useGCAllocations = false

func init() {
	useGCAllocations = true
}

// preinitHeap：预初始化窗口的分配区。需求：影子栈 64KB + State 64KB +
// 余量；512KB 足够（超限返回 nil → 适配器内存越界 trap，属不可达错误路径）。
const preinitHeapSize = 512 << 10

var preinitHeap [preinitHeapSize]byte
var preinitUsed uintptr

// preinitAlloc 在静态缓冲上做 bump 分配；不触碰 GC 与任何导入。
//
// go:nosplit：预初始化窗口内避免 morestack 机制（栈检查需要运行时）。
//
//go:nosplit
func preinitAlloc(size, align uintptr) unsafe.Pointer {
	if size == 0 {
		return nil
	}
	off := (preinitUsed + align - 1) &^ (align - 1)
	if off+size > preinitHeapSize {
		return nil
	}
	preinitUsed = off + size
	return unsafe.Add(unsafe.Pointer(&preinitHeap[0]), off)
}

//nolint:unused
func offset(ptr, align uintptr) uintptr {
	newptr := (ptr + align - 1) &^ (align - 1)
	return newptr - ptr
}

var pinner = runtime.Pinner{}

func Unpin() {
	pinner.Unpin()
}

//nolint:unused
//go:wasmimport wasi_snapshot_preview1 adapter_monotonic_clock_set_paused
func adapterMonotonicClockSetPaused(paused bool)

//nolint:unused
//go:wasmexport cabi_realloc
func cabiRealloc(oldPointer unsafe.Pointer, oldSize, align, newSize uintptr) unsafe.Pointer {
	if oldPointer != nil || oldSize != 0 {
		panic("todo")
	}

	if !useGCAllocations {
		// 运行时初始化前：纯 bump 分配，不回调适配器。
		return preinitAlloc(newSize, align)
	}

	adapterMonotonicClockSetPaused(true)
	pointer := Allocate(&pinner, newSize, align)
	adapterMonotonicClockSetPaused(false)
	return pointer
}
