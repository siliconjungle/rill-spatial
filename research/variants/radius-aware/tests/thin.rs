//! Candidate-specific independent memberships on fixtures that force the global
//! active-axis branch. The oracle reads only its own stable-ID point snapshot.
use rill_spatial::{BatchOptions, SpatialIndex};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

type Points<const D: usize> = BTreeMap<u64, [f64; D]>;
type Query<const D: usize> = ([f64; D], f64);

fn id(i: u64) -> u64 {
    i.wrapping_add(1)
        .wrapping_mul(0x9e3779b97f4a7c15)
        .rotate_left(23)
        ^ 0xd1b54a32d192ed03
}

fn fresh_id<const D: usize>(points: &Points<D>, seed: u64) -> u64 {
    let mut next = seed;
    loop {
        let candidate = id(next);
        if !points.contains_key(&candidate) {
            return candidate;
        }
        next = next.wrapping_add(1);
    }
}

// MSRV 1.85: f64::next_up/next_down were stabilized later. Only finite test
// coordinates/radii enter these helpers; stepping zero is defined explicitly.
fn next_up(value: f64) -> f64 {
    assert!(value.is_finite());
    if value == 0.0 {
        f64::from_bits(1)
    } else if value > 0.0 {
        f64::from_bits(value.to_bits() + 1)
    } else {
        f64::from_bits(value.to_bits() - 1)
    }
}

fn next_down(value: f64) -> f64 {
    -next_up(-value)
}

fn expected<const D: usize>(points: &Points<D>, centre: [f64; D], radius: f64) -> Vec<u64> {
    points
        .iter()
        .filter(|(_, point)| {
            if radius == 0.0 {
                point.iter().zip(centre).all(|(a, b)| *a == b)
            } else {
                // All nonzero radii in this targeted suite are normal, within
                // 1e-150..=1e150. This is the independent closed-f64 contract.
                point
                    .iter()
                    .zip(centre)
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum::<f64>()
                    <= radius * radius
            }
        })
        .map(|(&stable_id, _)| stable_id)
        .collect()
}

fn check<const D: usize>(
    index: &SpatialIndex<D>,
    points: &Points<D>,
    queries: &[Query<D>],
    stage: &str,
) {
    assert_eq!(index.len(), points.len(), "{stage}");
    let mut actual = vec![u64::MAX];
    for &(centre, radius) in queries {
        index.within_into(centre, radius, &mut actual).unwrap();
        actual.sort_unstable();
        assert_eq!(
            actual,
            expected(points, centre, radius),
            "stage={stage}, centre={centre:?}, radius={radius:?}"
        );
    }
}

fn check_parallel_four<const D: usize>(
    index: &SpatialIndex<D>,
    points: &Points<D>,
    queries: &[Query<D>],
) {
    assert!(queries.len() >= 64);
    let seen: Vec<_> = (0..queries.len()).map(|_| AtomicBool::new(false)).collect();
    let matches = AtomicUsize::new(0);
    let stats = index
        .within_batch_parallel(
            queries,
            BatchOptions {
                workers: 4,
                queries_per_group: 11,
            },
            |indices, outputs| {
                assert_eq!(indices.len(), outputs.len());
                assert!(outputs.len() <= 11);
                for (&q, row) in indices.iter().zip(outputs) {
                    assert!(!seen[q].swap(true, Ordering::SeqCst));
                    let (centre, radius) = queries[q];
                    let mut actual = row.clone();
                    actual.sort_unstable();
                    assert_eq!(actual, expected(points, centre, radius), "parallel q={q}");
                    matches.fetch_add(actual.len(), Ordering::SeqCst);
                }
            },
        )
        .unwrap();
    assert!(seen.iter().all(|visited| visited.load(Ordering::SeqCst)));
    assert_eq!(stats.workers, 4);
    assert_eq!(stats.queries, queries.len());
    assert_eq!(stats.matches, matches.load(Ordering::SeqCst) as u128);
}

fn spans<const D: usize>(points: &Points<D>) -> [f64; D] {
    std::array::from_fn(|axis| {
        let lo = points
            .values()
            .map(|p| p[axis])
            .fold(f64::INFINITY, f64::min);
        let hi = points
            .values()
            .map(|p| p[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        hi - lo
    })
}

fn assert_thin_fixture<const D: usize>(points: &Points<D>, hint: f64, thin_axis: usize) {
    assert!(
        points.len() >= 1000,
        "force tree construction, not the <=128 scan"
    );
    let widths = spans(points);
    for (axis, width) in widths.into_iter().enumerate() {
        if axis == thin_axis {
            assert!(width < hint, "fixture must disable axis {axis}");
        } else {
            assert!(width >= hint, "fixture must retain axis {axis}");
        }
    }
}

fn add_point_boundaries<const D: usize>(
    queries: &mut Vec<Query<D>>,
    points: &Points<D>,
    hint: f64,
    axis: usize,
) {
    for point in points.values().step_by(points.len() / 9).take(9) {
        for radius in [0.001, 0.25, hint, hint * 4.0] {
            let mut centre = *point;
            centre[axis] += radius;
            for r in [next_down(radius), radius, next_up(radius)] {
                queries.push((centre, r));
            }
        }
    }
}

fn exercise_thin_lifecycle<const D: usize>(
    mut points: Points<D>,
    hint: f64,
    thin_axis: usize,
    mut queries: Vec<Query<D>>,
) {
    assert_thin_fixture(&points, hint, thin_axis);
    add_point_boundaries(&mut queries, &points, hint, thin_axis);
    let mut index = SpatialIndex::<D>::from_points(
        hint,
        points.iter().map(|(&stable_id, &point)| (stable_id, point)),
    )
    .unwrap();
    index.prepare();
    check(&index, &points, &queries, "prepared: thin axis disabled");
    check_parallel_four(&index, &points, &queries);

    // Replace existing points only: no structural mutation can clear the view.
    // This exercises the dirty prepared view before prepare/refit is called.
    let moving: Vec<_> = points.keys().copied().take(96).collect();
    let count = points.len();
    for stable_id in moving {
        let mut point = points[&stable_id];
        for (axis, coordinate) in point.iter_mut().enumerate() {
            if axis == thin_axis {
                *coordinate *= 0.75;
            } else {
                *coordinate += 0.015625 * (axis + 1) as f64;
            }
        }
        assert_eq!(
            index.upsert(stable_id, point).unwrap(),
            points.insert(stable_id, point)
        );
    }
    assert_eq!(points.len(), count);
    assert_thin_fixture(&points, hint, thin_axis);
    check(&index, &points, &queries, "movement only: dirty view");
    index.prepare();
    check(&index, &points, &queries, "movement only: reprepare/refit");
    check_parallel_four(&index, &points, &queries);

    // Remove and insert arbitrary IDs while preserving the thin geometry.
    let removed: Vec<_> = points.keys().copied().take(24).collect();
    for stable_id in removed {
        assert_eq!(index.remove(stable_id), points.remove(&stable_id));
    }
    let seeds: Vec<_> = points.values().copied().take(24).collect();
    for (i, mut point) in seeds.into_iter().enumerate() {
        let axis = (thin_axis + 1) % D;
        point[axis] += 0.0625;
        let stable_id = fresh_id(&points, 10000 + i as u64);
        assert_eq!(index.upsert(stable_id, point).unwrap(), None);
        assert_eq!(points.insert(stable_id, point), None);
    }
    assert_thin_fixture(&points, hint, thin_axis);
    check(
        &index,
        &points,
        &queries,
        "structural churn: before prepare",
    );
    index.prepare();
    check(
        &index,
        &points,
        &queries,
        "structural churn: rebuilt thin view",
    );

    // A teleport makes the previously disabled axis globally active. Force a
    // rebuild with one new ID, so this does not depend on the refit heuristic.
    let teleport_id = *points.keys().next().unwrap();
    let original = points[&teleport_id];
    let mut teleported = original;
    teleported[thin_axis] = hint * 128.0;
    assert_eq!(
        index.upsert(teleport_id, teleported).unwrap(),
        Some(original)
    );
    points.insert(teleport_id, teleported);
    let mut teleport_queries = queries.clone();
    for r in [0.0, 0.001, hint / 64.0, hint, hint * 256.0] {
        teleport_queries.push((teleported, r));
    }
    check(
        &index,
        &points,
        &teleport_queries,
        "teleport: existing topology dirty",
    );
    let forcing_id = fresh_id(&points, 50000);
    assert_eq!(index.upsert(forcing_id, original).unwrap(), None);
    points.insert(forcing_id, original);
    assert!(spans(&points).into_iter().all(|width| width >= hint));
    check(
        &index,
        &points,
        &teleport_queries,
        "teleport: forced rebuild pending",
    );
    index.prepare();
    check(
        &index,
        &points,
        &teleport_queries,
        "teleport: all axes active on rebuild",
    );
    check_parallel_four(&index, &points, &teleport_queries);

    // Restore thin geometry and force the opposite active-mask transition.
    assert_eq!(
        index.upsert(teleport_id, original).unwrap(),
        Some(teleported)
    );
    points.insert(teleport_id, original);
    assert_eq!(index.remove(forcing_id), points.remove(&forcing_id));
    assert_thin_fixture(&points, hint, thin_axis);
    check(
        &index,
        &points,
        &teleport_queries,
        "restored slab: rebuild pending",
    );
    index.prepare();
    check(
        &index,
        &points,
        &teleport_queries,
        "restored slab: thin axis disabled again",
    );
}

#[test]
fn thin_3d_slab_disabled_z_faces_boundaries_and_axis_transition() {
    let hint = 16.0;
    let mut points = BTreeMap::new();
    for y in 0..40u64 {
        for x in 0..32u64 {
            let i = y * 32 + x;
            let z = [-0.5, 0.5, -0.25, 0.25, 0.0][i as usize % 5];
            assert!(
                points
                    .insert(id(i), [(x as f64 - 16.0) * 5.0, (y as f64 - 20.0) * 3.0, z])
                    .is_none()
            );
        }
    }
    assert_eq!(points.len(), 1280);
    let mut queries = Vec::new();
    for z in [
        next_down(-0.5),
        -0.5,
        next_up(-0.5),
        next_down(0.5),
        0.5,
        next_up(0.5),
    ] {
        for (x, y) in [
            (0.12345, -0.4567),
            (13.37, -22.45),
            (-80.25, 20.125),
            (1000.125, -1000.25),
        ] {
            let centre = [x, y, z];
            assert!(!points.values().any(|point| *point == centre));
            for radius in [0.0, 0.001, 0.1, 0.5, 1.0, 8.0, 16.0, 32.0, 512.0, 4096.0] {
                queries.push((centre, radius));
            }
        }
    }
    exercise_thin_lifecycle(points, hint, 2, queries);
}

#[test]
fn thin_2d_corridor_disabled_y_and_arbitrary_radii() {
    let hint = 8.0;
    let mut points = BTreeMap::new();
    for i in 0..1200u64 {
        let y = [-0.25, 0.25, -0.125, 0.125, 0.0][i as usize % 5];
        assert!(points.insert(id(i), [i as f64 * 0.25 - 150.0, y]).is_none());
    }
    let mut queries = Vec::new();
    for y in [
        next_down(-0.25),
        -0.25,
        next_up(-0.25),
        next_down(0.25),
        0.25,
        next_up(0.25),
    ] {
        for x in [-150.03125, 0.12345, 149.78125, 1000.125] {
            let centre = [x, y];
            assert!(!points.values().any(|point| *point == centre));
            for radius in [0.0, 0.001, 0.1, 0.25, 1.0, hint, 64.0, 2048.0] {
                queries.push((centre, radius));
            }
        }
    }
    exercise_thin_lifecycle(points, hint, 1, queries);
}

#[test]
fn all_axes_below_hint_falls_back_to_full_axis_tree() {
    let hint = 4.0;
    let mut points = BTreeMap::new();
    for z in 0..11u64 {
        for y in 0..11u64 {
            for x in 0..11u64 {
                let i = (z * 11 + y) * 11 + x;
                let point = [
                    (x as f64 - 5.0) / 16.0,
                    (y as f64 - 5.0) / 16.0,
                    (z as f64 - 5.0) / 16.0,
                ];
                assert!(points.insert(id(i), point).is_none());
            }
        }
    }
    assert_eq!(points.len(), 1331);
    assert!(spans(&points).into_iter().all(|width| width < hint));
    let mut queries = Vec::new();
    for axis in 0..3 {
        for face in [-0.3125, 0.3125] {
            for coordinate in [next_down(face), face, next_up(face)] {
                let mut centre = [0.03125, -0.09375, 0.15625];
                centre[axis] = coordinate;
                for radius in [
                    0.0,
                    0.001,
                    0.0625,
                    0.125,
                    0.5,
                    next_down(hint),
                    hint,
                    hint * 16.0,
                ] {
                    queries.push((centre, radius));
                }
            }
        }
    }
    add_point_boundaries(&mut queries, &points, hint, 2);
    let mut index = SpatialIndex::<3>::from_points(
        hint,
        points.iter().map(|(&stable_id, &point)| (stable_id, point)),
    )
    .unwrap();
    index.prepare();
    check(
        &index,
        &points,
        &queries,
        "all axes inactive: all-axis fallback",
    );
    check_parallel_four(&index, &points, &queries);
    for stable_id in points.keys().copied().take(80).collect::<Vec<_>>() {
        let mut point = points[&stable_id];
        point[0] += 0.015625;
        assert_eq!(
            index.upsert(stable_id, point).unwrap(),
            points.insert(stable_id, point)
        );
    }
    assert!(spans(&points).into_iter().all(|width| width < hint));
    check(
        &index,
        &points,
        &queries,
        "all-axis fallback: dirty movement",
    );
    index.prepare();
    check(&index, &points, &queries, "all-axis fallback: reprepare");
}
