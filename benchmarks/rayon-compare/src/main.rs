//! Side-by-side comparison of lace-native vs Rayon on fork-join workloads.
//!
//! Three benchmarks:
//! - **Fibonacci**: classic binary recursion (tests raw fork-join overhead)
//! - **N-Queens**: multi-way spawn (tests N-ary parallelism)
//! - **UTS**: Unbalanced Tree Search (tests irregular load balancing)

use sha1::{Digest, Sha1};
use std::time::Instant;

include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

// ═══════════════════════════════════════════════════════════════════════════════
// SHA-1 based RNG (matches UTS reference implementation)
// ═══════════════════════════════════════════════════════════════════════════════

/// UTS RNG state: 20 bytes (SHA-1 digest).
type RngState = [u8; 20];

/// Initialise root state from an integer seed.
fn rng_init(seed: i32) -> RngState {
    let mut input = [0u8; 20];
    input[16] = (seed >> 24) as u8;
    input[17] = (seed >> 16) as u8;
    input[18] = (seed >> 8) as u8;
    input[19] = seed as u8;

    let mut hasher = Sha1::new();
    hasher.update(input);
    let result = hasher.finalize();
    let mut state = [0u8; 20];
    state.copy_from_slice(&result);
    state
}

/// Derive child state from parent state + spawn number.
fn rng_spawn(parent: &RngState, spawn_number: i32) -> RngState {
    let bytes = [
        (spawn_number >> 24) as u8,
        (spawn_number >> 16) as u8,
        (spawn_number >> 8) as u8,
        spawn_number as u8,
    ];

    let mut hasher = Sha1::new();
    hasher.update(parent);
    hasher.update(bytes);
    let result = hasher.finalize();
    let mut state = [0u8; 20];
    state.copy_from_slice(&result);
    state
}

/// Extract a random value from the state (last 4 bytes, big-endian, masked).
fn rng_rand(state: &RngState) -> i32 {
    let b = ((state[16] as u32) << 24)
        | ((state[17] as u32) << 16)
        | ((state[18] as u32) << 8)
        | (state[19] as u32);
    (b & 0x7fff_ffff) as i32
}

fn rng_to_prob(n: i32) -> f64 {
    (n as f64) / 2147483648.0
}

// ═══════════════════════════════════════════════════════════════════════════════
// UTS tree generation — T3L (binomial distribution)
// ═══════════════════════════════════════════════════════════════════════════════

/// UTS T3L: binomial, b_0=2000, q=0.200014, m=5, seed=7
/// Expected: 111,345,631 nodes
const UTS_B_0: f64 = 2000.0;
const UTS_Q: f64 = 0.200014;
const UTS_M: i32 = 5;
const UTS_SEED: i32 = 7;

fn uts_num_children(state: &RngState, depth: i32) -> i32 {
    if depth == 0 {
        // BIN root gets floor(b_0) children directly
        return UTS_B_0.floor() as i32;
    }
    let v = rng_rand(state);
    let d: f64 = rng_to_prob(v);
    if d < UTS_Q {
        UTS_M
    } else {
        0
    }
}

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

fn uts(w: &Worker, state: *const u8, depth: i32) -> i64 {
    let parent_state: RngState = unsafe {
        let mut s = [0u8; 20];
        std::ptr::copy_nonoverlapping(state, s.as_mut_ptr(), 20);
        s
    };

    let num_children = uts_num_children(&parent_state, depth);
    if num_children == 0 {
        return 1; // leaf
    }

    // Compute all child states BEFORE spawning — the spawned tasks hold
    // raw pointers into this Vec, so it must stay alive until all syncs.
    let child_states: Vec<RngState> = (0..num_children)
        .map(|i| rng_spawn(&parent_state, i))
        .collect();

    for cs in &child_states {
        let _ = uts_spawn(w, cs.as_ptr(), depth + 1);
    }

    let mut count = 1i64; // this node
    for _ in 0..num_children {
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
        let num_children = uts_num_children(state, depth);
        if num_children == 0 {
            return 1;
        }

        let count: i64 = (0..num_children)
            .into_par_iter()
            .map(|i| {
                let child_state = rng_spawn(state, i);
                uts(&child_state, depth + 1)
            })
            .sum();

        count + 1 // +1 for this node
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

    // Detect worker count
    lace_native::start(n_workers, 0, 0);
    let workers = lace_native::worker_count();
    lace_native::stop();

    // Configure Rayon with same thread count and large stack (T3L reaches depth ~17K)
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

    // UTS T3L: -t 0 -b 2000 -q 0.200014 -m 5 -r 7 (binomial, ~111M nodes)
    let root_state = rng_init(UTS_SEED);

    // ── Lace benchmarks ──

    lace_native::start(workers, 0, 0);
    let (lace_fib, lace_fib_t) = time_it(|| fib_run(fib_n));
    lace_native::stop();
    assert_eq!(lace_fib, fib_expect, "Lace fib wrong");

    lace_native::start(workers, 0, 0);
    let (lace_nq, lace_nq_t) = time_it(|| nqueens_run(std::ptr::null(), nq_n, -1, 0));
    lace_native::stop();
    assert_eq!(lace_nq, nq_expect, "Lace nqueens wrong");

    lace_native::start(workers, 0, 64 * 1024 * 1024);
    let (lace_uts, lace_uts_t) = time_it(|| uts_run(root_state.as_ptr(), 0));
    lace_native::stop();

    // ── Rayon benchmarks ──

    let (rayon_fib, rayon_fib_t) = pool.install(|| time_it(|| rayon_impl::fib(fib_n)));
    assert_eq!(rayon_fib, fib_expect, "Rayon fib wrong");

    let (rayon_nq, rayon_nq_t) = pool.install(|| time_it(|| rayon_impl::nqueens(&[], nq_n, -1, 0)));
    assert_eq!(rayon_nq, nq_expect, "Rayon nqueens wrong");

    let (rayon_uts, rayon_uts_t) = pool.install(|| time_it(|| rayon_impl::uts(&root_state, 0)));

    // Verify UTS results match
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
    println!("Rayon nqueens uses par_iter (idiomatic). UTS uses T3L config (binomial/b=2000/q=0.200/m=5/seed=7).");
    println!();
}
