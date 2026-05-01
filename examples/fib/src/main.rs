use std::time::Instant;

include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

/// Fibonacci — guard style (Rust-idiomatic).
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

fn usage(name: &str) {
    eprintln!("Usage: {} [-w <workers>] [-q <dqsize>] <n>", name);
    eprintln!();
    eprintln!("  -w <workers>  Number of worker threads (0 = auto, default: 0)");
    eprintln!("  -q <dqsize>   Task deque size per worker (default: 0 = auto)");
    eprintln!("  <n>           Fibonacci number to compute (default: 42)");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut workers: u32 = 0;
    let mut dqsize: usize = 0;
    let mut n: i32 = 42;
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "-w" => { i += 1; workers = args[i].parse().expect("-w requires a number"); }
            "-q" => { i += 1; dqsize = args[i].parse().expect("-q requires a number"); }
            "-h" | "--help" => { usage(&args[0]); return; }
            _ => { n = args[i].parse().expect("expected a number for <n>"); }
        }
        i += 1;
    }

    lace_ws::start(workers, dqsize, 0);
    println!("Running fib({}) with {} workers...", n, lace_ws::worker_count());

    let t = Instant::now();
    let result = fib_run(n);
    let elapsed = t.elapsed();

    println!("Result: fib({}) = {}", n, result);
    println!("Time: {:.6}s", elapsed.as_secs_f64());

    lace_ws::stop();
}
