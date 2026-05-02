use sha1::{Digest, Sha1};
use std::f64::consts::PI;
use std::time::Instant;

include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

// ═══════════════════════════════════════════════════════════════════════════════
// SHA-1 based RNG (matches UTS reference implementation bit-for-bit)
// ═══════════════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════════════
// UTS tree types and shape functions
// ═══════════════════════════════════════════════════════════════════════════════

const MAXNUMCHILDREN: i32 = 100;

#[derive(Clone, Copy, PartialEq)]
enum TreeType {
    Binomial,  // 0
    Geometric, // 1
    Hybrid,    // 2
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
enum GeoShape {
    Linear, // 0: b_i = b_0 * (1 - depth/gen_mx)
    ExpDec, // 1: b_i = b_0 * depth^(-ln(b_0)/ln(gen_mx))
    Cyclic, // 2: b_i = b_0^sin(2π·depth/gen_mx)
    Fixed,  // 3: b_i = b_0 if depth < gen_mx, else 0
}

#[derive(Clone)]
struct UtsConfig {
    name: &'static str,
    tree_type: TreeType,
    b_0: f64,
    gen_mx: i32,
    shape: GeoShape,
    seed: i32,
    non_leaf_prob: f64, // q (binomial/hybrid)
    non_leaf_bf: i32,   // m (binomial/hybrid)
    shift_depth: f64,   // hybrid GEO→BIN transition fraction
}

/// Compute number of children for a geometric node.
fn num_children_geo(state: &RngState, depth: i32, cfg: &UtsConfig) -> i32 {
    let b_i: f64 = if depth > 0 {
        match cfg.shape {
            GeoShape::Fixed => {
                if depth < cfg.gen_mx {
                    cfg.b_0
                } else {
                    0.0
                }
            }
            GeoShape::Linear => cfg.b_0 * (1.0 - (depth as f64) / (cfg.gen_mx as f64)),
            GeoShape::ExpDec => {
                cfg.b_0 * (depth as f64).powf(-(cfg.b_0.ln()) / (cfg.gen_mx as f64).ln())
            }
            GeoShape::Cyclic => {
                if depth > 5 * cfg.gen_mx {
                    0.0
                } else {
                    cfg.b_0
                        .powf((2.0 * PI * (depth as f64) / (cfg.gen_mx as f64)).sin())
                }
            }
        }
    } else {
        cfg.b_0
    };

    let p: f64 = 1.0 / (1.0 + b_i);
    let h = rng_rand(state);
    let u: f64 = rng_to_prob(h);
    let num = ((1.0_f64 - u).ln() / (1.0_f64 - p).ln()).floor() as i32;
    num.min(MAXNUMCHILDREN)
}

/// Compute number of children for a binomial node.
fn num_children_bin(state: &RngState, cfg: &UtsConfig) -> i32 {
    let v = rng_rand(state);
    let d: f64 = rng_to_prob(v);
    if d < cfg.non_leaf_prob {
        cfg.non_leaf_bf
    } else {
        0
    }
}

/// Compute number of children based on tree type and depth.
/// Matches the C uts_numChildren exactly, including the special BIN root handling.
fn num_children(state: &RngState, depth: i32, node_type: TreeType, cfg: &UtsConfig) -> i32 {
    let nc = match node_type {
        TreeType::Binomial => {
            if depth == 0 {
                // BIN root gets floor(b_0) children, NOT the probabilistic check
                cfg.b_0.floor() as i32
            } else {
                num_children_bin(state, cfg)
            }
        }
        TreeType::Geometric => num_children_geo(state, depth, cfg),
        TreeType::Hybrid => {
            if depth < (cfg.shift_depth * cfg.gen_mx as f64) as i32 {
                num_children_geo(state, depth, cfg)
            } else {
                num_children_bin(state, cfg)
            }
        }
    };

    // Cap: only a BIN root can exceed MAXNUMCHILDREN
    if depth == 0 && node_type == TreeType::Binomial {
        nc.min(cfg.b_0.ceil() as i32)
    } else {
        nc.min(MAXNUMCHILDREN)
    }
}

/// Determine child type (only relevant for hybrid trees).
fn child_type(depth: i32, parent_type: TreeType, cfg: &UtsConfig) -> TreeType {
    match parent_type {
        TreeType::Hybrid => {
            if (depth as f64) < cfg.shift_depth * (cfg.gen_mx as f64) {
                TreeType::Hybrid
            } else {
                TreeType::Binomial
            }
        }
        other => other,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Standard UTS configurations from sample_trees.sh
// ═══════════════════════════════════════════════════════════════════════════════

// Small workloads (~4M nodes)
const T1: UtsConfig = UtsConfig {
    name: "T1",
    tree_type: TreeType::Geometric,
    b_0: 4.0,
    gen_mx: 10,
    shape: GeoShape::Fixed,
    seed: 19,
    non_leaf_prob: 0.0,
    non_leaf_bf: 0,
    shift_depth: 0.0,
};
const T5: UtsConfig = UtsConfig {
    name: "T5",
    tree_type: TreeType::Geometric,
    b_0: 4.0,
    gen_mx: 20,
    shape: GeoShape::Linear,
    seed: 34,
    non_leaf_prob: 0.0,
    non_leaf_bf: 0,
    shift_depth: 0.0,
};
const T2: UtsConfig = UtsConfig {
    name: "T2",
    tree_type: TreeType::Geometric,
    b_0: 6.0,
    gen_mx: 16,
    shape: GeoShape::Cyclic,
    seed: 502,
    non_leaf_prob: 0.0,
    non_leaf_bf: 0,
    shift_depth: 0.0,
};
const T3: UtsConfig = UtsConfig {
    name: "T3",
    tree_type: TreeType::Binomial,
    b_0: 2000.0,
    gen_mx: 0,
    shape: GeoShape::Fixed,
    seed: 42,
    non_leaf_prob: 0.124875,
    non_leaf_bf: 8,
    shift_depth: 0.0,
};
const T4: UtsConfig = UtsConfig {
    name: "T4",
    tree_type: TreeType::Hybrid,
    b_0: 6.0,
    gen_mx: 16,
    shape: GeoShape::Linear,
    seed: 1,
    non_leaf_prob: 0.234375,
    non_leaf_bf: 4,
    shift_depth: 0.5,
};

// Large workloads (~100M nodes)
const T1L: UtsConfig = UtsConfig {
    name: "T1L",
    tree_type: TreeType::Geometric,
    b_0: 4.0,
    gen_mx: 13,
    shape: GeoShape::Fixed,
    seed: 29,
    non_leaf_prob: 0.0,
    non_leaf_bf: 0,
    shift_depth: 0.0,
};
const T2L: UtsConfig = UtsConfig {
    name: "T2L",
    tree_type: TreeType::Geometric,
    b_0: 7.0,
    gen_mx: 23,
    shape: GeoShape::Cyclic,
    seed: 220,
    non_leaf_prob: 0.0,
    non_leaf_bf: 0,
    shift_depth: 0.0,
};
const T3L: UtsConfig = UtsConfig {
    name: "T3L",
    tree_type: TreeType::Binomial,
    b_0: 2000.0,
    gen_mx: 0,
    shape: GeoShape::Fixed,
    seed: 7,
    non_leaf_prob: 0.200014,
    non_leaf_bf: 5,
    shift_depth: 0.0,
};

// Extra large workloads (~1.6B nodes)
const T1XL: UtsConfig = UtsConfig {
    name: "T1XL",
    tree_type: TreeType::Geometric,
    b_0: 4.0,
    gen_mx: 15,
    shape: GeoShape::Fixed,
    seed: 29,
    non_leaf_prob: 0.0,
    non_leaf_bf: 0,
    shift_depth: 0.0,
};

// Extra extra large workloads (~3-10B nodes)
const T1XXL: UtsConfig = UtsConfig {
    name: "T1XXL",
    tree_type: TreeType::Geometric,
    b_0: 4.0,
    gen_mx: 15,
    shape: GeoShape::Fixed,
    seed: 19,
    non_leaf_prob: 0.0,
    non_leaf_bf: 0,
    shift_depth: 0.0,
};
const T3XXL: UtsConfig = UtsConfig {
    name: "T3XXL",
    tree_type: TreeType::Binomial,
    b_0: 2000.0,
    gen_mx: 0,
    shape: GeoShape::Fixed,
    seed: 316,
    non_leaf_prob: 0.499995,
    non_leaf_bf: 2,
    shift_depth: 0.0,
};

fn all_configs() -> Vec<&'static UtsConfig> {
    vec![
        &T1, &T5, &T2, &T3, &T4, &T1L, &T2L, &T3L, &T1XL, &T1XXL, &T3XXL,
    ]
}

// ═══════════════════════════════════════════════════════════════════════════════
// Thread-local config (accessed from task bodies via UTS_RUN dispatching)
// ═══════════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════════
// Global config (read-only during execution, set before start)
// ═══════════════════════════════════════════════════════════════════════════════

static UTS_CFG: std::sync::OnceLock<UtsConfig> = std::sync::OnceLock::new();

fn set_config(cfg: &UtsConfig) {
    let _ = UTS_CFG.set(cfg.clone());
}

fn with_config<F: FnOnce(&UtsConfig) -> R, R>(f: F) -> R {
    f(UTS_CFG.get().expect("UTS config not set"))
}

// ═══════════════════════════════════════════════════════════════════════════════
// Lace UTS task
// ═══════════════════════════════════════════════════════════════════════════════

/// UTS task. Parameters encode the node: state (20 bytes) and depth.
/// Node type is derived from depth and config (uniform for non-hybrid trees).
fn uts(w: &Worker, state: *const u8, depth: i32) -> i64 {
    let parent_state: RngState = unsafe {
        let mut s = [0u8; 20];
        std::ptr::copy_nonoverlapping(state, s.as_mut_ptr(), 20);
        s
    };

    let (nc, ct) = with_config(|cfg| {
        // For hybrid trees, determine the node type from depth
        let node_type = if cfg.tree_type == TreeType::Hybrid {
            if (depth as f64) < cfg.shift_depth * (cfg.gen_mx as f64) {
                TreeType::Hybrid
            } else {
                TreeType::Binomial
            }
        } else {
            cfg.tree_type
        };
        let nc = num_children(&parent_state, depth, node_type, cfg);
        let ct = child_type(depth, node_type, cfg);
        (nc, ct)
    });

    if nc == 0 {
        return 1;
    }

    // Compute all child states BEFORE spawning — the spawned tasks hold
    // raw pointers into this Vec, so it must stay alive until all syncs.
    let child_states: Vec<RngState> = (0..nc).map(|i| rng_spawn_state(&parent_state, i)).collect();

    // computeGranularity > 1 would add extra rng_spawn calls here
    // (omitted — standard configs all use granularity 1)
    let _ = ct; // child type is inferred from depth in each child call

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
    eprintln!("    T1    Geometric fixed       b=4  d=10 seed=19  (4,130,071 nodes)");
    eprintln!("    T5    Geometric linear       b=4  d=20 seed=34  (4,147,582 nodes)");
    eprintln!("    T2    Geometric cyclic       b=6  d=16 seed=502 (4,117,769 nodes)");
    eprintln!("    T3    Binomial               b=2000 q=0.125 m=8 (4,112,897 nodes)");
    eprintln!("    T4    Hybrid (GEO→BIN)       b=6  d=16          (4,132,453 nodes)");
    eprintln!();
    eprintln!("  Large (~100M nodes):");
    eprintln!("    T1L   Geometric fixed       b=4  d=13 seed=29  (102,181,082 nodes)");
    eprintln!("    T2L   Geometric cyclic       b=7  d=23 seed=220 (96,793,510 nodes)");
    eprintln!("    T3L   Binomial               b=2000 q=0.200 m=5 (111,345,631 nodes)");
    eprintln!();
    eprintln!("  Extra large (~1.6B nodes):");
    eprintln!("    T1XL  Geometric fixed       b=4  d=15 seed=29");
    eprintln!();
    eprintln!("  Extra extra large (~3-10B nodes):");
    eprintln!("    T1XXL Geometric fixed       b=4  d=15 seed=19");
    eprintln!("    T3XXL Binomial               b=2000 q=0.500 m=2");
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
                let found = all_configs().into_iter().find(|c| c.name == upper);
                match found {
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

    lace_native::start(workers, dqsize, 64 * 1024 * 1024);
    let wc = lace_native::worker_count();

    set_config(config);

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
