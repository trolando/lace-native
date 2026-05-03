# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] — unreleased

### Added

- `Worker::steal_random()` — attempt to steal and execute a task from another worker. Useful for keeping workers productive while blocking on external conditions (e.g., Sylvan-style GC).
- `Worker::rng()` — per-worker pseudo-random number generator (xoroshiro128**), contention-free.
- `set_verbosity(level)` — control Lace startup messages.
- `is_worker()` — check whether the calling thread is a Lace worker.
- `make_all_shared()` — expose all pending tasks on the current deque to thieves.
- `count_reset()` / `count_report()` — reset and print per-worker steal/task statistics (requires `stats` feature).
- `guard.is_stolen()` — check whether a spawned task has been stolen by another worker.
- `guard.is_completed()` — check whether a spawned task has been completed.
- `guard.result()` — non-blocking poll: returns `Some(value)` if completed, `None` otherwise.
- `task_result_ptr()` — low-level access to completed task result storage (doc-hidden, used by generated code).

## [0.1.0] — 2025-05-03

Initial release.

### Features

- **`lace-native`** runtime crate: `start`/`stop`, `Worker` handle, `barrier`, `get_worker`, `is_worker`, `set_verbosity`, `Worker::rng()`
- **`lace-native-build`** code generator: parses `tasks.def`, generates C wrappers and safe Rust bindings with doc comments
- Cargo features: `backoff` (default), `hwloc`, `stats`
- Guard-based borrow-checked spawn/sync for `&mut T` parameters
- C-style standalone `_sync`/`_drop` for multi-spawn loops
- Full Lace task API: `spawn`, `sync`, `drop`, `run`, `newframe`, `together`
- Method tasks via `impl Type { task name(&self, ...) }` syntax
- Vendored Lace 2.3.2 C sources with `LACE_DIR` override for development
- Vendor update script (`scripts/vendor-lace.sh`)
- Comprehensive doc comments on all public API items and generated code
- Helpful parse error messages with line numbers and context
- Developer's guide (`GUIDE.md`) with LTO docs, pitfall docs, benchmark guide
- CI: Linux, macOS, Windows with MSVC, feature matrix, clippy, rustfmt
- CI: Cross-language LTO verification (Clang + lld)
- CI: MSRV verification (Rust 1.70)

### Examples

- **fib** — classic fork-join Fibonacci with `-w`/`-q` options and timing
- **nqueens** — N-Queens with multi-spawn/sync loops
- **uts** — Unbalanced Tree Search (T1/T1L/T1XL/T3L and all standard configs)
- **tree** — parallel tree traversal demonstrating method tasks with `&self`
- **bench** — scaling benchmark: fib + nqueens + UTS across 1..N workers
- **rayon-compare** — side-by-side fib + nqueens + UTS T3L comparison with Rayon

### Benchmarks

- **criterion** — statistically rigorous measurements of fib, nqueens, UTS with confidence intervals
