//! Same RILLv001 fixtures, one reusable output: dense 20k workloads stay bounded.
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
            let updates: Vec<_> = (0..nu).map(|_| (rd.u32(), rd.point())).collect();
            let nq = rd.u32();
            let queries: Vec<_> = (0..nq).map(|_| (rd.point(), rd.f64())).collect();
            (updates, queries)
        })
        .collect();
    let repetitions = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);
    let stride = args
        .get(3)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);
    let mut checked = 0;
    if stride > 0 {
        let mut s = Index3::from_points(
            cell,
            points.iter().enumerate().map(|(id, &p)| (id as u64, p)),
        )
        .unwrap();
        let mut current = points.clone();
        let mut out = Vec::new();
        let mut expected = Vec::new();
        for (updates, queries) in &frames {
            for &(id, p) in updates {
                current[id] = p;
                s.upsert(id as u64, p).unwrap();
            }
            s.prepare();
            for (i, &(p, r)) in queries.iter().enumerate() {
                if i % stride != 0 {
                    continue;
                }
                s.within_into(p, r, &mut out).unwrap();
                out.sort_unstable();
                expected.clear();
                expected.extend(
                    current
                        .iter()
                        .enumerate()
                        .filter(|(_, q)| {
                            q.iter().zip(p).map(|(a, b)| (a - b) * (a - b)).sum::<f64>() <= r * r
                        })
                        .map(|(id, _)| id as u64),
                );
                assert_eq!(out, expected, "query {i}, centre={p:?}, radius={r:?}");
                checked += 1;
            }
        }
    }
    eprintln!(
        "oracle_queries={checked}, points={n}, frames={nf}, output_storage=one_reused_vector"
    );
    for _ in 0..repetitions {
        let t = Instant::now();
        let mut s = Index3::from_points(
            cell,
            points.iter().enumerate().map(|(id, &p)| (id as u64, p)),
        )
        .unwrap();
        let build = t.elapsed().as_nanos();
        let (mut update, mut query, mut matches, mut queries) = (0, 0, 0u64, 0u64);
        let mut out = Vec::new();
        for (updates, qs) in &frames {
            let t = Instant::now();
            for &(id, p) in updates {
                s.upsert(id as u64, black_box(p)).unwrap();
            }
            if !qs.is_empty() {
                s.prepare();
            }
            update += t.elapsed().as_nanos();
            let t = Instant::now();
            for &(p, r) in qs {
                s.within_into(black_box(p), black_box(r), &mut out).unwrap();
                black_box(&out);
                matches += out.len() as u64;
                queries += 1;
            }
            query += t.elapsed().as_nanos();
        }
        println!(
            "{{\"build_ns\":{build},\"update_ns\":{update},\"query_ns\":{query},\"total_ns\":{},\"queries\":{queries},\"matches\":{matches},\"checked_queries\":{checked},\"strategy\":{:?}}}",
            build + update + query,
            std::env::var("RILL_CELL_STRATEGY").unwrap_or("sorted".into())
        );
    }
}
