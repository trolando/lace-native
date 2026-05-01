include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

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
