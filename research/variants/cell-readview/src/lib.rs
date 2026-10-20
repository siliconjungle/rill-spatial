//! Independent dependency-free CSR cell read view. Mutations invalidate the view
//! and are immediately queryable with an exact contiguous scan until prepare.
#![forbid(unsafe_code)]
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

// Fixed-width integer cell keys do not need a string-oriented keyed hasher.
// Equality still resolves collisions, including arbitrary/adversarial geometry.
#[derive(Default, Clone)]
struct CellHasher(u64);
impl Hasher for CellHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(word));
        }
    }
    fn write_u64(&mut self, x: u64) {
        self.0 = (self.0.rotate_left(5) ^ x).wrapping_mul(0x517cc1b727220a95);
    }
    fn write_i64(&mut self, x: i64) {
        self.write_u64(x as u64);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Dimension,
    CellSize,
    Coordinate,
    Radius,
    BatchSize,
}

/// Diagnostic work counters; never a compact substitute for reporting IDs.
#[derive(Debug, Default, Clone, Copy)]
pub struct BatchStats {
    pub node_pairs: u64,
    pub rejected_pairs: u64,
    pub accepted_pairs: u64,
    pub point_tests: u64,
    pub blocks: u64,
    pub reported_pairs: u64,
    /// Number of query cells for which a shared candidate list was constructed.
    pub groups: u64,
    /// Sum of shared candidate-list lengths, before per-query exact filtering.
    pub candidate_cells: u64,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// Different traversal algorithms over the same packed cell representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Hybrid,
    HashGrid,
    SortedSlab,
    OccupiedScan,
}
impl Strategy {
    fn environment() -> Self {
        match std::env::var("RILL_CELL_STRATEGY").as_deref() {
            Ok("hash") => Self::HashGrid,
            Ok("sorted") => Self::SortedSlab,
            Ok("occupied") => Self::OccupiedScan,
            Ok("hybrid") => Self::Hybrid,
            _ => Self::SortedSlab,
        }
    }
}

#[derive(Clone, Copy)]
struct Entry<const D: usize> {
    id: u64,
    point: [f64; D],
}
#[derive(Clone)]
struct Cell<const D: usize> {
    key: [i64; D],
    start: usize,
    end: usize,
    lo: [f64; D],
    hi: [f64; D],
}
#[derive(Clone)]
struct View<const D: usize> {
    points: Vec<Entry<D>>,
    cells: Vec<Cell<D>>,
    map: HashMap<[i64; D], usize, BuildHasherDefault<CellHasher>>,
}
#[derive(Clone)]
pub struct SpatialIndex<const D: usize> {
    cell_size: f64,
    entries: Vec<Entry<D>>,
    ids: HashMap<u64, usize>,
    view: Option<View<D>>,
    strategy: Strategy,
    containment: bool,
}
pub type Index2 = SpatialIndex<2>;
pub type Index3 = SpatialIndex<3>;

fn distance<const D: usize>(p: [f64; D], c: [f64; D]) -> f64 {
    let mut d = 0.0;
    for i in 0..D {
        let x = p[i] - c[i];
        d += x * x;
    }
    d
}

impl<const D: usize> SpatialIndex<D> {
    /// Group arbitrary query centres by cell and share candidate discovery.
    /// Each callback receives complete results and original query indices.
    /// Query validation precedes every callback; at most min(max_queries, 64)
    /// complete result Vecs exist simultaneously. Sorting, candidate discovery,
    /// exact filtering, containment expansion and output writes happen here.
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
            if p.iter().any(|x| !x.is_finite()) {
                return Err(Error::Coordinate);
            }
            if !r.is_finite() || *r < 0.0 {
                return Err(Error::Radius);
            }
        }
        let mut stats = BatchStats::default();
        if queries.is_empty() {
            return Ok(stats);
        }
        let width = max_queries.min(64).min(queries.len());
        let mut outputs: Vec<Vec<u64>> = (0..width).map(|_| Vec::new()).collect();
        // No stale views: every mutation invalidates the view before this call.
        let Some(view) = &self.view else {
            let order: Vec<_> = (0..queries.len()).collect();
            for indices in order.chunks(width) {
                for (&q, out) in indices.iter().zip(&mut outputs) {
                    let (c, r) = queries[q];
                    self.within_into(c, r, out)?;
                    stats.point_tests += self.len() as u64;
                    stats.reported_pairs += out.len() as u64;
                    stats.blocks += u64::from(!out.is_empty());
                }
                visit(indices, &outputs[..indices.len()]);
            }
            return Ok(stats);
        };
        let mut order: Vec<_> = (0..queries.len())
            .map(|q| (self.key(queries[q].0), q))
            .collect();
        order.sort_unstable_by_key(|&(key, _)| key);
        let indices: Vec<_> = order.iter().map(|&(_, q)| q).collect();
        let mut candidates = Vec::new();
        let mut start = 0;
        while start < order.len() {
            let mut end = start + 1;
            while end < order.len() && order[end].0 == order[start].0 {
                end += 1;
            }
            let group = &indices[start..end];
            if order[start].0.is_none() {
                // Exceptional query keys safely use the existing exact query.
                for chunk in group.chunks(width) {
                    for (&q, out) in chunk.iter().zip(&mut outputs) {
                        let (c, r) = queries[q];
                        let r2 = r * r;
                        out.clear();
                        for e in &self.entries {
                            if distance(e.point, c) <= r2 {
                                out.push(e.id);
                            }
                        }
                        stats.reported_pairs += out.len() as u64;
                        stats.blocks += u64::from(!out.is_empty());
                        stats.point_tests += self.len() as u64;
                    }
                    visit(chunk, &outputs[..chunk.len()]);
                }
                start = end;
                continue;
            }
            stats.groups += 1;
            let mut qlo = [f64::INFINITY; D];
            let mut qhi = [f64::NEG_INFINITY; D];
            let mut max_r2 = 0.0f64;
            for &q in group {
                let (c, r) = queries[q];
                max_r2 = max_r2.max(r * r);
                for i in 0..D {
                    qlo[i] = qlo[i].min(c[i]);
                    qhi[i] = qhi[i].max(c[i]);
                }
            }
            candidates.clear();
            // Actual centre bounds plus maximum squared radius conservatively
            // include every query's candidate cells, including variable radii.
            let bounds = self
                .bounds(qlo, max_r2)
                .zip(self.bounds(qhi, max_r2))
                .and_then(|((lo, _, _), (_, hi, _))| {
                    let mut volume = 1usize;
                    for i in 0..D {
                        volume = volume.checked_mul((hi[i] - lo[i] + 1).try_into().ok()?)?;
                    }
                    Some((lo, hi, volume))
                });
            let mut consider = |at: usize| {
                stats.node_pairs += 1;
                let b = &view.cells[at];
                let mut nearest = 0.0;
                for i in 0..D {
                    let d = if b.lo[i] > qhi[i] {
                        b.lo[i] - qhi[i]
                    } else if b.hi[i] < qlo[i] {
                        b.hi[i] - qlo[i]
                    } else {
                        0.0
                    };
                    nearest += d * d;
                }
                if nearest > max_r2 {
                    stats.rejected_pairs += 1;
                } else {
                    candidates.push(at);
                }
            };
            let mut discovered = false;
            if let Some((lo, hi, volume)) = bounds {
                let begin = view.cells.partition_point(|b| b.key[0] < lo[0]);
                let stop = view.cells.partition_point(|b| b.key[0] <= hi[0]);
                let hash = self.strategy == Strategy::HashGrid
                    || (self.strategy == Strategy::Hybrid
                        && volume.saturating_mul(4) < stop - begin);
                if hash && volume <= view.cells.len().saturating_mul(4).min(100_000) {
                    let mut key = lo;
                    'enumerate: loop {
                        if let Some(&at) = view.map.get(&key) {
                            consider(at);
                        }
                        let mut axis = D;
                        loop {
                            if axis == 0 {
                                break 'enumerate;
                            }
                            axis -= 1;
                            if key[axis] < hi[axis] {
                                key[axis] += 1;
                                break;
                            }
                            key[axis] = lo[axis];
                        }
                    }
                    discovered = true;
                } else if self.strategy != Strategy::OccupiedScan {
                    for at in begin..stop {
                        let b = &view.cells[at];
                        if (1..D).all(|i| b.key[i] >= lo[i] && b.key[i] <= hi[i]) {
                            consider(at);
                        }
                    }
                    discovered = true;
                }
            }
            if !discovered {
                for at in 0..view.cells.len() {
                    consider(at);
                }
            }
            stats.candidate_cells += candidates.len() as u64;
            for chunk in group.chunks(width) {
                for (&q, out) in chunk.iter().zip(&mut outputs) {
                    out.clear();
                    let (c, r) = queries[q];
                    let r2 = r * r;
                    for &at in &candidates {
                        stats.node_pairs += 1;
                        let b = &view.cells[at];
                        let mut near = 0.0;
                        let mut far = 0.0;
                        for i in 0..D {
                            let a = b.lo[i] - c[i];
                            let z = b.hi[i] - c[i];
                            let d = if a > 0.0 {
                                a
                            } else if z < 0.0 {
                                z
                            } else {
                                0.0
                            };
                            near += d * d;
                            let d = a.abs().max(z.abs());
                            far += d * d;
                        }
                        if near > r2 {
                            stats.rejected_pairs += 1;
                            continue;
                        }
                        let points = &view.points[b.start..b.end];
                        if self.containment && far <= r2 {
                            stats.accepted_pairs += 1;
                            stats.blocks += 1;
                            out.extend(points.iter().map(|p| p.id));
                        } else {
                            stats.point_tests += points.len() as u64;
                            for p in points {
                                if distance(p.point, c) <= r2 {
                                    out.push(p.id);
                                }
                            }
                        }
                    }
                    stats.reported_pairs += out.len() as u64;
                }
                visit(chunk, &outputs[..chunk.len()]);
            }
            start = end;
        }
        Ok(stats)
    }

    pub fn new(cell_size: f64) -> Result<Self, Error> {
        if D != 2 && D != 3 {
            return Err(Error::Dimension);
        }
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return Err(Error::CellSize);
        }
        Ok(Self {
            cell_size,
            entries: Vec::new(),
            ids: HashMap::new(),
            view: None,
            strategy: Strategy::environment(),
            containment: true,
        })
    }
    pub fn from_points(
        cell_size: f64,
        points: impl IntoIterator<Item = (u64, [f64; D])>,
    ) -> Result<Self, Error> {
        let mut s = Self::new(cell_size)?;
        for (id, p) in points {
            s.upsert(id, p)?;
        }
        Ok(s)
    }
    pub fn set_strategy(&mut self, strategy: Strategy) {
        self.strategy = strategy;
    }
    pub fn set_containment(&mut self, containment: bool) {
        self.containment = containment;
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
    pub fn reserve(&mut self, n: usize) {
        self.entries.reserve(n);
        self.ids.reserve(n);
    }
    pub fn upsert(&mut self, id: u64, point: [f64; D]) -> Result<Option<[f64; D]>, Error> {
        if point.iter().any(|v| !v.is_finite()) {
            return Err(Error::Coordinate);
        }
        self.view = None;
        if let Some(&i) = self.ids.get(&id) {
            return Ok(Some(std::mem::replace(&mut self.entries[i].point, point)));
        }
        self.ids.insert(id, self.entries.len());
        self.entries.push(Entry { id, point });
        Ok(None)
    }
    pub fn remove(&mut self, id: u64) -> Option<[f64; D]> {
        let i = self.ids.remove(&id)?;
        self.view = None;
        let removed = self.entries.swap_remove(i);
        if i < self.entries.len() {
            self.ids.insert(self.entries[i].id, i);
        }
        Some(removed.point)
    }
    fn key(&self, p: [f64; D]) -> Option<[i64; D]> {
        let mut out = [0; D];
        for i in 0..D {
            let n = (p[i] / self.cell_size).floor();
            if !n.is_finite() || n.abs() > (1u64 << 48) as f64 {
                return None;
            }
            out[i] = n as i64;
        }
        Some(out)
    }
    /// Sort keys once and pack coordinates and identities together in cell order.
    pub fn prepare(&mut self) {
        if self.view.is_some() {
            return;
        }
        let mut keyed = Vec::with_capacity(self.entries.len());
        for &e in &self.entries {
            let Some(key) = self.key(e.point) else {
                return;
            };
            keyed.push((key, e));
        }
        keyed.sort_unstable_by_key(|(key, _)| *key);
        let mut points = Vec::with_capacity(keyed.len());
        let mut cells: Vec<Cell<D>> = Vec::new();
        for (key, e) in keyed {
            if cells.last().is_none_or(|c| c.key != key) {
                cells.push(Cell {
                    key,
                    start: points.len(),
                    end: points.len(),
                    lo: e.point,
                    hi: e.point,
                });
            }
            let cell = cells.last_mut().unwrap();
            for i in 0..D {
                cell.lo[i] = cell.lo[i].min(e.point[i]);
                cell.hi[i] = cell.hi[i].max(e.point[i]);
            }
            cell.end += 1;
            points.push(e);
        }
        let map = cells.iter().enumerate().map(|(i, c)| (c.key, i)).collect();
        self.view = Some(View { points, cells, map });
    }
    fn bounds(&self, c: [f64; D], r2: f64) -> Option<([i64; D], [i64; D], usize)> {
        // Squared-distance underflow/overflow follows the scalar oracle exactly.
        // Those exceptional radii use occupied cells, avoiding an unsafe radius
        // box assumption. Round the remaining box outward before quantization.
        if !r2.is_normal() {
            return None;
        }
        let reach = r2.sqrt() * (1.0 + 4.0 * f64::EPSILON);
        let mut lo = [0; D];
        let mut hi = [0; D];
        for i in 0..D {
            lo[i] = self.key_axis((c[i] - reach).next_down())?.checked_sub(1)?;
            hi[i] = self.key_axis((c[i] + reach).next_up())?.checked_add(1)?;
        }
        let mut volume = 1usize;
        for i in 0..D {
            volume = volume.checked_mul((hi[i] - lo[i] + 1).try_into().ok()?)?;
        }
        Some((lo, hi, volume))
    }
    fn key_axis(&self, x: f64) -> Option<i64> {
        let q = (x / self.cell_size).floor();
        (q.is_finite() && q.abs() <= (1u64 << 48) as f64).then_some(q as i64)
    }
    pub fn within_into(
        &self,
        centre: [f64; D],
        radius: f64,
        out: &mut Vec<u64>,
    ) -> Result<(), Error> {
        if centre.iter().any(|x| !x.is_finite()) {
            return Err(Error::Coordinate);
        }
        if !radius.is_finite() || radius < 0.0 {
            return Err(Error::Radius);
        }
        out.clear();
        let r2 = radius * radius;
        let Some(v) = &self.view else {
            for e in &self.entries {
                if distance(e.point, centre) <= r2 {
                    out.push(e.id);
                }
            }
            return Ok(());
        };
        let bounds = self.bounds(centre, r2);
        let visit = |cell: &Cell<D>, out: &mut Vec<u64>| {
            let mut nearest = 0.0;
            let mut farthest = 0.0;
            for i in 0..D {
                let a = cell.lo[i] - centre[i];
                let b = cell.hi[i] - centre[i];
                let near = if a > 0.0 {
                    a
                } else if b < 0.0 {
                    b
                } else {
                    0.0
                };
                nearest += near * near;
                let far = a.abs().max(b.abs());
                farthest += far * far;
            }
            if nearest > r2 {
                return;
            }
            let entries = &v.points[cell.start..cell.end];
            if self.containment && farthest <= r2 {
                out.extend(entries.iter().map(|e| e.id));
            } else {
                for e in entries {
                    if distance(e.point, centre) <= r2 {
                        out.push(e.id);
                    }
                }
            }
        };
        if let Some((lo, hi, volume)) = bounds {
            let begin = v.cells.partition_point(|b| b.key[0] < lo[0]);
            let end = v.cells.partition_point(|b| b.key[0] <= hi[0]);
            let hash = self.strategy == Strategy::HashGrid
                || (self.strategy == Strategy::Hybrid && volume.saturating_mul(4) < end - begin);
            if hash && volume <= v.cells.len().saturating_mul(4).min(100_000) {
                let mut key = lo;
                loop {
                    if let Some(&i) = v.map.get(&key) {
                        visit(&v.cells[i], out);
                    }
                    let mut axis = D;
                    loop {
                        if axis == 0 {
                            return Ok(());
                        }
                        axis -= 1;
                        if key[axis] < hi[axis] {
                            key[axis] += 1;
                            break;
                        }
                        key[axis] = lo[axis];
                    }
                }
            }
            if self.strategy != Strategy::OccupiedScan {
                for b in &v.cells[begin..end] {
                    if (1..D).all(|i| b.key[i] >= lo[i] && b.key[i] <= hi[i]) {
                        visit(b, out);
                    }
                }
                return Ok(());
            }
        }
        for b in &v.cells {
            visit(b, out);
        }
        Ok(())
    }
}
