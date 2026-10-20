use rill_spatial::{Error, SpatialIndex, Strategy};
use std::collections::BTreeMap;

fn check<const D: usize>(
    s: &SpatialIndex<D>,
    points: &BTreeMap<u64, [f64; D]>,
    c: [f64; D],
    r: f64,
) {
    let mut expected: Vec<_> = points
        .iter()
        .filter(|(_, p)| p.iter().zip(c).map(|(a, b)| (a - b) * (a - b)).sum::<f64>() <= r * r)
        .map(|(&id, _)| id)
        .collect();
    let mut actual = vec![u64::MAX];
    s.within_into(c, r, &mut actual).unwrap();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected, "centre={c:?}, radius={r:?}");
}
fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}
fn point<const D: usize>(seed: &mut u64) -> [f64; D] {
    std::array::from_fn(|_| ((next(seed) % 20001) as f64 - 10000.0) / 31.0)
}
fn mutations<const D: usize>() {
    for strategy in [
        Strategy::Hybrid,
        Strategy::HashGrid,
        Strategy::SortedSlab,
        Strategy::OccupiedScan,
    ] {
        for containment in [false, true] {
            let mut seed = 97123;
            let mut s = SpatialIndex::<D>::new(13.0).unwrap();
            s.set_strategy(strategy);
            s.set_containment(containment);
            let mut points = BTreeMap::new();
            for step in 0..2400 {
                let id = (next(&mut seed) % 311).wrapping_mul(0x9e3779b97f4a7c15);
                if step % 5 == 0 {
                    assert_eq!(s.remove(id), points.remove(&id));
                } else {
                    let p = point(&mut seed);
                    assert_eq!(s.upsert(id, p).unwrap(), points.insert(id, p));
                }
                assert_eq!(s.len(), points.len());
                let c = point(&mut seed);
                let r = (next(&mut seed) % 800) as f64;
                check(&s, &points, c, r); // immediately after mutation
                if step % 7 == 0 {
                    s.prepare();
                    check(&s, &points, c, r);
                    check(&s, &points, c, 0.0);
                    check(&s, &points, c, 3000.0);
                }
            }
        }
    }
}
#[test]
fn oracle_mutations_2d() {
    mutations::<2>();
}
#[test]
fn oracle_mutations_3d() {
    mutations::<3>();
}

fn extremes<const D: usize>() {
    let values = [
        -f64::MAX,
        -1e200,
        -1e150,
        -1.0,
        -f64::MIN_POSITIVE,
        -1e-162,
        -0.0,
        0.0,
        f64::from_bits(1),
        1e-162,
        f64::MIN_POSITIVE,
        1.0,
        1e150,
        1e200,
        f64::MAX,
    ];
    for size in [f64::from_bits(1), 1e-200, 0.5, 1e150, f64::MAX] {
        let points: BTreeMap<_, _> = values
            .iter()
            .enumerate()
            .map(|(i, &x)| {
                (
                    i as u64,
                    std::array::from_fn(|j| if j == 0 { x } else { 0.0 }),
                )
            })
            .collect();
        let mut s =
            SpatialIndex::<D>::from_points(size, points.iter().map(|(&id, &p)| (id, p))).unwrap();
        s.prepare();
        for strategy in [
            Strategy::Hybrid,
            Strategy::HashGrid,
            Strategy::SortedSlab,
            Strategy::OccupiedScan,
        ] {
            s.set_strategy(strategy);
            for &x in &values {
                let c = std::array::from_fn(|j| if j == 0 { x } else { 0.0 });
                for radius in [
                    0.0,
                    f64::from_bits(1),
                    1e-162,
                    1e-150,
                    1.0,
                    1e150,
                    1e200,
                    f64::MAX,
                ] {
                    check(&s, &points, c, radius);
                }
            }
        }
    }
}
#[test]
fn extreme_float_2d() {
    extremes::<2>();
}
#[test]
fn extreme_float_3d() {
    extremes::<3>();
}

#[test]
fn near_boundaries_and_arbitrary_centres() {
    let mut seed = 237891;
    for exponent in [-150, -100, -10, 0, 10, 100, 150] {
        let scale = 10.0f64.powi(exponent);
        let mut s = SpatialIndex::<3>::new(scale).unwrap();
        let mut points = BTreeMap::new();
        for i in 0..600u64 {
            let x = (next(&mut seed) % 2000) as f64 * scale / 10.0;
            let p = [x, x.next_down(), x.next_up()];
            points.insert(i, p);
            s.upsert(i, p).unwrap();
        }
        s.prepare();
        for strategy in [
            Strategy::Hybrid,
            Strategy::HashGrid,
            Strategy::SortedSlab,
            Strategy::OccupiedScan,
        ] {
            s.set_strategy(strategy);
            for _ in 0..400 {
                let c = [(next(&mut seed) % 2000) as f64 * scale / 10.0; 3];
                let p = points.get(&(next(&mut seed) % 600)).unwrap();
                let radius = p
                    .iter()
                    .zip(c)
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum::<f64>()
                    .sqrt();
                for r in [radius.next_down().max(0.0), radius, radius.next_up()] {
                    check(&s, &points, c, r);
                }
            }
        }
    }
}

#[test]
fn invalid_operations_are_atomic_and_ids_unique() {
    assert!(matches!(SpatialIndex::<1>::new(1.0), Err(Error::Dimension)));
    assert!(matches!(SpatialIndex::<3>::new(0.0), Err(Error::CellSize)));
    let mut s = SpatialIndex::<2>::from_points(
        1.0,
        [(99, [1.0, 2.0]), (99, [2.0, 3.0]), (u64::MAX, [0.0, 0.0])],
    )
    .unwrap();
    assert_eq!(s.len(), 2);
    s.prepare();
    assert_eq!(s.upsert(99, [f64::NAN, 0.0]), Err(Error::Coordinate));
    let points = BTreeMap::from([(99, [2.0, 3.0]), (u64::MAX, [0.0, 0.0])]);
    check(&s, &points, [0.0, 0.0], 10.0);
    let mut out = vec![123];
    assert_eq!(
        s.within_into([0.0, 0.0], -1.0, &mut out),
        Err(Error::Radius)
    );
    assert_eq!(out, [123]);
    assert_eq!(
        s.within_into([f64::INFINITY, 0.0], 1.0, &mut out),
        Err(Error::Coordinate)
    );
    assert_eq!(out, [123]);
}
