//! Internal coherent traversal and scoped-worker composition. Not a competitor.
use crate::{Adapter, Point, Query};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

/// One immutable index, spatially ordered independent queries, bounded outputs.
pub struct Coherent<A, const WORKERS: usize>(A);
fn partition(
    queries: &[Query],
    order: &mut [usize],
    base: usize,
    limit: usize,
    groups: &mut Vec<std::ops::Range<usize>>,
) {
    if order.len() <= limit {
        if !order.is_empty() {
            groups.push(base..base + order.len());
        }
        return;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &i in order.iter() {
        for d in 0..3 {
            lo[d] = lo[d].min(queries[i].point[d]);
            hi[d] = hi[d].max(queries[i].point[d]);
        }
    }
    let axis = (0..3)
        .max_by(|&a, &b| (hi[a] - lo[a]).total_cmp(&(hi[b] - lo[b])))
        .unwrap();
    let mid = order.len() / 2;
    order.select_nth_unstable_by(mid, |&a, &b| {
        queries[a].point[axis]
            .total_cmp(&queries[b].point[axis])
            .then(a.cmp(&b))
    });
    let (left, right) = order.split_at_mut(mid);
    partition(queries, left, base, limit, groups);
    partition(queries, right, base + mid, limit, groups);
}
impl<A: Adapter + Sync, const WORKERS: usize> Adapter for Coherent<A, WORKERS> {
    fn build(points: &[Point], size: f64) -> Self {
        Self(A::build(points, size))
    }
    fn update(&mut self, changes: &[(usize, Point)]) {
        self.0.update(changes);
    }
    fn prepare(&mut self) {
        self.0.prepare();
    }
    fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
        self.0.query(p, r, out);
    }
    fn visit_groups(
        &self,
        queries: &[Query],
        limit: usize,
        emit: &mut (dyn FnMut(&[usize], &[Vec<u64>]) + Send),
    ) {
        assert!((1..=4).contains(&WORKERS));
        assert!(limit > 0);
        if queries.is_empty() {
            return;
        }
        // The paused-oracle runner cannot subtract a serial callback's time from
        // overlapping worker execution. Parallel measurements require a complete
        // proof pass followed by uninterrupted timing with full consumption.
        if WORKERS > 1 {
            assert_eq!(std::env::var("RILL_CONTINUOUS").as_deref(), Ok("1"));
        }
        let mut order: Vec<_> = (0..queries.len()).collect();
        let mut groups = Vec::new();
        partition(queries, &mut order, 0, limit, &mut groups);
        let next = AtomicUsize::new(0);
        let emit = Mutex::new(emit);
        std::thread::scope(|scope| {
            for _ in 0..WORKERS {
                let order = &order;
                let groups = &groups;
                let next = &next;
                let emit = &emit;
                let index = &self.0;
                scope.spawn(move || {
                    let mut outputs: Vec<Vec<u64>> = (0..limit).map(|_| Vec::new()).collect();
                    let mut local = Vec::with_capacity(limit);
                    loop {
                        let g = next.fetch_add(1, Ordering::Relaxed);
                        let Some(range) = groups.get(g) else {
                            break;
                        };
                        let indices = &order[range.clone()];
                        local.clear();
                        local.extend(indices.iter().map(|&i| queries[i]));
                        index.query_batch(&local, &mut outputs[..indices.len()]);
                        let mut consumer = emit.lock().unwrap();
                        consumer(indices, &outputs[..indices.len()]);
                    }
                });
            }
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::rust_adapters::Rill;
    #[test]
    fn coherent_rows_restore_query_identity() {
        let points: Vec<_> = (0..521)
            .map(|i| [(i % 37) as f64, (i / 37) as f64, (i % 7) as f64])
            .collect();
        let queries: Vec<_> = (0..113)
            .map(|i| Query {
                point: [(i % 29) as f64 + 0.25, (i % 17) as f64, -1.0],
                radius: (i % 13) as f64,
            })
            .collect();
        let mut index = Coherent::<Rill, 1>::build(&points, 4.0);
        index.prepare();
        for limit in [1, 7, 64] {
            let mut visited = vec![false; queries.len()];
            index.visit_groups(&queries, limit, &mut |ids, rows| {
                assert_eq!(ids.len(), rows.len());
                assert!(ids.len() <= limit);
                for (&q, row) in ids.iter().zip(rows) {
                    assert!(!visited[q]);
                    visited[q] = true;
                    let mut expected = Vec::new();
                    index.query(queries[q].point, queries[q].radius, &mut expected);
                    let mut actual = row.clone();
                    actual.sort_unstable();
                    expected.sort_unstable();
                    assert_eq!(actual, expected);
                }
            });
            assert!(visited.iter().all(|&v| v));
        }
    }
}
