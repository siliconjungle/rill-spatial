//! Query-cell candidate reuse diagnostics. Concurrent timings are exploratory.
//! Every grouping, candidate-list, expansion and output step is timed; oracle
//! checking in callbacks is explicitly subtracted. One <=64-result group lives.
use rill_spatial::Index3;
use std::{hint::black_box, time::Instant};
type Point = [f64; 3];
struct Reader {
    bytes: Vec<u8>,
    at: usize,
}
impl Reader {
    fn u32(&mut self) -> usize {
        let x = u32::from_le_bytes(self.bytes[self.at..self.at + 4].try_into().unwrap());
        self.at += 4;
        x as usize
    }
    fn f64(&mut self) -> f64 {
        let x = f64::from_le_bytes(self.bytes[self.at..self.at + 8].try_into().unwrap());
        self.at += 8;
        x
    }
    fn point(&mut self) -> Point {
        [self.f64(), self.f64(), self.f64()]
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mut rd = Reader {
        bytes: std::fs::read(&args[1]).unwrap(),
        at: 8,
    };
    assert_eq!(&rd.bytes[..8], b"RILLv001");
    let len = rd.u32();
    rd.at += len;
    let cell = rd.f64();
    let n = rd.u32();
    let nf = rd.u32();
    let points: Vec<_> = (0..n).map(|_| rd.point()).collect();
    let frames: Vec<_> = (0..nf)
        .map(|_| {
            let nu = rd.u32();
            let us: Vec<_> = (0..nu).map(|_| (rd.u32(), rd.point())).collect();
            let nq = rd.u32();
            let qs: Vec<_> = (0..nq).map(|_| (rd.point(), rd.f64())).collect();
            (us, qs)
        })
        .collect();
    let repeats = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let stride = args
        .get(3)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);
    let width = 64.min((16 * 1024 * 1024 / (n.max(1) * 8)).max(1));
    for repetition in 0..repeats {
        let t = Instant::now();
        let mut index = Index3::from_points(
            cell,
            points.iter().enumerate().map(|(id, &p)| (id as u64, p)),
        )
        .unwrap();
        let build = t.elapsed().as_nanos();
        let mut current = points.clone();
        let mut outputs: Vec<Vec<u64>> = (0..width).map(|_| Vec::new()).collect();
        let (
            mut update,
            mut batch_ns,
            mut single_ns,
            mut matches,
            mut checked,
            mut groups,
            mut candidate_cells,
            mut point_tests,
            mut query_count,
        ) = (0, 0, 0, 0u64, 0, 0u64, 0u64, 0u64, 0u64);
        for (updates, queries) in &frames {
            for &(id, p) in updates {
                current[id] = p;
            }
            let t = Instant::now();
            for &(id, p) in updates {
                index.upsert(id as u64, black_box(p)).unwrap();
            }
            index.prepare();
            update += t.elapsed().as_nanos();
            let mut seen = vec![false; queries.len()];
            let mut validation_ns = 0;
            let mut batch_matches = 0u64;
            let mut do_batch = || {
                let t = Instant::now();
                let stats = index
                    .batch_visit_groups(black_box(queries), width, |indices, results| {
                        for ids in results {
                            black_box(ids);
                            batch_matches += ids.len() as u64;
                        }
                        let validation = Instant::now();
                        assert_eq!(indices.len(), results.len());
                        assert!(indices.len() <= width);
                        for (&q, ids) in indices.iter().zip(results) {
                            assert!(!seen[q]);
                            seen[q] = true;
                            if stride == 0 || q % stride != 0 {
                                continue;
                            }
                            let (c, r) = queries[q];
                            let expected: Vec<_> = current
                                .iter()
                                .enumerate()
                                .filter(|(_, p)| {
                                    p.iter().zip(c).map(|(a, b)| (a - b) * (a - b)).sum::<f64>()
                                        <= r * r
                                })
                                .map(|(id, _)| id as u64)
                                .collect();
                            let mut actual = ids.clone();
                            actual.sort_unstable();
                            assert_eq!(actual, expected, "query {q}");
                            checked += 1;
                        }
                        validation_ns += validation.elapsed().as_nanos();
                    })
                    .unwrap();
                batch_ns += t.elapsed().as_nanos() - validation_ns;
                groups += stats.groups;
                candidate_cells += stats.candidate_cells;
                point_tests += stats.point_tests;
                assert!(seen.iter().all(|x| *x));
                assert_eq!(stats.reported_pairs, batch_matches);
            };
            let mut single_matches = 0u64;
            let mut do_single = || {
                let t = Instant::now();
                for chunk in queries.chunks(width) {
                    for (&(c, r), out) in chunk.iter().zip(&mut outputs) {
                        index.within_into(black_box(c), black_box(r), out).unwrap();
                        black_box(&out);
                        single_matches += out.len() as u64;
                    }
                }
                single_ns += t.elapsed().as_nanos();
            };
            if repetition % 2 == 0 {
                do_single();
                do_batch();
            } else {
                do_batch();
                do_single();
            }
            assert_eq!(batch_matches, single_matches);
            matches += batch_matches;
            query_count += queries.len() as u64;
        }
        println!(
            "{{\"experiment\":\"query_cell_candidate_reuse\",\"timing_status\":\"exploratory_concurrent_not_acceptance\",\"strategy\":{:?},\"repetition\":{repetition},\"points\":{n},\"queries\":{query_count},\"max_outputs\":{width},\"build_ns\":{build},\"update_ns\":{update},\"single_ns\":{single_ns},\"batch_ns\":{batch_ns},\"reported_pairs\":{matches},\"oracle_queries\":{checked},\"query_cells\":{groups},\"candidate_cells\":{candidate_cells},\"point_tests\":{point_tests}}}",
            std::env::var("RILL_CELL_STRATEGY").unwrap_or("sorted".into())
        );
    }
}
