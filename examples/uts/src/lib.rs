//! UTS (Unbalanced Tree Search) tree generation library.
//!
//! Provides the SHA-1 based RNG and tree configuration types used by the
//! UTS benchmark. This module is shared across the UTS example, the scaling
//! benchmark, and the Rayon comparison.

use sha1::{Digest, Sha1};
use std::f64::consts::PI;

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

/// Extract a random value from the state (last 4 bytes, big-endian, masked).
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
// UTS tree types and shape functions
// ═══════════════════════════════════════════════════════════════════════════════

const MAXNUMCHILDREN: i32 = 100;

#[derive(Clone, Copy, PartialEq)]
pub enum TreeType {
    Binomial,
    Geometric,
    Hybrid,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum GeoShape {
    Linear,
    ExpDec,
    Cyclic,
    Fixed,
}

#[derive(Clone)]
pub struct UtsConfig {
    pub name: &'static str,
    pub tree_type: TreeType,
    pub b_0: f64,
    pub gen_mx: i32,
    pub shape: GeoShape,
    pub seed: i32,
    pub non_leaf_prob: f64,
    pub non_leaf_bf: i32,
    pub shift_depth: f64,
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

/// Compute number of children for a node.
///
/// Matches the C `uts_numChildren` exactly, including the special BIN root
/// handling where the root gets `floor(b_0)` children directly.
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

/// Determine the node type for children of a given node.
pub fn child_type(depth: i32, parent_type: TreeType, cfg: &UtsConfig) -> TreeType {
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

/// Determine the node type at a given depth for a config.
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
// Standard UTS configurations from sample_trees.sh
// ═══════════════════════════════════════════════════════════════════════════════

// Small (~4M nodes)
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

// Large (~100M nodes)
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

// Extra large (~1.6B nodes)
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

// Extra extra large (~3-10B nodes)
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

pub fn all_configs() -> Vec<&'static UtsConfig> {
    vec![
        &T1, &T5, &T2, &T3, &T4, &T1L, &T2L, &T3L, &T1XL, &T1XXL, &T3XXL,
    ]
}
