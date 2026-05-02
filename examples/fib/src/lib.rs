//! Fibonacci task for Lace — demonstrates basic fork-join parallelism.

include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

/// Fibonacci — guard style (Rust-idiomatic).
pub(crate) fn fib(w: &Worker, n: i32) -> i32 {
    if n < 2 {
        return n;
    }
    let guard = fib_spawn(w, n - 1);
    let a = fib(w, n - 2);
    let b = guard.sync(w);
    a + b
}

/// Fibonacci — C style (no guard).
#[allow(dead_code)]
pub(crate) fn fib_c_style(w: &Worker, n: i32) -> i32 {
    if n < 2 {
        return n;
    }
    let _ = fib_c_style_spawn(w, n - 1);
    let a = fib_c_style(w, n - 2);
    let b = fib_c_style_sync(w);
    a + b
}
