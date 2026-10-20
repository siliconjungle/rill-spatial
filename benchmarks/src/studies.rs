//! Internal independently developed candidates; not additional competitors.
use crate::{Adapter, Point, Query};
pub struct Cell<const MODE: u8>(rill_cells::Index3);
impl<const MODE: u8> Adapter for Cell<MODE> {
    fn build(points: &[Point], size: f64) -> Self {
        let mut index = rill_cells::Index3::from_points(
            size,
            points.iter().enumerate().map(|(i, &p)| (i as u64, p)),
        )
        .unwrap();
        index.set_strategy(match MODE {
            0 => rill_cells::Strategy::SortedSlab,
            1 => rill_cells::Strategy::Hybrid,
            2 => rill_cells::Strategy::HashGrid,
            _ => rill_cells::Strategy::OccupiedScan,
        });
        Self(index)
    }
    fn update(&mut self, updates: &[(usize, Point)]) {
        for &(id, p) in updates {
            self.0.upsert(id as u64, p).unwrap();
        }
    }
    fn prepare(&mut self) {
        self.0.prepare();
    }
    fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
        self.0.within_into(p, r, out).unwrap();
    }
}
pub struct Dual(rill_dual::Index3);
impl Adapter for Dual {
    fn build(points: &[Point], size: f64) -> Self {
        Self(
            rill_dual::Index3::from_points(
                size,
                points.iter().enumerate().map(|(i, &p)| (i as u64, p)),
            )
            .unwrap(),
        )
    }
    fn update(&mut self, updates: &[(usize, Point)]) {
        for &(id, p) in updates {
            self.0.upsert(id as u64, p).unwrap();
        }
    }
    fn prepare(&mut self) {
        self.0.prepare();
    }
    fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
        self.0.within_into(p, r, out).unwrap();
    }
    fn query_batch(&self, queries: &[Query], outputs: &mut [Vec<u64>]) {
        outputs.iter_mut().for_each(Vec::clear);
        let queries: Vec<_> = queries.iter().map(|q| (q.point, q.radius)).collect();
        self.0
            .batch_blocks(&queries, |query_ids, ids| {
                for &q in query_ids {
                    outputs[q].extend_from_slice(ids);
                }
            })
            .unwrap();
    }
    fn visit_groups(
        &self,
        queries: &[Query],
        max_queries: usize,
        emit: &mut (dyn FnMut(&[usize], &[Vec<u64>]) + Send),
    ) {
        let queries: Vec<_> = queries.iter().map(|q| (q.point, q.radius)).collect();
        self.0
            .batch_visit_groups(&queries, max_queries, emit)
            .unwrap();
    }
}

pub struct CellCoherent<const MODE: u8>(Cell<MODE>);
impl<const MODE: u8> Adapter for CellCoherent<MODE> {
    fn build(points: &[Point], size: f64) -> Self {
        Self(Cell::build(points, size))
    }
    fn update(&mut self, updates: &[(usize, Point)]) {
        self.0.update(updates);
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
        max_queries: usize,
        emit: &mut (dyn FnMut(&[usize], &[Vec<u64>]) + Send),
    ) {
        let queries: Vec<_> = queries.iter().map(|q| (q.point, q.radius)).collect();
        self.0
            .0
            .batch_visit_groups(&queries, max_queries, emit)
            .unwrap();
    }
}

/// Public main-library batch surface, separate from generic scheduler prototype.
pub struct PublicBatch<const WORKERS: usize>(rill_spatial::Index3);
impl<const WORKERS: usize> Adapter for PublicBatch<WORKERS> {
    fn build(points: &[Point], size: f64) -> Self {
        Self(
            rill_spatial::Index3::from_points(
                size,
                points.iter().enumerate().map(|(i, &p)| (i as u64, p)),
            )
            .unwrap(),
        )
    }
    fn update(&mut self, changes: &[(usize, Point)]) {
        for &(id, p) in changes {
            self.0.upsert(id as u64, p).unwrap();
        }
    }
    fn prepare(&mut self) {
        self.0.prepare();
    }
    fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
        self.0.within_into(p, r, out).unwrap();
    }
    fn visit_groups(
        &self,
        queries: &[Query],
        max_queries: usize,
        emit: &mut (dyn FnMut(&[usize], &[Vec<u64>]) + Send),
    ) {
        if WORKERS > 1 {
            assert_eq!(std::env::var("RILL_CONTINUOUS").as_deref(), Ok("1"));
        }
        let queries: Vec<_> = queries.iter().map(|q| (q.point, q.radius)).collect();
        self.0
            .within_batch(
                &queries,
                rill_spatial::BatchOptions {
                    workers: WORKERS,
                    queries_per_group: max_queries,
                },
                emit,
            )
            .unwrap();
    }
}

/// Explicit concurrent consumer API; complete result rows remain materialized.
pub struct PublicParallel<const WORKERS: usize>(PublicBatch<WORKERS>);
impl<const WORKERS: usize> Adapter for PublicParallel<WORKERS> {
    fn build(points: &[Point], size: f64) -> Self {
        Self(PublicBatch::build(points, size))
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
        let consumer = std::sync::Mutex::new(emit);
        self.visit_groups_shared(queries, limit, &|ids, rows| {
            consumer.lock().unwrap()(ids, rows)
        });
    }
    fn visit_groups_shared(
        &self,
        queries: &[Query],
        limit: usize,
        emit: &(dyn Fn(&[usize], &[Vec<u64>]) + Sync),
    ) {
        assert_eq!(std::env::var("RILL_CONTINUOUS").as_deref(), Ok("1"));
        let queries: Vec<_> = queries.iter().map(|q| (q.point, q.radius)).collect();
        self.0
            .0
            .within_batch_parallel(
                &queries,
                rill_spatial::BatchOptions {
                    workers: WORKERS,
                    queries_per_group: limit,
                },
                emit,
            )
            .unwrap();
    }
}

/// Radius-hinted topology; same complete-row scheduler as public baseline.
pub struct RadiusAware<const WORKERS: usize>(rill_radius::Index3);
impl<const WORKERS: usize> Adapter for RadiusAware<WORKERS> {
    fn build(points: &[Point], size: f64) -> Self {
        Self(
            rill_radius::Index3::from_points(
                size,
                points.iter().enumerate().map(|(i, &p)| (i as u64, p)),
            )
            .unwrap(),
        )
    }
    fn update(&mut self, changes: &[(usize, Point)]) {
        for &(id, p) in changes {
            self.0.upsert(id as u64, p).unwrap();
        }
    }
    fn prepare(&mut self) {
        self.0.prepare();
    }
    fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
        self.0.within_into(p, r, out).unwrap();
    }
    fn visit_groups(
        &self,
        queries: &[Query],
        limit: usize,
        emit: &mut (dyn FnMut(&[usize], &[Vec<u64>]) + Send),
    ) {
        let consumer = std::sync::Mutex::new(emit);
        self.visit_groups_shared(queries, limit, &|ids, rows| {
            consumer.lock().unwrap()(ids, rows)
        });
    }
    fn visit_groups_shared(
        &self,
        queries: &[Query],
        limit: usize,
        emit: &(dyn Fn(&[usize], &[Vec<u64>]) + Sync),
    ) {
        if WORKERS > 1 {
            assert_eq!(std::env::var("RILL_CONTINUOUS").as_deref(), Ok("1"));
        }
        let queries: Vec<_> = queries.iter().map(|q| (q.point, q.radius)).collect();
        self.0
            .within_batch_parallel(
                &queries,
                rill_radius::BatchOptions {
                    workers: WORKERS,
                    queries_per_group: limit,
                },
                emit,
            )
            .unwrap();
    }
}

/// Original public geometry with adaptive sampled output windows.
pub struct AutoOutput<const WORKERS: usize>(rill_auto::Index3);
impl<const WORKERS: usize> Adapter for AutoOutput<WORKERS> {
    fn build(points: &[Point], size: f64) -> Self {
        Self(
            rill_auto::Index3::from_points(
                size,
                points.iter().enumerate().map(|(i, &p)| (i as u64, p)),
            )
            .unwrap(),
        )
    }
    fn update(&mut self, changes: &[(usize, Point)]) {
        for &(id, p) in changes {
            self.0.upsert(id as u64, p).unwrap();
        }
    }
    fn prepare(&mut self) {
        self.0.prepare();
    }
    fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
        self.0.within_into(p, r, out).unwrap();
    }
    fn visit_groups(
        &self,
        queries: &[Query],
        limit: usize,
        emit: &mut (dyn FnMut(&[usize], &[Vec<u64>]) + Send),
    ) {
        let consumer = std::sync::Mutex::new(emit);
        self.visit_groups_shared(queries, limit, &|ids, rows| {
            consumer.lock().unwrap()(ids, rows)
        });
    }
    fn visit_groups_shared(
        &self,
        queries: &[Query],
        limit: usize,
        emit: &(dyn Fn(&[usize], &[Vec<u64>]) + Sync),
    ) {
        if WORKERS > 1 {
            assert_eq!(std::env::var("RILL_CONTINUOUS").as_deref(), Ok("1"));
        }
        let queries: Vec<_> = queries.iter().map(|q| (q.point, q.radius)).collect();
        self.0
            .within_batch_parallel(
                &queries,
                rill_auto::BatchOptions {
                    workers: WORKERS,
                    queries_per_group: limit,
                },
                emit,
            )
            .unwrap();
    }
}
