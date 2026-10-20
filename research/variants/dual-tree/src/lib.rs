//! Exact streaming radius batches using a pair traversal of two median kd trees.
//! Results use the closed squared-f64 predicate, including its overflow behavior.
#![forbid(unsafe_code)]

use std::collections::HashMap;
const LEAF: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Dimension,
    CellSize,
    Coordinate,
    Radius,
    BatchSize,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy)]
struct Entry<const D: usize> {
    id: u64,
    point: [f64; D],
}

#[derive(Clone)]
struct Node<const D: usize> {
    lo: [f64; D],
    hi: [f64; D],
    start: usize,
    end: usize,
    children: Option<(usize, usize)>,
    min_r2: f64,
    max_r2: f64,
}

#[derive(Clone, Default)]
struct Tree<const D: usize> {
    nodes: Vec<Node<D>>,
    order: Vec<usize>,
}
impl<const D: usize> Tree<D> {
    fn new(count: usize, point: impl Fn(usize) -> [f64; D], r2: impl Fn(usize) -> f64) -> Self {
        Self::with_leaf(count, point, r2, LEAF)
    }
    fn with_leaf(
        count: usize,
        point: impl Fn(usize) -> [f64; D],
        r2: impl Fn(usize) -> f64,
        leaf: usize,
    ) -> Self {
        let mut tree = Self {
            nodes: Vec::new(),
            order: (0..count).collect(),
        };
        if count > 0 {
            tree.build(0, count, &point, &r2, leaf);
        }
        tree
    }
    fn build(
        &mut self,
        start: usize,
        end: usize,
        point: &impl Fn(usize) -> [f64; D],
        r2: &impl Fn(usize) -> f64,
        leaf: usize,
    ) -> usize {
        let mut lo = [f64::INFINITY; D];
        let mut hi = [f64::NEG_INFINITY; D];
        let (mut min_r2, mut max_r2) = (f64::INFINITY, 0.0_f64);
        for &i in &self.order[start..end] {
            let p = point(i);
            for axis in 0..D {
                lo[axis] = lo[axis].min(p[axis]);
                hi[axis] = hi[axis].max(p[axis]);
            }
            min_r2 = min_r2.min(r2(i));
            max_r2 = max_r2.max(r2(i));
        }
        let at = self.nodes.len();
        self.nodes.push(Node {
            lo,
            hi,
            start,
            end,
            children: None,
            min_r2,
            max_r2,
        });
        if end - start > leaf {
            let axis = (0..D)
                .max_by(|&a, &b| (hi[a] - lo[a]).total_cmp(&(hi[b] - lo[b])))
                .unwrap();
            let mid = start + (end - start) / 2;
            self.order[start..end].select_nth_unstable_by(mid - start, |&a, &b| {
                point(a)[axis]
                    .total_cmp(&point(b)[axis])
                    .then_with(|| a.cmp(&b))
            });
            let a = self.build(start, mid, point, r2, leaf);
            let b = self.build(mid, end, point, r2, leaf);
            self.nodes[at].children = Some((a, b));
        }
        at
    }
}

#[derive(Clone)]
struct Prepared<const D: usize> {
    tree: Tree<D>,
    ids: Vec<u64>,
}

/// Counters describe work, not a substitute for expanding complete outputs.
#[derive(Clone, Copy, Debug, Default)]
pub struct BatchStats {
    pub node_pairs: u64,
    pub rejected_pairs: u64,
    pub accepted_pairs: u64,
    pub point_tests: u64,
    pub blocks: u64,
    pub reported_pairs: u64,
}

/// Stable-ID mutable index. Mutation invalidates the packed tree immediately;
/// queries safely scan until `prepare()` rebuilds it.
#[derive(Clone)]
pub struct SpatialIndex<const D: usize> {
    entries: Vec<Entry<D>>,
    ids: HashMap<u64, usize>,
    prepared: Option<Prepared<D>>,
    cell_size: f64,
}
pub type Index2 = SpatialIndex<2>;
pub type Index3 = SpatialIndex<3>;

fn valid_point<const D: usize>(p: &[f64; D]) -> Result<(), Error> {
    if p.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(Error::Coordinate)
    }
}
fn valid_radius(r: f64) -> Result<(), Error> {
    if r.is_finite() && r >= 0.0 {
        Ok(())
    } else {
        Err(Error::Radius)
    }
}
fn distance<const D: usize>(a: &[f64; D], b: &[f64; D]) -> f64 {
    let mut d = 0.0;
    for axis in 0..D {
        let x = a[axis] - b[axis];
        d += x * x;
    }
    d
}
fn lower<const D: usize>(a: &Node<D>, b: &Node<D>) -> f64 {
    let mut d = 0.0;
    for axis in 0..D {
        let gap = if a.lo[axis] > b.hi[axis] {
            a.lo[axis] - b.hi[axis]
        } else if b.lo[axis] > a.hi[axis] {
            b.lo[axis] - a.hi[axis]
        } else {
            0.0
        };
        d += gap * gap;
    }
    d
}
fn upper<const D: usize>(a: &Node<D>, b: &Node<D>) -> f64 {
    let mut d = 0.0;
    for axis in 0..D {
        let gap = (a.lo[axis] - b.hi[axis])
            .abs()
            .max((a.hi[axis] - b.lo[axis]).abs());
        d += gap * gap;
    }
    d
}

impl<const D: usize> SpatialIndex<D> {
    pub fn new(cell_size: f64) -> Result<Self, Error> {
        if D != 2 && D != 3 {
            return Err(Error::Dimension);
        }
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return Err(Error::CellSize);
        }
        Ok(Self {
            entries: Vec::new(),
            ids: HashMap::new(),
            prepared: None,
            cell_size,
        })
    }
    pub fn from_points(
        cell_size: f64,
        points: impl IntoIterator<Item = (u64, [f64; D])>,
    ) -> Result<Self, Error> {
        let mut index = Self::new(cell_size)?;
        for (id, p) in points {
            index.upsert(id, p)?;
        }
        Ok(index)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn cell_size(&self) -> f64 {
        self.cell_size
    }
    pub fn reserve(&mut self, additional: usize) {
        self.entries.reserve(additional);
        self.ids.reserve(additional);
    }
    pub fn get(&self, id: u64) -> Option<[f64; D]> {
        self.ids.get(&id).map(|&at| self.entries[at].point)
    }
    pub fn upsert(&mut self, id: u64, point: [f64; D]) -> Result<Option<[f64; D]>, Error> {
        valid_point(&point)?;
        self.prepared = None;
        if let Some(&at) = self.ids.get(&id) {
            return Ok(Some(std::mem::replace(&mut self.entries[at].point, point)));
        }
        self.ids.insert(id, self.entries.len());
        self.entries.push(Entry { id, point });
        Ok(None)
    }
    pub fn remove(&mut self, id: u64) -> Option<[f64; D]> {
        let at = self.ids.remove(&id)?;
        self.prepared = None;
        let entry = self.entries.swap_remove(at);
        if at < self.entries.len() {
            self.ids.insert(self.entries[at].id, at);
        }
        Some(entry.point)
    }
    pub fn prepare(&mut self) {
        if self.prepared.is_some() {
            return;
        }
        let tree = Tree::new(self.len(), |i| self.entries[i].point, |_| 0.0);
        let ids = tree.order.iter().map(|&i| self.entries[i].id).collect();
        self.prepared = Some(Prepared { tree, ids });
    }

    /// Clears and fills a complete unordered result. Invalid queries preserve output.
    pub fn within_into(
        &self,
        centre: [f64; D],
        radius: f64,
        out: &mut Vec<u64>,
    ) -> Result<(), Error> {
        valid_point(&centre)?;
        valid_radius(radius)?;
        out.clear();
        if let Some(prepared) = &self.prepared {
            let query = Node {
                lo: centre,
                hi: centre,
                start: 0,
                end: 1,
                children: None,
                min_r2: radius * radius,
                max_r2: radius * radius,
            };
            if !prepared.tree.nodes.is_empty() {
                self.single(prepared, 0, &query, out);
            }
        } else {
            let r2 = radius * radius;
            out.extend(
                self.entries
                    .iter()
                    .filter(|e| distance(&e.point, &centre) <= r2)
                    .map(|e| e.id),
            );
        }
        Ok(())
    }
    fn single(&self, prepared: &Prepared<D>, at: usize, q: &Node<D>, out: &mut Vec<u64>) {
        let t = &prepared.tree.nodes[at];
        if lower(q, t) > q.max_r2 {
            return;
        }
        if upper(q, t) <= q.min_r2 {
            out.extend_from_slice(&prepared.ids[t.start..t.end]);
            return;
        }
        if let Some((a, b)) = t.children {
            self.single(prepared, a, q, out);
            self.single(prepared, b, q, out);
        } else {
            for i in t.start..t.end {
                if distance(&self.entries[prepared.tree.order[i]].point, &q.lo) <= q.max_r2 {
                    out.push(prepared.ids[i]);
                }
            }
        }
    }

    /// Emits exact Cartesian result blocks `(query_indices, target_ids)`.
    /// Every query-target match appears exactly once across all blocks. Indices
    /// refer to input query order. Callback slices are borrowed only for a call.
    /// Consumers requiring individual memberships must expand their product.
    /// This method never stores those products. All queries validate before callbacks.
    /// If dirty, a streaming full scan is immediately correct, without rebuilding.
    pub fn batch_blocks(
        &self,
        queries: &[([f64; D], f64)],
        mut emit: impl FnMut(&[usize], &[u64]),
    ) -> Result<BatchStats, Error> {
        for (p, r) in queries {
            valid_point(p)?;
            valid_radius(*r)?;
        }
        let mut stats = BatchStats::default();
        if queries.is_empty() || self.is_empty() {
            return Ok(stats);
        }
        let Some(prepared) = &self.prepared else {
            let mut scratch = Vec::with_capacity(LEAF);
            for (qi, (q, r)) in queries.iter().enumerate() {
                for chunk in self.entries.chunks(LEAF) {
                    scratch.clear();
                    for e in chunk {
                        stats.point_tests += 1;
                        if distance(&e.point, q) <= r * r {
                            scratch.push(e.id);
                        }
                    }
                    if !scratch.is_empty() {
                        stats.blocks += 1;
                        stats.reported_pairs += scratch.len() as u64;
                        emit(&[qi], &scratch);
                    }
                }
            }
            return Ok(stats);
        };
        let qt = Tree::new(
            queries.len(),
            |i| queries[i].0,
            |i| queries[i].1 * queries[i].1,
        );
        let mut scratch = Vec::with_capacity(LEAF);
        self.pair(
            prepared,
            &qt,
            queries,
            0,
            0,
            &mut scratch,
            &mut stats,
            &mut emit,
        );
        Ok(stats)
    }
    /// Expands every block, charging O(K) callback work for K reported pairs.
    /// The callback can stream to a bounded sink without retaining the batch.
    pub fn batch_each(
        &self,
        queries: &[([f64; D], f64)],
        mut emit: impl FnMut(usize, u64),
    ) -> Result<BatchStats, Error> {
        self.batch_blocks(queries, |qs, ids| {
            for &q in qs {
                for &id in ids {
                    emit(q, id);
                }
            }
        })
    }
    /// Materializes complete result vectors for spatial query-tree partitions,
    /// then visits each original query index once. At most `max_queries` vectors
    /// are retained. Order of visits/results is unspecified. Query-tree building,
    /// traversal, full ID expansion and Vec writes all occur inside this method.
    /// Memory is O(n+q+max_queries*n), avoiding storage of all q*n IDs. Callers
    /// can cap max_queries by a byte budget divided by n*sizeof(u64).
    pub fn batch_visit(
        &self,
        queries: &[([f64; D], f64)],
        max_queries: usize,
        mut visit: impl FnMut(usize, &[u64]),
    ) -> Result<BatchStats, Error> {
        self.batch_visit_groups(queries, max_queries, |indices, outputs| {
            for (&q, ids) in indices.iter().zip(outputs) {
                visit(q, ids);
            }
        })
    }
    /// Same bounded materialization as `batch_visit`, with one callback per
    /// spatial partition. `indices[i]` identifies complete result `outputs[i]`.
    /// This lets validation pause timing once per <=max_queries partition.
    pub fn batch_visit_groups(
        &self,
        queries: &[([f64; D], f64)],
        max_queries: usize,
        mut visit: impl FnMut(&[usize], &[Vec<u64>]),
    ) -> Result<BatchStats, Error> {
        if max_queries == 0 {
            return Err(Error::BatchSize);
        }
        for (p, r) in queries {
            valid_point(p)?;
            valid_radius(*r)?;
        }
        let mut stats = BatchStats::default();
        if queries.is_empty() {
            return Ok(stats);
        }
        let mut outputs: Vec<Vec<u64>> = (0..max_queries.min(queries.len()))
            .map(|_| Vec::new())
            .collect();
        if self.prepared.is_none() || self.is_empty() {
            let order: Vec<_> = (0..queries.len()).collect();
            for indices in order.chunks(max_queries) {
                for (&i, out) in indices.iter().zip(&mut outputs) {
                    let (p, r) = queries[i];
                    self.within_into(p, r, out)?;
                    stats.point_tests += self.len() as u64;
                    stats.reported_pairs += out.len() as u64;
                    stats.blocks += (!out.is_empty()) as u64;
                }
                visit(indices, &outputs[..indices.len()]);
            }
            return Ok(stats);
        }
        let prepared = self.prepared.as_ref().unwrap();
        let qt = Tree::with_leaf(
            queries.len(),
            |i| queries[i].0,
            |i| queries[i].1 * queries[i].1,
            LEAF.min(max_queries),
        );
        let mut inverse = vec![0; queries.len()];
        for (at, &q) in qt.order.iter().enumerate() {
            inverse[q] = at;
        }
        let mut stack = vec![0];
        let mut scratch = Vec::with_capacity(LEAF);
        while let Some(qi) = stack.pop() {
            let q = &qt.nodes[qi];
            if q.end - q.start > max_queries {
                let (a, b) = q.children.unwrap();
                stack.push(b);
                stack.push(a);
                continue;
            }
            for out in &mut outputs {
                out.clear();
            }
            self.pair(
                prepared,
                &qt,
                queries,
                qi,
                0,
                &mut scratch,
                &mut stats,
                &mut |qs, ids| {
                    for &i in qs {
                        outputs[inverse[i] - q.start].extend_from_slice(ids);
                    }
                },
            );
            visit(&qt.order[q.start..q.end], &outputs[..q.end - q.start]);
        }
        Ok(stats)
    }
    fn pair(
        &self,
        p: &Prepared<D>,
        qt: &Tree<D>,
        queries: &[([f64; D], f64)],
        qi: usize,
        ti: usize,
        scratch: &mut Vec<u64>,
        s: &mut BatchStats,
        emit: &mut impl FnMut(&[usize], &[u64]),
    ) {
        let (q, t) = (&qt.nodes[qi], &p.tree.nodes[ti]);
        s.node_pairs += 1;
        if lower(q, t) > q.max_r2 {
            s.rejected_pairs += 1;
            return;
        }
        if upper(q, t) <= q.min_r2 {
            s.accepted_pairs += 1;
            s.blocks += 1;
            s.reported_pairs += (q.end - q.start) as u64 * (t.end - t.start) as u64;
            emit(&qt.order[q.start..q.end], &p.ids[t.start..t.end]);
            return;
        }
        let split_q =
            q.children.is_some() && (t.children.is_none() || q.end - q.start > t.end - t.start);
        if split_q {
            let (a, b) = q.children.unwrap();
            self.pair(p, qt, queries, a, ti, scratch, s, emit);
            self.pair(p, qt, queries, b, ti, scratch, s, emit);
        } else if let Some((a, b)) = t.children {
            self.pair(p, qt, queries, qi, a, scratch, s, emit);
            self.pair(p, qt, queries, qi, b, scratch, s, emit);
        } else {
            for &qindex in &qt.order[q.start..q.end] {
                scratch.clear();
                let (centre, radius) = queries[qindex];
                for i in t.start..t.end {
                    s.point_tests += 1;
                    if distance(&self.entries[p.tree.order[i]].point, &centre) <= radius * radius {
                        scratch.push(p.ids[i]);
                    }
                }
                if !scratch.is_empty() {
                    s.blocks += 1;
                    s.reported_pairs += scratch.len() as u64;
                    emit(&[qindex], scratch);
                }
            }
        }
    }
}
