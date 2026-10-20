//! Bounded exact batches over an immutable index.
use crate::{Error, SpatialIndex};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

/// Resource choices for [`SpatialIndex::within_batch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchOptions {
    /// Maximum scoped query workers, from 1 through 64. Default: 1.
    pub workers: usize,
    /// Maximum complete result rows retained by each worker. Must be positive.
    /// Default: 64. Smaller groups reduce dense-output memory and may lose locality.
    pub queries_per_group: usize,
}
impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            workers: 1,
            queries_per_group: 64,
        }
    }
}
/// Work completed by a successful batch. Counts include empty query rows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BatchStats {
    pub queries: usize,
    pub groups: usize,
    /// Workers actually used, capped by the number of groups; zero for no queries.
    pub workers: usize,
    pub matches: u128,
}
type Query<const D: usize> = ([f64; D], f64);
fn partition<const D: usize>(
    queries: &[Query<D>],
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
    let mut lo = [f64::INFINITY; D];
    let mut hi = [f64::NEG_INFINITY; D];
    for &i in order.iter() {
        for d in 0..D {
            lo[d] = lo[d].min(queries[i].0[d]);
            hi[d] = hi[d].max(queries[i].0[d]);
        }
    }
    let axis = (0..D)
        .max_by(|&a, &b| (hi[a] - lo[a]).total_cmp(&(hi[b] - lo[b])))
        .unwrap();
    let mid = order.len() / 2;
    order.select_nth_unstable_by(mid, |&a, &b| {
        queries[a].0[axis]
            .total_cmp(&queries[b].0[axis])
            .then(a.cmp(&b))
    });
    let (left, right) = order.split_at_mut(mid);
    partition(queries, left, base, limit, groups);
    partition(queries, right, base + mid, limit, groups);
}
impl<const D: usize> SpatialIndex<D> {
    /// Report an exact batch in spatially coherent groups with bounded output storage.
    ///
    /// Each input is `(centre, radius)`. `visit(indices, rows)` receives aligned
    /// original query indices and complete unordered ID vectors. Every query is
    /// visited exactly once, including empty results. Group/row order is unspecified.
    /// Rows are borrowed and reused after the callback; copy only what you retain.
    /// Callbacks are serialized, even when query work runs on several threads.
    ///
    /// All query/configuration validation completes before the first callback.
    /// Updates are excluded by this method's immutable borrow; unprepared/dirty
    /// indexes remain correct using the same fallback as `within_into`.
    ///
    /// This creates scoped threads per call, not a persistent pool. One worker
    /// runs on the caller's thread. At most `workers * queries_per_group` result
    /// vectors are live, plus O(query count) ordering/scheduling storage. Vector
    /// capacity may exceed length; each exact row may contain the entire index.
    /// The caller chooses a suitable worker count and memory budget. A callback
    /// panic propagates after scoped workers join.
    pub fn within_batch<F>(
        &self,
        queries: &[Query<D>],
        options: BatchOptions,
        visit: F,
    ) -> Result<BatchStats, Error>
    where
        F: FnMut(&[usize], &[Vec<u64>]) + Send,
    {
        if !(1..=64).contains(&options.workers) || options.queries_per_group == 0 {
            return Err(Error::BatchConfiguration);
        }
        for (centre, radius) in queries {
            if centre.iter().any(|x| !x.is_finite()) {
                return Err(Error::Coordinate);
            }
            if !radius.is_finite() || *radius < 0.0 {
                return Err(Error::Radius);
            }
        }
        if queries.is_empty() {
            return Ok(BatchStats::default());
        }
        let mut order: Vec<_> = (0..queries.len()).collect();
        let mut groups = Vec::new();
        partition(
            queries,
            &mut order,
            0,
            options.queries_per_group,
            &mut groups,
        );
        let workers = options.workers.min(groups.len());
        let next = AtomicUsize::new(0);
        let consumer = Mutex::new((visit, 0u128));
        let work = || {
            let mut outputs: Vec<Vec<u64>> = (0..options.queries_per_group.min(queries.len()))
                .map(|_| Vec::new())
                .collect();
            loop {
                let at = next.fetch_add(1, Ordering::Relaxed);
                let Some(range) = groups.get(at) else {
                    break;
                };
                let indices = &order[range.clone()];
                let rows = &mut outputs[..indices.len()];
                for (&q, out) in indices.iter().zip(rows.iter_mut()) {
                    self.within_into(queries[q].0, queries[q].1, out)
                        .expect("prevalidated query");
                }
                let mut consumer = consumer.lock().expect("batch callback panicked");
                consumer.1 += rows.iter().map(|r| r.len() as u128).sum::<u128>();
                (consumer.0)(indices, rows);
            }
        };
        if workers == 1 {
            work();
        } else {
            std::thread::scope(|scope| {
                for _ in 1..workers {
                    scope.spawn(work);
                }
                work();
            });
        }
        let matches = consumer.into_inner().expect("batch callback panicked").1;
        Ok(BatchStats {
            queries: queries.len(),
            groups: groups.len(),
            workers,
            matches,
        })
    }
}
