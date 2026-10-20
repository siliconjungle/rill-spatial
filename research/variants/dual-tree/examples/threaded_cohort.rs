//! Scoped-thread diagnostic: complete output materialization and consumption.
//! Every worker count receives full membership proof before any timing runs.
#[path = "support/cohort.rs"]
mod cohort;
use rill_spatial::Index3;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

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
    fn frame(&mut self) -> (Vec<(usize, Point)>, Vec<cohort::Query>) {
        let n = self.u32();
        let updates = (0..n).map(|_| (self.u32(), self.point())).collect();
        let n = self.u32();
        let queries = (0..n).map(|_| (self.point(), self.f64())).collect();
        (updates, queries)
    }
}
struct Fixture {
    bytes: Vec<u8>,
    name: String,
    cell: f64,
    points: Vec<Point>,
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
        let frames_at = r.at;
        Self {
            bytes,
            name,
            cell,
            points,
            frames,
            frames_at,
        }
    }
    fn index(&self) -> Index3 {
        Index3::from_points(
            self.cell,
            self.points.iter().enumerate().map(|(i, &p)| (i as u64, p)),
        )
        .unwrap()
    }
    fn reader(&self) -> Reader<'_> {
        Reader {
            b: &self.bytes,
            at: self.frames_at,
        }
    }
}

fn prove(f: &Fixture, workers: usize, cohort: usize) -> Vec<u64> {
    let proof = Instant::now();
    let mut index = f.index();
    let mut points = f.points.clone();
    let mut r = f.reader();
    let mut counts = Vec::new();
    for _ in 0..f.frames {
        let (updates, queries) = r.frame();
        for (id, p) in updates {
            points[id] = p;
            index.upsert(id as u64, p).unwrap();
        }
        if !queries.is_empty() {
            index.prepare();
        }
        let visited: Vec<_> = (0..queries.len()).map(|_| AtomicBool::new(false)).collect();
        let s = cohort::run(&index, &queries, workers, cohort, |_, indices, outputs| {
            assert!(indices.len() <= cohort);
            assert_eq!(indices.len(), outputs.len());
            let mut bits = vec![0u64; (points.len() + 63) / 64];
            for (&q, out) in indices.iter().zip(outputs) {
                assert!(!visited[q].swap(true, Ordering::Relaxed));
                bits.fill(0);
                for &id in out {
                    assert!(id < (points.len() as u64));
                    let i = id as usize;
                    let mask = 1 << (i % 64);
                    assert_eq!(bits[i / 64] & mask, 0, "duplicate q={q} id={id}");
                    bits[i / 64] |= mask;
                }
                let (c, r) = queries[q];
                for (i, p) in points.iter().enumerate() {
                    let expected =
                        p.iter().zip(c).map(|(x, y)| (x - y) * (x - y)).sum::<f64>() <= r * r;
                    assert_eq!(
                        (bits[i / 64] >> (i % 64)) & 1,
                        expected as u64,
                        "membership q={q} id={i}"
                    );
                }
            }
        });
        assert!(visited.iter().all(|v| v.load(Ordering::Relaxed)));
        assert_eq!(s.queries, queries.len() as u64);
        counts.push(s.results);
    }
    let elapsed = proof.elapsed().as_nanos();
    let name = &f.name;
    let results: u64 = counts.iter().sum();
    println!("{{\"kind\":\"parallel_full_membership_proof\",\"fixture\":{name:?},\"workers\":{workers},\"cohort_limit\":{cohort},\"reported_results\":{results},\"proof_wall_including_oracle_ns\":{elapsed},\"full_present_absent_and_duplicate_checks\":true}}");
    counts
}

fn measure(
    f: &Fixture,
    expected: &[u64],
    workers: usize,
    cohort: usize,
    repetition: usize,
    deadline_ns: u128,
) {
    let build = Instant::now();
    let mut index = f.index();
    let build_ns = build.elapsed().as_nanos();
    let mut r = f.reader();
    let (
        mut total_query_wall,
        mut total_frame_service,
        mut total_order,
        mut sum_workers,
        mut max_frame,
        mut results,
        mut misses,
        mut query_frames,
    ) = (0u128, 0u128, 0u128, 0u128, 0u128, 0u64, 0usize, 0usize);
    let name = &f.name;
    for (frame, &expected_results) in expected.iter().enumerate() {
        // Parsing/input I/O is outside service timing: all workloads receive the same trace.
        let (updates, queries) = r.frame();
        let frame_start = Instant::now();
        for (id, p) in updates {
            index.upsert(id as u64, p).unwrap();
        }
        if !queries.is_empty() {
            index.prepare();
        }
        let update_prepare_ns = frame_start.elapsed().as_nanos();
        let s = cohort::run(&index, &queries, workers, cohort, |_, _, _| {});
        let frame_service_ns = frame_start.elapsed().as_nanos();
        assert_eq!(s.results, expected_results);
        assert_eq!(s.queries, queries.len() as u64);
        let deadline_met = frame_service_ns <= deadline_ns;
        if !queries.is_empty() {
            query_frames += 1;
            misses += (!deadline_met) as usize;
            max_frame = max_frame.max(frame_service_ns);
            total_query_wall += s.wall_ns;
            total_frame_service += frame_service_ns;
            total_order += s.ordering_ns;
            sum_workers += s.sum_worker_elapsed_ns;
            results += s.results;
        }
        let (wall, order, sum, max_work, nq, cohorts, tests, pairs) = (
            s.wall_ns,
            s.ordering_ns,
            s.sum_worker_elapsed_ns,
            s.max_worker_elapsed_ns,
            s.queries,
            s.cohorts,
            s.point_tests,
            s.node_pairs,
        );
        println!("{{\"kind\":\"threaded_query_frame\",\"fixture\":{name:?},\"workers\":{workers},\"repetition\":{repetition},\"frame\":{frame},\"cohort_limit\":{cohort},\"queries\":{nq},\"results\":{expected_results},\"update_prepare_ns\":{update_prepare_ns},\"ordering_ns\":{order},\"query_wall_ns_including_spawn_join\":{wall},\"sum_worker_elapsed_ns_not_cpu\":{sum},\"max_worker_elapsed_ns\":{max_work},\"frame_service_ns\":{frame_service_ns},\"deadline_ns\":{deadline_ns},\"deadline_met\":{deadline_met},\"cohorts\":{cohorts},\"point_tests\":{tests},\"node_pairs\":{pairs},\"output\":\"complete_materialized_ids_and_per_id_consumption\",\"timing_status\":\"exploratory_concurrent\"}}");
    }
    println!("{{\"kind\":\"threaded_summary\",\"fixture\":{name:?},\"workers\":{workers},\"repetition\":{repetition},\"cohort_limit\":{cohort},\"initial_build_ns\":{build_ns},\"query_frames\":{query_frames},\"reported_results\":{results},\"sum_query_wall_ns\":{total_query_wall},\"sum_frame_service_ns\":{total_frame_service},\"sum_ordering_ns\":{total_order},\"sum_worker_elapsed_ns_not_cpu\":{sum_workers},\"max_query_frame_service_ns\":{max_frame},\"deadline_ns\":{deadline_ns},\"deadline_misses\":{misses},\"timing_status\":\"exploratory_concurrent\"}}");
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let f=Fixture::load(args.get(1).expect("usage: threaded_cohort fixture.bin [repetitions=3] [deadline_ms=16.666667] [--verify-only]"));
    let repetitions: usize = args
        .get(2)
        .filter(|s| !s.starts_with("--"))
        .map(|s| s.parse().unwrap())
        .unwrap_or(3);
    let deadline_ms: f64 = args
        .get(3)
        .filter(|s| !s.starts_with("--"))
        .map(|s| s.parse().unwrap())
        .unwrap_or(16.666667);
    assert!(deadline_ms.is_finite() && deadline_ms > 0.);
    let deadline_ns = (deadline_ms * 1_000_000.) as u128;
    // Exactly the same spatial cohorts at every worker count; budget for max4.
    let cohort = cohort::cohort_size(f.points.len(), 4);
    let expected = prove(&f, 1, cohort);
    for workers in [2, 4] {
        assert_eq!(prove(&f, workers, cohort), expected);
    }
    if args.iter().any(|s| s == "--verify-only") {
        return;
    }
    for repetition in 0..repetitions {
        let mut counts = [1, 2, 4];
        counts.rotate_left(repetition % 3);
        for workers in counts {
            measure(&f, &expected, workers, cohort, repetition, deadline_ns);
        }
    }
}
