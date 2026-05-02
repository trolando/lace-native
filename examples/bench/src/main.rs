use std::time::Instant;

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

    lace_native::start(max_workers, 0, 0);
    let avail = lace_native::worker_count();
    lace_native::stop();

    let fib_expect = 267914296i32;
    let nq_expect = 73712i64;
    let uts_expect = 111345631i64;

    lace_example_uts::set_uts_config(&lace_example_uts::T3L);
    let uts_root = lace_example_uts::rng_init(lace_example_uts::T3L.seed);

    println!("lace-native scaling benchmark");
    println!("=============================");
    println!();
    println!("Available workers: {}", avail);
    println!();

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
        lace_native::start(nw, 0, 64 * 1024 * 1024);

        let fib_time = bench("fib", fib_expect, || lace_example_fib::fib_run(42));
        let nq_time = bench("nqueens", nq_expect, || {
            lace_example_nqueens::nqueens_run(std::ptr::null(), 13, -1, 0)
        });
        let uts_time = bench("uts", uts_expect, || {
            lace_example_uts::uts_run(uts_root.as_ptr(), 0)
        });

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
    println!("UTS T3L: binomial, {} nodes", uts_expect);
    println!();
}
