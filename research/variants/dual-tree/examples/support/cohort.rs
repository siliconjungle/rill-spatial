//! Experiment-only scoped cohort runner. The stable library API is unchanged.
use rill_spatial::Index3;
use std::{
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
pub type Query = ([f64; 3], f64);

#[derive(Clone, Copy, Debug, Default)]
pub struct RunStats {
    pub wall_ns: u128,
    pub ordering_ns: u128,
    /// Sum of elapsed worker lifetimes, including descheduling; NOT CPU time.
    pub sum_worker_elapsed_ns: u128,
    pub max_worker_elapsed_ns: u128,
    pub queries: u64,
    pub results: u64,
    pub cohorts: u64,
    pub point_tests: u64,
    pub node_pairs: u64,
}

fn order_range(
    order: &mut [usize],
    queries: &[Query],
    max_queries: usize,
    first: usize,
    cohorts: &mut Vec<(usize, usize)>,
) {
    if order.is_empty() {
        return;
    }
    if order.len() <= max_queries {
        cohorts.push((first, first + order.len()));
        return;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &q in order.iter() {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(queries[q].0[axis]);
            hi[axis] = hi[axis].max(queries[q].0[axis]);
        }
    }
    let axis = (0..3)
        .max_by(|&a, &b| (hi[a] - lo[a]).total_cmp(&(hi[b] - lo[b])))
        .unwrap();
    let mid = order.len() / 2;
    order.select_nth_unstable_by(mid, |&a, &b| {
        queries[a].0[axis]
            .total_cmp(&queries[b].0[axis])
            .then_with(|| a.cmp(&b))
    });
    let (left, right) = order.split_at_mut(mid);
    order_range(left, queries, max_queries, first, cohorts);
    order_range(right, queries, max_queries, first + mid, cohorts);
}

/// Conservative logical ID budget across all workers, allowing 2x Vec growth.
/// The total logical output cap is 32MiB, never more than 64 queries per worker.
pub fn cohort_size(points: usize, workers: usize) -> usize {
    64.min((32 * 1024 * 1024 / (points.max(1) * 8 * workers.max(1))).max(1))
}

/// Complete result vectors exist before the callback; each original query visits
/// once. The callback can validate or consume borrowed outputs concurrently.
/// Every emitted individual ID is also consumed through black_box in this runner.
/// Oracle callback time is deliberately included during proof runs; timed runs
/// supply an empty callback, so their service interval contains no oracle work.
pub fn run(
    index: &Index3,
    queries: &[Query],
    workers: usize,
    max_queries: usize,
    callback: impl Fn(usize, &[usize], &[Vec<u64>]) + Sync,
) -> RunStats {
    assert!((1..=4).contains(&workers));
    assert!(max_queries > 0);
    let wall = Instant::now();
    let ordering = Instant::now();
    let mut order: Vec<_> = (0..queries.len()).collect();
    let mut cohorts = Vec::new();
    order_range(&mut order, queries, max_queries, 0, &mut cohorts);
    let ordering_ns = ordering.elapsed().as_nanos();
    let next = AtomicUsize::new(0);
    let mut stats = RunStats {
        ordering_ns,
        ..RunStats::default()
    };
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for worker in 0..workers {
            let (order, cohorts, next, callback) = (&order, &cohorts, &next, &callback);
            handles.push(scope.spawn(move || {
                let start = Instant::now();
                let mut local = RunStats::default();
                let mut outputs: Vec<Vec<u64>> = (0..max_queries).map(|_| Vec::new()).collect();
                let mut qs = Vec::with_capacity(max_queries);
                loop {
                    let cohort = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(first, end)) = cohorts.get(cohort) else {
                        break;
                    };
                    let indices = &order[first..end];
                    qs.clear();
                    qs.extend(indices.iter().map(|&i| queries[i]));
                    for out in &mut outputs {
                        out.clear();
                    }
                    let batch = index
                        .batch_blocks(&qs, |qis, ids| {
                            for &q in qis {
                                outputs[q].extend_from_slice(ids);
                            }
                        })
                        .expect("all benchmark queries must be valid");
                    local.queries += indices.len() as u64;
                    local.results += batch.reported_pairs;
                    local.point_tests += batch.point_tests;
                    local.node_pairs += batch.node_pairs;
                    local.cohorts += 1;
                    // Keep the fully materialized vectors observable as vectors,
                    // before consuming individual IDs in the timed diagnostic.
                    black_box(&outputs[..indices.len()]);
                    for (&q, ids) in indices.iter().zip(&outputs) {
                        for &id in ids {
                            black_box((q, id));
                        }
                    }
                    callback(worker, indices, &outputs[..indices.len()]);
                }
                drop(qs);
                drop(outputs);
                local.wall_ns = start.elapsed().as_nanos();
                local
            }));
        }
        for handle in handles {
            let local = handle.join().unwrap();
            stats.sum_worker_elapsed_ns += local.wall_ns;
            stats.max_worker_elapsed_ns = stats.max_worker_elapsed_ns.max(local.wall_ns);
            stats.queries += local.queries;
            stats.results += local.results;
            stats.cohorts += local.cohorts;
            stats.point_tests += local.point_tests;
            stats.node_pairs += local.node_pairs;
        }
    });
    drop(order);
    drop(cohorts);
    stats.wall_ns = wall.elapsed().as_nanos();
    stats
}
