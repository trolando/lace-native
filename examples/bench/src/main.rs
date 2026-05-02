use lace_example_uts::{self as uts_lib, RngState};
use std::time::Instant;

include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

// ── Fibonacci ──

fn fib(w: &Worker, n: i32) -> i32 {
    if n < 2 {
        return n;
    }
    let guard = fib_spawn(w, n - 1);
    let a = fib(w, n - 2);
    guard.sync(w) + a
}

// ── N-Queens ──

fn nqueens(w: &Worker, a: *const i32, n: i32, d: i32, i: i32) -> i64 {
    let mut aa = Vec::with_capacity((d + 2).max(0) as usize);
    for j in 0..d.max(0) {
        let aj = unsafe { *a.add(j as usize) };
        aa.push(aj);
        let diff = aj - i;
        let dist = d - j;
        if diff == 0 || dist == diff || dist + diff == 0 {
            return 0;
        }
    }
    if d >= 0 {
        aa.push(i);
    }
    let d = d + 1;
    if d == n {
        return 1;
    }
    for k in 0..n {
        let _ = nqueens_spawn(w, aa.as_ptr(), n, d, k);
    }
    let mut sum = 0i64;
    for _ in 0..n {
        sum += nqueens_sync(w);
    }
    sum
}

// ── UTS (uses T3L config from lace-example-uts) ──

fn uts_num_children(state: &RngState, depth: i32) -> i32 {
    uts_lib::num_children(
        state,
        depth,
        uts_lib::node_type_at_depth(depth, &uts_lib::T3L),
        &uts_lib::T3L,
    )
}

fn uts(w: &Worker, state: *const u8, depth: i32) -> i64 {
    let parent_state: RngState = unsafe {
        let mut s = [0u8; 20];
        std::ptr::copy_nonoverlapping(state, s.as_mut_ptr(), 20);
        s
    };

    let nc = uts_num_children(&parent_state, depth);
    if nc == 0 {
        return 1;
    }

    let child_states: Vec<RngState> = (0..nc)
        .map(|i| uts_lib::rng_spawn(&parent_state, i))
        .collect();

    for cs in &child_states {
        let _ = uts_spawn(w, cs.as_ptr(), depth + 1);
    }

    let mut count = 1i64;
    for _ in 0..nc {
        count += uts_sync(w);
    }
    count
}

// ── Harness ──

fn bench<F, T>(name: &str, expected: T, f: F) -> f64
where
    F: FnOnce() -> T,
    T: PartialEq + std::fmt::Debug,
{
    let t = Instant::now();
    let result = f();
    let elapsed = t.elapsed().as_secs_f64();
    assert_eq!(result, expected, "{} returned wrong result", name);
    elapsed
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut max_workers: u32 = 0;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-w" => {
                i += 1;
                max_workers = args[i].parse().expect("-w requires a number");
            }
            "-h" | "--help" => {
                eprintln!("Usage: {} [-w <max_workers>]", args[0]);
                return;
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                return;
            }
        }
        i += 1;
    }

    lace_native::start(max_workers, 0, 0);
    let avail = lace_native::worker_count();
    lace_native::stop();

    let fib_n = 42i32;
    let fib_expect = 267914296i32;
    let nq_n = 13i32;
    let nq_expect = 73712i64;
    let uts_expect = 111345631i64;
    let uts_root = uts_lib::rng_init(uts_lib::T3L.seed);

    println!("lace-native scaling benchmark");
    println!("=============================");
    println!();
    println!("Available workers: {}", avail);
    println!();

    let mut worker_counts = vec![1u32];
    let mut w = 2;
    while w < avail {
        worker_counts.push(w);
        w *= 2;
    }
    if avail > 1 {
        worker_counts.push(avail);
    }
    worker_counts.dedup();

    println!(
        "{:>8}  {:>12}  {:>8}  {:>12}  {:>8}  {:>12}  {:>8}",
        "workers", "fib(42)", "speedup", "nqueens(13)", "speedup", "uts T3L", "speedup"
    );
    println!(
        "{:>8}  {:>12}  {:>8}  {:>12}  {:>8}  {:>12}  {:>8}",
        "-------", "----------", "-------", "-----------", "-------", "----------", "-------"
    );

    let mut fib_base = 0.0f64;
    let mut nq_base = 0.0f64;
    let mut uts_base = 0.0f64;

    for &nw in &worker_counts {
        lace_native::start(nw, 0, 64 * 1024 * 1024);

        let fib_time = bench("fib", fib_expect, || fib_run(fib_n));
        let nq_time = bench("nqueens", nq_expect, || {
            nqueens_run(std::ptr::null(), nq_n, -1, 0)
        });
        let uts_time = bench("uts", uts_expect, || uts_run(uts_root.as_ptr(), 0));

        if nw == 1 {
            fib_base = fib_time;
            nq_base = nq_time;
            uts_base = uts_time;
        }

        let fib_sp = if fib_base > 0.0 {
            fib_base / fib_time
        } else {
            1.0
        };
        let nq_sp = if nq_base > 0.0 {
            nq_base / nq_time
        } else {
            1.0
        };
        let uts_sp = if uts_base > 0.0 {
            uts_base / uts_time
        } else {
            1.0
        };

        println!(
            "{:>8}  {:>10.4}s  {:>7.2}x  {:>10.4}s  {:>7.2}x  {:>10.4}s  {:>7.2}x",
            nw, fib_time, fib_sp, nq_time, nq_sp, uts_time, uts_sp
        );

        lace_native::stop();
    }

    println!();
    println!("UTS T3L: binomial, {} nodes", uts_expect);
    println!();
}
