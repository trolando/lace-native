//! Rust bindings for the [Lace](https://github.com/trolando/lace)
//! work-stealing framework for multi-core fork-join parallelism.
//!
//! This crate provides the Lace runtime: worker lifecycle management
//! and the [`Worker`] handle type. Task definitions are written in a
//! `tasks.def` file and compiled by the companion crate
//! [`lace-native-build`](https://crates.io/crates/lace-native-build).
//!
//! # Quick start
//!
//! **`Cargo.toml`:**
//! ```toml
//! [dependencies]
//! lace-native = "0.1"
//!
//! [build-dependencies]
//! lace-native-build = "0.1"
//! ```
//!
//! **`build.rs`:**
//! ```rust,ignore
//! fn main() {
//!     lace_native_build::process("tasks.def").compile();
//! }
//! ```
//!
//! **`tasks.def`:**
//! ```text
//! c {
//!     #include "lace.h"
//! }
//! task fib(n: i32) -> i32
//! ```
//!
//! **`src/main.rs`:**
//! ```rust,ignore
//! include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));
//!
//! fn fib(w: &Worker, n: i32) -> i32 {
//!     if n < 2 { return n; }
//!     let guard = fib_spawn(w, n - 1);
//!     let a = fib(w, n - 2);
//!     guard.sync(w) + a
//! }
//!
//! fn main() {
//!     lace_native::start(0, 0, 0);
//!     println!("fib(42) = {}", fib_run(42));
//!     lace_native::stop();
//! }
//! ```
//!
//! # Lifecycle
//!
//! Call [`start()`] to launch worker threads, then use task functions
//! (`_run`, `_spawn`/`_sync`, etc.) to execute parallel work. Call
//! [`stop()`] when done. Lace supports multiple start/stop cycles
//! within the same process.
//!
//! # Features
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `backoff` | ✓ | Workers sleep when idle (futex-based progressive backoff) |
//! | `hwloc` | | Pin workers to CPU cores using hwloc |
//! | `stats` | | Print per-worker steal/task/split counters on [`stop()`] |

use std::ffi::c_uint;

/// Opaque Lace worker type (`lace_worker` in C).
///
/// This is an FFI implementation detail used by generated code.
/// Safe code uses [`Worker`].
#[doc(hidden)]
#[repr(C)]
pub struct LaceWorker {
    _opaque: [u8; 0],
}

/// A handle to a Lace worker thread.
///
/// Task body functions receive `&Worker` as their first parameter.
/// All spawn/sync operations require a `&Worker`.
///
/// `Worker` cannot be constructed directly — it is provided by the
/// Lace framework when executing task bodies, or obtained via
/// [`get_worker()`].
#[repr(transparent)]
pub struct Worker {
    raw: *mut LaceWorker,
}

impl Worker {
    /// Create a Worker from a raw `lace_worker` pointer.
    ///
    /// # Safety
    /// The pointer must be a valid `lace_worker*` from a running Lace thread.
    #[doc(hidden)]
    pub unsafe fn from_raw(raw: *mut LaceWorker) -> Worker {
        Worker { raw }
    }

    /// Get the raw `lace_worker*` pointer for FFI calls.
    #[doc(hidden)]
    pub fn as_ptr(&self) -> *mut LaceWorker {
        self.raw
    }

    /// Returns this worker's ID (0-based index).
    ///
    /// Returns -1 if called from a non-worker thread, though in
    /// normal usage task bodies always run on worker threads.
    pub fn id(&self) -> i32 {
        unsafe { lace_worker_id_ext() }
    }

    /// Returns the total number of Lace workers.
    ///
    /// Equivalent to the value passed to [`start()`], or the
    /// auto-detected core count if 0 was passed.
    pub fn count(&self) -> u32 {
        unsafe { lace_worker_count() }
    }

    /// Check for and handle interrupting tasks (NEWFRAME/TOGETHER).
    ///
    /// Call this periodically in long-running tasks to enable
    /// cooperative interruption. Without this, NEWFRAME and TOGETHER
    /// tasks may be delayed until the next spawn or sync.
    pub fn yield_now(&self) {
        unsafe { lace_yield(self.raw) }
    }

    /// Thread-local pseudo-random number generator (xoroshiro128**).
    ///
    /// Each worker has its own RNG state, so this function is
    /// contention-free. Useful for randomized algorithms that need
    /// fast, non-synchronized random numbers.
    pub fn rng(&self) -> u64 {
        unsafe { lace_rng_ext(self.raw) }
    }
}

unsafe impl Send for Worker {}

/// Start the Lace framework with the given number of workers.
///
/// This launches worker threads that will execute spawned tasks via
/// work-stealing. Must be called before any task functions (`_run`,
/// `_spawn`, etc.).
///
/// # Arguments
///
/// * `n_workers` — Number of worker threads. Pass 0 to auto-detect
///   from the number of available CPU cores.
/// * `dqsize` — Task deque size per worker (number of task slots).
///   Pass 0 for the default (typically 1M slots).
/// * `stacksize` — Program stack size per worker thread in bytes.
///   Pass 0 for the system default.
pub fn start(n_workers: u32, dqsize: usize, stacksize: usize) {
    unsafe { lace_start(n_workers as c_uint, dqsize, stacksize) }
}

/// Stop the Lace framework, joining all worker threads.
///
/// After this call, [`is_running()`] returns `false`. You may call
/// [`start()`] again to restart the framework.
pub fn stop() {
    unsafe { lace_stop() }
}

/// Returns `true` if the Lace framework is currently running.
pub fn is_running() -> bool {
    unsafe { lace_is_running() != 0 }
}

/// Returns the number of Lace worker threads.
///
/// Only meaningful while Lace is running.
pub fn worker_count() -> u32 {
    unsafe { lace_worker_count() }
}

/// Returns a [`Worker`] handle for the current thread.
///
/// This is only valid when called from a Lace worker thread (i.e.,
/// from within a task body). For external threads, use the `_run`
/// functions instead, which handle worker dispatch automatically.
///
/// # Panics
///
/// Panics if the current thread is not a Lace worker.
pub fn get_worker() -> Worker {
    unsafe {
        let w = lace_get_worker_ext();
        assert!(!w.is_null(), "get_worker() called from a non-Lace thread");
        Worker { raw: w }
    }
}

/// Block until all Lace workers reach this barrier.
///
/// Typically used inside `_together` tasks to synchronize all workers
/// before proceeding.
pub fn barrier() {
    unsafe { lace_barrier() }
}

/// Set the verbosity level for Lace startup messages.
///
/// Call before [`start()`]. Level 0 (default) suppresses output;
/// level 1 prints startup diagnostics (worker count, deque size, etc.).
pub fn set_verbosity(level: i32) {
    unsafe { lace_set_verbosity(level) }
}

/// Returns `true` if the calling thread is a Lace worker.
///
/// Useful in library code that needs to behave differently depending
/// on whether it's running inside a Lace task or from an external thread.
pub fn is_worker() -> bool {
    unsafe { lace_is_worker_ext() != 0 }
}

extern "C" {
    fn lace_start(n_workers: c_uint, dqsize: usize, stacksize: usize);
    fn lace_stop();
    fn lace_is_running() -> i32;
    fn lace_worker_count() -> c_uint;
    fn lace_barrier();
    fn lace_set_verbosity(level: i32);
    fn lace_yield(lw: *mut LaceWorker);
    fn lace_get_worker_ext() -> *mut LaceWorker;
    fn lace_worker_id_ext() -> i32;
    fn lace_is_worker_ext() -> i32;
    fn lace_rng_ext(lw: *mut LaceWorker) -> u64;
}
