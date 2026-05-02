use lace_example_uts as uts_lib;

include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

// ── Task implementations ──────────────────────────

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

fn uts_test(w: &Worker, state: *const u8, depth: i32) -> i64 {
    let parent_state: uts_lib::RngState = unsafe {
        let mut s = [0u8; 20];
        std::ptr::copy_nonoverlapping(state, s.as_mut_ptr(), 20);
        s
    };

    // T1: geometric fixed, b=4, depth=10
    let nc = uts_lib::num_children(
        &parent_state,
        depth,
        uts_lib::node_type_at_depth(depth, &uts_lib::T1),
        &uts_lib::T1,
    );
    if nc == 0 {
        return 1;
    }

    let child_states: Vec<uts_lib::RngState> = (0..nc)
        .map(|i| uts_lib::rng_spawn(&parent_state, i))
        .collect();

    for cs in &child_states {
        let _ = uts_test_spawn(w, cs.as_ptr(), depth + 1);
    }

    let mut count = 1i64;
    for _ in 0..nc {
        count += uts_test_sync(w);
    }
    count
}

// ── Tests ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LACE_MUTEX: Mutex<()> = Mutex::new(());

    // ── Single-worker correctness ──

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

        let root = uts_lib::rng_init(uts_lib::T1.seed);
        let nodes = uts_test_run(root.as_ptr(), 0);
        assert_eq!(nodes, 4130071, "UTS T1 should have 4130071 nodes");

        lace_native::stop();
    }

    // ── Multi-worker correctness (exercises stealing) ──

    #[test]
    fn fib_multi_worker() {
        let _lock = LACE_MUTEX.lock().unwrap();
        lace_native::start(4, 0, 0);

        assert_eq!(fib_run(30), 832040);
        assert_eq!(fib_run(20), 6765);

        lace_native::stop();
    }

    #[test]
    fn nqueens_multi_worker() {
        let _lock = LACE_MUTEX.lock().unwrap();
        lace_native::start(4, 0, 0);

        assert_eq!(nqueens_run(std::ptr::null(), 10, -1, 0), 724);
        assert_eq!(nqueens_run(std::ptr::null(), 12, -1, 0), 14200);

        lace_native::stop();
    }

    #[test]
    fn uts_multi_worker() {
        let _lock = LACE_MUTEX.lock().unwrap();
        lace_native::start(4, 0, 0);

        let root = uts_lib::rng_init(uts_lib::T1.seed);
        let nodes = uts_test_run(root.as_ptr(), 0);
        assert_eq!(
            nodes, 4130071,
            "UTS T1 multi-worker should match single-worker"
        );

        lace_native::stop();
    }

    // ── Lifecycle ──

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

    // ── C-style sync ──

    #[test]
    fn nqueens_uses_c_style_sync() {
        let _lock = LACE_MUTEX.lock().unwrap();
        lace_native::start(1, 0, 0);
        assert_eq!(nqueens_run(std::ptr::null(), 8, -1, 0), 92);
        lace_native::stop();
    }
}
