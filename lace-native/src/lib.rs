//! # lace-native
//!
//! Rust bindings for the [Lace](https://github.com/trolando/lace)
//! work-stealing framework for multi-core fork-join parallelism.
//!
//! This crate provides the runtime: starting/stopping Lace and the [`Worker`]
//! handle type. Task definitions live in downstream crates using
//! [`lace-native-build`](https://crates.io/crates/lace-native-build) to generate
//! bindings from a `tasks.def` file.
//!
//! ## Quick start
//!
//! See the [`examples/fib`](https://github.com/trolando/lace-rs/tree/main/examples/fib)
//! directory for a complete working example.

use std::ffi::c_uint;

/// Opaque Lace worker type (`lace_worker` in C).
///
/// Used only in FFI signatures. Safe code uses [`Worker`].
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

    /// Worker ID (0-based index), or -1 if not a worker thread.
    pub fn id(&self) -> i32 {
        unsafe { lace_worker_id_ext() }
    }

    /// Total number of Lace workers.
    pub fn count(&self) -> u32 {
        unsafe { lace_worker_count() }
    }

    /// Check for and handle interrupting tasks (NEWFRAME/TOGETHER).
    ///
    /// Call this periodically in long-running tasks to enable
    /// cooperative interruption.
    pub fn yield_now(&self) {
        unsafe { lace_yield(self.raw) }
    }
}

unsafe impl Send for Worker {}

/// Start the Lace framework.
///
/// - `n_workers`: number of worker threads, or 0 for auto-detect.
/// - `dqsize`: task deque size per worker, or 0 for default (1M slots).
/// - `stacksize`: program stack size per worker, or 0 for default.
pub fn start(n_workers: u32, dqsize: usize, stacksize: usize) {
    unsafe { lace_start(n_workers as c_uint, dqsize, stacksize) }
}

/// Stop the Lace framework, terminating all workers.
pub fn stop() {
    unsafe { lace_stop() }
}

/// Check whether Lace is currently running.
pub fn is_running() -> bool {
    unsafe { lace_is_running() != 0 }
}

/// Get the number of Lace workers.
pub fn worker_count() -> u32 {
    unsafe { lace_worker_count() }
}

/// Get the [`Worker`] handle for the current Lace thread.
///
/// # Panics
/// Panics if called from a non-Lace thread.
pub fn get_worker() -> Worker {
    unsafe {
        let w = lace_get_worker_ext();
        assert!(!w.is_null(), "get_worker() called from a non-Lace thread");
        Worker { raw: w }
    }
}

/// Barrier: block until all Lace workers reach this point.
///
/// Typically used inside TOGETHER tasks.
pub fn barrier() {
    unsafe { lace_barrier() }
}

extern "C" {
    fn lace_start(n_workers: c_uint, dqsize: usize, stacksize: usize);
    fn lace_stop();
    fn lace_is_running() -> i32;
    fn lace_worker_count() -> c_uint;
    fn lace_barrier();
    fn lace_yield(lw: *mut LaceWorker);
    fn lace_get_worker_ext() -> *mut LaceWorker;
    fn lace_worker_id_ext() -> i32;
}
