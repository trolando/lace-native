# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0] — unreleased

Initial release.

### Features

- **`lace-native`** runtime crate: `start`/`stop`, `Worker` handle, `barrier`, `get_worker`
- **`lace-native-build`** code generator: parses `tasks.def`, generates C wrappers and safe Rust bindings
- Cargo features: `backoff` (default), `hwloc`, `stats`
- Guard-based borrow-checked spawn/sync for `&mut T` parameters
- C-style standalone `_sync`/`_drop` for multi-spawn loops
- Full Lace task API: `spawn`, `sync`, `drop`, `run`, `newframe`, `together`
- Method tasks via `impl Type { task name(&self, ...) }` syntax
- Vendored Lace 2.3.2 C sources with `LACE_DIR` override for development
- Vendor update script (`scripts/vendor-lace.sh`)
- Comprehensive doc comments on all public API items
- Helpful parse error messages with line numbers and context
- Developer's guide (`GUIDE.md`) with cross-language LTO documentation
- CI: Linux, macOS, Windows with MSVC, feature matrix, clippy, rustfmt

### Examples

- **fib** — classic fork-join Fibonacci with `-w`/`-q` options and timing
- **nqueens** — N-Queens with multi-spawn/sync loops
- **uts** — Unbalanced Tree Search (T1/T1L/T1XL configs, SHA-1 RNG)
- **bench** — scaling benchmark: measures fib + nqueens across 1..N workers

### Benchmarks

- **rayon-compare** — side-by-side fib + nqueens + UTS comparison with Rayon
