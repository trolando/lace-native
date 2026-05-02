use sha1::{Digest, Sha1};
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

// ── UTS (T3L: binomial, b=2000, q=0.200014, m=5, seed=7) ──

type RngState = [u8; 20];

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

fn rng_spawn_state(parent: &RngState, spawn_number: i32) -> RngState {
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

const UTS_B_0: f64 = 2000.0;
const UTS_Q: f64 = 0.200014;
const UTS_M: i32 = 5;

fn uts_num_children(state: &RngState, depth: i32) -> i32 {
    if depth == 0 {
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

fn uts(w: &Worker, state: *const u8, depth: i32) -> i64 {
    let parent_state: RngState = unsafe {
        let mut s = [0u8; 20];
        std::ptr::copy_nonoverlapping(state, s.as_mut_ptr(), 20);
        s
    };

    let num_children = uts_num_children(&parent_state, depth);
    if num_children == 0 {
        return 1;
    }

    let child_states: Vec<RngState> = (0..num_children)
        .map(|i| rng_spawn_state(&parent_state, i))
        .collect();

    for cs in &child_states {
        let _ = uts_spawn(w, cs.as_ptr(), depth + 1);
    }

    let mut count = 1i64;
    for _ in 0..num_children {
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

    // Determine available workers
    lace_native::start(max_workers, 0, 0);
    let avail = lace_native::worker_count();
    lace_native::stop();

    let fib_n = 42i32;
    let fib_expect = 267914296i32;
    let nq_n = 13i32;
    let nq_expect = 73712i64;
    let uts_expect = 111345631i64;
    let uts_root = rng_init(7);

    println!("lace-native scaling benchmark");
    println!("=============================");
    println!();
    println!("Available workers: {}", avail);
    println!();

    // Worker counts: 1, 2, 4, ..., avail
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
        // Use 64MB stack for UTS T3L (depth ~17K)
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
    println!(
        "UTS T3L: binomial, b=2000, q=0.200, m=5, seed=7, {} nodes",
        uts_expect
    );
    println!();
}
