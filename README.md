# lace-rs

Rust bindings for the [Lace](https://github.com/trolando/lace) work-stealing
framework for multi-core fork-join parallelism.

Tasks are defined in a `tasks.def` DSL file. At build time, `lace-ws-build`
generates safe Rust wrappers with borrow-checked guards and `extern "C"`
trampolines. All `unsafe` code lives in the generated layer — user code is
pure safe Rust.

## Crates

| Crate | Description |
|---|---|
| [`lace-ws`](lace-ws/) | Runtime library: compiles Lace, provides `Worker`, lifecycle functions |
| [`lace-ws-build`](lace-ws-build/) | Build-time code generator, used as a build-dependency |

## Quick start

Add to your `Cargo.toml`:
```toml
[dependencies]
lace-ws = "0.1"

[build-dependencies]
lace-ws-build = "0.1"
```

Create `build.rs`:
```rust
fn main() {
    lace_ws_build::process("tasks.def").compile();
}
```

Create `tasks.def`:
```
c {
    #include "lace.h"
}
task fib(n: i32) -> i32
```

Write `src/main.rs`:
```rust
include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

fn fib(w: &Worker, n: i32) -> i32 {
    if n < 2 { return n; }
    let guard = fib_spawn(w, n - 1);
    let a = fib(w, n - 2);
    guard.sync(w) + a
}

fn main() {
    lace_ws::start(0, 0, 0);  // auto workers, default deque, default stack
    println!("fib(42) = {}", fib_run(42));
    lace_ws::stop();
}
```

## Building and running

```bash
# Build the workspace
cargo build --release

# Run fibonacci (default: 42, auto-detect workers)
cargo run --release -p lace-example-fib

# Run with options
cargo run --release -p lace-example-fib -- -w 4 42

# Run N-Queens (default: n=14)
cargo run --release -p lace-example-nqueens -- -w 4 14
```

Both examples accept:
- `-w <n>` — number of worker threads (0 = auto-detect, default)
- `-q <n>` — task deque size per worker (0 = default)

## Features

Features are set on the `lace-ws` crate and control how the Lace C runtime
is compiled. They propagate automatically to `lace-ws-build` so that the
task wrappers and runtime always agree on configuration.

| Feature | Default | Description |
|---|---|---|
| `backoff` | ✓ | Workers sleep when idle (futex-based progressive backoff) |
| `hwloc` | | Pin workers to CPU cores using hwloc |
| `stats` | | Print per-worker steal/task/split counters on `lace_stop()` |

```bash
# Build with statistics enabled
cargo build --release --features lace-ws/stats

# Build with hwloc pinning (requires libhwloc-dev)
cargo build --release --features lace-ws/hwloc

# Build with no backoff (busy-wait stealing)
cargo build --release --no-default-features

# Combine features
cargo build --release --features lace-ws/stats,lace-ws/hwloc

# In your Cargo.toml for fine-grained control:
# [dependencies]
# lace-ws = { version = "0.1", default-features = false, features = ["stats"] }
```

## Generated API per task

For `task fib(n: i32) -> i32`:

| Function | Description |
|---|---|
| `fib_spawn(w, n) → FibGuard` | Fork: spawn task, returns guard |
| `guard.sync(w) → i32` | Join via guard (Rust style) |
| `guard.drop(w)` | Cancel via guard (unless already stolen) |
| `fib_sync(w) → i32` | Join without guard (C style) |
| `fib_drop(w)` | Cancel without guard (C style) |
| `fib(w, n) → i32` | Direct call, no parallelism |
| `fib_run(n) → i32` | Auto-dispatch: CALL if on worker, RUN if external |
| `fib_newframe(n) → i32` | Interrupt: all workers help |
| `fib_together(n)` | All workers run a copy |

**Guard vs C-style:** Both compile to identical code. Use guards when
parameters include `&mut T` (the borrow checker enforces safety). Use the
C-style standalone `fib_sync(w)` for loops with multiple spawn/sync, as in
the N-Queens example.

## tasks.def format

```
# Comments start with #

# C headers for type resolution (lines inside are passed verbatim,
# so #include works — it is NOT treated as a comment here)
c {
    #include "lace.h"
    #include "my_types.h"
}

# Rust imports for types used in signatures
rust {
    use crate::MyStruct;
}

# Custom value types: RustType = CType
# Primitives (i8..i64, u8..u64, usize, isize, bool, f32, f64) are built-in.
# References (&T, &mut T) map to void* automatically.
# Raw pointers (*const T, *mut T) map to void* automatically.
types {
    BDD = uint64_t
}

# Free functions — user implements fn name(w: &Worker, ...) -> RetType
task fib(n: i32) -> i32
task do_work(start: usize, end: usize)
task process(data: &MyData, count: usize) -> u64

# Methods — user implements fn name(&self, w: &Worker, ...) -> RetType
impl MyTree {
    task search(&self, key: u64) -> bool
    task insert(&mut self, key: u64, value: u64)
}
```

## Borrowing rules

- Parameters by reference (`&T`, `&mut T`) are held by the guard until
  `sync` or `drop`.
- For `&mut T`: the borrow checker prevents using `T` between spawn and
  sync — enforced at compile time.
- For `&T`: multiple tasks may share concurrent access — use for lock-free
  data structures.
- Guards are `#[must_use]` — forgetting to sync is a compile warning.

## Development

To develop against a local Lace checkout instead of vendored sources:
```bash
export LACE_DIR=/path/to/lace
cargo build
```

When `LACE_DIR` is set, the `lace-ws` crate compiles from that tree instead
of the vendored `lace.h`/`lace.c`. This lets you test Lace changes without
re-vendoring.

To update the vendored Lace sources:
```bash
cp /path/to/lace/src/lace.{h,c} lace-ws/vendor/
```

## License

Apache-2.0. Lace C source is © Tom van Dijk, also Apache-2.0.
