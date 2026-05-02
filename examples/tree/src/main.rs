//! Parallel tree traversal using Lace method tasks.
//!
//! Demonstrates:
//! - `impl Type { task name(&self, ...) }` syntax in tasks.def
//! - Shared `&self` access: multiple workers traverse concurrently
//! - Borrow safety: the guard holds `&Tree`, preventing mutation

use std::time::Instant;

/// A simple binary tree with a value at each node.
pub struct Tree {
    value: i64,
    left: Option<Box<Tree>>,
    right: Option<Box<Tree>>,
}

include!(concat!(env!("OUT_DIR"), "/lace_tasks.rs"));

impl Tree {
    fn leaf(value: i64) -> Box<Tree> {
        Box::new(Tree {
            value,
            left: None,
            right: None,
        })
    }

    fn node(value: i64, left: Box<Tree>, right: Box<Tree>) -> Box<Tree> {
        Box::new(Tree {
            value,
            left: Some(left),
            right: Some(right),
        })
    }

    /// Build a balanced binary tree of given depth.
    fn balanced(depth: i32, next_val: &mut i64) -> Box<Tree> {
        let v = *next_val;
        *next_val += 1;
        if depth == 0 {
            Tree::leaf(v)
        } else {
            let left = Tree::balanced(depth - 1, next_val);
            let right = Tree::balanced(depth - 1, next_val);
            Tree::node(v, left, right)
        }
    }

    /// Count nodes — parallel via Lace method task.
    ///
    /// The generated `count_spawn` takes `&self`, so multiple workers
    /// can traverse different subtrees concurrently. The borrow checker
    /// ensures no one mutates the tree during traversal.
    fn count(&self, w: &Worker) -> i64 {
        match (&self.left, &self.right) {
            (Some(left), Some(right)) => {
                let guard = left.count_spawn(w);
                let r = right.count(w);
                let l = guard.sync(w);
                1 + l + r
            }
            (Some(child), None) | (None, Some(child)) => 1 + child.count(w),
            (None, None) => 1,
        }
    }

    /// Sum all values — parallel via Lace method task.
    fn sum(&self, w: &Worker) -> i64 {
        match (&self.left, &self.right) {
            (Some(left), Some(right)) => {
                let guard = left.sum_spawn(w);
                let r = right.sum(w);
                let l = guard.sync(w);
                self.value + l + r
            }
            (Some(child), None) | (None, Some(child)) => self.value + child.sum(w),
            (None, None) => self.value,
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut workers: u32 = 0;
    let mut depth: i32 = 24;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-w" => {
                i += 1;
                workers = args[i].parse().expect("-w requires a number");
            }
            "-d" => {
                i += 1;
                depth = args[i].parse().expect("-d requires a number");
            }
            "-h" | "--help" => {
                eprintln!("Usage: {} [-w <workers>] [-d <depth>]", args[0]);
                eprintln!("  Default: depth=24 (~33M nodes)");
                return;
            }
            _ => {
                eprintln!("Unknown: {}", args[i]);
                return;
            }
        }
        i += 1;
    }

    println!("Building balanced tree of depth {}...", depth);
    let mut val = 1i64;
    let tree = Tree::balanced(depth, &mut val);
    let expected_nodes = (1i64 << (depth + 1)) - 1;
    let expected_sum = expected_nodes * (expected_nodes + 1) / 2;
    println!(
        "Tree has {} nodes, expected sum = {}",
        expected_nodes, expected_sum
    );

    lace_native::start(workers, 0, 0);
    let wc = lace_native::worker_count();
    println!("Counting with {} workers...", wc);

    let t = Instant::now();
    let count = tree.count_run();
    let count_t = t.elapsed().as_secs_f64();

    let t = Instant::now();
    let sum = tree.sum_run();
    let sum_t = t.elapsed().as_secs_f64();

    assert_eq!(count, expected_nodes, "count mismatch");
    assert_eq!(sum, expected_sum, "sum mismatch");

    println!("Count: {} ({:.4}s)", count, count_t);
    println!("Sum:   {} ({:.4}s)", sum, sum_t);

    lace_native::stop();
}
