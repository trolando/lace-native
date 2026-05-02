use lace_example_uts::*;
use std::time::Instant;

fn usage(name: &str) {
    eprintln!("Usage: {} [-w <workers>] [-q <dqsize>] [CONFIG]", name);
    eprintln!();
    eprintln!("  -w <workers>  Number of worker threads (0 = auto, default: 0)");
    eprintln!("  -q <dqsize>   Task deque size per worker (default: 0 = auto)");
    eprintln!();
    eprintln!("Configurations: T1, T5, T2, T3, T4, T1L, T2L, T3L, T1XL, T1XXL, T3XXL");
    eprintln!("Default: T3L");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut workers: u32 = 0;
    let mut dqsize: usize = 0;
    let mut config = &T3L;
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
            name => {
                let upper = name.to_uppercase();
                match all_configs().into_iter().find(|c| c.name == upper) {
                    Some(c) => config = c,
                    None => {
                        eprintln!("Unknown config: {}", args[i]);
                        usage(&args[0]);
                        return;
                    }
                }
            }
        }
        i += 1;
    }

    set_uts_config(config);
    let root_state = rng_init(config.seed);

    let type_str = match config.tree_type {
        TreeType::Binomial => "Binomial",
        TreeType::Geometric => match config.shape {
            GeoShape::Fixed => "Geometric/fixed",
            GeoShape::Linear => "Geometric/linear",
            GeoShape::Cyclic => "Geometric/cyclic",
            GeoShape::ExpDec => "Geometric/expdec",
        },
        TreeType::Hybrid => "Hybrid",
    };

    lace_native::start(workers, dqsize, 64 * 1024 * 1024);
    let wc = lace_native::worker_count();
    println!(
        "UTS {} ({}, b={}, seed={}) with {} workers...",
        config.name, type_str, config.b_0, config.seed, wc
    );

    let t = Instant::now();
    let nodes = uts_run(root_state.as_ptr(), 0);
    let elapsed = t.elapsed().as_secs_f64();

    println!("Nodes: {}", nodes);
    println!("Time:  {:.6}s", elapsed);
    println!(
        "Rate:  {:.2}M nodes/s",
        (nodes as f64) / elapsed / 1_000_000.0
    );

    lace_native::stop();
}
