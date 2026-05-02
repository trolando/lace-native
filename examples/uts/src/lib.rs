//! UTS (Unbalanced Tree Search) benchmark for Lace.
//!
//! Provides the SHA-1 based RNG, all standard tree configurations,
//! and the Lace task implementation. Used as a library by bench,
//! criterion, and rayon-compare.

use sha1::{Digest, Sha1};
use std::f64::consts::PI;

include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

// ═══════════════════════════════════════════════════════════════════════════════
// SHA-1 based RNG (matches UTS reference implementation bit-for-bit)
// ═══════════════════════════════════════════════════════════════════════════════

/// UTS RNG state: 20 bytes (SHA-1 digest).
pub type RngState = [u8; 20];

/// Initialise root state from an integer seed.
pub fn rng_init(seed: i32) -> RngState {
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

/// Derive child state from parent state + spawn number.
pub fn rng_spawn(parent: &RngState, spawn_number: i32) -> RngState {
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

/// Extract a random value from the state.
pub fn rng_rand(state: &RngState) -> i32 {
    let b = ((state[16] as u32) << 24)
        | ((state[17] as u32) << 16)
        | ((state[18] as u32) << 8)
        | (state[19] as u32);
    (b & 0x7fff_ffff) as i32
}

/// Convert RNG value to probability in [0, 1).
pub fn rng_to_prob(n: i32) -> f64 {
    (n as f64) / 2147483648.0
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tree types and shape functions
// ═══════════════════════════════════════════════════════════════════════════════

const MAXNUMCHILDREN: i32 = 100;

/// UTS tree type.
#[derive(Clone, Copy, PartialEq)]
pub enum TreeType {
    Binomial,
    Geometric,
    Hybrid,
}

/// Shape function for geometric trees.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum GeoShape {
    Linear,
    ExpDec,
    Cyclic,
    Fixed,
}

/// Configuration for a UTS tree instance.
#[derive(Clone)]
pub struct UtsConfig {
    /// Short name (e.g. "T1", "T3L").
    pub name: &'static str,
    /// Tree generation type.
    pub tree_type: TreeType,
    /// Root branching factor.
    pub b_0: f64,
    /// Maximum depth for geometric trees.
    pub gen_mx: i32,
    /// Shape function for geometric trees.
    pub shape: GeoShape,
    /// RNG seed.
    pub seed: i32,
    /// Non-leaf probability for binomial trees (q).
    pub non_leaf_prob: f64,
    /// Non-leaf branching factor for binomial trees (m).
    pub non_leaf_bf: i32,
    /// Hybrid tree: depth fraction where GEO switches to BIN.
    pub shift_depth: f64,
}

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
    let u: f64 = rng_to_prob(rng_rand(state));
    ((1.0_f64 - u).ln() / (1.0_f64 - p).ln())
        .floor()
        .min(MAXNUMCHILDREN as f64) as i32
}

fn num_children_bin(state: &RngState, cfg: &UtsConfig) -> i32 {
    let d: f64 = rng_to_prob(rng_rand(state));
    if d < cfg.non_leaf_prob {
        cfg.non_leaf_bf
    } else {
        0
    }
}

/// Compute number of children for a node.
pub fn num_children(state: &RngState, depth: i32, node_type: TreeType, cfg: &UtsConfig) -> i32 {
    let nc = match node_type {
        TreeType::Binomial => {
            if depth == 0 {
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
    if depth == 0 && node_type == TreeType::Binomial {
        nc.min(cfg.b_0.ceil() as i32)
    } else {
        nc.min(MAXNUMCHILDREN)
    }
}

/// Determine node type at a given depth.
pub fn node_type_at_depth(depth: i32, cfg: &UtsConfig) -> TreeType {
    if cfg.tree_type == TreeType::Hybrid {
        if (depth as f64) < cfg.shift_depth * (cfg.gen_mx as f64) {
            TreeType::Hybrid
        } else {
            TreeType::Binomial
        }
    } else {
        cfg.tree_type
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Standard configurations from sample_trees.sh
// ═══════════════════════════════════════════════════════════════════════════════

/// T1: Geometric/fixed, ~4.1M nodes, b=4, d=10, seed=19.
pub const T1: UtsConfig = UtsConfig {
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
/// T5: Geometric/linear, ~4.1M nodes, b=4, d=20, seed=34.
pub const T5: UtsConfig = UtsConfig {
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
/// T2: Geometric/cyclic, ~4.1M nodes, b=6, d=16, seed=502.
pub const T2: UtsConfig = UtsConfig {
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
/// T3: Binomial (unbalanced), ~4.1M nodes, q=0.125, m=8, seed=42.
pub const T3: UtsConfig = UtsConfig {
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
/// T4: Hybrid (GEO→BIN), ~4.1M nodes, b=6, d=16, seed=1.
pub const T4: UtsConfig = UtsConfig {
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

/// T1L: Geometric/fixed, ~102M nodes, b=4, d=13, seed=29.
pub const T1L: UtsConfig = UtsConfig {
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
/// T2L: Geometric/cyclic, ~97M nodes, b=7, d=23, seed=220.
pub const T2L: UtsConfig = UtsConfig {
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
/// T3L: Binomial (unbalanced), ~111M nodes, q=0.200, m=5, seed=7.
pub const T3L: UtsConfig = UtsConfig {
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

/// T1XL: Geometric/fixed, ~1.6B nodes, b=4, d=15, seed=29.
pub const T1XL: UtsConfig = UtsConfig {
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
/// T1XXL: Geometric/fixed, ~4.2B nodes, b=4, d=15, seed=19.
pub const T1XXL: UtsConfig = UtsConfig {
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
/// T3XXL: Binomial (unbalanced), ~2.8B nodes, q=0.500, m=2, seed=316.
pub const T3XXL: UtsConfig = UtsConfig {
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

/// All standard UTS configurations.
pub fn all_configs() -> Vec<&'static UtsConfig> {
    vec![
        &T1, &T5, &T2, &T3, &T4, &T1L, &T2L, &T3L, &T1XL, &T1XXL, &T3XXL,
    ]
}

// ═══════════════════════════════════════════════════════════════════════════════
// Global config (set before each run, read by task body)
// ═══════════════════════════════════════════════════════════════════════════════

static UTS_CFG: std::sync::RwLock<Option<UtsConfig>> = std::sync::RwLock::new(None);

/// Set the UTS config. Call before `lace_native::start()`.
pub fn set_uts_config(cfg: &UtsConfig) {
    *UTS_CFG.write().unwrap() = Some(cfg.clone());
}

// ═══════════════════════════════════════════════════════════════════════════════
// Lace UTS task
// ═══════════════════════════════════════════════════════════════════════════════

/// UTS task body. Uses the config set by [`set_uts_config`].
pub(crate) fn uts(w: &Worker, state: *const u8, depth: i32) -> i64 {
    let parent_state: RngState = unsafe {
        let mut s = [0u8; 20];
        std::ptr::copy_nonoverlapping(state, s.as_mut_ptr(), 20);
        s
    };

    let nc = {
        let guard = UTS_CFG.read().unwrap();
        let cfg = guard
            .as_ref()
            .expect("call set_uts_config before running UTS");
        let nt = node_type_at_depth(depth, cfg);
        num_children(&parent_state, depth, nt, cfg)
    }; // lock released here

    if nc == 0 {
        return 1;
    }

    let child_states: Vec<RngState> = (0..nc).map(|i| rng_spawn(&parent_state, i)).collect();

    for cs in &child_states {
        let _ = uts_spawn(w, cs.as_ptr(), depth + 1);
    }

    let mut count = 1i64;
    for _ in 0..nc {
        count += uts_sync(w);
    }
    count
}
