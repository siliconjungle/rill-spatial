//! Main-library diagnostic. Normal preparation versus charged full reconstruction.
use main_rill::Index3;
use std::{collections::HashMap, hint::black_box, time::Instant};
type Point = [f64; 3];
struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}
impl Reader<'_> {
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
    fn frame(&mut self) -> (Vec<(usize, Point)>, Vec<(Point, f64)>) {
        let n = self.u32();
        let updates = (0..n).map(|_| (self.u32(), self.point())).collect();
        let n = self.u32();
        let qs = (0..n).map(|_| (self.point(), self.f64())).collect();
        (updates, qs)
    }
}
struct Fixture {
    bytes: Vec<u8>,
    name: String,
    cell: f64,
    points: Vec<Point>,
    ids: Vec<u64>,
    frames: usize,
    frames_at: usize,
}
impl Fixture {
    fn load(path: &str) -> Self {
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[..8], b"RILLv001");
        let mut r = Reader { b: &bytes, at: 8 };
        let n = r.u32();
        let name = String::from_utf8(bytes[r.at..r.at + n].to_vec()).unwrap();
        r.at += n;
        let cell = r.f64();
        let n = r.u32();
        let frames = r.u32();
        let points = (0..n).map(|_| r.point()).collect();
        let ids = (0..n)
            .map(|i| (i as u64).wrapping_mul(0x9e3779b97f4a7c15) ^ 0xa0761d6478bd642f)
            .collect();
        let frames_at = r.at;
        Self {
            bytes,
            name,
            cell,
            points,
            ids,
            frames,
            frames_at,
        }
    }
    fn index(&self, points: &[Point]) -> Index3 {
        Index3::from_points(
            self.cell,
            self.ids.iter().copied().zip(points.iter().copied()),
        )
        .unwrap()
    }
}
#[derive(Clone, Copy, Debug)]
enum Mode {
    Normal,
    Reconstruct,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal_prepare",
            Self::Reconstruct => "force_reconstruct",
        }
    }
}

fn execute(f: &Fixture, mode: Mode, window: usize, verify: bool, repetition: usize) -> Vec<u64> {
    let start = Instant::now();
    let mut index = f.index(&f.points);
    let build_ns = start.elapsed().as_nanos();
    let mut current = f.points.clone();
    let mut reader = Reader {
        b: &f.bytes,
        at: f.frames_at,
    };
    let ids: HashMap<_, _> = if verify {
        f.ids.iter().enumerate().map(|(i, &id)| (id, i)).collect()
    } else {
        HashMap::new()
    };
    let mut bits = if verify {
        vec![0u64; (current.len() + 63) / 64]
    } else {
        Vec::new()
    };
    let mut outputs: Vec<Vec<u64>> = (0..window).map(|_| Vec::new()).collect();
    let (
        mut total_update,
        mut total_prepare,
        mut total_reconstruct,
        mut total_query,
        mut total_consume,
    ) = (0u128, 0u128, 0u128, 0u128, 0u128);
    let mut counts = Vec::new();
    let mut total_queries = 0usize;
    for frame in 0..f.frames {
        let (updates, queries) = reader.frame();
        for &(i, p) in &updates {
            current[i] = p;
        }
        let t = Instant::now();
        for &(i, p) in &updates {
            index.upsert(f.ids[i], black_box(p)).unwrap();
        }
        let update_ns = t.elapsed().as_nanos();
        let (mut prepare_ns, mut reconstruct_ns, mut query_ns, mut consume_ns, mut count) =
            (0u128, 0u128, 0u128, 0u128, 0u64);
        if !queries.is_empty() {
            if matches!(mode, Mode::Reconstruct) {
                let t = Instant::now();
                index = f.index(&current);
                reconstruct_ns = t.elapsed().as_nanos();
            }
            let t = Instant::now();
            index.prepare();
            prepare_ns = t.elapsed().as_nanos();
        }
        for (chunk, qs) in queries.chunks(window).enumerate() {
            let t = Instant::now();
            for (q, &(centre, radius)) in qs.iter().enumerate() {
                index
                    .within_into(black_box(centre), black_box(radius), &mut outputs[q])
                    .unwrap();
            }
            black_box(&outputs[..qs.len()]);
            query_ns += t.elapsed().as_nanos();
            if verify {
                for (q, &(centre, radius)) in qs.iter().enumerate() {
                    bits.fill(0);
                    for &id in &outputs[q] {
                        let i = ids[&id];
                        let mask = 1 << (i % 64);
                        assert_eq!(
                            bits[i / 64] & mask,
                            0,
                            "duplicate frame={frame} query={} id={id}",
                            chunk * window + q
                        );
                        bits[i / 64] |= mask;
                    }
                    for (i, p) in current.iter().enumerate() {
                        let expected = p
                            .iter()
                            .zip(centre)
                            .map(|(x, y)| (x - y) * (x - y))
                            .sum::<f64>()
                            <= radius * radius;
                        assert_eq!(
                            (bits[i / 64] >> (i % 64)) & 1,
                            expected as u64,
                            "membership frame={frame} query={} id={}",
                            chunk * window + q,
                            f.ids[i]
                        );
                    }
                }
            }
            let t = Instant::now();
            for (q, out) in outputs.iter().take(qs.len()).enumerate() {
                count += out.len() as u64;
                for &id in out {
                    black_box((chunk * window + q, id));
                }
            }
            consume_ns += t.elapsed().as_nanos();
        }
        counts.push(count);
        total_queries += queries.len();
        total_update += update_ns;
        total_prepare += prepare_ns;
        total_reconstruct += reconstruct_ns;
        total_query += query_ns;
        total_consume += consume_ns;
        if !verify {
            let name = &f.name;
            let mode = mode.name();
            let nq = queries.len();
            let service = update_ns + prepare_ns + reconstruct_ns + query_ns + consume_ns;
            println!("{{\"kind\":\"main_refit_frame\",\"fixture\":{name:?},\"mode\":{mode:?},\"window\":{window},\"repetition\":{repetition},\"frame\":{frame},\"queries\":{nq},\"results\":{count},\"update_ns\":{update_ns},\"reconstruct_ns\":{reconstruct_ns},\"prepare_ns\":{prepare_ns},\"query_ns\":{query_ns},\"consume_ns\":{consume_ns},\"summed_service_ns\":{service},\"status\":\"exploratory_concurrent\"}}");
        }
    }
    if !verify {
        let name = &f.name;
        let mode = mode.name();
        let total = build_ns
            + total_update
            + total_prepare
            + total_reconstruct
            + total_query
            + total_consume;
        let results: u64 = counts.iter().sum();
        println!("{{\"kind\":\"main_refit_summary\",\"fixture\":{name:?},\"mode\":{mode:?},\"window\":{window},\"repetition\":{repetition},\"queries\":{total_queries},\"results\":{results},\"initial_build_ns\":{build_ns},\"update_ns\":{total_update},\"reconstruct_ns\":{total_reconstruct},\"prepare_ns\":{total_prepare},\"query_ns\":{total_query},\"consume_ns\":{total_consume},\"total_ns_including_rebuild\":{total},\"status\":\"exploratory_concurrent\"}}");
    }
    counts
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let f = Fixture::load(
        args.get(1)
            .expect("usage: refit_diagnostic fixture.bin [repetitions=3] [--verify-only]"),
    );
    let repeats = args
        .get(2)
        .filter(|s| !s.starts_with("--"))
        .map(|s| s.parse().unwrap())
        .unwrap_or(3usize);
    let mut expected = None;
    for window in [1, 64] {
        for mode in [Mode::Normal, Mode::Reconstruct] {
            let start = Instant::now();
            let counts = execute(&f, mode, window, true, 0);
            if let Some(want) = &expected {
                assert_eq!(&counts, want);
            } else {
                expected = Some(counts);
            }
            let elapsed = start.elapsed().as_nanos();
            let name = &f.name;
            let mode = mode.name();
            println!("{{\"kind\":\"main_refit_full_membership_proof\",\"fixture\":{name:?},\"mode\":{mode:?},\"window\":{window},\"proof_wall_including_oracle_ns\":{elapsed},\"arbitrary_stable_ids\":true,\"all_present_absent_and_duplicate_memberships\":true}}");
        }
    }
    if args.iter().any(|s| s == "--verify-only") {
        return;
    }
    for repetition in 0..repeats {
        let mut modes = [Mode::Normal, Mode::Reconstruct];
        if repetition % 2 == 1 {
            modes.reverse();
        }
        let mut windows = [1, 64];
        if repetition % 2 == 1 {
            windows.reverse();
        }
        for window in windows {
            for mode in modes {
                assert_eq!(
                    execute(&f, mode, window, false, repetition),
                    *expected.as_ref().unwrap()
                );
            }
        }
    }
}
