mod bounded;
mod cohorts;
mod continuous;
mod more_adapters;
mod rust_adapters;
mod studies;
use serde::{Deserialize, Serialize};
use std::hint::black_box;
use std::time::Instant;

pub type Point = [f64; 3];
#[derive(Clone, Copy, Deserialize)]
pub struct Query {
    pub point: Point,
    pub radius: f64,
}
#[derive(Deserialize)]
pub struct Frame {
    pub updates: Vec<(usize, Point)>,
    pub queries: Vec<Query>,
}
#[derive(Deserialize)]
pub struct Fixture {
    pub name: String,
    pub points: Vec<Point>,
    pub cell_size: f64,
    #[serde(default)]
    pub bounded: bool,
    pub frames: Vec<Frame>,
}
// Keep callback object lifetimes tied to their borrows across prototype adapters.
#[allow(clippy::type_complexity)]
pub trait Adapter {
    fn build(points: &[Point], cell_size: f64) -> Self;
    fn update(&mut self, updates: &[(usize, Point)]);
    fn prepare(&mut self) {}
    fn query(&self, point: Point, radius: f64, out: &mut Vec<u64>);
    fn query_batch(&self, queries: &[Query], outputs: &mut [Vec<u64>]) {
        for (q, out) in queries.iter().zip(outputs) {
            self.query(q.point, q.radius, out);
        }
    }
    fn visit_groups_shared(
        &self,
        queries: &[Query],
        max_queries: usize,
        emit: &(dyn Fn(&[usize], &[Vec<u64>]) + Sync),
    ) {
        self.visit_groups(queries, max_queries, &mut |indices, rows| {
            emit(indices, rows)
        });
    }
    fn visit_groups(
        &self,
        queries: &[Query],
        max_queries: usize,
        emit: &mut (dyn FnMut(&[usize], &[Vec<u64>]) + Send),
    ) {
        let mut outputs: Vec<Vec<u64>> = (0..max_queries.min(queries.len()))
            .map(|_| Vec::new())
            .collect();
        let mut indices = Vec::with_capacity(max_queries);
        for (begin, chunk) in queries.chunks(max_queries).enumerate() {
            indices.clear();
            indices.extend(begin * max_queries..begin * max_queries + chunk.len());
            self.query_batch(chunk, &mut outputs[..chunk.len()]);
            emit(&indices, &outputs[..chunk.len()]);
        }
    }
}
#[inline]
pub fn squared(a: &Point, b: &Point) -> f64 {
    let x = a[0] - b[0];
    let y = a[1] - b[1];
    let z = a[2] - b[2];
    x * x + y * y + z * z
}
#[derive(Serialize)]
struct Sample {
    build_ns: u64,
    update_ns: u64,
    query_ns: u64,
    total_ns: u64,
    results: usize,
}

fn oracle(f: &Fixture) -> Vec<Vec<u64>> {
    let mut points = f.points.clone();
    let mut result = Vec::new();
    for frame in &f.frames {
        for &(id, p) in &frame.updates {
            points[id] = p;
        }
        for q in &frame.queries {
            result.push(
                points
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| squared(p, &q.point) <= q.radius * q.radius)
                    .map(|(i, _)| i as u64)
                    .collect(),
            );
        }
    }
    result
}

fn run<A: Adapter>(name: &str, f: &Fixture, samples: usize) {
    if f.bounded {
        bounded::run::<A>(name, f, samples);
        return;
    }
    let expected = oracle(f);
    let count = expected.len();
    let mut rows = Vec::new();
    for sample in 0..samples {
        let mut outputs: Vec<Vec<u64>> = (0..count).map(|_| Vec::new()).collect();
        let start = Instant::now();
        let mut index = A::build(black_box(&f.points), f.cell_size);
        let build_ns = start.elapsed().as_nanos() as u64;
        let mut update_ns = 0;
        let mut query_ns = 0;
        let mut cursor = 0;
        for frame in &f.frames {
            let t = Instant::now();
            index.update(black_box(&frame.updates));
            update_ns += t.elapsed().as_nanos() as u64;
            if !frame.queries.is_empty() {
                let t = Instant::now();
                index.prepare();
                update_ns += t.elapsed().as_nanos() as u64;
            }
            let t = Instant::now();
            for q in &frame.queries {
                index.query(
                    black_box(q.point),
                    black_box(q.radius),
                    &mut outputs[cursor],
                );
                cursor += 1;
            }
            query_ns += t.elapsed().as_nanos() as u64;
        }
        black_box(&outputs);
        let mut total = 0;
        for (i, (actual, want)) in outputs.iter_mut().zip(&expected).enumerate() {
            actual.sort_unstable();
            assert_eq!(actual, want, "{name} {} sample {sample} query {i}", f.name);
            total += actual.len();
        }
        rows.push(Sample {
            build_ns,
            update_ns,
            query_ns,
            total_ns: build_ns + update_ns + query_ns,
            results: total,
        });
    }
    println!(
        "{}",
        serde_json::json!({"implementation":name,"scenario":f.name,"points":f.points.len(),"queries":count,"correct":true,"samples":rows})
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: rill-spatial-benchmarks IMPLEMENTATION FIXTURE.json [SAMPLES]");
        std::process::exit(2);
    }
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(&args[2]).expect("fixture file"))
        .expect("valid fixture");
    let samples = args.get(3).and_then(|x| x.parse().ok()).unwrap_or(5);
    use more_adapters::*;
    use rust_adapters::*;
    match args[1].as_str() {
        "rill-spatial" => run::<Rill>(&args[1], &fixture, samples),
        "rill-auto-1" => run::<studies::AutoOutput<1>>(&args[1], &fixture, samples),
        "rill-auto-4" => run::<studies::AutoOutput<4>>(&args[1], &fixture, samples),
        "rill-radius-1" => run::<studies::RadiusAware<1>>(&args[1], &fixture, samples),
        "rill-radius-4" => run::<studies::RadiusAware<4>>(&args[1], &fixture, samples),
        "rill-parallel-1" => run::<studies::PublicParallel<1>>(&args[1], &fixture, samples),
        "rill-parallel-2" => run::<studies::PublicParallel<2>>(&args[1], &fixture, samples),
        "rill-parallel-4" => run::<studies::PublicParallel<4>>(&args[1], &fixture, samples),
        "study-kiddo-1" => run::<cohorts::Coherent<KiddoImmutable, 1>>(&args[1], &fixture, samples),
        "study-kiddo-4" => run::<cohorts::Coherent<KiddoImmutable, 4>>(&args[1], &fixture, samples),
        "study-static-aabb-1" => {
            run::<cohorts::Coherent<StaticAabb, 1>>(&args[1], &fixture, samples)
        }
        "study-static-aabb-4" => {
            run::<cohorts::Coherent<StaticAabb, 4>>(&args[1], &fixture, samples)
        }
        "study-geo-rtree-1" => run::<cohorts::Coherent<GeoRtree, 1>>(&args[1], &fixture, samples),
        "study-geo-rtree-4" => run::<cohorts::Coherent<GeoRtree, 4>>(&args[1], &fixture, samples),
        "rill-batch-1" => run::<studies::PublicBatch<1>>(&args[1], &fixture, samples),
        "rill-batch-2" => run::<studies::PublicBatch<2>>(&args[1], &fixture, samples),
        "rill-batch-4" => run::<studies::PublicBatch<4>>(&args[1], &fixture, samples),
        "rill-coherent-1" => run::<cohorts::Coherent<Rill, 1>>(&args[1], &fixture, samples),
        "rill-coherent-2" => run::<cohorts::Coherent<Rill, 2>>(&args[1], &fixture, samples),
        "rill-coherent-4" => run::<cohorts::Coherent<Rill, 4>>(&args[1], &fixture, samples),
        "rill-cell-sorted" => run::<studies::Cell<0>>(&args[1], &fixture, samples),
        "rill-cell-hybrid" => run::<studies::Cell<1>>(&args[1], &fixture, samples),
        "rill-cell-hash" => run::<studies::Cell<2>>(&args[1], &fixture, samples),
        "rill-cell-occupied" => run::<studies::Cell<3>>(&args[1], &fixture, samples),
        "rill-dual" => run::<studies::Dual>(&args[1], &fixture, samples),
        "rill-cell-batch-sorted" => run::<studies::CellCoherent<0>>(&args[1], &fixture, samples),
        "rill-cell-batch-hybrid" => run::<studies::CellCoherent<1>>(&args[1], &fixture, samples),
        "rill-cell-batch-hash" => run::<studies::CellCoherent<2>>(&args[1], &fixture, samples),
        "rill-cell-batch-occupied" => run::<studies::CellCoherent<3>>(&args[1], &fixture, samples),
        "brute-force" => run::<Brute>(&args[1], &fixture, samples),
        "rstar" => run::<Rstar>(&args[1], &fixture, samples),
        "kdtree" => run::<KdTree>(&args[1], &fixture, samples),
        "kd-tree" => run::<KdTreeStatic>(&args[1], &fixture, samples),
        "kiddo-mutable" => run::<KiddoMutable>(&args[1], &fixture, samples),
        "kiddo-immutable" => run::<KiddoImmutable>(&args[1], &fixture, samples),
        "static-aabb2d" => run::<StaticAabb>(&args[1], &fixture, samples),
        "geo-index-kd" => run::<GeoKd>(&args[1], &fixture, samples),
        "geo-index-rtree" => run::<GeoRtree>(&args[1], &fixture, samples),
        "spart-kd" => run::<SpartKd>(&args[1], &fixture, samples),
        "spart-rtree" => run::<SpartRtree>(&args[1], &fixture, samples),
        "spart-rstar" => run::<SpartRstar>(&args[1], &fixture, samples),
        "spart-octree" => run::<SpartOctree>(&args[1], &fixture, samples),
        "spart-quadtree" => run::<SpartQuadtree>(&args[1], &fixture, samples),
        "acap-kd" => run::<AcapKd>(&args[1], &fixture, samples),
        "acap-flat-kd" => run::<AcapFlatKd>(&args[1], &fixture, samples),
        "acap-vp" => run::<AcapVp>(&args[1], &fixture, samples),
        "acap-flat-vp" => run::<AcapFlatVp>(&args[1], &fixture, samples),
        "acap-exhaustive" => run::<AcapExhaustive>(&args[1], &fixture, samples),
        "vpsearch" => run::<VpSearch>(&args[1], &fixture, samples),
        "petal-ball" => run::<PetalBall>(&args[1], &fixture, samples),
        "neighbourhood-owned" => run::<NeighbourhoodOwned>(&args[1], &fixture, samples),
        "neighbourhood-index" => run::<NeighbourhoodIndex>(&args[1], &fixture, samples),
        "quadtree-f64" => run::<QuadF64>(&args[1], &fixture, samples),
        "quadtree-rs" => run::<QuadRs>(&args[1], &fixture, samples),
        "rust-octree" => run::<RustOctree>(&args[1], &fixture, samples),
        "spatial-linear" => run::<SpatialLinear>(&args[1], &fixture, samples),
        "spatial-quadratic" => run::<SpatialQuadratic>(&args[1], &fixture, samples),
        "spatial-rstar" => run::<SpatialRstar>(&args[1], &fixture, samples),
        "nabo" => run::<Nabo>(&args[1], &fixture, samples),
        _ => {
            eprintln!("unknown implementation");
            std::process::exit(2)
        }
    }
}
