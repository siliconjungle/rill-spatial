//! Full memberships against an independently owned point snapshot; no result
//! matrix is retained. These tests are source-only until root validates them.
use rill_spatial::{BatchOptions, BatchStats, Error, Index3};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

type Point = [f64; 3];
type Query = (Point, f64);

fn points(n: usize) -> Vec<(u64, Point)> {
    (0..n)
        .map(|i| {
            (
                u64::MAX - i as u64 * 97,
                [(i % 128) as f64, ((i / 128) % 128) as f64, 0.0],
            )
        })
        .collect()
}

fn check(
    index: &Index3,
    points: &[(u64, Point)],
    queries: &[Query],
    serialized: bool,
    expected_max_rows: usize,
) -> (BatchStats, usize, usize) {
    let lookup: HashMap<_, _> = points
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    let seen: Vec<_> = (0..queries.len()).map(|_| AtomicBool::new(false)).collect();
    let groups = AtomicUsize::new(0);
    let matches = AtomicUsize::new(0);
    let max_rows = AtomicUsize::new(0);
    let max_logical_bytes = AtomicUsize::new(0);
    let visit = |indices: &[usize], outputs: &[Vec<u64>]| {
        assert_eq!(indices.len(), outputs.len());
        assert!(outputs.len() <= expected_max_rows);
        groups.fetch_add(1, Ordering::Relaxed);
        max_rows.fetch_max(outputs.len(), Ordering::Relaxed);
        max_logical_bytes.fetch_max(
            outputs.iter().map(|row| row.len() * 8).sum(),
            Ordering::Relaxed,
        );
        let mut bits = vec![0u64; points.len().div_ceil(64)];
        for (&q, row) in indices.iter().zip(outputs) {
            assert!(
                !seen[q].swap(true, Ordering::Relaxed),
                "duplicate query {q}"
            );
            bits.fill(0);
            for &id in row {
                let i = lookup[&id];
                let mask = 1 << (i % 64);
                assert_eq!(bits[i / 64] & mask, 0, "duplicate q={q} id={id}");
                bits[i / 64] |= mask;
            }
            let (centre, radius) = queries[q];
            for (i, (_, point)) in points.iter().enumerate() {
                let expected = point
                    .iter()
                    .zip(centre)
                    .map(|(x, y)| (x - y) * (x - y))
                    .sum::<f64>()
                    <= radius * radius;
                assert_eq!(
                    (bits[i / 64] >> (i % 64)) & 1,
                    expected as u64,
                    "q={q} point={i}"
                );
            }
            matches.fetch_add(row.len(), Ordering::Relaxed);
        }
    };
    let options = BatchOptions {
        workers: 4,
        queries_per_group: 64,
    };
    let stats = if serialized {
        index.within_batch(queries, options, visit).unwrap()
    } else {
        index
            .within_batch_parallel(queries, options, visit)
            .unwrap()
    };
    assert!(seen.iter().all(|seen| seen.load(Ordering::Relaxed)));
    assert_eq!(stats.queries, queries.len());
    assert_eq!(stats.groups, groups.load(Ordering::Relaxed));
    assert_eq!(stats.matches, matches.load(Ordering::Relaxed) as u128);
    (
        stats,
        max_rows.load(Ordering::Relaxed),
        max_logical_bytes.load(Ordering::Relaxed),
    )
}

#[test]
fn dense_1024_queries_shrink_to_eight_rows_without_extra_emissions() {
    let points = points(16_384);
    let mut index = Index3::from_points(4.0, points.iter().copied()).unwrap();
    index.prepare();
    let queries = vec![([0.125, 0.25, 0.5], 1e6); 1024];
    let (stats, max_rows, _) = check(&index, &points, &queries, false, 8);
    assert_eq!(stats.groups, 128);
    assert_eq!(stats.matches, 16_384 * 1024);
    assert_eq!(max_rows, 8);
}

#[test]
fn sparse_samples_keep_configured_window_and_complete_empty_rows() {
    let points = points(4096);
    let index = Index3::from_points(4.0, points.iter().copied()).unwrap();
    let queries = vec![([1e9; 3], 1.0); 1024];
    let (stats, max_rows, _) = check(&index, &points, &queries, true, 64);
    assert_eq!(stats.groups, 16);
    assert_eq!(stats.matches, 0);
    assert_eq!(max_rows, 64);
}

#[test]
fn fewer_than_1024_queries_keep_configured_window() {
    let points = points(2049);
    let mut index = Index3::from_points(4.0, points.iter().copied()).unwrap();
    index.prepare();
    let queries = vec![([0.125, 0.25, 0.5], 1e6); 1023];
    // Worst-case bytes exceed 1 MiB, but the query-count precondition is false.
    let (stats, max_rows, _) = check(&index, &points, &queries, true, 64);
    assert_eq!(stats.groups, 16);
    assert_eq!(stats.matches, 2049 * 1023);
    assert_eq!(max_rows, 64);
}

#[test]
fn missed_dense_hotspots_are_exact_and_can_exceed_soft_target() {
    let points = points(4096);
    let mut index = Index3::from_points(4.0, points.iter().copied()).unwrap();
    index.prepare();
    let mut queries = vec![([0.125, 0.25, 0.5], 1e6); 1024];
    for sample in [0, 256, 512, 768] {
        queries[sample] = ([1e9; 3], 1.0);
    }
    // All four probes are empty. Dense unsampled results still expand fully.
    for serialized in [false, true] {
        let (stats, max_rows, max_bytes) = check(&index, &points, &queries, serialized, 64);
        assert_eq!(stats.groups, 16);
        assert_eq!(stats.matches, 4096 * 1020);
        assert_eq!(max_rows, 64);
        assert!(
            max_bytes > 1024 * 1024,
            "the target must not be treated as a hard bound"
        );
    }
}

#[test]
fn uneven_query_count_samples_floor_three_quarters() {
    let points = points(4096);
    let index = Index3::from_points(4.0, points.iter().copied()).unwrap();
    let mut queries = vec![([1e9; 3], 1.0); 1027];
    let sample = 3 * queries.len() / 4;
    queries[sample] = ([0.125, 0.25, 0.5], 1e6);
    let (stats, max_rows, _) = check(&index, &points, &queries, false, 32);
    assert_eq!(stats.matches, 4096);
    assert!(stats.groups > 1027usize.div_ceil(64));
    assert!(max_rows <= 32);
}

#[test]
fn late_invalid_query_emits_nothing_in_large_adaptive_batch() {
    let points = points(4096);
    let index = Index3::from_points(4.0, points).unwrap();
    let mut queries = vec![([0.0; 3], 1e6); 1024];
    queries[1023].1 = f64::NAN;
    let options = BatchOptions {
        workers: 4,
        queries_per_group: 64,
    };
    assert_eq!(
        index.within_batch(&queries, options, |_, _| panic!("invalid batch emitted")),
        Err(Error::Radius)
    );
    assert_eq!(
        index.within_batch_parallel(&queries, options, |_, _| panic!("invalid batch emitted")),
        Err(Error::Radius)
    );
}
