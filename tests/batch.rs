use rill_spatial::{BatchOptions, Error, SpatialIndex};
fn check<const D: usize>(
    index: &SpatialIndex<D>,
    queries: &[([f64; D], f64)],
    workers: usize,
    limit: usize,
) {
    let mut visited = vec![false; queries.len()];
    let mut matches = 0u128;
    let stats = index
        .within_batch(
            queries,
            BatchOptions {
                workers,
                queries_per_group: limit,
            },
            |ids, rows| {
                assert_eq!(ids.len(), rows.len());
                assert!(rows.len() <= limit);
                for (&q, row) in ids.iter().zip(rows) {
                    assert!(!visited[q]);
                    visited[q] = true;
                    let (centre, radius) = queries[q];
                    let mut expected: Vec<_> = index
                        .iter()
                        .filter(|(_, p)| {
                            if radius == 0.0 {
                                return p.iter().zip(centre).all(|(a, b)| *a == b);
                            }
                            if !(1e-150..=1e150).contains(&radius) {
                                let mut distance = 0.0f64;
                                for d in 0..D {
                                    distance = distance.hypot(p[d] - centre[d]);
                                }
                                distance <= radius
                            } else {
                                let mut squared = 0.0;
                                for d in 0..D {
                                    let delta = p[d] - centre[d];
                                    squared += delta * delta;
                                }
                                squared <= radius * radius
                            }
                        })
                        .map(|(id, _)| id)
                        .collect();
                    let mut actual = row.clone();
                    actual.sort_unstable();
                    expected.sort_unstable();
                    assert_eq!(actual, expected);
                    matches += row.len() as u128;
                }
            },
        )
        .unwrap();
    assert!(visited.iter().all(|&v| v));
    assert_eq!(stats.matches, matches);
    assert_eq!(stats.queries, queries.len());
    assert!(stats.workers <= workers);
    if queries.is_empty() {
        assert_eq!(stats.groups, 0);
        assert_eq!(stats.workers, 0);
    }
}
#[test]
fn batch_full_membership_across_workers_epochs_and_dimensions() {
    fn scenario<const D: usize>() {
        let mut index = SpatialIndex::<D>::from_points(
            7.0,
            (0..769u64).map(|i| {
                (
                    u64::MAX - i * 7919,
                    std::array::from_fn(|d| (((i * 37 + d as u64 * 71) % 103) as f64 - 51.0) / 4.0),
                )
            }),
        )
        .unwrap();
        let mut queries: Vec<_> = (0..137u64)
            .map(|i| {
                (
                    std::array::from_fn(|d| (((i * 13 + d as u64 * 11) % 89) as f64 - 44.0) / 3.0),
                    (i % 17) as f64 / 2.0,
                )
            })
            .collect();
        queries.extend([
            ([0.0; D], 0.0),
            ([0.0; D], 1e-200),
            ([0.0; D], 1e200),
            ([f64::MAX; D], f64::MAX),
            ([1e-200; D], 1e-200),
        ]);
        for state in 0..5 {
            if state == 1 || state == 4 {
                index.prepare();
            }
            if state == 2 {
                index.upsert(u64::MAX, [3.0; D]).unwrap();
            }
            if state == 3 {
                index.remove(u64::MAX - 7919);
                index.upsert(123, [0.0; D]).unwrap();
            }
            for workers in [1, 2, 4] {
                for limit in [1, 7, 64, 1024] {
                    check(&index, &queries, workers, limit);
                }
            }
        }
        index.clear();
        check(&index, &queries, 4, 7);
        check(&index, &[], 4, 7);
    }
    scenario::<2>();
    scenario::<3>();
}
#[test]
fn batch_invalid_input_has_no_callbacks() {
    let index = SpatialIndex::<2>::new(1.0).unwrap();
    for (queries, options, error) in [
        (
            vec![([0.0, 0.0], 1.0), ([f64::NAN, 0.0], 1.0)],
            BatchOptions::default(),
            Error::Coordinate,
        ),
        (
            vec![([0.0, 0.0], 1.0), ([0.0, 0.0], -1.0)],
            BatchOptions::default(),
            Error::Radius,
        ),
        (
            vec![],
            BatchOptions {
                workers: 0,
                queries_per_group: 7,
            },
            Error::BatchConfiguration,
        ),
        (
            vec![],
            BatchOptions {
                workers: 65,
                queries_per_group: 7,
            },
            Error::BatchConfiguration,
        ),
        (
            vec![],
            BatchOptions {
                workers: 1,
                queries_per_group: 0,
            },
            Error::BatchConfiguration,
        ),
    ] {
        assert_eq!(
            index.within_batch(&queries, options, |_, _| panic!(
                "invalid batch emitted output"
            )),
            Err(error)
        );
    }
}

#[test]
fn callback_panic_unwinds_and_index_remains_usable() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut index =
        SpatialIndex::<3>::from_points(2.0, (0..257).map(|i| (i, [i as f64, 0.0, 0.0]))).unwrap();
    index.prepare();
    let queries = vec![([0.0; 3], 300.0); 79];
    for workers in [1, 4] {
        let calls = AtomicUsize::new(0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = index.within_batch(
                &queries,
                BatchOptions {
                    workers,
                    queries_per_group: 1,
                },
                |_, _| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    panic!("expected consumer panic");
                },
            );
        }));
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(index.within([0.0; 3], 300.0).unwrap().len(), 257);
        check(&index, &queries, workers, 7);
    }
}
