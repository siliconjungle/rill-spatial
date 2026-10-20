//! Full independent proof, then uninterrupted service with concurrent consumers.
use crate::{Adapter, Fixture, Sample, bounded::Expected};
use std::{
    hint::black_box,
    sync::{
        Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Instant,
};
fn checksum(query: usize, row: &[u64]) -> u64 {
    // Distributivity in Z/(2^64) preserves v2's observable checksum while
    // avoiding a scalar64-bit multiply for every ID. Every ID is still read.
    let sum = row.iter().fold(0u64, |sum, &id| sum.wrapping_add(id));
    black_box(
        sum.wrapping_mul(0x9e3779b97f4a7c15)
            .wrapping_add(query as u64),
    )
}

struct Proof {
    seen: Vec<u64>,
    visited: Vec<bool>,
    counts: Vec<usize>,
    results: usize,
    checksum: u64,
    peak: usize,
}
pub(crate) fn run<A: Adapter>(name: &str, f: &Fixture, samples: usize, window: usize) {
    // Admission/normal-scale numerical domain was checked by bounded::run.
    let proof_start = Instant::now();
    let mut reference = f.points.clone();
    let mut index = A::build(&f.points, f.cell_size);
    let mut counts = Vec::new();
    let mut expected_total = 0usize;
    let mut expected_checksum = 0u64;
    let mut peak_group = 0usize;
    for frame in &f.frames {
        index.update(&frame.updates);
        for &(id, p) in &frame.updates {
            reference[id] = p;
        }
        if frame.queries.is_empty() {
            continue;
        }
        index.prepare();
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for point in &reference {
            for d in 0..3 {
                lo[d] = lo[d].min(point[d]);
                hi[d] = hi[d].max(point[d]);
            }
        }
        let proof = Mutex::new(Proof {
            seen: vec![0; reference.len().div_ceil(64)],
            visited: vec![false; frame.queries.len()],
            counts: Vec::with_capacity(frame.queries.len()),
            results: 0,
            checksum: 0,
            peak: 0,
        });
        index.visit_groups_shared(&frame.queries, window, &|ids, rows| {
            black_box(rows);
            assert_eq!(ids.len(), rows.len());
            assert!(ids.len() <= window);
            let mut proof = proof.lock().unwrap();
            for (&q, row) in ids.iter().zip(rows) {
                assert!(
                    q < frame.queries.len() && !proof.visited[q],
                    "query missing/duplicated"
                );
                proof.visited[q] = true;
                let query = &frame.queries[q];
                Expected::new(&reference, &query.point, query.radius, &lo, &hi).validate(
                    row,
                    &mut proof.seen,
                    reference.len(),
                );
                proof.results += row.len();
                proof.counts.push(row.len());
                proof.checksum ^= checksum(q, row);
            }
            proof.peak = proof.peak.max(rows.iter().map(|r| r.capacity() * 8).sum());
        });
        let proof = proof.into_inner().unwrap();
        assert!(proof.visited.iter().all(|&v| v));
        expected_total += proof.results;
        expected_checksum ^= proof.checksum;
        peak_group = peak_group.max(proof.peak);
        counts.extend(proof.counts);
    }
    drop(index);
    drop(reference);
    let proof_ns = proof_start.elapsed().as_nanos();
    let mut rows = Vec::new();
    let mut frames = Vec::new();
    for sample in 0..samples {
        let start = Instant::now();
        let mut index = A::build(black_box(&f.points), f.cell_size);
        let build_ns = start.elapsed().as_nanos() as u64;
        let (mut update_ns, mut query_ns, mut results, mut combined) = (0u64, 0u64, 0usize, 0u64);
        for (number, frame) in f.frames.iter().enumerate() {
            let frame_start = Instant::now();
            let start = Instant::now();
            index.update(black_box(&frame.updates));
            if !frame.queries.is_empty() {
                index.prepare();
            }
            let update = start.elapsed().as_nanos() as u64;
            update_ns += update;
            if frame.queries.is_empty() {
                frames.push(serde_json::json!({"sample":sample,"frame":number,"queries":0,"update_prepare_ns":update,"query_wall_ns":0,"frame_service_ns":frame_start.elapsed().as_nanos()as u64}));
                continue;
            }
            let count = AtomicUsize::new(0);
            let hash = AtomicU64::new(0);
            let peak = AtomicUsize::new(0);
            let start = Instant::now();
            index.visit_groups_shared(black_box(&frame.queries), window, &|ids, outputs| {
                black_box(outputs);
                assert_eq!(ids.len(), outputs.len());
                assert!(ids.len() <= window);
                let (mut local_count, mut local_hash) = (0usize, 0u64);
                for (&q, row) in ids.iter().zip(outputs) {
                    assert!(q < frame.queries.len(), "query index out of range");
                    local_count += row.len();
                    local_hash ^= checksum(q, row);
                }
                count.fetch_add(local_count, Ordering::Relaxed);
                hash.fetch_xor(local_hash, Ordering::Relaxed);
                peak.fetch_max(
                    outputs.iter().map(|v| v.capacity() * 8).sum(),
                    Ordering::Relaxed,
                );
            });
            let query = start.elapsed().as_nanos() as u64;
            let frame_ns = frame_start.elapsed().as_nanos() as u64;
            query_ns += query;
            results += count.into_inner();
            combined ^= hash.into_inner();
            peak_group = peak_group.max(peak.into_inner());
            frames.push(serde_json::json!({"sample":sample,"frame":number,"queries":frame.queries.len(),"update_prepare_ns":update,"query_wall_ns":query,"frame_service_ns":frame_ns}));
        }
        assert_eq!(
            results, expected_total,
            "timed output count differs from full proof"
        );
        assert_eq!(
            combined, expected_checksum,
            "timed output checksum differs from full proof"
        );
        rows.push(Sample {
            build_ns,
            update_ns,
            query_ns,
            total_ns: build_ns + update_ns + query_ns,
            results,
        });
    }
    counts.sort_unstable();
    let total: usize = counts.iter().sum();
    let query_count = counts.len();
    println!(
        "{}",
        serde_json::json!({"implementation":name,"scenario":f.name,"points":f.points.len(),"queries":query_count,"correct":true,"continuous_full_consumption":true,"continuous_protocol":"v3 shared consumer; equivalent factored checksum, no per-query shared timed counters; complete parallel-path membership/visit proof before timing","output_window_queries":window,"materialization":"bounded complete u64 result vectors; every ID consumed by same synthetic checksum; timed count/checksum agree with independent full-membership and exact query-visitation proof","timing_scope":"uninterrupted query-frame service including update/prepare, ordering, allocations, dispatch, scoped threads, full materialization and synthetic every-ID consumption; no scheduled-load queue, network or game simulation","proof_wall_ns":proof_ns.to_string(),"largest_emitted_group_capacity_bytes":peak_group,"capacity_scope":"one group only; not sum of live worker buffers","matches_per_query":{"mean":total as f64/query_count.max(1)as f64,"p99":counts.get((query_count*99/100).min(query_count.saturating_sub(1))).copied().unwrap_or(0),"max":counts.last().copied().unwrap_or(0)},"frame_samples":frames,"samples":rows})
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn factored_checksum_preserves_wrapping_observable() {
        for query in [0, 1, usize::MAX] {
            for row in [
                vec![],
                vec![u64::MAX],
                vec![u64::MAX, 1, 0, 999],
                (0..257u64)
                    .map(|x| x.wrapping_mul(0x0123456789abcdef))
                    .collect(),
            ] {
                let naive = row.iter().fold(query as u64, |sum, &id| {
                    sum.wrapping_add(id.wrapping_mul(0x9e3779b97f4a7c15))
                });
                assert_eq!(super::checksum(query, &row), naive);
            }
        }
    }
}
