//! N-Queens task for Lace — demonstrates multi-spawn/sync loops.

include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

/// N-Queens task body.
///
/// Given queens already placed in array `a` (length `d`), attempt to place
/// a queen at row `d`, column `i` on an `n×n` board. Returns the number
/// of valid complete placements reachable from this configuration.
///
/// Uses multiple spawn/sync in a loop — the C-style standalone
/// `nqueens_sync(w)` is essential here since guards would be awkward.
pub(crate) fn nqueens(w: &Worker, a: *const i32, n: i32, d: i32, i: i32) -> i64 {
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
