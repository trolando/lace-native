# lace-native

[![CI](https://github.com/trolando/lace-native/actions/workflows/ci.yml/badge.svg)](https://github.com/trolando/lace-native/actions/workflows/ci.yml)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

Rust bindings for the native C implementation of [Lace](https://github.com/trolando/lace), a work-stealing framework for fine-grained fork-join parallelism on multi-core computers.

`lace-native` is not a pure Rust reimplementation of Lace. It builds and links the C Lace runtime, then provides Rust-side lifecycle functions, worker access, and generated task wrappers. Tasks are described in a small `tasks.def` file; at build time, `lace-native-build` generates Rust wrappers and C trampolines that connect Rust task implementations to the Lace runtime.

## Crates

| Crate | Purpose |
|---|---|
| `lace-native` | Runtime crate: builds or links Lace, exposes `Worker`, lifecycle functions, and runtime feature flags. |
| `lace-native-build` | Build-time code generator for `tasks.def` files. Used from `build.rs`. |

## Features

- Rust API for Lace's C work-stealing runtime.
- Build-time generation of Rust task wrappers and `extern "C"` trampolines.
- Guard-based spawn/sync API for borrow-checked task joins.
- Support for free functions and methods in task definitions.
- Optional Lace runtime features: idle-worker backoff, statistics, and `hwloc` pinning.
- Can build against vendored Lace sources or a local Lace checkout via `LACE_DIR`.

## Platform support

The CI configuration tests the workspace on:

- Linux
- macOS
- Windows with MSVC

The C backend is compiled as C11. On MSVC, the build uses the compiler flags required by Lace for C11 atomics and the conforming preprocessor.

## Quick start

Until the crates are published, depend on the repository directly:

```toml
[dependencies]
lace-native = { git = "https://github.com/trolando/lace-native" }

[build-dependencies]
lace-native-build = { git = "https://github.com/trolando/lace-native" }
```

After publication, this can become:

```toml
[dependencies]
lace-native = "0.1"

[build-dependencies]
lace-native-build = "0.1"
```

Create a `build.rs` file:

```rust
fn main() {
    lace_native_build::process("tasks.def").compile();
}
```

Create `tasks.def`:

```c
c {
    #include "lace.h"
}

task fib(n: i32) -> i32
```

Write `src/main.rs`:

```rust
use lace_native::Worker;

include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

fn fib(w: &Worker, n: i32) -> i32 {
    if n < 2 {
        return n;
    }

    let guard = fib_spawn(w, n - 1);
    let a = fib(w, n - 2);
    guard.sync(w) + a
}

fn main() {
    // 0 = auto-detect workers, default deque size, default stack size.
    lace_native::start(0, 0, 0);
    println!("fib(42) = {}", fib_run(42));
    lace_native::stop();
}
```

## Building and running the examples

```bash
# Build the workspace.
cargo build --release

# Run Fibonacci; default is fib(42) with auto-detected worker count.
cargo run --release -p lace-example-fib

# Run Fibonacci with four workers.
cargo run --release -p lace-example-fib -- -w 4 42

# Run N-Queens; default is n = 14.
cargo run --release -p lace-example-nqueens -- -w 4 14

# Run the scaling benchmark (fib + nqueens across 1..N workers).
cargo run --release -p lace-example-bench
```

The examples accept:

| Option | Meaning |
|---|---|
| `-w <n>` | Number of worker threads. `0` means auto-detect. |
| `-q <n>` | Task deque size per worker. `0` means use Lace's default. |

## Runtime feature flags

Feature flags are set on `lace-native`. They affect how the C Lace runtime and generated task wrappers are compiled, so the runtime crate and build crate must use the same configuration.

| Feature | Default | Description |
|---|---:|---|
| `backoff` | yes | Let idle workers sleep instead of continuously busy-waiting. |
| `hwloc` | no | Pin workers to CPU cores using `hwloc`. Requires the system `hwloc` library. |
| `stats` | no | Enable Lace runtime statistics, printed by `lace_stop()`. |

Examples:

```bash
# Build with statistics enabled.
cargo build --release --features lace-native/stats

# Build with hwloc pinning enabled.
cargo build --release --features lace-native/hwloc

# Build without the default backoff feature.
cargo build --release --no-default-features

# Combine features.
cargo build --release --features lace-native/stats,lace-native/hwloc
```

In `Cargo.toml`:

```toml
[dependencies]
lace-native = { version = "0.1", default-features = false, features = ["stats"] }
```

## Generated API per task

For this task definition:

```c
task fib(n: i32) -> i32
```

`lace-native-build` generates the following Rust-facing API:

| Function | Meaning |
|---|---|
| `fib_spawn(w, n) -> FibGuard` | Spawn task and return a guard. |
| `guard.sync(w) -> i32` | Join the spawned task through the guard. |
| `guard.drop(w)` | Cancel the task through the guard, unless it has already been stolen. |
| `fib_sync(w) -> i32` | Join the most recently spawned task using the C-style API. |
| `fib_drop(w)` | Cancel the most recently spawned task using the C-style API. |
| `fib(w, n) -> i32` | Direct call, without spawning parallel work. |
| `fib_run(n) -> i32` | Run from inside or outside Lace workers. |
| `fib_newframe(n) -> i32` | Run as a new Lace frame. |
| `fib_together(n)` | Ask all workers to run a copy. |

The guard API and the C-style spawn/sync API target the same Lace operations. The guard API is usually more idiomatic in Rust, especially when task arguments include references. The C-style API is convenient for loops that spawn multiple tasks and then sync them later, as in the N-Queens example.

## `tasks.def` format

```c
# Comments start with #.

# C headers for type resolution. Lines inside c { ... } are passed
# verbatim to C, so #include is not treated as a tasks.def comment here.
c {
    #include "lace.h"
    #include "my_types.h"
}

# Rust imports for types used in generated signatures.
rust {
    use crate::MyStruct;
}

# Custom value types: RustType = CType.
# Primitive integers, floats, bool, usize, and isize are built in.
# References and raw pointers map through void* on the C side.
types {
    BDD = uint64_t
}

# Free functions: user implements fn name(w: &Worker, ...) -> RetType.
task fib(n: i32) -> i32
task do_work(start: usize, end: usize)
task process(data: &MyData, count: usize) -> u64

# Methods: user implements fn name(&self, w: &Worker, ...) -> RetType.
impl MyTree {
    task search(&self, key: u64) -> bool
    task insert(&mut self, key: u64, value: u64)
}
```

## Borrowing rules

- Arguments passed by reference are held by the generated guard until `sync` or `drop`.
- For `&mut T`, Rust prevents access to the borrowed value between spawn and sync.
- For `&T`, multiple tasks may share read-only access.
- Guards are marked `#[must_use]`, so forgetting to sync or drop a spawned task is a compiler warning.

## Developing against a local Lace checkout

By default, `lace-native` uses the vendored Lace sources included in the runtime crate. To build against a local Lace checkout instead, set `LACE_DIR`:

```bash
export LACE_DIR=/path/to/lace
cargo build
```

When `LACE_DIR` is set, the runtime crate compiles Lace from that directory. This is useful when testing changes to Lace itself without copying updated C sources into this repository.

To update the vendored Lace sources manually:

```bash
cp /path/to/lace/src/lace.{h,c} lace-native/vendor/
```

## Relationship to Lace

[Lace](https://github.com/trolando/lace) is the C runtime. `lace-native` is a Rust interface to that runtime. The performance-critical work-stealing deque, worker management, and scheduling behaviour remain implemented by Lace.

Use Lace directly for C or C++ projects. Use `lace-native` when you want to implement Lace tasks in Rust while keeping the native C backend.

See the [Developer's Guide](GUIDE.md) for a tutorial covering task definitions, spawn/sync patterns, borrowing rules, method tasks, and performance tips.

## Benchmarks

The `benchmarks/rayon-compare` crate runs the same algorithms (Fibonacci, N-Queens) in both lace-native and Rayon with the same thread count:

```bash
cargo run --release -p rayon-compare -- -w 4
```

This is not meant as a competition — the frameworks have different design goals. Lace targets fine-grained fork-join with minimal per-task overhead; Rayon provides a broader parallel iterator ecosystem.

## Academic publications

If you use Lace or `lace-native` in academic work, please cite the original Lace publication:

T. van Dijk and J.C. van de Pol (2014). [Lace: Non-blocking Split Deque for Work-Stealing](https://doi.org/10.1007/978-3-319-14313-2_18). In: *Euro-Par 2014: Parallel Processing Workshops*. LNCS 8806, Springer.

## License

`lace-native` is licensed under the [Apache License 2.0](https://opensource.org/licenses/Apache-2.0).

The vendored Lace C sources are © Tom van Dijk and are also licensed under the Apache License 2.0.
