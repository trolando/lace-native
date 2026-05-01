# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0] — 2025-xx-xx

Initial release.

### Features

- **`lace-ws`** runtime crate: `start`/`stop`, `Worker` handle, `barrier`, `get_worker`
- **`lace-ws-build`** code generator: parses `tasks.def`, generates C wrappers and safe Rust bindings
- Cargo features: `backoff` (default), `hwloc`, `stats`
- Guard-based borrow-checked spawn/sync for `&mut T` parameters
- C-style standalone `_sync`/`_drop` for multi-spawn loops
- Full Lace task API: `spawn`, `sync`, `drop`, `run`, `newframe`, `together`
- Method tasks via `impl Type { task name(&self, ...) }` syntax
- Vendored Lace C sources with `LACE_DIR` override for development
- CI: Linux, macOS, Windows, feature matrix

### Examples

- **fib** — classic fork-join Fibonacci
- **nqueens** — N-Queens with multi-spawn/sync loops
