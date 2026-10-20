use rill_spatial::{BatchStats, Error, Index2, Index3, SpatialIndex};
use std::collections::HashMap;

fn expected<const D: usize>(p: &[f64; D], q: &[f64; D], radius: f64) -> bool {
    p.iter().zip(q).map(|(a, b)| (a - b) * (a - b)).sum::<f64>() <= radius * radius
}

// Every membership is checked, including absent memberships; duplicate results fail.
// Only one bit per possible membership is retained, never Vec<Vec<u64>>.
fn verify<const D: usize>(
    index: &SpatialIndex<D>,
    points: &[(u64, [f64; D])],
    queries: &[([f64; D], f64)],
) -> BatchStats {
    let n = points.len();
    let mut bits = vec![0u64; (n * queries.len() + 63) / 64];
    let ids: HashMap<_, _> = points
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    let stats = index
        .batch_each(queries, |q, id| {
            let i = q * n + ids[&id];
            assert_eq!(
                bits[i / 64] & (1 << (i % 64)),
                0,
                "duplicate query={q} id={id}"
            );
            bits[i / 64] |= 1 << (i % 64);
        })
        .unwrap();
    let mut count = 0;
    for (q, (centre, radius)) in queries.iter().enumerate() {
        for (i, (id, p)) in points.iter().enumerate() {
            let at = q * n + i;
            let wanted = expected(p, centre, *radius);
            assert_eq!(
                (bits[at / 64] >> (at % 64)) & 1,
                wanted as u64,
                "query={q} id={id}"
            );
            count += wanted as u64;
        }
    }
    assert_eq!(stats.reported_pairs, count);
    stats
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn scalar(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn point(&mut self) -> [f64; 3] {
        [
            self.scalar() * 200. - 100.,
            self.scalar() * 200. - 100.,
            self.scalar() * 200. - 100.,
        ]
    }
}

#[test]
fn arbitrary_ids_centres_variable_radii_and_mutations() {
    let mut rng = Rng(0xabcdef123);
    let mut points: Vec<_> = (0..2000)
        .map(|i| (u64::MAX - i * 7919, rng.point()))
        .collect();
    let queries: Vec<_> = (0..573)
        .map(|_| (rng.point(), rng.scalar() * 160.))
        .collect();
    let mut index = Index3::from_points(7.5, points.iter().copied()).unwrap();
    assert_eq!(verify(&index, &points, &queries).node_pairs, 0);
    index.prepare();
    assert!(verify(&index, &points, &queries).node_pairs > 0);
    for &(centre, radius) in queries.iter().take(30) {
        let mut got = vec![123];
        index.within_into(centre, radius, &mut got).unwrap();
        got.sort_unstable();
        let mut want: Vec<_> = points
            .iter()
            .filter(|(_, p)| expected(p, &centre, radius))
            .map(|(id, _)| *id)
            .collect();
        want.sort_unstable();
        assert_eq!(got, want);
    }
    for i in 0..73 {
        points[i].1 = rng.point();
        index.upsert(points[i].0, points[i].1).unwrap();
    }
    assert_eq!(verify(&index, &points, &queries).node_pairs, 0);
    let removed = points.swap_remove(351);
    assert_eq!(index.remove(removed.0), Some(removed.1));
    let added = (5, rng.point());
    points.push(added);
    index.upsert(added.0, added.1).unwrap();
    verify(&index, &points, &queries);
    index.prepare();
    verify(&index, &points, &queries);
}

#[test]
fn numeric_closed_boundaries_overflow_underflow_and_2d() {
    let scales = [0., f64::from_bits(1), 1e-200, 1., 1e150, 1e200, f64::MAX];
    let mut points = Vec::new();
    for scale in scales {
        for sign in [-1., 1.] {
            points.push((points.len() as u64 * 99, [sign * scale, 0.]));
        }
    }
    points.extend((0..100).map(|i| (10000 + i, [i as f64, -0.])));
    let mut queries = Vec::new();
    for centre in [0., 1., 1e150, -1e200, f64::MAX, -f64::MAX] {
        for r in scales {
            queries.push(([centre, 0.], r));
        }
    }
    let mut index = Index2::from_points(f64::from_bits(1), points.iter().copied()).unwrap();
    verify(&index, &points, &queries);
    index.prepare();
    verify(&index, &points, &queries);
    let mut bits_rng = Rng(23719237);
    let points: Vec<_> = (0..600)
        .map(|i| {
            let mut coordinate = || {
                let b = bits_rng.next();
                f64::from_bits(b & !(0x7ffu64 << 52) | (((b >> 52) % 2047) << 52))
            };
            (i, [coordinate(), coordinate(), coordinate()])
        })
        .collect();
    let queries: Vec<_> = points
        .iter()
        .step_by(3)
        .enumerate()
        .map(|(i, (_, p))| (*p, [0., 1e-200, 1., 1e150, 1e200, f64::MAX][i % 6]))
        .collect();
    let mut index = Index3::from_points(1., points.iter().copied()).unwrap();
    index.prepare();
    verify(&index, &points, &queries);
}

#[test]
fn validation_is_atomic_empty_and_duplicate_upsert() {
    assert!(matches!(SpatialIndex::<1>::new(1.), Err(Error::Dimension)));
    assert!(matches!(Index3::new(0.), Err(Error::CellSize)));
    let mut index = Index3::from_points(1., [(7, [0.; 3]), (7, [1.; 3])]).unwrap();
    assert_eq!(index.len(), 1);
    assert_eq!(index.get(7), Some([1.; 3]));
    index.prepare();
    assert_eq!(index.upsert(7, [f64::NAN, 0., 0.]), Err(Error::Coordinate));
    assert_eq!(index.get(7), Some([1.; 3]));
    let mut out = vec![99];
    assert_eq!(
        index.within_into([0.; 3], -1., &mut out),
        Err(Error::Radius)
    );
    assert_eq!(out, [99]);
    for queries in [
        vec![([0.; 3], 1.), ([f64::NAN, 0., 0.], 1.)],
        vec![([0.; 3], 1.), ([0.; 3], f64::INFINITY)],
    ] {
        let mut calls = 0;
        assert!(index.batch_blocks(&queries, |_, _| calls += 1).is_err());
        assert_eq!(calls, 0);
    }
    assert_eq!(
        index
            .batch_each(&[], |_, _| panic!())
            .unwrap()
            .reported_pairs,
        0
    );
    index.remove(7);
    index.prepare();
    assert_eq!(
        index
            .batch_each(&[([0.; 3], 1.)], |_, _| panic!())
            .unwrap()
            .reported_pairs,
        0
    );
}

#[test]
fn spatial_partition_visitor_full_membership_and_limits() {
    let mut rng = Rng(9123791);
    let points: Vec<_> = (0..1200)
        .map(|i| (u64::MAX - i * 17, rng.point()))
        .collect();
    let queries: Vec<_> = (0..203)
        .map(|_| (rng.point(), rng.scalar() * 150.))
        .collect();
    let mut index = Index3::from_points(1., points.iter().copied()).unwrap();
    for prepare in [false, true] {
        if prepare {
            index.prepare();
        }
        for limit in [1, 7, 16, 63, 64, 1000] {
            let mut visited = vec![false; queries.len()];
            let mut total = 0;
            let stats = index
                .batch_visit(&queries, limit, |q, ids| {
                    assert!(!visited[q]);
                    visited[q] = true;
                    let (centre, r) = queries[q];
                    let mut want: Vec<_> = points
                        .iter()
                        .filter(|(_, p)| expected(p, &centre, r))
                        .map(|(id, _)| *id)
                        .collect();
                    want.sort_unstable();
                    let mut got = ids.to_vec();
                    got.sort_unstable();
                    assert_eq!(want, got);
                    total += ids.len() as u64;
                })
                .unwrap();
            assert!(visited.iter().all(|v| *v));
            assert_eq!(stats.reported_pairs, total);
        }
    }
    assert!(matches!(
        index.batch_visit(&queries, 0, |_, _| panic!()),
        Err(Error::BatchSize)
    ));
    let mut groups = 0;
    index
        .batch_visit_groups(&queries, 7, |indices, outputs| {
            assert!(indices.len() <= 7);
            assert_eq!(indices.len(), outputs.len());
            groups += 1;
        })
        .unwrap();
    assert!(groups > 1);
    let empty = Index3::new(1.).unwrap();
    let mut visited = 0;
    empty
        .batch_visit(&queries, 64, |_, ids| {
            assert!(ids.is_empty());
            visited += 1;
        })
        .unwrap();
    assert_eq!(visited, queries.len());
}

#[test]
fn twenty_thousand_dense_full_membership_bounded_memory() {
    let n = 20_000;
    let points: Vec<_> = (0..n)
        .map(|i| {
            (
                u64::MAX - i as u64,
                [i as f64 % 100., (i / 100) as f64 % 100., (i / 10000) as f64],
            )
        })
        .collect();
    let queries: Vec<_> = (0..n)
        .map(|i| {
            (
                [i as f64 % 100. + 0.125, (i / 100) as f64 % 100. - 0.25, 0.5],
                500.,
            )
        })
        .collect();
    let mut index = Index3::from_points(10., points.iter().copied()).unwrap();
    index.prepare();
    let stats = verify(&index, &points, &queries);
    assert_eq!(stats.reported_pairs, 400_000_000);
    assert_eq!(stats.accepted_pairs, 1);
    assert_eq!(stats.point_tests, 0);
}

#[test]
fn twenty_thousand_sparse_variable_radius_full_membership() {
    let n = 20_000;
    let points: Vec<_> = (0..n)
        .map(|i| {
            (
                i as u64 * 97,
                [
                    i as f64 % 100. * 4.,
                    (i / 100) as f64 % 100. * 4.,
                    (i / 10000) as f64 * 4.,
                ],
            )
        })
        .collect();
    let queries: Vec<_> = (0..n)
        .map(|i| {
            (
                [
                    i as f64 % 100. * 4. + 0.125,
                    (i / 100) as f64 % 100. * 4. - 0.25,
                    0.5,
                ],
                (i % 13) as f64,
            )
        })
        .collect();
    let mut index = Index3::from_points(10., points.iter().copied()).unwrap();
    index.prepare();
    let stats = verify(&index, &points, &queries);
    assert!(stats.rejected_pairs > 0);
    assert!(stats.point_tests < 400_000_000);
}

#[test]
fn shared_fixtures_full_membership() {
    fn u32_at(b: &[u8], at: &mut usize) -> usize {
        let n = u32::from_le_bytes(b[*at..*at + 4].try_into().unwrap());
        *at += 4;
        n as usize
    }
    fn f64_at(b: &[u8], at: &mut usize) -> f64 {
        let n = f64::from_le_bytes(b[*at..*at + 8].try_into().unwrap());
        *at += 8;
        n
    }
    fn point_at(b: &[u8], at: &mut usize) -> [f64; 3] {
        [f64_at(b, at), f64_at(b, at), f64_at(b, at)]
    }
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for dir in ["benchmarks/fixtures", "benchmarks/holdout-97123"] {
        for name in [
            "small",
            "uniform",
            "clusters",
            "dense",
            "contained",
            "floors",
            "boundaries",
            "teleports",
            "large",
        ] {
            let b = std::fs::read(base.join(dir).join(format!("{name}.bin"))).unwrap();
            assert_eq!(&b[..8], b"RILLv001");
            let mut at = 8;
            let len = u32_at(&b, &mut at);
            at += len;
            let cell = f64_at(&b, &mut at);
            let n = u32_at(&b, &mut at);
            let frames = u32_at(&b, &mut at);
            let mut points: Vec<_> = (0..n).map(|i| (i as u64, point_at(&b, &mut at))).collect();
            let mut index = Index3::from_points(cell, points.iter().copied()).unwrap();
            for _ in 0..frames {
                let updates = u32_at(&b, &mut at);
                for _ in 0..updates {
                    let i = u32_at(&b, &mut at);
                    let p = point_at(&b, &mut at);
                    points[i].1 = p;
                    index.upsert(i as u64, p).unwrap();
                }
                let count = u32_at(&b, &mut at);
                let queries: Vec<_> = (0..count)
                    .map(|_| (point_at(&b, &mut at), f64_at(&b, &mut at)))
                    .collect();
                index.prepare();
                verify(&index, &points, &queries);
            }
            eprintln!("full-membership parity: {dir}/{name}");
        }
    }
}
