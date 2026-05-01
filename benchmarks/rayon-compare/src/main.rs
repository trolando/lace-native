//! Side-by-side comparison of lace-native vs Rayon on fork-join workloads.
//!
//! Both frameworks implement the same algorithms (fibonacci, N-Queens)
//! using their native fork-join APIs. Results are printed as a table
//! showing wall-clock time for each.

use std::time::Instant;

// ═══════════════════════════════════════════════════════════════════════════════
// Lace implementation
// ═══════════════════════════════════════════════════════════════════════════════

include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

fn fib(w: &Worker, n: i32) -> i32 {
    if n < 2 {
        return n;
    }
    let guard = fib_spawn(w, n - 1);
    let a = fib(w, n - 2);
    let b = guard.sync(w);
    a + b
}

fn nqueens(w: &Worker, a: *const i32, n: i32, d: i32, i: i32) -> i64 {
    let mut aa = Vec::with_capacity((d + 2) as usize);
    for j in 0..d {
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

// ═══════════════════════════════════════════════════════════════════════════════
// Rayon implementation
// ═══════════════════════════════════════════════════════════════════════════════

mod rayon_impl {
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

        // Spawn all columns — use rayon::join for 1:1 comparison with Lace
        let results: Vec<i64> = (0..n)
            .map(|k| {
                let aa = aa.clone();
                rayon::join(|| nqueens(&aa, n, d, k), || 0).0
            })
            .collect();
        results.iter().sum()
    }

    // A more idiomatic Rayon version using par_iter
    pub fn nqueens_par_iter(a: &[i32], n: i32, d: i32, i: i32) -> i64 {
        use rayon::prelude::*;

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

        (0..n)
            .into_par_iter()
            .map(|k| nqueens_par_iter(&aa, n, d, k))
            .sum()
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

    // Detect worker count
    lace_native::start(n_workers, 0, 0);
    let workers = lace_native::worker_count();
    lace_native::stop();

    // Configure Rayon to use same thread count
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers as usize)
        .build()
        .expect("failed to build Rayon pool");

    println!("lace-native vs Rayon comparison");
    println!("===============================");
    println!();
    println!("Workers: {}", workers);
    println!();

    // ── Fibonacci ──

    let fib_n = 42;
    let fib_expect = 267914296;

    lace_native::start(workers, 0, 0);
    let (lace_fib, lace_fib_t) = time_it(|| fib_run(fib_n));
    lace_native::stop();
    assert_eq!(lace_fib, fib_expect);

    let (rayon_fib, rayon_fib_t) = pool.install(|| time_it(|| rayon_impl::fib(fib_n)));
    assert_eq!(rayon_fib, fib_expect);

    // ── N-Queens ──

    let nq_n = 13;
    let nq_expect: i64 = 73712;

    lace_native::start(workers, 0, 0);
    let (lace_nq, lace_nq_t) = time_it(|| nqueens_run(std::ptr::null(), nq_n, -1, 0));
    lace_native::stop();
    assert_eq!(lace_nq, nq_expect);

    let (rayon_nq, rayon_nq_t) = pool.install(|| time_it(|| rayon_impl::nqueens(&[], nq_n, -1, 0)));
    assert_eq!(rayon_nq, nq_expect);

    let (rayon_nq_pi, rayon_nq_pi_t) =
        pool.install(|| time_it(|| rayon_impl::nqueens_par_iter(&[], nq_n, -1, 0)));
    assert_eq!(rayon_nq_pi, nq_expect);

    // ── Results ──

    println!(
        "{:<25} {:>12} {:>12} {:>10}",
        "Benchmark", "lace-native", "Rayon", "ratio"
    );
    println!(
        "{:<25} {:>12} {:>12} {:>10}",
        "-------------------------", "----------", "----------", "--------"
    );

    let ratio = |l: f64, r: f64| -> String {
        if l < r {
            format!("{:.2}x faster", r / l)
        } else if r < l {
            format!("{:.2}x slower", l / r)
        } else {
            "equal".into()
        }
    };

    println!(
        "{:<25} {:>10.4}s {:>10.4}s {:>10}",
        format!("fib({})", fib_n),
        lace_fib_t,
        rayon_fib_t,
        ratio(lace_fib_t, rayon_fib_t)
    );
    println!(
        "{:<25} {:>10.4}s {:>10.4}s {:>10}",
        format!("nqueens({})", nq_n),
        lace_nq_t,
        rayon_nq_t,
        ratio(lace_nq_t, rayon_nq_t)
    );
    println!(
        "{:<25} {:>10.4}s {:>10.4}s {:>10}",
        format!("nqueens({}) par_iter", nq_n),
        lace_nq_t,
        rayon_nq_pi_t,
        ratio(lace_nq_t, rayon_nq_pi_t)
    );

    println!();
    println!(
        "Note: Both use {} threads. Rayon nqueens shown with both",
        workers
    );
    println!("rayon::join (1:1 port) and par_iter (idiomatic Rayon) styles.");
    println!("Lower is better. 'ratio' compares lace-native to the Rayon column.");
    println!();
}
