use rill_spatial::{Error, SpatialIndex, Strategy};
use std::collections::BTreeMap;

fn oracle<const D: usize>(points: &BTreeMap<u64, [f64; D]>, c: [f64; D], r: f64) -> Vec<u64> {
    points
        .iter()
        .filter(|(_, p)| p.iter().zip(c).map(|(a, b)| (a - b) * (a - b)).sum::<f64>() <= r * r)
        .map(|(&id, _)| id)
        .collect()
}
fn check<const D: usize>(
    s: &SpatialIndex<D>,
    points: &BTreeMap<u64, [f64; D]>,
    queries: &[([f64; D], f64)],
    width: usize,
) {
    let mut seen = vec![false; queries.len()];
    let mut count = 0;
    let stats = s
        .batch_visit_groups(queries, width, |indices, outputs| {
            assert_eq!(indices.len(), outputs.len());
            assert!(indices.len() <= width.min(64));
            for (&q, actual) in indices.iter().zip(outputs) {
                assert!(!seen[q], "duplicate query {q}");
                seen[q] = true;
                let (c, r) = queries[q];
                let expected = oracle(points, c, r);
                let mut actual = actual.clone();
                actual.sort_unstable();
                assert_eq!(actual, expected, "q={q}, centre={c:?}, radius={r:?}");
                count += actual.len() as u64;
            }
        })
        .unwrap();
    assert!(seen.iter().all(|x| *x));
    assert_eq!(stats.reported_pairs, count);
}
fn exercise<const D: usize>() {
    for strategy in [
        Strategy::HashGrid,
        Strategy::SortedSlab,
        Strategy::OccupiedScan,
        Strategy::Hybrid,
    ] {
        for containment in [false, true] {
            let mut points = BTreeMap::new();
            let mut s = SpatialIndex::<D>::new(8.0).unwrap();
            s.set_strategy(strategy);
            s.set_containment(containment);
            for i in 0..500u64 {
                let id = i.wrapping_mul(0x9e3779b97f4a7c15);
                let p = std::array::from_fn(|axis| {
                    if i < 250 {
                        ((i * (17 + axis as u64 * 4)) % 80) as f64 / 20.0
                    } else {
                        ((i * (31 + axis as u64 * 6)) % 500) as f64 - 250.0
                    }
                });
                points.insert(id, p);
                s.upsert(id, p).unwrap();
            }
            let queries: Vec<_> = (0..183)
                .map(|i| {
                    let c = std::array::from_fn(|axis| {
                        if i < 130 {
                            (i % 13) as f64 / 10.0 + axis as f64 * 0.12345
                        } else {
                            (i % 29) as f64 * 13.37 - 200.0
                        }
                    });
                    let r = match i % 11 {
                        0 => 0.0,
                        1 => f64::from_bits(1),
                        2 => 1e-162,
                        3 => 2000.0,
                        _ => (i % 17) as f64 * 0.71,
                    };
                    (c, r)
                })
                .chain([(std::array::from_fn(|_| 100000.5), 0.25)])
                .collect();
            for prepared in [false, true] {
                if prepared {
                    s.prepare();
                }
                for width in [1, 7, 64, usize::MAX] {
                    check(&s, &points, &queries, width);
                }
            }
            // Teleports and removals invalidate any previous candidate views.
            for i in 0..150u64 {
                let id = i.wrapping_mul(0x9e3779b97f4a7c15);
                if i % 3 == 0 {
                    s.remove(id);
                    points.remove(&id);
                } else {
                    let p = [-1000.0 - i as f64; D];
                    s.upsert(id, p).unwrap();
                    points.insert(id, p);
                }
            }
            check(&s, &points, &queries, 17);
            s.prepare();
            check(&s, &points, &queries, 17);
        }
    }
}
#[test]
fn variable_radii_dense_teleports_2d() {
    exercise::<2>();
}
#[test]
fn variable_radii_dense_teleports_3d() {
    exercise::<3>();
}

#[test]
fn exceptional_scales_and_underflow_are_exact() {
    for size in [f64::from_bits(1), 1e-165, 1.0, 1e150, f64::MAX] {
        let points = BTreeMap::from([
            (1, [0.0, 0.0, 0.0]),
            (3, [1e-162, 0.0, 0.0]),
            (7, [1.0, 1.0, 1.0]),
            (99, [f64::MAX, 0.0, -f64::MAX]),
        ]);
        let queries = vec![
            ([0.0; 3], 0.0),
            ([0.0; 3], 1e-162),
            ([0.0; 3], 1e-150),
            ([0.0; 3], 1.0),
            ([0.0; 3], f64::MAX),
            ([f64::MAX, 0.0, -f64::MAX], 0.0),
            ([-f64::MAX; 3], f64::MAX),
        ];
        for strategy in [
            Strategy::HashGrid,
            Strategy::SortedSlab,
            Strategy::OccupiedScan,
            Strategy::Hybrid,
        ] {
            let mut s =
                SpatialIndex::<3>::from_points(size, points.iter().map(|(&id, &p)| (id, p)))
                    .unwrap();
            s.set_strategy(strategy);
            s.prepare();
            check(&s, &points, &queries, 2);
        }
        // A prepared view with many separate tiny cells must still accept
        // underflow-distance matches at radius zero.
        let tiny = BTreeMap::from([(5, [0.0, 0.0, 0.0]), (13, [1e-162, 0.0, 0.0])]);
        let mut s =
            SpatialIndex::<3>::from_points(size, tiny.iter().map(|(&id, &p)| (id, p))).unwrap();
        s.prepare();
        check(&s, &tiny, &queries[..3], 64);
    }
}

#[test]
fn batch_validation_is_atomic_and_empty_queries_are_visited() {
    let mut s = SpatialIndex::<2>::new(1.0).unwrap();
    s.prepare();
    let mut calls = 0;
    assert_eq!(
        s.batch_visit_groups(&[([0.0; 2], 1.0)], 0, |_, _| calls += 1)
            .unwrap_err(),
        Error::BatchSize
    );
    for (bad, error) in [
        (([f64::NAN, 0.0], 1.0), Error::Coordinate),
        (([0.0; 2], -1.0), Error::Radius),
        (([0.0; 2], f64::INFINITY), Error::Radius),
    ] {
        assert_eq!(
            s.batch_visit_groups(&[([0.0; 2], 1.0), bad], 64, |_, _| calls += 1)
                .unwrap_err(),
            error
        );
    }
    assert_eq!(calls, 0);
    let stats = s.batch_visit_groups(&[], 64, |_, _| calls += 1).unwrap();
    assert_eq!(calls, 0);
    assert_eq!(stats.reported_pairs, 0);
    let points = BTreeMap::new();
    check(&s, &points, &[([0.0; 2], 0.0); 150], usize::MAX);
}

#[test]
fn a_query_cell_shares_discovery_across_output_partitions() {
    let mut s = SpatialIndex::<2>::from_points(8.0, [(9, [0.0, 0.0]), (99, [0.5, 0.5])]).unwrap();
    s.prepare();
    let queries: Vec<_> = (0..150)
        .map(|i| ([0.123 + i as f64 * 0.0001, 0.234], 2.0))
        .collect();
    let mut calls = 0;
    let stats = s
        .batch_visit_groups(&queries, 7, |indices, outputs| {
            calls += 1;
            assert!(indices.len() <= 7);
            for ids in outputs {
                let mut ids = ids.clone();
                ids.sort_unstable();
                assert_eq!(ids, [9, 99]);
            }
        })
        .unwrap();
    assert_eq!(calls, 22);
    assert_eq!(stats.groups, 1);
    assert_eq!(stats.candidate_cells, 1);
    assert_eq!(stats.reported_pairs, 300);
    assert_eq!(stats.point_tests, 0);
}
