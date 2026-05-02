use lace_example_nqueens::*;
use std::time::Instant;

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
            "-w" => {
                i += 1;
                workers = args[i].parse().expect("-w requires a number");
            }
            "-q" => {
                i += 1;
                dqsize = args[i].parse().expect("-q requires a number");
            }
            "-h" | "--help" => {
                usage(&args[0]);
                return;
            }
            _ => {
                n = args[i].parse().expect("expected a number for n");
            }
        }
        i += 1;
    }

    lace_native::start(workers, dqsize, 0);
    println!(
        "Running nqueens n={} with {} workers...",
        n,
        lace_native::worker_count()
    );

    let t = Instant::now();
    let result = nqueens_run(std::ptr::null(), n, -1, 0);
    let elapsed = t.elapsed();

    println!("Result: nqueens({}) = {}", n, result);
    println!("Time: {:.6}s", elapsed.as_secs_f64());

    lace_native::stop();
}
