// Pull in lace types and generated task bindings
include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

/// Fibonacci — guard style (Rust-idiomatic).
///
/// The guard tracks the spawn→sync obligation.
/// For &mut parameters, it also enforces borrow safety.
fn fib(w: &Worker, n: i32) -> i32 {
    if n < 2 {
        return n;
    }
    let guard = fib_spawn(w, n - 1);
    let a = fib(w, n - 2);
    let b = guard.sync(w);
    a + b
}

/// Fibonacci — C style (no guard).
///
/// Closer to the original C API. The `let _ =` silences the
/// #[must_use] warning on the guard.
#[allow(dead_code)]
fn fib_c_style(w: &Worker, n: i32) -> i32 {
    if n < 2 {
        return n;
    }
    let _ = fib_spawn(w, n - 1);
    let a = fib_c_style(w, n - 2);
    let b = fib_sync(w);
    a + b
}

fn main() {
    let n: i32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);

    lace_ws::start(0, 0, 0);
    let result = fib_run(n);
    println!("fib({}) = {}", n, result);
    lace_ws::stop();
}
