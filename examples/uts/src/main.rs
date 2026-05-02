use lace_example_uts::*;
use std::time::Instant;

include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

// ═══════════════════════════════════════════════════════════════════════════════
// Global config (read-only during execution, set before start)
// ═══════════════════════════════════════════════════════════════════════════════

static UTS_CFG: std::sync::OnceLock<UtsConfig> = std::sync::OnceLock::new();

fn get_config() -> &'static UtsConfig {
    UTS_CFG.get().expect("UTS config not set")
}

// ═══════════════════════════════════════════════════════════════════════════════
// Lace UTS task
// ═══════════════════════════════════════════════════════════════════════════════

fn uts(w: &Worker, state: *const u8, depth: i32) -> i64 {
    let parent_state: RngState = unsafe {
        let mut s = [0u8; 20];
        std::ptr::copy_nonoverlapping(state, s.as_mut_ptr(), 20);
        s
    };

    let cfg = get_config();
    let nt = node_type_at_depth(depth, cfg);
    let nc = num_children(&parent_state, depth, nt, cfg);

    if nc == 0 {
        return 1;
    }

    let child_states: Vec<RngState> = (0..nc).map(|i| rng_spawn(&parent_state, i)).collect();

    let _ = child_type(depth, nt, cfg); // child type inferred from depth

    for cs in &child_states {
        let _ = uts_spawn(w, cs.as_ptr(), depth + 1);
    }

    let mut count = 1i64;
    for _ in 0..nc {
        count += uts_sync(w);
    }
    count
}

// ═══════════════════════════════════════════════════════════════════════════════
// Main
// ═══════════════════════════════════════════════════════════════════════════════

fn usage(name: &str) {
    eprintln!("Usage: {} [-w <workers>] [-q <dqsize>] [CONFIG]", name);
    eprintln!();
    eprintln!("  -w <workers>  Number of worker threads (0 = auto, default: 0)");
    eprintln!("  -q <dqsize>   Task deque size per worker (default: 0 = auto)");
    eprintln!();
    eprintln!("Configurations:");
    eprintln!("  Small (~4M nodes):");
    eprintln!("    T1    Geometric fixed       (4,130,071 nodes)");
    eprintln!("    T5    Geometric linear       (4,147,582 nodes)");
    eprintln!("    T2    Geometric cyclic       (4,117,769 nodes)");
    eprintln!("    T3    Binomial               (4,112,897 nodes)");
    eprintln!("    T4    Hybrid (GEO→BIN)       (4,132,453 nodes)");
    eprintln!();
    eprintln!("  Large (~100M nodes):");
    eprintln!("    T1L   Geometric fixed       (102,181,082 nodes)");
    eprintln!("    T2L   Geometric cyclic       (96,793,510 nodes)");
    eprintln!("    T3L   Binomial               (111,345,631 nodes)");
    eprintln!();
    eprintln!("  Extra large: T1XL, T1XXL, T3XXL");
    eprintln!();
    eprintln!("  Default: T3L");
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

    let root_state = rng_init(config.seed);
    let _ = UTS_CFG.set(config.clone());

    lace_native::start(workers, dqsize, 64 * 1024 * 1024);
    let wc = lace_native::worker_count();

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
