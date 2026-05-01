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

**Cargo.toml:**
```toml
[dependencies]
lace-ws = "0.1"

[build-dependencies]
lace-ws-build = "0.1"
```

**build.rs:**
```rust
fn main() {
    lace_ws_build::process("tasks.def").compile();
}
```

**tasks.def:**
```
c {
    #include "lace.h"
}
task fib(n: i32) -> i32
```

**src/main.rs:**
```rust
include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

fn fib(w: &Worker, n: i32) -> i32 {
    if n < 2 { return n; }
    let guard = fib_spawn(w, n - 1);
    let a = fib(w, n - 2);
    guard.sync(w) + a
}

fn main() {
    lace_ws::start(0, 0, 0);
    println!("fib(42) = {}", fib_run(42));
    lace_ws::stop();
}
```

## Generated API per task

For `task fib(n: i32) -> i32`:

| Function | Description |
|---|---|
| `fib_spawn(w, n) → FibGuard` | Fork: spawn task, returns guard |
| `guard.sync(w) → i32` | Join via guard (Rust style) |
| `fib_sync(w) → i32` | Join without guard (C style) |
| `guard.drop(w)` / `fib_drop(w)` | Cancel (unless already stolen) |
| `fib(w, n) → i32` | Direct call, no parallelism |
| `fib_run(n) → i32` | Auto-dispatch: CALL if on worker, RUN if external |
| `fib_newframe(n) → i32` | Interrupt: all workers help |
| `fib_together(n)` | All workers run a copy |

## tasks.def format

```
# C headers for type resolution
c {
    #include "lace.h"
    #include "my_types.h"
}

# Rust imports
rust {
    use crate::MyStruct;
}

# Custom value types: RustType = CType
types {
    BDD = uint64_t
}

# Free functions
task fib(n: i32) -> i32
task process(data: &MyData, count: usize) -> u64

# Methods
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

## Features

| Feature | Default | Description |
|---|---|---|
| `backoff` | ✓ | Workers sleep when idle (futex-based) |
| `hwloc` | | Pin workers to cores using hwloc |
| `stats` | | Enable per-worker steal/task/split counters |

## Development

To develop against a local Lace checkout instead of vendored sources:
```bash
export LACE_DIR=/path/to/lace
cargo build
```

## License

Apache-2.0. Lace C source is © Tom van Dijk, also Apache-2.0.
