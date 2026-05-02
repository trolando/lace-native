# Developer's Guide

This guide covers how to write parallel programs with `lace-native`.
It assumes you have completed the [Quick Start](README.md#quick-start)
and have a working `build.rs`, `tasks.def`, and `Cargo.toml`.

## How Lace works

Lace is a **work-stealing** framework. When you call `lace_native::start()`,
it creates a pool of worker threads. Each worker owns a double-ended queue
(deque) of tasks. When you **spawn** a task, it is pushed onto the current
worker's deque. If another worker runs out of work, it **steals** tasks from
the other end of your deque.

This design is optimised for recursive divide-and-conquer algorithms:
spawning is cheap (a local push), and stealing only happens when there is
contention — which means parallelism scales naturally with the available work.

## Defining tasks

Tasks are declared in `tasks.def` and implemented as ordinary Rust functions.
Every task function receives `&Worker` as its first parameter:

```
# tasks.def
c {
    #include "lace.h"
}

task fib(n: i32) -> i32
```

```rust
// src/main.rs
include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

fn fib(w: &Worker, n: i32) -> i32 {
    if n < 2 { return n; }
    let guard = fib_spawn(w, n - 1);  // push onto deque
    let a = fib(w, n - 2);            // do work while waiting
    let b = guard.sync(w);            // retrieve spawned result
    a + b
}
```

The `Worker` parameter is threaded through automatically by the generated
trampolines. You never construct a `Worker` yourself.

## The spawn/sync pattern

The fundamental pattern in Lace is:

```rust
// 1. Spawn: push a task onto the deque (may be stolen)
let guard = task_spawn(w, args...);

// 2. Work: do something useful while the spawned task might run in parallel
let local_result = do_work(w);

// 3. Sync: retrieve the result (waits if stolen, executes locally if not)
let spawned_result = guard.sync(w);
```

**Spawning is cheap.** In the common case (no one steals), spawn+sync is
just a local push and pop — a few nanoseconds. The overhead only increases
when work is actually stolen, which is exactly when parallelism is useful.

**LIFO order.** Sync always retrieves the **most recently spawned** task.
If you spawn A then B, you must sync B before A. This is enforced by the
deque structure.

## Guard style vs C style

For tasks with only value parameters (like `fib`), you have two equivalent
options:

```rust
// Guard style — the guard tracks the spawn/sync obligation
let guard = fib_spawn(w, n - 1);
let a = fib(w, n - 2);
let b = guard.sync(w);

// C style — standalone functions, closer to the C API
let _ = fib_spawn(w, n - 1);
let a = fib(w, n - 2);
let b = fib_sync(w);
```

Both compile to identical code. Use whichever reads better.

**Guards become essential for reference parameters.** When a task takes
`&mut T`, the guard holds the mutable borrow, preventing you from
using `T` between spawn and sync — the Rust compiler enforces this:

```rust
// This won't compile — guard holds &mut tree
let guard = tree.insert_spawn(w, key, value);
tree.lookup(key);       // ERROR: tree is mutably borrowed
guard.sync(w);          // borrow released here
```

**C-style sync is essential for multi-spawn loops.** When you spawn N
tasks in a loop and sync them all afterward (like N-Queens), guards
would be awkward. Use the standalone sync:

```rust
for k in 0..n {
    let _ = nqueens_spawn(w, board.as_ptr(), n, depth, k);
}
let mut total = 0;
for _ in 0..n {
    total += nqueens_sync(w);  // syncs in LIFO order
}
```

## Parameter types

Tasks can accept:

| Rust type | C mapping | Notes |
|-----------|-----------|-------|
| `i8`..`i64`, `u8`..`u64` | `int8_t`..`uint64_t` | Value types, passed directly |
| `usize`, `isize` | `size_t`, `ptrdiff_t` | |
| `bool` | `_Bool` | |
| `f32`, `f64` | `float`, `double` | |
| `&T` | `void*` | Shared reference — guard holds borrow |
| `&mut T` | `void*` | Exclusive reference — guard holds borrow |
| `*const T`, `*mut T` | `void*` | Raw pointers — no borrow tracking |
| Custom value types | Declared in `types { }` | e.g. `BDD = uint64_t` |

**Important:** reference parameters (`&T`, `&mut T`) must remain valid
until the corresponding sync. The guard enforces this for the borrow
checker. For raw pointers, you are responsible for lifetime correctness
yourself — just as in C.

## Method tasks

Tasks can be defined as methods on a type using `impl` blocks in `tasks.def`:

```
impl NodeTable {
    task lookup(&self, hash: u64) -> u64
    task insert(&mut self, key: u64, value: u64)
}
```

The user implements these as normal Rust methods with an added `&Worker`
parameter:

```rust
impl NodeTable {
    fn lookup(&self, w: &Worker, hash: u64) -> u64 {
        // ...
    }

    fn insert(&mut self, w: &Worker, key: u64, value: u64) {
        // ...
    }
}
```

Generated spawn/sync methods are added to the type:

```rust
// &self method — multiple tasks can share concurrent access
let guard = table.lookup_spawn(w, hash);
// ... do other work ...
let result = guard.sync(w);

// &mut self method — exclusive access enforced by borrow checker
let guard = table.insert_spawn(w, key, value);
// table cannot be used here — guard holds &mut borrow
guard.sync(w);
// table is usable again
```

**Shared `&self` methods** are the right choice for concurrent data structures
(lock-free hash tables, node tables with interior mutability). Multiple
workers can call `lookup_spawn` on the same table simultaneously.

**Exclusive `&mut self` methods** are for operations that need sole access.
The borrow checker prevents concurrent use at compile time — a safety
guarantee that C Lace does not provide.

## Running tasks from external threads

The `_run` function works from any thread — Lace worker or not:

```rust
fn main() {
    lace_native::start(0, 0, 0);

    // fib_run works here because main() is not a Lace worker
    let result = fib_run(42);

    lace_native::stop();
}
```

Internally, `_run` detects whether the caller is a Lace worker. If yes,
it calls the task directly. If not, it submits the task to the worker pool
and blocks until completion. This means library code can use `_run` without
knowing its calling context.

## Interrupting workers: TOGETHER and NEWFRAME

Two special operations interrupt all workers at the next synchronisation point:

**`_together(args...)`** runs a copy of the task on every worker simultaneously.
All workers start together (barrier semantics). Use this for per-worker
initialisation:

```rust
// Every worker initialises its own thread-local cache
init_cache_together();
lace_native::barrier();  // wait for all workers to finish
```

**`_newframe(args...)`** suspends the current work and runs the given task
across the worker pool. The previous work frame is resumed after completion.
Use this for stop-the-world operations like garbage collection:

```rust
// Interrupt all workers, run GC, then resume
gc_collect_newframe();
```

**Cooperative scheduling.** These operations take effect at the next `_sync`
call or when a worker is idle. Long-running tasks without sync points should
call `w.yield_now()` periodically to check for interrupts.

## Task granularity

Spawning a task has a small but nonzero cost (writing to the deque, memory
fence). For very fine-grained work, add a **sequential cutoff**:

```rust
fn fib(w: &Worker, n: i32) -> i32 {
    if n < 20 {
        return fib_seq(n);  // sequential below threshold
    }
    let guard = fib_spawn(w, n - 1);
    let a = fib(w, n - 2);
    guard.sync(w) + a
}

fn fib_seq(n: i32) -> i32 {
    if n < 2 { return n; }
    fib_seq(n - 1) + fib_seq(n - 2)
}
```

The right cutoff depends on the cost per task. As a rule of thumb, if a
task does less than ~1 microsecond of work, it is too fine-grained to
benefit from spawning.

## Pitfall: dangling pointers in spawn loops

When passing raw pointers to spawned tasks, ensure the data stays alive
until after all corresponding syncs. This is a common mistake:

```rust
// WRONG — child_state is dropped at end of each iteration,
// but the spawned task holds a pointer to it!
for i in 0..n {
    let child_state = compute_child(i);
    let _ = task_spawn(w, child_state.as_ptr());
}
for _ in 0..n {
    total += task_sync(w);  // dangling pointer!
}
```

The fix is to keep all data alive until after all syncs:

```rust
// CORRECT — all child states live until after all syncs
let children: Vec<State> = (0..n)
    .map(|i| compute_child(i))
    .collect();

for cs in &children {
    let _ = task_spawn(w, cs.as_ptr());
}
for _ in 0..n {
    total += task_sync(w);  // children is still alive
}
```

This is the same issue you'd encounter in C with stack-allocated temporaries
and spawned tasks. Reference parameters (`&T`, `&mut T`) are protected by
the guard's borrow tracking, but raw pointers (`*const T`, `*mut T`) are
your responsibility — the framework cannot track their lifetimes.

## Lifecycle and multiple start/stop cycles

Lace supports starting and stopping multiple times in the same process:

```rust
lace_native::start(4, 0, 0);
let a = fib_run(42);
lace_native::stop();

// ... later ...

lace_native::start(2, 0, 0);  // different worker count is fine
let b = fib_run(42);
lace_native::stop();
```

This is useful for applications that need parallelism only during certain
phases, or for testing with different worker counts.

## Custom value types

When your tasks use domain-specific types that are passed by value (not by
reference), declare them in the `types { }` block:

```
types {
    BDD = uint64_t
    MTBDD = uint64_t
    NodeIndex = uint32_t
}
```

The left side is the Rust type name (used in task signatures and user code).
The right side is the C type used in the generated wrappers. These must be
ABI-compatible — typically both are the same fixed-width integer.

## C preamble: including headers

The `c { }` block is copied verbatim into the generated C file. Use it to
include headers that define types referenced in the TASK macro expansion:

```
c {
    #include "lace.h"
    #include "sylvan.h"
    #include "my_types.h"
}
```

Lines inside `c { }` are **not** treated as comments, so `#include` works
as expected — the `#` is not a comment character here.

## Rust preamble: importing types

The `rust { }` block is copied into the generated Rust file. Use it to
bring types into scope for the generated code:

```
rust {
    use crate::bdd::BDD;
    use crate::table::NodeTable;
}
```

## Using LACE_DIR for development

By default, `lace-native` builds against the vendored Lace source files in
`lace-native/vendor/`. To test against a local Lace checkout:

```bash
export LACE_DIR=/path/to/lace
cargo build
```

Both `lace-native` and `lace-native-build` respect this variable. The
vendored sources are ignored when `LACE_DIR` is set.

## Feature flags

Set features on the `lace-native` crate. They control the C compilation
and propagate to `lace-native-build` automatically:

```toml
[dependencies]
lace-native = { version = "0.1", features = ["stats"] }
```

| Feature | Default | Effect |
|---------|---------|--------|
| `backoff` | yes | Idle workers sleep progressively (saves CPU) |
| `hwloc` | no | Pin workers to cores (requires `libhwloc-dev`) |
| `stats` | no | Print steal/task/split counters on `stop()` |

## Cross-language LTO

By default, each task spawn and sync crosses the Rust→C FFI boundary
through a non-inline wrapper function. This adds a small overhead per
call (typically one extra function call frame). For workloads with
very fine-grained tasks, this overhead can become visible.

**Cross-language LTO** (Link-Time Optimization) allows the compiler to
inline the C deque operations through the FFI boundary into the Rust
call sites, potentially eliminating the wrapper overhead entirely.

### Requirements

Cross-language LTO requires all code (Rust and C) to be compiled to
LLVM bitcode and linked with an LLVM-based linker:

- **Clang** as the C compiler (not GCC)
- **lld** or **mold** as the linker (not GNU ld)
- **Rust nightly** or stable with `-C linker-plugin-lto`

### Setup

Create a `.cargo/config.toml` in your project root:

```toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "linker-plugin-lto", "-C", "link-arg=-fuse-ld=lld"]

[target.aarch64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "linker-plugin-lto", "-C", "link-arg=-fuse-ld=lld"]

[target.aarch64-apple-darwin]
linker = "clang"
rustflags = ["-C", "linker-plugin-lto"]
# macOS uses Apple's linker by default, which supports LTO natively.
```

Set the C compiler for the `cc` crate:

```bash
export CC=clang
cargo build --release
```

### How it works

With cross-language LTO enabled:

1. `lace-native` compiles `lace.c` with Clang and `-flto=thin`,
   producing LLVM bitcode instead of machine code.
2. `lace-native-build` compiles the generated `lace_tasks.c` wrappers
   the same way.
3. `rustc` produces LLVM bitcode for the Rust code.
4. The LLVM linker sees all bitcode together and can inline across
   the C/Rust boundary.

In particular, the `fib_SPAWN_w` wrapper (which just calls the
`fib_SPAWN` inline function) can be inlined into the Rust `fib_spawn`
call site, eliminating the FFI call overhead entirely.

### Verifying LTO is active

Build with verbose output and check for bitcode flags:

```bash
CC=clang cargo build --release -vv 2>&1 | grep -E "flto|linker-plugin"
```

You should see `-flto=thin` in the C compilation and
`-Clinker-plugin-lto` in the Rust compilation.

### When to use it

For most workloads, the FFI overhead is negligible compared to the
actual task computation. Cross-language LTO is most beneficial for:

- Micro-benchmarks measuring raw spawn/sync overhead
- Algorithms with extremely fine-grained tasks (sub-microsecond)
- Comparisons with native C Lace performance

For typical workloads with reasonable task granularity (>1 µs per task),
standard compilation is sufficient.
