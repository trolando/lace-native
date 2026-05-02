use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use lace_example_uts as uts_lib;

// ═══════════════════════════════════════════════════════════════════════════════
// Rayon implementations (same algorithms for fair comparison)
// ═══════════════════════════════════════════════════════════════════════════════

mod rayon_impl {
    use super::uts_lib;
    use rayon::prelude::*;

    pub fn fib(n: i32) -> i32 {
        if n < 2 {
            return n;
        }
        let (a, b) = rayon::join(|| fib(n - 1), || fib(n - 2));
        a + b
    }

    pub fn nqueens(a: &[i32], n: i32, d: i32, i: i32) -> i64 {
        let mut aa = Vec::with_capacity((d + 2).max(0) as usize);
        for j in 0..d.max(0) {
            let aj = a[j as usize];
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
        (0..n).into_par_iter().map(|k| nqueens(&aa, n, d, k)).sum()
    }

    pub fn uts(state: &uts_lib::RngState, depth: i32, cfg: &uts_lib::UtsConfig) -> i64 {
        let nt = uts_lib::node_type_at_depth(depth, cfg);
        let nc = uts_lib::num_children(state, depth, nt, cfg);
        if nc == 0 {
            return 1;
        }
        let count: i64 = (0..nc)
            .into_par_iter()
            .map(|i| {
                let child = uts_lib::rng_spawn(state, i);
                uts(&child, depth + 1, cfg)
            })
            .sum();
        count + 1
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn worker_counts() -> Vec<u32> {
    lace_native::start(0, 0, 0);
    let avail = lace_native::worker_count();
    lace_native::stop();
    let mut counts = vec![1u32];
    let mut w = 2;
    while w < avail {
        counts.push(w);
        w *= 2;
    }
    if avail > 1 {
        counts.push(avail);
    }
    counts.dedup();
    counts
}

fn max_workers() -> u32 {
    lace_native::start(0, 0, 0);
    let n = lace_native::worker_count();
    lace_native::stop();
    n
}

fn rayon_pool(n: u32) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new()
        .num_threads(n as usize)
        .stack_size(64 * 1024 * 1024)
        .build()
        .unwrap()
}

// ═══════════════════════════════════════════════════════════════════════════════
// Lace scaling benchmarks
// ═══════════════════════════════════════════════════════════════════════════════

fn bench_fib_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("fib-scaling");
    for nw in worker_counts() {
        group.bench_with_input(
            BenchmarkId::new("lace", format!("{}w", nw)),
            &nw,
            |b, &nw| {
                lace_native::start(nw, 0, 0);
                b.iter(|| {
                    let r = lace_example_fib::fib_run(42);
                    assert_eq!(r, 267914296);
                    r
                });
                lace_native::stop();
            },
        );
    }
    group.finish();
}

fn bench_nqueens_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("nqueens-scaling");
    for nw in worker_counts() {
        group.bench_with_input(
            BenchmarkId::new("lace", format!("{}w", nw)),
            &nw,
            |b, &nw| {
                lace_native::start(nw, 0, 0);
                b.iter(|| {
                    let r = lace_example_nqueens::nqueens_run(std::ptr::null(), 13, -1, 0);
                    assert_eq!(r, 73712);
                    r
                });
                lace_native::stop();
            },
        );
    }
    group.finish();
}

fn bench_uts_scaling(c: &mut Criterion) {
    let configs: Vec<(&uts_lib::UtsConfig, i64)> =
        vec![(&uts_lib::T3, 4112897), (&uts_lib::T2, 4117769)];

    for (cfg, expected) in &configs {
        uts_lib::set_uts_config(cfg);
        let root = uts_lib::rng_init(cfg.seed);
        let mut group = c.benchmark_group(format!("uts-{}-scaling", cfg.name));
        for nw in worker_counts() {
            group.bench_with_input(
                BenchmarkId::new("lace", format!("{}w", nw)),
                &nw,
                |b, &nw| {
                    lace_native::start(nw, 0, 0);
                    b.iter(|| {
                        let r = uts_lib::uts_run(root.as_ptr(), 0);
                        assert_eq!(r, *expected);
                        r
                    });
                    lace_native::stop();
                },
            );
        }
        group.finish();
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Lace vs Rayon comparison (same worker count, side by side)
// ═══════════════════════════════════════════════════════════════════════════════

fn bench_fib_compare(c: &mut Criterion) {
    let nw = max_workers();
    let pool = rayon_pool(nw);
    let mut group = c.benchmark_group("fib-compare");

    group.bench_function(BenchmarkId::new("lace", format!("{}w", nw)), |b| {
        lace_native::start(nw, 0, 0);
        b.iter(|| {
            let r = lace_example_fib::fib_run(42);
            assert_eq!(r, 267914296);
            r
        });
        lace_native::stop();
    });

    group.bench_function(BenchmarkId::new("rayon", format!("{}w", nw)), |b| {
        pool.install(|| {
            b.iter(|| {
                let r = rayon_impl::fib(42);
                assert_eq!(r, 267914296);
                r
            });
        });
    });

    group.finish();
}

fn bench_nqueens_compare(c: &mut Criterion) {
    let nw = max_workers();
    let pool = rayon_pool(nw);
    let mut group = c.benchmark_group("nqueens-compare");

    group.bench_function(BenchmarkId::new("lace", format!("{}w", nw)), |b| {
        lace_native::start(nw, 0, 0);
        b.iter(|| {
            let r = lace_example_nqueens::nqueens_run(std::ptr::null(), 13, -1, 0);
            assert_eq!(r, 73712);
            r
        });
        lace_native::stop();
    });

    group.bench_function(BenchmarkId::new("rayon", format!("{}w", nw)), |b| {
        pool.install(|| {
            b.iter(|| {
                let r = rayon_impl::nqueens(&[], 13, -1, 0);
                assert_eq!(r, 73712);
                r
            });
        });
    });

    group.finish();
}

fn bench_uts_compare(c: &mut Criterion) {
    let nw = max_workers();
    let pool = rayon_pool(nw);
    let cfg = &uts_lib::T3;
    let expected = 4112897i64;
    uts_lib::set_uts_config(cfg);
    let root = uts_lib::rng_init(cfg.seed);

    let mut group = c.benchmark_group("uts-T3-compare");

    group.bench_function(BenchmarkId::new("lace", format!("{}w", nw)), |b| {
        lace_native::start(nw, 0, 0);
        b.iter(|| {
            let r = uts_lib::uts_run(root.as_ptr(), 0);
            assert_eq!(r, expected);
            r
        });
        lace_native::stop();
    });

    group.bench_function(BenchmarkId::new("rayon", format!("{}w", nw)), |b| {
        pool.install(|| {
            b.iter(|| {
                let r = rayon_impl::uts(&root, 0, cfg);
                assert_eq!(r, expected);
                r
            });
        });
    });

    group.finish();
}

// ═══════════════════════════════════════════════════════════════════════════════

criterion_group!(
    scaling,
    bench_fib_scaling,
    bench_nqueens_scaling,
    bench_uts_scaling
);
criterion_group!(
    comparison,
    bench_fib_compare,
    bench_nqueens_compare,
    bench_uts_compare
);
criterion_main!(scaling, comparison);
