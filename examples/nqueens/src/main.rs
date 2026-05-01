use std::time::Instant;

include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

/// N-Queens task body.
///
/// Given queens already placed in array `a` (length `d`), attempt to place
/// a queen at row `d`, column `i` on an `n×n` board. Returns the number
/// of valid complete placements reachable from this configuration.
///
/// Uses multiple spawn/sync in a loop — this is where the C-style
/// standalone `nqueens_sync(w)` shines over guards.
fn nqueens(w: &Worker, a: *const i32, n: i32, d: i32, i: i32) -> i64 {
    // Copy existing queens and check for conflicts with new position
    let mut aa = Vec::with_capacity((d + 2) as usize);

    for j in 0..d {
        let aj = unsafe { *a.add(j as usize) };
        aa.push(aj);

        let diff = aj - i;
        let dist = d - j;
        if diff == 0 || dist == diff || dist + diff == 0 {
            return 0; // conflict
        }
    }

    // Place the queen (d == -1 on the initial call, so skip)
    if d >= 0 {
        aa.push(i);
    }

    let d = d + 1;

    // All queens placed? Found a solution.
    if d == n {
        return 1;
    }

    // Spawn a task for each column on the next row
    for k in 0..n {
        let _ = nqueens_spawn(w, aa.as_ptr(), n, d, k);
    }

    // Sync all and sum results
    // Note: aa stays alive on the stack until all syncs complete,
    // so the pointer passed to spawned tasks remains valid.
    let mut sum = 0i64;
    for _ in 0..n {
        sum += nqueens_sync(w);
    }
    sum
}

fn usage(name: &str) {
    eprintln!("Usage: {} [-w <workers>] [-q <dqsize>] [n]", name);
    eprintln!();
    eprintln!("  -w <workers>  Number of worker threads (0 = auto, default: 0)");
    eprintln!("  -q <dqsize>   Task deque size per worker (default: 0 = auto)");
    eprintln!("  n             Board size (default: 14)");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut workers: u32 = 0;
    let mut dqsize: usize = 0;
    let mut n: i32 = 14;
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "-w" => { i += 1; workers = args[i].parse().expect("-w requires a number"); }
            "-q" => { i += 1; dqsize = args[i].parse().expect("-q requires a number"); }
            "-h" | "--help" => { usage(&args[0]); return; }
            _ => { n = args[i].parse().expect("expected a number for n"); }
        }
        i += 1;
    }

    lace_native::start(workers, dqsize, 0);
    println!("Running nqueens n={} with {} workers...", n, lace_native::worker_count());

    let t = Instant::now();
    let result = nqueens_run(std::ptr::null(), n, -1, 0);
    let elapsed = t.elapsed();

    println!("Result: nqueens({}) = {}", n, result);
    println!("Time: {:.6}s", elapsed.as_secs_f64());

    lace_native::stop();
}
