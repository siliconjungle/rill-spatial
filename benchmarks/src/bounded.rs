//! Exact bounded-memory measurement. Full output IDs are produced in timed
//! windows, then checked for membership, omissions and duplicates outside timing.
use crate::{Adapter, Fixture, Point, Sample, squared};
use std::{hint::black_box, time::Instant};

enum Expected {
    All,
    Sparse(Vec<u32>),
    Dense { bits: Vec<u64>, count: usize },
}
impl Expected {
    fn new(points: &[Point], p: &Point, r: f64, lo: &Point, hi: &Point) -> Self {
        let far = std::array::from_fn(|d| (p[d] - lo[d]).abs().max((p[d] - hi[d]).abs()));
        if squared(&far, &[0.0; 3]) <= r * r {
            return Self::All;
        }
        let words = points.len().div_ceil(64);
        let mut ids = Vec::new();
        let mut bits = Vec::new();
        let mut count = 0;
        for (i, q) in points.iter().enumerate() {
            if squared(q, p) <= r * r {
                count += 1;
                if bits.is_empty() {
                    ids.push(u32::try_from(i).expect("benchmark population must fit u32"));
                    if ids.len() > words * 2 {
                        bits.resize(words, 0);
                        for &id in &ids {
                            bits[id as usize / 64] |= 1 << (id % 64);
                        }
                        ids.clear();
                    }
                } else {
                    bits[i / 64] |= 1 << (i % 64);
                }
            }
        }
        if bits.is_empty() {
            Self::Sparse(ids)
        } else {
            Self::Dense { bits, count }
        }
    }
    fn validate(&self, actual: &[u64], seen: &mut [u64], n: usize) {
        seen.fill(0);
        let expected_count = match self {
            Self::All => n,
            Self::Sparse(ids) => ids.len(),
            Self::Dense { count, .. } => *count,
        };
        assert_eq!(actual.len(), expected_count, "result count mismatch");
        for &id in actual {
            let id = usize::try_from(id).expect("ID overflow");
            assert!(id < n, "out-of-range ID");
            let mask = 1u64 << (id % 64);
            assert_eq!(seen[id / 64] & mask, 0, "duplicate ID");
            seen[id / 64] |= mask;
            if let Self::Dense { bits, .. } = self {
                assert_ne!(bits[id / 64] & mask, 0, "unexpected ID");
            }
        }
        if let Self::Sparse(ids) = self {
            for &id in ids {
                assert_ne!(seen[id as usize / 64] & (1 << (id % 64)), 0, "missing ID");
            }
        }
    }
}
pub(crate) fn run<A: Adapter>(name: &str, f: &Fixture, samples: usize) {
    // This fixture protocol is deliberately normal-scale binary64. General public
    // scale/boundary semantics are tested independently in library/prototype suites.
    assert!(
        f.points.len() <= 100_000,
        "bounded runner admission: <=100k targets"
    );
    assert!(
        f.frames.iter().all(|fr| fr.queries.len() <= 20_000),
        "bounded runner admission: <=20k queries/frame"
    );
    assert!(
        f.points
            .iter()
            .flatten()
            .all(|x| x.is_finite() && x.abs() <= 1e6)
    );
    assert!(f.frames.iter().all(|fr| {
        fr.updates.iter().all(|(id, p)| {
            *id < f.points.len() && p.iter().all(|x| x.is_finite() && x.abs() <= 1e6)
        }) && fr.queries.iter().all(|q| {
            q.point.iter().all(|x| x.is_finite() && x.abs() <= 1e6)
                && q.radius >= 1e-100
                && q.radius <= 1e100
        })
    }));
    let window: usize = std::env::var("RILL_OUTPUT_WINDOW")
        .unwrap_or_else(|_| "64".into())
        .parse()
        .expect("output window integer");
    assert!(
        (1..=64).contains(&window),
        "output window admission: 1..=64"
    );
    let continuous = std::env::var("RILL_CONTINUOUS").as_deref() == Ok("1");
    let mut frame_samples = Vec::new();
    let mut rows = Vec::new();
    let mut peak_capacity = 0;
    let mut match_counts = Vec::new();
    let mut validation_ns = 0u128;
    for sample in 0..samples + usize::from(continuous) {
        let verify = !continuous || sample == 0;
        let mut checksum = 0u64;
        let mut reference = f.points.clone();
        let mut seen = vec![0; f.points.len().div_ceil(64)];
        let mut visited_queries = Vec::new();
        let t = Instant::now();
        let mut index = A::build(black_box(&f.points), f.cell_size);
        let build_ns = t.elapsed().as_nanos() as u64;
        let (mut update_ns, mut query_ns, mut results) = (0, 0, 0);
        for (frame_number, frame) in f.frames.iter().enumerate() {
            let frame_start = Instant::now();
            let t = Instant::now();
            index.update(black_box(&frame.updates));
            if !frame.queries.is_empty() {
                index.prepare();
            }
            let frame_update_ns = t.elapsed().as_nanos() as u64;
            update_ns += frame_update_ns;
            if verify {
                for &(id, p) in &frame.updates {
                    reference[id] = p;
                }
            }
            if frame.queries.is_empty() {
                continue;
            }
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            if verify {
                for p in &reference {
                    for d in 0..3 {
                        lo[d] = lo[d].min(p[d]);
                        hi[d] = hi[d].max(p[d]);
                    }
                }
            }
            visited_queries.clear();
            visited_queries.resize(frame.queries.len(), false);
            let mut excluded = 0u128;
            let t = Instant::now();
            index.visit_groups(
                black_box(&frame.queries),
                window,
                &mut |indices, outputs| {
                    let pause = verify.then(Instant::now);
                    black_box(outputs);
                    assert_eq!(indices.len(), outputs.len());
                    assert!(indices.len() <= window);
                    for (&q, out) in indices.iter().zip(outputs) {
                        assert!(
                            q < frame.queries.len() && !visited_queries[q],
                            "query missing/duplicated"
                        );
                        visited_queries[q] = true;
                        let query = &frame.queries[q];
                        if verify {
                            Expected::new(&reference, &query.point, query.radius, &lo, &hi)
                                .validate(out, &mut seen, reference.len());
                        } else {
                            let mut row_hash = q as u64;
                            for &id in out {
                                row_hash =
                                    row_hash.wrapping_add(id.wrapping_mul(0x9e3779b97f4a7c15));
                            }
                            checksum ^= black_box(row_hash);
                        }
                        results += out.len();
                        if sample == 0 {
                            match_counts.push(out.len());
                        }
                    }
                    peak_capacity =
                        peak_capacity.max(outputs.iter().map(|v| v.capacity() * 8).sum::<usize>());
                    if let Some(pause) = pause {
                        excluded += pause.elapsed().as_nanos();
                    }
                },
            );
            let wall = t.elapsed().as_nanos();
            query_ns += wall
                .checked_sub(excluded)
                .expect("invalid timing exclusion") as u64;
            validation_ns += excluded;
            let frame_service_ns = frame_start.elapsed().as_nanos() as u64;
            if continuous && !verify {
                frame_samples.push(serde_json::json!({"sample":sample-1,"frame":frame_number,"update_prepare_ns":frame_update_ns,"query_wall_ns":wall as u64,"frame_service_ns":frame_service_ns,"queries":frame.queries.len()}));
            }
            assert!(visited_queries.iter().all(|x| *x), "missing query result");
        }
        black_box(checksum);
        if continuous && verify {
            continue;
        }
        rows.push(Sample {
            build_ns,
            update_ns,
            query_ns,
            total_ns: build_ns + update_ns + query_ns,
            results,
        });
    }
    match_counts.sort_unstable();
    let mean = match_counts.iter().sum::<usize>() as f64 / match_counts.len().max(1) as f64;
    let p99 = match_counts
        .get((match_counts.len() * 99 / 100).min(match_counts.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0);
    println!(
        "{}",
        serde_json::json!({"implementation":name,"scenario":f.name,"points":f.points.len(),"queries":f.frames.iter().map(|x|x.queries.len()).sum::<usize>(),"correct":true,"output_window_queries":window,"materialization":"bounded reusable complete-query u64 outputs; every ID materialized in timing and exact window oracle validated afterward","timing_scope":if continuous {"proof pass precedes separate uninterrupted query timing; full u64 materialization and every-ID consumption; frame service includes updates, prepare, ordering, dispatch, threads, output and consumption; no network/game simulation"} else {"summed kernel service time; oracle callback time excluded; not uninterrupted tick wall latency"},"continuous_full_consumption":continuous,"frame_samples":frame_samples,"excluded_validation_ns":validation_ns.to_string(),"largest_emitted_group_capacity_bytes":peak_capacity,"capacity_scope":"one emitted group, not sum of concurrently live worker buffers","matches_per_query":{"mean":mean,"p99":p99,"max":match_counts.last().copied().unwrap_or(0)},"samples":rows})
    );
}
