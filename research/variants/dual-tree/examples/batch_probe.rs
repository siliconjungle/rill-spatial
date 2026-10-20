//! Bounded materialization and streaming diagnostics, not acceptance rankings.
//! Read the parent's RILLv001 fixtures without copying them into this crate.
use rill_spatial::Index3;
use std::{hint::black_box, time::Instant};
type Point = [f64; 3];
struct Reader {
    b: Vec<u8>,
    at: usize,
}
impl Reader {
    fn u32(&mut self) -> usize {
        let n = u32::from_le_bytes(self.b[self.at..self.at + 4].try_into().unwrap());
        self.at += 4;
        n as usize
    }
    fn f64(&mut self) -> f64 {
        let n = f64::from_le_bytes(self.b[self.at..self.at + 8].try_into().unwrap());
        self.at += 8;
        n
    }
    fn point(&mut self) -> Point {
        [self.f64(), self.f64(), self.f64()]
    }
}
fn oracle(points: &[Point], qs: &[(Point, f64)]) -> Vec<u64> {
    let n = points.len();
    let mut bits = vec![0u64; (n * qs.len() + 63) / 64];
    for (q, (c, r)) in qs.iter().enumerate() {
        for (i, p) in points.iter().enumerate() {
            if p.iter().zip(c).map(|(a, b)| (a - b) * (a - b)).sum::<f64>() <= r * r {
                let at = q * n + i;
                bits[at / 64] |= 1 << (at % 64);
            }
        }
    }
    bits
}
fn verify(n: usize, expected: &[u64], outputs: &[Vec<u64>]) {
    let mut seen = vec![0u64; expected.len()];
    for (q, ids) in outputs.iter().enumerate() {
        for &id in ids {
            assert!(id < (n as u64));
            let at = q * n + id as usize;
            let mask = 1 << (at % 64);
            assert_eq!(seen[at / 64] & mask, 0, "duplicate q={q} id={id}");
            seen[at / 64] |= mask;
        }
    }
    assert_eq!(seen, expected, "full membership mismatch");
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let bytes = std::fs::read(
        args.get(1)
            .expect("usage: batch_probe fixture.bin [repetitions]"),
    )
    .unwrap();
    assert_eq!(&bytes[..8], b"RILLv001");
    let mut r = Reader { b: bytes, at: 8 };
    let name_len = r.u32();
    let name = String::from_utf8(r.b[r.at..r.at + name_len].to_vec()).unwrap();
    r.at += name_len;
    let cell = r.f64();
    let n = r.u32();
    let frame_count = r.u32();
    let points: Vec<_> = (0..n).map(|_| r.point()).collect();
    let frames_at = r.at;
    let repeats: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(1);
    // At most 16 MiB of logical vector IDs at once (capacity growth may double).
    let chunk = 64.min((16 * 1024 * 1024 / (n.max(1) * 8)).max(1));
    for repetition in 0..repeats {
        r.at = frames_at;
        let mut current = points.clone();
        let t = Instant::now();
        let mut index = Index3::from_points(
            cell,
            current.iter().enumerate().map(|(i, &p)| (i as u64, p)),
        )
        .unwrap();
        let build = t.elapsed().as_nanos();
        // Independent, equally cold result buffers; both retain capacity across frames.
        let mut single_outputs: Vec<Vec<u64>> = (0..chunk).map(|_| Vec::new()).collect();
        let mut batch_outputs: Vec<Vec<u64>> = (0..chunk).map(|_| Vec::new()).collect();
        let (
            mut update,
            mut single,
            mut materialize,
            mut consume,
            mut blocks_time,
            mut expanded_time,
        ) = (0, 0, 0, 0, 0, 0);
        let (mut visitor_time, mut visitor_consume) = (0, 0);
        let (mut query_count, mut result_count, mut block_count, mut pair_tests, mut node_pairs) =
            (0u64, 0u64, 0u64, 0u64, 0u64);
        for _ in 0..frame_count {
            let count = r.u32();
            let updates: Vec<_> = (0..count).map(|_| (r.u32(), r.point())).collect();
            for &(id, p) in &updates {
                current[id] = p;
            }
            let t = Instant::now();
            for &(id, p) in &updates {
                index.upsert(id as u64, black_box(p)).unwrap();
            }
            update += t.elapsed().as_nanos();
            let count = r.u32();
            let queries: Vec<_> = (0..count).map(|_| (r.point(), r.f64())).collect();
            query_count += count as u64;
            if !queries.is_empty() {
                let t = Instant::now();
                index.prepare();
                update += t.elapsed().as_nanos();
            }
            let mut expected_results = 0u64;
            for (chunk_index, qs) in queries.chunks(chunk).enumerate() {
                // Full independent oracle construction and verification are untimed.
                let expected = oracle(&current, qs);
                expected_results += expected.iter().map(|x| x.count_ones() as u64).sum::<u64>();
                // Alternate order; both paths include output clearing and allocations.
                let mut run_single = || {
                    let t = Instant::now();
                    for (q, &(p, r)) in qs.iter().enumerate() {
                        index
                            .within_into(black_box(p), black_box(r), &mut single_outputs[q])
                            .unwrap();
                    }
                    single += t.elapsed().as_nanos();
                };
                let mut run_batch = || {
                    let t = Instant::now();
                    for out in batch_outputs.iter_mut().take(qs.len()) {
                        out.clear();
                    }
                    index
                        .batch_blocks(black_box(qs), |qindices, ids| {
                            for &q in qindices {
                                batch_outputs[q].extend_from_slice(ids);
                            }
                        })
                        .unwrap();
                    materialize += t.elapsed().as_nanos();
                };
                if (repetition + chunk_index) % 2 == 0 {
                    run_single();
                    run_batch();
                } else {
                    run_batch();
                    run_single();
                }
                verify(n, &expected, &single_outputs[..qs.len()]);
                verify(n, &expected, &batch_outputs[..qs.len()]);
                let t = Instant::now();
                for (q, ids) in batch_outputs.iter().take(qs.len()).enumerate() {
                    for &id in ids {
                        black_box((q, id));
                    }
                }
                consume += t.elapsed().as_nanos();
            }
            let mut validation_ns = 0;
            let mut visited = vec![false; queries.len()];
            let mut visitor_count = 0u64;
            let t = Instant::now();
            let visitor_stats = index
                .batch_visit_groups(black_box(&queries), chunk, |indices, results| {
                    let validation = Instant::now();
                    {
                        assert!(indices.len() <= chunk);
                        assert_eq!(indices.len(), results.len());
                        let qs: Vec<_> = indices
                            .iter()
                            .map(|&i| {
                                assert!(!visited[i]);
                                visited[i] = true;
                                queries[i]
                            })
                            .collect();
                        let expected = oracle(&current, &qs);
                        verify(n, &expected, results);
                    }
                    validation_ns += validation.elapsed().as_nanos();
                    let consume_start = Instant::now();
                    for (&q, ids) in indices.iter().zip(results) {
                        visitor_count += ids.len() as u64;
                        for &id in ids {
                            black_box((q, id));
                        }
                    }
                    visitor_consume += consume_start.elapsed().as_nanos();
                })
                .unwrap();
            visitor_time += t.elapsed().as_nanos() - validation_ns;
            assert!(visited.iter().all(|v| *v));
            assert_eq!(visitor_count, expected_results);
            assert_eq!(visitor_stats.reported_pairs, expected_results);
            // Discovery is a compact result representation. It does NOT expand IDs.
            let t = Instant::now();
            let block_stats = index
                .batch_blocks(black_box(&queries), |qs, ids| {
                    black_box((qs, ids));
                })
                .unwrap();
            blocks_time += t.elapsed().as_nanos();
            // Expanded streaming includes one opaque consumption per individual pair.
            let mut emitted = 0u64;
            let t = Instant::now();
            let stats = index
                .batch_each(black_box(&queries), |q, id| {
                    black_box((q, id));
                    emitted += 1;
                })
                .unwrap();
            expanded_time += t.elapsed().as_nanos();
            assert_eq!(stats.reported_pairs, expected_results);
            assert_eq!(block_stats.reported_pairs, expected_results);
            assert_eq!(emitted, expected_results);
            result_count += emitted;
            block_count += stats.blocks;
            pair_tests += stats.point_tests;
            node_pairs += stats.node_pairs;
        }
        let visitor_materialize = visitor_time - visitor_consume;
        println!("{{\"fixture\":{name:?},\"repetition\":{repetition},\"points\":{n},\"queries\":{query_count},\"results\":{result_count},\"materialization_chunk\":{chunk},\"build_ns\":{build},\"update_ns\":{update},\"single_materialize_ns\":{single},\"batch_materialize_chunk_ns\":{materialize},\"materialized_consume_ns\":{consume},\"spatial_visitor_materialize_ns\":{visitor_materialize},\"spatial_visitor_consume_ns\":{visitor_consume},\"spatial_visitor_materialize_and_consume_ns\":{visitor_time},\"whole_batch_blocks_ns\":{blocks_time},\"whole_batch_expanded_consume_ns\":{expanded_time},\"blocks\":{block_count},\"point_tests\":{pair_tests},\"node_pairs\":{node_pairs},\"full_membership_oracle\":\"single_chunked_batch_and_spatial_visitor\",\"timing_status\":\"exploratory_concurrent\"}}");
    }
}
