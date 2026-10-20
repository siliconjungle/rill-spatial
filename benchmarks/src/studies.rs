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
        emit: &mut dyn FnMut(&[usize], &[Vec<u64>]),
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
        emit: &mut dyn FnMut(&[usize], &[Vec<u64>]),
    ) {
        let queries: Vec<_> = queries.iter().map(|q| (q.point, q.radius)).collect();
        self.0
            .0
            .batch_visit_groups(&queries, max_queries, emit)
            .unwrap();
    }
}
