include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

use sha1::{Digest, Sha1};

// ── Task implementations ──────────────────────────

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

// ── UTS support (T1 config only) ─────────────────

type RngState = [u8; 20];

#[allow(dead_code)]
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

/// UTS T1: geometric, fixed shape, b=4, depth=10, seed=19
fn uts_num_children_t1(state: &RngState, depth: i32) -> i32 {
    let b_i: f64 = if depth < 10 { 4.0 } else { 0.0 };
    let p: f64 = 1.0 / (1.0 + b_i);
    let h = rng_rand(state);
    let u: f64 = rng_to_prob(h);
    let num = ((1.0_f64 - u).ln() / (1.0_f64 - p).ln()).floor() as i32;
    num.min(100)
}

fn uts(w: &Worker, state: *const u8, depth: i32) -> i64 {
    let parent_state: RngState = unsafe {
        let mut s = [0u8; 20];
        std::ptr::copy_nonoverlapping(state, s.as_mut_ptr(), 20);
        s
    };

    let num_children = uts_num_children_t1(&parent_state, depth);
    if num_children == 0 {
        return 1;
    }

    // Keep all child states alive until after all syncs
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

// ── Tests ─────────────────────────────────────────
// Lace is a process-global resource. Tests are serialized via LACE_MUTEX
// so they work correctly even with parallel test threads.

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LACE_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn fib_correctness() {
        let _lock = LACE_MUTEX.lock().unwrap();

        lace_native::start(1, 0, 0);

        assert_eq!(fib_run(0), 0);
        assert_eq!(fib_run(1), 1);
        assert_eq!(fib_run(2), 1);
        assert_eq!(fib_run(5), 5);
        assert_eq!(fib_run(10), 55);
        assert_eq!(fib_run(20), 6765);
        assert_eq!(fib_run(30), 832040);

        lace_native::stop();
    }

    #[test]
    fn nqueens_correctness() {
        let _lock = LACE_MUTEX.lock().unwrap();

        lace_native::start(1, 0, 0);

        let expected: &[(i32, i64)] = &[
            (1, 1),
            (2, 0),
            (3, 0),
            (4, 2),
            (5, 10),
            (6, 4),
            (7, 40),
            (8, 92),
            (9, 352),
            (10, 724),
            (11, 2680),
            (12, 14200),
        ];

        for &(n, expect) in expected {
            let result = nqueens_run(std::ptr::null(), n, -1, 0);
            assert_eq!(result, expect, "nqueens({}) failed", n);
        }

        lace_native::stop();
    }

    #[test]
    fn uts_t1_correctness() {
        let _lock = LACE_MUTEX.lock().unwrap();

        lace_native::start(1, 0, 0);

        let root = rng_init(19);
        let nodes = uts_run(root.as_ptr(), 0);
        assert_eq!(nodes, 4130071, "UTS T1 should have 4130071 nodes");

        lace_native::stop();
    }

    #[test]
    fn lifecycle() {
        let _lock = LACE_MUTEX.lock().unwrap();

        assert!(!lace_native::is_running());

        lace_native::start(2, 0, 0);
        assert!(lace_native::is_running());
        assert!(lace_native::worker_count() >= 1);
        assert_eq!(fib_run(10), 55);
        lace_native::stop();

        assert!(!lace_native::is_running());

        lace_native::start(1, 0, 0);
        assert!(lace_native::is_running());
        assert_eq!(lace_native::worker_count(), 1);
        assert_eq!(fib_run(10), 55);
        lace_native::stop();
    }

    #[test]
    fn nqueens_uses_c_style_sync() {
        let _lock = LACE_MUTEX.lock().unwrap();

        lace_native::start(1, 0, 0);
        assert_eq!(nqueens_run(std::ptr::null(), 8, -1, 0), 92);
        lace_native::stop();
    }
}
