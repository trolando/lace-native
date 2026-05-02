//! Side-by-side comparison of lace-native vs Rayon on fork-join workloads.
//!
//! - **Fibonacci**: binary recursion (tests raw fork-join overhead)
//! - **N-Queens**: multi-way spawn (tests N-ary parallelism)
//! - **UTS T3L**: Unbalanced Tree Search (tests irregular load balancing)

use lace_example_uts::{self as uts_lib, RngState};
use std::time::Instant;

include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

// ═══════════════════════════════════════════════════════════════════════════════
// Lace implementations
// ═══════════════════════════════════════════════════════════════════════════════

fn fib(w: &Worker, n: i32) -> i32 {
    if n < 2 {
        return n;
    }
    let guard = fib_spawn(w, n - 1);
    let a = fib(w, n - 2);
    guard.sync(w) + a
}

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

// ═══════════════════════════════════════════════════════════════════════════════
// Rayon implementations
// ═══════════════════════════════════════════════════════════════════════════════

mod rayon_impl {
    use super::*;
    use rayon::prelude::*;

    pub fn fib(n: i32) -> i32 {
        if n < 2 {
            return n;
        }
        let (a, b) = rayon::join(|| fib(n - 1), || fib(n - 2));
        a + b
    }

    /// N-Queens using par_iter — spawns N parallel tasks and sums results,
    /// structurally matching Lace's multi-spawn + multi-sync pattern.
    pub fn nqueens(a: &[i32], n: i32, d: i32, i: i32) -> i64 {
        let mut aa = Vec::with_capacity((d + 2).max(0) as usize);
        for j in 0..d.max(0) {
            let aj = a[j as usize];
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

        (0..n).into_par_iter().map(|k| nqueens(&aa, n, d, k)).sum()
    }

    pub fn uts(state: &RngState, depth: i32) -> i64 {
        let nc = super::uts_num_children(state, depth);
        if nc == 0 {
            return 1;
        }

        let count: i64 = (0..nc)
            .into_par_iter()
            .map(|i| {
                let child_state = uts_lib::rng_spawn(state, i);
                uts(&child_state, depth + 1)
            })
            .sum();

        count + 1
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Benchmark harness
// ═══════════════════════════════════════════════════════════════════════════════

fn time_it<F: FnOnce() -> T, T>(f: F) -> (T, f64) {
    let t = Instant::now();
    let result = f();
    (result, t.elapsed().as_secs_f64())
}

fn ratio_str(lace_t: f64, rayon_t: f64) -> String {
    if lace_t < rayon_t {
        format!("{:.2}x faster", rayon_t / lace_t)
    } else if rayon_t < lace_t {
        format!("{:.2}x slower", lace_t / rayon_t)
    } else {
        "equal".into()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut n_workers: u32 = 0;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-w" => {
                i += 1;
                n_workers = args[i].parse().expect("-w requires a number");
            }
            "-h" | "--help" => {
                eprintln!("Usage: {} [-w <workers>]", args[0]);
                return;
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                return;
            }
        }
        i += 1;
    }

    lace_native::start(n_workers, 0, 0);
    let workers = lace_native::worker_count();
    lace_native::stop();

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers as usize)
        .stack_size(64 * 1024 * 1024)
        .build()
        .expect("failed to build Rayon pool");

    println!("lace-native vs Rayon comparison");
    println!("===============================");
    println!();
    println!("Workers: {}", workers);

    let fib_n = 42;
    let fib_expect = 267914296;
    let nq_n = 13;
    let nq_expect: i64 = 73712;
    let uts_root = uts_lib::rng_init(uts_lib::T3L.seed);

    // ── Lace ──

    lace_native::start(workers, 0, 0);
    let (lace_fib, lace_fib_t) = time_it(|| fib_run(fib_n));
    lace_native::stop();
    assert_eq!(lace_fib, fib_expect, "Lace fib wrong");

    lace_native::start(workers, 0, 0);
    let (lace_nq, lace_nq_t) = time_it(|| nqueens_run(std::ptr::null(), nq_n, -1, 0));
    lace_native::stop();
    assert_eq!(lace_nq, nq_expect, "Lace nqueens wrong");

    lace_native::start(workers, 0, 64 * 1024 * 1024);
    let (lace_uts, lace_uts_t) = time_it(|| uts_run(uts_root.as_ptr(), 0));
    lace_native::stop();

    // ── Rayon ──

    let (rayon_fib, rayon_fib_t) = pool.install(|| time_it(|| rayon_impl::fib(fib_n)));
    assert_eq!(rayon_fib, fib_expect, "Rayon fib wrong");

    let (rayon_nq, rayon_nq_t) = pool.install(|| time_it(|| rayon_impl::nqueens(&[], nq_n, -1, 0)));
    assert_eq!(rayon_nq, nq_expect, "Rayon nqueens wrong");

    let (rayon_uts, rayon_uts_t) = pool.install(|| time_it(|| rayon_impl::uts(&uts_root, 0)));
    assert_eq!(
        lace_uts, rayon_uts,
        "UTS node counts differ: lace={}, rayon={}",
        lace_uts, rayon_uts
    );

    // ── Results ──

    println!();
    println!(
        "{:<25} {:>12} {:>12} {:>12}",
        "Benchmark", "lace-native", "Rayon", "ratio"
    );
    println!(
        "{:<25} {:>12} {:>12} {:>12}",
        "-------------------------", "----------", "----------", "----------"
    );
    println!(
        "{:<25} {:>10.4}s {:>10.4}s {:>12}",
        format!("fib({})", fib_n),
        lace_fib_t,
        rayon_fib_t,
        ratio_str(lace_fib_t, rayon_fib_t)
    );
    println!(
        "{:<25} {:>10.4}s {:>10.4}s {:>12}",
        format!("nqueens({})", nq_n),
        lace_nq_t,
        rayon_nq_t,
        ratio_str(lace_nq_t, rayon_nq_t)
    );
    println!(
        "{:<25} {:>10.4}s {:>10.4}s {:>12}",
        format!("uts T3L ({} nodes)", lace_uts),
        lace_uts_t,
        rayon_uts_t,
        ratio_str(lace_uts_t, rayon_uts_t)
    );

    println!();
    println!(
        "Both frameworks use {} thread(s). Lower time is better.",
        workers
    );
    println!(
        "Rayon nqueens uses par_iter (spawns N tasks, sums results — matches Lace multi-spawn)."
    );
    println!("UTS uses T3L: binomial, ~111M nodes, depth ~17K.");
    println!();
}
