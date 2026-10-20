#[path = "../examples/support/cohort.rs"]
mod cohort;
use rill_spatial::Index3;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};

fn check(
    index: &Index3,
    points: &[(u64, [f64; 3])],
    queries: &[cohort::Query],
    workers: usize,
    max_queries: usize,
) {
    let ids: HashMap<_, _> = points
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    let visited: Vec<_> = (0..queries.len()).map(|_| AtomicBool::new(false)).collect();
    let stats = cohort::run(
        index,
        queries,
        workers,
        max_queries,
        |_, indices, outputs| {
            assert!(indices.len() <= max_queries);
            assert_eq!(indices.len(), outputs.len());
            let mut bits = vec![0u64; (points.len() + 63) / 64];
            for (&q, out) in indices.iter().zip(outputs) {
                assert!(!visited[q].swap(true, Ordering::Relaxed));
                bits.fill(0);
                for &id in out {
                    let i = ids[&id];
                    let mask = 1 << (i % 64);
                    assert_eq!(bits[i / 64] & mask, 0, "duplicate q={q} id={id}");
                    bits[i / 64] |= mask;
                }
                let (centre, radius) = queries[q];
                for (i, (id, p)) in points.iter().enumerate() {
                    let expected = p
                        .iter()
                        .zip(centre)
                        .map(|(x, y)| (x - y) * (x - y))
                        .sum::<f64>()
                        <= radius * radius;
                    assert_eq!(
                        (bits[i / 64] >> (i % 64)) & 1,
                        expected as u64,
                        "missing/extra q={q} id={id}"
                    );
                }
            }
        },
    );
    assert!(visited.iter().all(|b| b.load(Ordering::Relaxed)));
    assert_eq!(stats.queries, queries.len() as u64);
    assert!(stats.ordering_ns <= stats.wall_ns);
}

#[test]
fn parallel_full_membership_arbitrary_ids_variable_radii_and_mutation() {
    let mut points: Vec<_> = (0..1703)
        .map(|i| {
            (
                u64::MAX - i * 97,
                [
                    ((i * 373) % 1009) as f64,
                    ((i * 43) % 719) as f64,
                    (i % 7) as f64,
                ],
            )
        })
        .collect();
    let queries: Vec<_> = (0..413)
        .map(|i| {
            (
                [
                    ((i * 367) % 1009) as f64 + 0.125,
                    ((i * 41) % 719) as f64 - 0.25,
                    0.5,
                ],
                [0., 3., 17., 91., 2000.][i % 5],
            )
        })
        .collect();
    let mut index = Index3::from_points(4., points.iter().copied()).unwrap();
    index.prepare();
    for workers in [1, 2, 4] {
        for max_queries in [1, 7, 16, 64] {
            check(&index, &points, &queries, workers, max_queries);
        }
    }
    for point in &mut points[..31] {
        point.1 = [-9., 4., 0.];
        index.upsert(point.0, point.1).unwrap();
    }
    for workers in [1, 2, 4] {
        check(&index, &points, &queries, workers, 31);
    }
    index.prepare();
    for workers in [1, 2, 4] {
        check(&index, &points, &queries, workers, 64);
    }
}

#[test]
fn parallel_empty_and_extreme_closed_float_membership() {
    let mut points = vec![
        (3, [f64::MAX, 0., 0.]),
        (9, [-f64::MAX, 0., 0.]),
        (17, [f64::from_bits(1), 0., 0.]),
    ];
    points.extend((0..100).map(|i| (1000 + i, [i as f64, 0., 0.])));
    let queries = vec![
        ([0.; 3], 0.),
        ([0.; 3], 1.),
        ([0.; 3], f64::MAX),
        ([f64::MAX, 0., 0.], 0.),
        ([-f64::MAX, 0., 0.], f64::MAX),
    ];
    let mut index = Index3::from_points(1., points.iter().copied()).unwrap();
    index.prepare();
    for workers in [1, 2, 4] {
        check(&index, &points, &queries, workers, 2);
        check(&index, &points, &[], workers, 2);
    }
    let mut empty = Index3::new(1.).unwrap();
    empty.prepare();
    for workers in [1, 2, 4] {
        check(&empty, &[], &queries, workers, 2);
    }
    assert_eq!(cohort::cohort_size(20_000, 4), 52);
}
