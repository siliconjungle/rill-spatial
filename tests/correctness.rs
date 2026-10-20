use rill_spatial::{Error, Index2, Index3, SpatialIndex};
use std::collections::BTreeMap;

fn random(seed: &mut u64) -> f64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*seed >> 32) as u32 as f64 / u32::MAX as f64) * 2000.0 - 1000.0
}

fn mutation_oracle<const D: usize>() {
    let mut seed = 2051;
    for cell_size in [0.125, 8.0, 64.0, 2000.0] {
        let mut index = SpatialIndex::<D>::new(cell_size).unwrap();
        let mut oracle = BTreeMap::new();
        for step in 0..6000 {
            let id = ((random(&mut seed).abs() * 17.0) as u64) % 300;
            if step % 7 == 0 {
                assert_eq!(index.remove(id), oracle.remove(&id));
            } else {
                let point = std::array::from_fn(|_| random(&mut seed));
                assert_eq!(index.upsert(id, point).unwrap(), oracle.insert(id, point));
            }
            if step % 37 == 0 {
                if step % 2 == 0 {
                    index.prepare();
                }
                let centre = std::array::from_fn(|_| random(&mut seed));
                let radius = random(&mut seed).abs();
                let mut expected: Vec<u64> = oracle
                    .iter()
                    .filter(|(_, p)| {
                        p.iter()
                            .zip(centre)
                            .map(|(a, b)| (a - b) * (a - b))
                            .sum::<f64>()
                            <= radius * radius
                    })
                    .map(|(&id, _)| id)
                    .collect();
                let mut actual = index.within(centre, radius).unwrap();
                actual.sort_unstable();
                expected.sort_unstable();
                assert_eq!(actual, expected, "D={D} cell={cell_size} step={step}");
                assert_eq!(index.len(), oracle.len());
                for (&id, p) in &oracle {
                    assert_eq!(index.get(id), Some(p));
                }
            }
            if step % 200 == 0 {
                index.refresh_bounds();
            }
        }
    }
}

#[test]
fn random_mutations_2d() {
    mutation_oracle::<2>();
}
#[test]
fn random_mutations_3d() {
    mutation_oracle::<3>();
}

#[test]
fn closed_boundary_duplicates_and_clear() {
    let mut index = Index3::new(1.0).unwrap();
    for (id, p) in [
        (1, [0.0, 0.0, 0.0]),
        (2, [3.0, 4.0, 0.0]),
        (3, [0.0, 0.0, 0.0]),
        (4, [-3.0, -4.0, 0.0]),
    ] {
        index.upsert(id, p).unwrap();
    }
    let mut all = index.within([0.0; 3], 5.0).unwrap();
    all.sort_unstable();
    assert_eq!(all, vec![1, 2, 3, 4]);
    let mut zero = index.within([-0.0; 3], 0.0).unwrap();
    zero.sort_unstable();
    assert_eq!(zero, vec![1, 3]);
    index.remove(1);
    index.remove(2);
    index.upsert(99, [100.0; 3]).unwrap();
    let cloned = index.clone();
    index.clear();
    assert!(index.is_empty());
    assert_eq!(cloned.len(), 3);
}

#[test]
fn invalid_inputs_are_atomic() {
    assert!(matches!(Index2::new(0.0), Err(Error::CellSize)));
    assert!(matches!(SpatialIndex::<4>::new(1.0), Err(Error::Dimension)));
    let mut index = Index3::new(1.0).unwrap();
    index.upsert(1, [0.0; 3]).unwrap();
    assert_eq!(
        index.upsert(1, [f64::NAN, 0.0, 0.0]),
        Err(Error::Coordinate)
    );
    assert_eq!(index.get(1), Some(&[0.0; 3]));
    let mut out = vec![99];
    assert_eq!(
        index.within_into([0.0; 3], -1.0, &mut out),
        Err(Error::Radius)
    );
    assert_eq!(out, vec![99]);
    assert_eq!(
        index.within_into([f64::INFINITY; 3], 1.0, &mut out),
        Err(Error::Coordinate)
    );
    assert_eq!(out, vec![99]);
}

#[test]
fn exceptional_scales_do_not_overflow_or_lose_points() {
    for cell in [f64::MIN_POSITIVE, 1e-200, 1.0, 1e200] {
        let mut index = Index3::new(cell).unwrap();
        index.upsert(1, [1e200, 0.0, 0.0]).unwrap();
        index.upsert(2, [-1e200, 0.0, 0.0]).unwrap();
        assert_eq!(index.within([1e200, 0.0, 0.0], 0.0).unwrap(), vec![1]);
        assert!(index.within([0.0; 3], 1e199).unwrap().is_empty());
        let mut all = index.within([0.0; 3], 1e201).unwrap();
        all.sort_unstable();
        assert_eq!(all, vec![1, 2]);
    }
    let mut index = Index2::new(1.0).unwrap();
    index.upsert(1, [1e-200, 0.0]).unwrap();
    assert!(index.within([0.0; 2], 0.0).unwrap().is_empty());
    assert_eq!(index.within([0.0; 2], 1e-200).unwrap(), vec![1]);
}

#[test]
fn independent_parallel_reads() {
    let mut index = Index3::new(4.0).unwrap();
    for id in 0..500 {
        index.upsert(id, [id as f64, 0.0, 0.0]).unwrap();
    }
    std::thread::scope(|s| {
        for _ in 0..4 {
            let reference = &index;
            s.spawn(move || {
                for i in 0..500 {
                    assert!(
                        reference
                            .within([i as f64, 0.0, 0.0], 0.1)
                            .unwrap()
                            .contains(&(i as u64))
                    );
                }
            });
        }
    });
}

#[test]
fn prepared_motion_matches_unprepared_with_stable_and_sparse_ids() {
    // Exercise refits across cell boundaries, teleport-driven rebuilds, and the
    // contiguous-ID fast path becoming a sparse-ID map after swap removal.
    let mut index = Index3::new(16.0).unwrap();
    for id in 0..1024 {
        index
            .upsert(id, [(id % 32) as f64, (id / 32) as f64, (id % 7) as f64])
            .unwrap();
    }
    for frame in 0..20 {
        index.prepare();
        for id in (0..1024).step_by(3) {
            let p = if frame % 4 == 0 {
                [id as f64 * 64.0, -(id as f64), frame as f64]
            } else {
                [
                    (id % 32) as f64 + frame as f64 * 0.1,
                    (id / 32) as f64,
                    (id % 7) as f64,
                ]
            };
            index.upsert(id, p).unwrap();
        }
        for prepared in [false, true] {
            if prepared {
                index.prepare();
            }
            for centre in [[0.0; 3], [16.0, 16.0, 3.0], [32768.0, -512.0, 4.0]] {
                let mut expected: Vec<_> = index
                    .iter()
                    .filter(|(_, p)| {
                        p.iter()
                            .zip(centre)
                            .map(|(x, y)| (x - y) * (x - y))
                            .sum::<f64>()
                            <= 256.0
                    })
                    .map(|(id, _)| id)
                    .collect();
                let mut actual = index.within(centre, 16.0).unwrap();
                expected.sort_unstable();
                actual.sort_unstable();
                assert_eq!(actual, expected);
            }
        }
    }
    index.remove(17);
    index.upsert(u64::MAX, [0.0; 3]).unwrap();
    index.prepare();
    assert_eq!(
        index.get(1023),
        Some(&[
            ((1023 % 32) as f64) + 1.9,
            (1023 / 32) as f64,
            (1023 % 7) as f64
        ])
    );
    assert!(index.within([0.0; 3], 0.0).unwrap().contains(&u64::MAX));
    index.clear();
    for id in 0..140 {
        index.upsert(id, [id as f64; 3]).unwrap();
    }
    index.prepare();
    assert_eq!(index.within([139.0; 3], 0.0).unwrap(), vec![139]);
}
