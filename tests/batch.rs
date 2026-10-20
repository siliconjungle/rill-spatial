use rill_spatial::{BatchOptions, Error, SpatialIndex};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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

fn check_parallel<const D: usize>(
    index: &SpatialIndex<D>,
    queries: &[([f64; D], f64)],
    workers: usize,
    limit: usize,
) {
    // Snapshot the authoritative point rows before invoking the query API. The
    // oracle never calls within_into/within_batch or reads prepared tree state.
    let points: Vec<_> = index.iter().map(|(id, point)| (id, *point)).collect();
    let visited: Vec<_> = (0..queries.len()).map(|_| AtomicBool::new(false)).collect();
    let matches = AtomicUsize::new(0);
    let stats = index
        .within_batch_parallel(
            queries,
            BatchOptions {
                workers,
                queries_per_group: limit,
            },
            |indices, rows| {
                assert_eq!(indices.len(), rows.len());
                assert!(rows.len() <= limit);
                for (&q, row) in indices.iter().zip(rows) {
                    assert!(!visited[q].swap(true, Ordering::Relaxed));
                    let (centre, radius) = queries[q];
                    let mut expected: Vec<_> = points
                        .iter()
                        .filter(|(_, point)| {
                            if radius == 0.0 {
                                return point.iter().zip(centre).all(|(a, b)| *a == b);
                            }
                            if !(1e-150..=1e150).contains(&radius) {
                                point
                                    .iter()
                                    .zip(centre)
                                    .fold(0.0f64, |distance, (a, b)| distance.hypot(a - b))
                                    <= radius
                            } else {
                                point
                                    .iter()
                                    .zip(centre)
                                    .map(|(a, b)| (a - b) * (a - b))
                                    .sum::<f64>()
                                    <= radius * radius
                            }
                        })
                        .map(|(id, _)| *id)
                        .collect();
                    let mut actual = row.clone();
                    expected.sort_unstable();
                    actual.sort_unstable();
                    // Full membership and duplicate parity, including empty rows.
                    assert_eq!(actual, expected, "query {q}");
                    matches.fetch_add(row.len(), Ordering::Relaxed);
                }
            },
        )
        .unwrap();
    assert!(
        visited
            .iter()
            .all(|visited| visited.load(Ordering::Relaxed))
    );
    assert_eq!(stats.matches, matches.load(Ordering::Relaxed) as u128);
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
                    check_parallel(&index, &queries, workers, limit);
                }
            }
        }
        index.clear();
        check(&index, &queries, 4, 7);
        check(&index, &[], 4, 7);
        check_parallel(&index, &queries, 4, 7);
        check_parallel(&index, &[], 4, 7);
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
            vec![([0.0, 0.0], 1.0), ([f64::INFINITY, 0.0], 1.0)],
            BatchOptions::default(),
            Error::Coordinate,
        ),
        (
            vec![([0.0, 0.0], 1.0), ([0.0, 0.0], f64::NAN)],
            BatchOptions::default(),
            Error::Radius,
        ),
        (
            vec![([0.0, 0.0], 1.0), ([0.0, 0.0], f64::INFINITY)],
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
        assert_eq!(
            index.within_batch_parallel(&queries, options, |_, _| panic!(
                "invalid parallel batch emitted output"
            )),
            Err(error)
        );
    }
}

#[test]
fn callback_panic_unwinds_and_index_remains_usable() {
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

#[test]
fn parallel_callback_panic_unwinds_and_index_remains_usable() {
    let mut index =
        SpatialIndex::<3>::from_points(2.0, (0..257).map(|i| (i, [i as f64, 0.0, 0.0]))).unwrap();
    index.prepare();
    let queries = vec![([0.0; 3], 300.0); 79];
    for workers in [1, 2, 4] {
        let calls = AtomicUsize::new(0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = index.within_batch_parallel(
                &queries,
                BatchOptions {
                    workers,
                    queries_per_group: 1,
                },
                |_, _| {
                    calls.fetch_add(1, Ordering::Relaxed);
                    panic!("expected parallel consumer panic");
                },
            );
        }));
        assert!(result.is_err());
        assert!((1..=workers).contains(&calls.load(Ordering::Relaxed)));
        assert_eq!(index.within([0.0; 3], 300.0).unwrap().len(), 257);
        check_parallel(&index, &queries, workers, 7);
        check(&index, &queries, workers, 7);
    }
}

#[test]
fn parallel_callbacks_overlap_and_caller_participates() {
    let index = SpatialIndex::<2>::from_points(1.0, [(u64::MAX, [0.0; 2])]).unwrap();
    let queries = vec![([0.0; 2], 1.0); 16];
    let arrived = AtomicUsize::new(0);
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let caller_seen = AtomicBool::new(false);
    let caller = std::thread::current().id();
    index
        .within_batch_parallel(
            &queries,
            BatchOptions {
                workers: 4,
                queries_per_group: 1,
            },
            |_, rows| {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0], [u64::MAX]);
                if std::thread::current().id() == caller {
                    caller_seen.store(true, Ordering::Relaxed);
                }
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(current, Ordering::SeqCst);
                let arrival = arrived.fetch_add(1, Ordering::SeqCst);
                if arrival < 4 {
                    // A bounded rendezvous fails rather than deadlocking if a
                    // consumer mutex is accidentally introduced in this path.
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
                    while arrived.load(Ordering::SeqCst) < 4 && std::time::Instant::now() < deadline
                    {
                        std::thread::yield_now();
                    }
                    assert!(
                        arrived.load(Ordering::SeqCst) >= 4,
                        "callbacks were serialized"
                    );
                }
                active.fetch_sub(1, Ordering::SeqCst);
            },
        )
        .unwrap();
    assert_eq!(peak.load(Ordering::Relaxed), 4);
    assert!(caller_seen.load(Ordering::Relaxed));
}

#[test]
fn parallel_callback_requires_sync_without_requiring_send() {
    let index = SpatialIndex::<2>::new(1.0).unwrap();
    let state = std::sync::Mutex::new(11);
    let guard = state.lock().unwrap();
    // MutexGuard is Sync but not Send. The owned callback stays on the caller,
    // while workers borrow it through shared references.
    index
        .within_batch_parallel(
            &[([0.0; 2], 1.0); 7],
            BatchOptions {
                workers: 4,
                queries_per_group: 1,
            },
            move |_, _| assert_eq!(*guard, 11),
        )
        .unwrap();
}
