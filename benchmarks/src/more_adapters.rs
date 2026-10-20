use crate::{Adapter, Point, squared};
use acap::{Euclidean, EuclideanDistance, NearestNeighbors, Proximity};
#[derive(Clone)]
pub struct Apoint {
    p: Point,
    id: u64,
}
impl acap::Coordinates for Apoint {
    type Value = f64;
    fn dims(&self) -> usize {
        3
    }
    fn coord(&self, i: usize) -> f64 {
        self.p[i]
    }
}
type Epoint = Euclidean<Apoint>;
struct Radius<'a> {
    target: &'a Epoint,
    bound: EuclideanDistance<f64>,
    out: &'a mut Vec<u64>,
}
impl<'a, 'v> acap::knn::Neighborhood<&'a Epoint, &'v Epoint> for Radius<'a> {
    fn target(&self) -> &'a Epoint {
        self.target
    }
    fn contains<T>(&self, d: T) -> bool
    where
        T: PartialOrd<EuclideanDistance<f64>>,
    {
        d <= self.bound
    }
    fn consider(&mut self, item: &'v Epoint) -> EuclideanDistance<f64> {
        let d = self.target.distance(item);
        if d <= self.bound {
            self.out.push(item.0.id);
        }
        d
    }
}
macro_rules! acap_tree {
    ($name:ident,$tree:ty,$ctor:expr) => {
        pub struct $name {
            tree: $tree,
            points: Vec<Point>,
            dirty: bool,
        }
        impl $name {
            fn make(p: &[Point]) -> $tree {
                let rows = p
                    .iter()
                    .enumerate()
                    .map(|(i, &p)| Euclidean(Apoint { p, id: i as u64 }))
                    .collect::<Vec<_>>();
                ($ctor)(rows)
            }
        }
        impl Adapter for $name {
            fn build(p: &[Point], _: f64) -> Self {
                Self {
                    tree: Self::make(p),
                    points: p.to_vec(),
                    dirty: false,
                }
            }
            fn update(&mut self, u: &[(usize, Point)]) {
                for &(i, p) in u {
                    self.points[i] = p;
                }
                self.dirty |= !u.is_empty();
            }
            fn prepare(&mut self) {
                if self.dirty {
                    self.tree = Self::make(&self.points);
                    self.dirty = false;
                }
            }
            fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
                out.clear();
                let target = Euclidean(Apoint { p, id: 0 });
                self.tree.search(Radius {
                    target: &target,
                    bound: EuclideanDistance::from_squared(r * r),
                    out,
                });
            }
        }
    };
}
acap_tree!(AcapKd, acap::kd::KdTree<Epoint>, acap::kd::KdTree::balanced);
acap_tree!(
    AcapFlatKd,
    acap::kd::FlatKdTree<Epoint>,
    acap::kd::FlatKdTree::balanced
);
acap_tree!(AcapVp, acap::vp::VpTree<Epoint>, acap::vp::VpTree::balanced);
acap_tree!(
    AcapFlatVp,
    acap::vp::FlatVpTree<Epoint>,
    acap::vp::FlatVpTree::balanced
);
acap_tree!(
    AcapExhaustive,
    acap::exhaustive::ExhaustiveSearch<Epoint>,
    |rows: Vec<Epoint>| rows.into_iter().collect()
);

#[derive(Clone)]
struct Vpoint {
    p: Point,
    id: u64,
}
impl vpsearch::MetricSpace for Vpoint {
    type UserData = ();
    type Distance = f64;
    fn distance(&self, other: &Self, _: &()) -> f64 {
        squared(&self.p, &other.p).sqrt()
    }
}
struct Vradius<'a> {
    needle: Point,
    r: f64,
    out: &'a mut Vec<u64>,
}
impl vpsearch::BestCandidate<Vpoint, ()> for Vradius<'_> {
    type Output = ();
    fn consider(&mut self, p: &Vpoint, _: f64, _: usize, _: &()) {
        if squared(&p.p, &self.needle) <= self.r * self.r {
            self.out.push(p.id);
        }
    }
    fn distance(&self) -> f64 {
        self.r + 1e-9
    }
    fn result(self, _: &()) {}
}
pub struct VpSearch {
    tree: vpsearch::Tree<Vpoint>,
    points: Vec<Point>,
    dirty: bool,
}
impl VpSearch {
    fn make(p: &[Point]) -> vpsearch::Tree<Vpoint> {
        vpsearch::Tree::new(
            &p.iter()
                .enumerate()
                .map(|(i, &p)| Vpoint { p, id: i as u64 })
                .collect::<Vec<_>>(),
        )
    }
}
impl Adapter for VpSearch {
    fn build(p: &[Point], _: f64) -> Self {
        Self {
            tree: Self::make(p),
            points: p.to_vec(),
            dirty: false,
        }
    }
    fn update(&mut self, u: &[(usize, Point)]) {
        for &(i, p) in u {
            self.points[i] = p;
        }
        self.dirty |= !u.is_empty();
    }
    fn prepare(&mut self) {
        if self.dirty {
            self.tree = Self::make(&self.points);
            self.dirty = false;
        }
    }
    fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
        out.clear();
        self.tree
            .find_nearest_custom(&Vpoint { p, id: 0 }, &(), Vradius { needle: p, r, out });
    }
}
pub struct PetalBall {
    tree: petal_neighbors::BallTree<'static, f64, petal_neighbors::distance::Euclidean>,
    points: Vec<Point>,
    dirty: bool,
}
impl PetalBall {
    fn make(
        p: &[Point],
    ) -> petal_neighbors::BallTree<'static, f64, petal_neighbors::distance::Euclidean> {
        petal_neighbors::BallTree::euclidean(
            ndarray::Array2::from_shape_vec((p.len(), 3), p.iter().flatten().copied().collect())
                .unwrap(),
        )
        .unwrap()
    }
}
impl Adapter for PetalBall {
    fn build(p: &[Point], _: f64) -> Self {
        Self {
            tree: Self::make(p),
            points: p.to_vec(),
            dirty: false,
        }
    }
    fn update(&mut self, u: &[(usize, Point)]) {
        for &(i, p) in u {
            self.points[i] = p;
        }
        self.dirty |= !u.is_empty();
    }
    fn prepare(&mut self) {
        if self.dirty {
            self.tree = Self::make(&self.points);
            self.dirty = false;
        }
    }
    fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
        out.clear();
        for id in self.tree.query_radius(&ndarray::aview1(&p), r + 1e-9) {
            if squared(&self.points[id], &p) <= r * r {
                out.push(id as u64);
            }
        }
    }
}
pub struct NeighbourhoodOwned {
    tree: neighbourhood::KdTree<f64, 3>,
    ids: Vec<u64>,
    points: Vec<Point>,
    dirty: bool,
}
impl NeighbourhoodOwned {
    fn make(p: &[Point]) -> (neighbourhood::KdTree<f64, 3>, Vec<u64>) {
        let tree = neighbourhood::KdTree::new(p.to_vec());
        let mut map = std::collections::HashMap::<[u64; 3], Vec<u64>>::new();
        for (i, p) in p.iter().enumerate() {
            map.entry(p.map(f64::to_bits)).or_default().push(i as u64);
        }
        let ids = tree
            .data()
            .iter()
            .map(|p| map.get_mut(&p.map(f64::to_bits)).unwrap().pop().unwrap())
            .collect();
        (tree, ids)
    }
}
impl Adapter for NeighbourhoodOwned {
    fn build(p: &[Point], _: f64) -> Self {
        let (tree, ids) = Self::make(p);
        Self {
            tree,
            ids,
            points: p.to_vec(),
            dirty: false,
        }
    }
    fn update(&mut self, u: &[(usize, Point)]) {
        for &(i, p) in u {
            self.points[i] = p;
        }
        self.dirty |= !u.is_empty();
    }
    fn prepare(&mut self) {
        if self.dirty {
            (self.tree, self.ids) = Self::make(&self.points);
            self.dirty = false;
        }
    }
    fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
        out.clear();
        for at in self.tree.neighbourhood_by_index(&p, r + 1e-9) {
            let id = self.ids[at];
            if squared(&self.points[id as usize], &p) <= r * r {
                out.push(id);
            }
        }
    }
}
type NIndex<'a> = neighbourhood::KdIndexTree<'a, f64, 3>;
self_cell::self_cell! {struct BorrowedN{owner:Vec<Point>,#[covariant]dependent:NIndex,}}
pub struct NeighbourhoodIndex {
    tree: BorrowedN,
    points: Vec<Point>,
    dirty: bool,
}
impl Adapter for NeighbourhoodIndex {
    fn build(p: &[Point], _: f64) -> Self {
        Self {
            tree: BorrowedN::new(p.to_vec(), |p| NIndex::new(p)),
            points: p.to_vec(),
            dirty: false,
        }
    }
    fn update(&mut self, u: &[(usize, Point)]) {
        for &(i, p) in u {
            self.points[i] = p;
        }
        self.dirty |= !u.is_empty();
    }
    fn prepare(&mut self) {
        if self.dirty {
            self.tree = BorrowedN::new(self.points.clone(), |p| NIndex::new(p));
            self.dirty = false;
        }
    }
    fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
        out.clear();
        self.tree.with_dependent(|_, tree| {
            for id in tree.neighbourhood_by_index(&p, r + 1e-9) {
                if squared(&self.points[id], &p) <= r * r {
                    out.push(id as u64);
                }
            }
        });
    }
}
pub struct QuadF64 {
    tree: quadtree_f32::QuadTree,
    points: Vec<Point>,
    dirty: bool,
}
fn quadpoint(p: Point) -> quadtree_f32::Item {
    quadtree_f32::Item::Point(quadtree_f32::Point { x: p[0], y: p[1] })
}
impl Adapter for QuadF64 {
    fn build(p: &[Point], _: f64) -> Self {
        let mut tree = quadtree_f32::QuadTree::new();
        tree.insert(
            quadtree_f32::ItemId(usize::MAX),
            quadtree_f32::Item::Rect(quadtree_f32::Rect {
                min_x: -1e6,
                min_y: -1e6,
                max_x: 1e6,
                max_y: 1e6,
            }),
        );
        for (i, &p) in p.iter().enumerate() {
            tree.insert(quadtree_f32::ItemId(i), quadpoint(p));
        }
        Self {
            tree,
            points: p.to_vec(),
            dirty: false,
        }
    }
    fn update(&mut self, u: &[(usize, Point)]) {
        for &(id, p) in u {
            self.points[id] = p;
        }
        self.dirty |= !u.is_empty();
    }
    fn prepare(&mut self) {
        if self.dirty {
            *self = Self::build(&self.points, 0.0);
        }
    }
    fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
        out.clear();
        for id in self.tree.get_ids_that_overlap(&quadtree_f32::Rect {
            min_x: p[0] - r - 1e-9,
            min_y: p[1] - r - 1e-9,
            max_x: p[0] + r + 1e-9,
            max_y: p[1] + r + 1e-9,
        }) {
            if id.0 != usize::MAX && squared(&self.points[id.0], &p) <= r * r {
                out.push(id.0 as u64);
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

pub struct QuadRs {
    tree: quadtree_rs::Quadtree<i64, u64>,
    handles: Vec<u64>,
    points: Vec<Point>,
}
fn qp(p: Point) -> quadtree_rs::point::Point<i64> {
    quadtree_rs::point::Point {
        x: p[0].floor() as i64,
        y: p[1].floor() as i64,
    }
}
impl Adapter for QuadRs {
    fn build(p: &[Point], _: f64) -> Self {
        let mut tree = quadtree_rs::Quadtree::new_with_anchor(
            quadtree_rs::point::Point {
                x: -1048576,
                y: -1048576,
            },
            21,
        );
        let handles = p
            .iter()
            .enumerate()
            .map(|(i, &p)| tree.insert_pt(qp(p), i as u64).unwrap())
            .collect();
        Self {
            tree,
            handles,
            points: p.to_vec(),
        }
    }
    fn update(&mut self, u: &[(usize, Point)]) {
        for &(i, p) in u {
            self.tree.delete_by_handle(self.handles[i]).unwrap();
            self.handles[i] = self.tree.insert_pt(qp(p), i as u64).unwrap();
            self.points[i] = p;
        }
    }
    fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
        out.clear();
        let area = quadtree_rs::area::AreaBuilder::default()
            .anchor(quadtree_rs::point::Point {
                x: (p[0] - r).floor() as i64 - 1,
                y: (p[1] - r).floor() as i64 - 1,
            })
            .dimensions(((2.0 * r).ceil() as i64 + 4, (2.0 * r).ceil() as i64 + 4))
            .build()
            .unwrap();
        for e in self.tree.query(area) {
            let id = *e.value_ref();
            if squared(&self.points[id as usize], &p) <= r * r {
                out.push(id);
            }
        }
    }
}
pub struct RustOctree {
    tree: octree::Octree,
    points: Vec<Point>,
    dirty: bool,
}
impl RustOctree {
    fn make(p: &[Point]) -> octree::Octree {
        let mut t = octree::Octree::new(p.iter().copied());
        t.build(16);
        t
    }
}
impl Adapter for RustOctree {
    fn build(p: &[Point], _: f64) -> Self {
        Self {
            tree: Self::make(p),
            points: p.to_vec(),
            dirty: false,
        }
    }
    fn update(&mut self, u: &[(usize, Point)]) {
        for &(i, p) in u {
            self.points[i] = p;
        }
        self.dirty |= !u.is_empty();
    }
    fn prepare(&mut self) {
        if self.dirty {
            self.tree = Self::make(&self.points);
            self.dirty = false;
        }
    }
    fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
        out.clear();
        for (id, _) in self.tree.search(p, r + 1e-9) {
            if squared(&self.points[id], &p) <= r * r {
                out.push(id as u64);
            }
        }
    }
}
macro_rules! spatial_tree {
    ($name:ident,$tree:ty,$ctor:expr) => {
        pub struct $name {
            tree: $tree,
            points: Vec<Point>,
        }
        impl Adapter for $name {
            fn build(p: &[Point], _: f64) -> Self {
                let mut tree: $tree = $ctor;
                for (i, &p) in p.iter().enumerate() {
                    tree.insert(spatial::geometry::Point::new(p), i as u64);
                }
                Self {
                    tree,
                    points: p.to_vec(),
                }
            }
            fn update(&mut self, u: &[(usize, Point)]) {
                use spatial::tree::mbr::MbrRectQuery;
                for &(id, p) in u {
                    let old = self.points[id];
                    let rect = spatial::geometry::Rect::new(old.map(|x| (x - 1e-9, x + 1e-9)));
                    self.tree
                        .retain(MbrRectQuery::Overlaps(rect), |&value| value != id as u64);
                    self.tree
                        .insert(spatial::geometry::Point::new(p), id as u64);
                    self.points[id] = p;
                }
            }
            fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
                out.clear();
                let rect = spatial::geometry::Rect::new(p.map(|x| (x - r - 1e-9, x + r + 1e-9)));
                for (_, id) in self
                    .tree
                    .iter_query(spatial::tree::mbr::MbrRectQuery::Overlaps(rect))
                {
                    if squared(&self.points[*id as usize], &p) <= r * r {
                        out.push(*id);
                    }
                }
            }
        }
    };
}
spatial_tree!(SpatialLinear,spatial::RLinearTree<f64,3,spatial::geometry::Point<f64,3>,u64>,spatial::RTree::new_linear());
spatial_tree!(SpatialQuadratic,spatial::RQuadraticTree<f64,3,spatial::geometry::Point<f64,3>,u64>,spatial::RTree::new_quadratic());
spatial_tree!(SpatialRstar,spatial::RStarTree<f64,3,spatial::geometry::Point<f64,3>,u64>,spatial::RStar::new());

#[derive(Default, Clone)]
struct Npoint(Point);
impl nabo::Point<f64> for Npoint {
    const DIM: u32 = 3;
    fn set(&mut self, i: u32, v: nabo::NotNan<f64>) {
        self.0[i as usize] = v.into_inner();
    }
    fn get(&self, i: u32) -> nabo::NotNan<f64> {
        nabo::NotNan::new(self.0[i as usize]).unwrap()
    }
}
pub struct Nabo {
    tree: nabo::KDTree<f64, Npoint>,
    points: Vec<Point>,
    dirty: bool,
}
impl Nabo {
    fn make(p: &[Point]) -> nabo::KDTree<f64, Npoint> {
        nabo::KDTree::new(&p.iter().map(|&p| Npoint(p)).collect::<Vec<_>>())
    }
}
impl Adapter for Nabo {
    fn build(p: &[Point], _: f64) -> Self {
        Self {
            tree: Self::make(p),
            points: p.to_vec(),
            dirty: false,
        }
    }
    fn update(&mut self, u: &[(usize, Point)]) {
        for &(i, p) in u {
            self.points[i] = p;
        }
        self.dirty |= !u.is_empty();
    }
    fn prepare(&mut self) {
        if self.dirty {
            self.tree = Self::make(&self.points);
            self.dirty = false;
        }
    }
    fn query(&self, p: Point, r: f64, out: &mut Vec<u64>) {
        out.clear();
        let params = nabo::Parameters {
            epsilon: 0.0,
            max_radius: r + 1e-9,
            allow_self_match: true,
            sort_results: false,
        };
        for x in self.tree.knn_advanced(
            self.points.len() as u32,
            &Npoint(p),
            nabo::CandidateContainer::BinaryHeap,
            &params,
            None,
        ) {
            if squared(&self.points[x.index as usize], &p) <= r * r {
                out.push(x.index as u64);
            }
        }
    }
}
