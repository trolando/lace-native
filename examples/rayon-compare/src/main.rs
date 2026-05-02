//! Side-by-side comparison of lace-native vs Rayon on fork-join workloads.

use lace_example_uts as uts_lib;
use std::time::Instant;

// ═══════════════════════════════════════════════════════════════════════════════
// Rayon implementations
// ═══════════════════════════════════════════════════════════════════════════════

mod rayon_impl {
    use super::uts_lib;
    use rayon::prelude::*;

    pub fn fib(n: i32) -> i32 {
        if n < 2 {
            return n;
        }
        let (a, b) = rayon::join(|| fib(n - 1), || fib(n - 2));
        a + b
    }

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

    pub fn uts(state: &uts_lib::RngState, depth: i32, cfg: &uts_lib::UtsConfig) -> i64 {
        let nt = uts_lib::node_type_at_depth(depth, cfg);
        let nc = uts_lib::num_children(state, depth, nt, cfg);
        if nc == 0 {
            return 1;
        }

        let count: i64 = (0..nc)
            .into_par_iter()
            .map(|i| {
                let child = uts_lib::rng_spawn(state, i);
                uts(&child, depth + 1, cfg)
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

    let fib_expect = 267914296;
    let nq_expect: i64 = 73712;
    let uts_cfg = &uts_lib::T3L;
    let uts_root = uts_lib::rng_init(uts_cfg.seed);

    // ── Lace ──

    lace_native::start(workers, 0, 0);
    let (lace_fib, lace_fib_t) = time_it(|| lace_example_fib::fib_run(42));
    lace_native::stop();
    assert_eq!(lace_fib, fib_expect);

    lace_native::start(workers, 0, 0);
    let (lace_nq, lace_nq_t) =
        time_it(|| lace_example_nqueens::nqueens_run(std::ptr::null(), 13, -1, 0));
    lace_native::stop();
    assert_eq!(lace_nq, nq_expect);

    uts_lib::set_uts_config(uts_cfg);
    lace_native::start(workers, 0, 64 * 1024 * 1024);
    let (lace_uts, lace_uts_t) = time_it(|| uts_lib::uts_run(uts_root.as_ptr(), 0));
    lace_native::stop();

    // ── Rayon ──

    let (rayon_fib, rayon_fib_t) = pool.install(|| time_it(|| rayon_impl::fib(42)));
    assert_eq!(rayon_fib, fib_expect);

    let (rayon_nq, rayon_nq_t) = pool.install(|| time_it(|| rayon_impl::nqueens(&[], 13, -1, 0)));
    assert_eq!(rayon_nq, nq_expect);

    let (rayon_uts, rayon_uts_t) =
        pool.install(|| time_it(|| rayon_impl::uts(&uts_root, 0, uts_cfg)));
    assert_eq!(
        lace_uts, rayon_uts,
        "UTS mismatch: lace={}, rayon={}",
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
        "fib(42)",
        lace_fib_t,
        rayon_fib_t,
        ratio_str(lace_fib_t, rayon_fib_t)
    );
    println!(
        "{:<25} {:>10.4}s {:>10.4}s {:>12}",
        "nqueens(13)",
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
    println!();
}
