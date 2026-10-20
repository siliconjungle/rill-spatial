use crate::{Adapter,Point,squared};

pub struct Rill(rill_spatial::Index3);
impl Adapter for Rill {
 fn build(p:&[Point],s:f64)->Self{let mut x=rill_spatial::Index3::new(s).unwrap();x.reserve(p.len());for(i,&p)in p.iter().enumerate(){x.upsert(i as u64,p).unwrap();}Self(x)}
 fn update(&mut self,u:&[(usize,Point)]){for &(id,p)in u{self.0.upsert(id as u64,p).unwrap();}}
 fn prepare(&mut self){self.0.prepare();}
 fn query(&self,p:Point,r:f64,out:&mut Vec<u64>){self.0.within_into(p,r,out).unwrap();}
}
pub struct Brute(Vec<Point>);
impl Adapter for Brute {
 fn build(p:&[Point],_:f64)->Self{Self(p.to_vec())}
 fn update(&mut self,u:&[(usize,Point)]){for &(id,p)in u{self.0[id]=p;}}
 fn query(&self,p:Point,r:f64,out:&mut Vec<u64>){out.clear();for(i,q)in self.0.iter().enumerate(){if squared(q,&p)<=r*r{out.push(i as u64);}}}
}

#[derive(Clone,PartialEq)]struct Rpoint{p:Point,id:u64}
impl rstar::RTreeObject for Rpoint{type Envelope=rstar::AABB<Point>;fn envelope(&self)->Self::Envelope{rstar::AABB::from_point(self.p)}}
impl rstar::PointDistance for Rpoint{fn distance_2(&self,p:&Point)->f64{squared(&self.p,p)}}
pub struct Rstar{tree:rstar::RTree<Rpoint>,points:Vec<Point>}
impl Adapter for Rstar{
 fn build(p:&[Point],_:f64)->Self{Self{tree:rstar::RTree::bulk_load(p.iter().enumerate().map(|(i,&p)|Rpoint{p,id:i as u64}).collect()),points:p.to_vec()}}
 fn update(&mut self,u:&[(usize,Point)]){for &(id,p)in u{self.tree.remove(&Rpoint{p:self.points[id],id:id as u64});self.tree.insert(Rpoint{p,id:id as u64});self.points[id]=p;}}
 fn query(&self,p:Point,r:f64,out:&mut Vec<u64>){out.clear();out.extend(self.tree.locate_within_distance(p,r*r).map(|x|x.id));}
}
pub struct KdTree{tree:kdtree::KdTree<f64,u64,Point>,points:Vec<Point>}
impl Adapter for KdTree{
 fn build(p:&[Point],_:f64)->Self{let mut tree=kdtree::KdTree::new(3);for(i,&p)in p.iter().enumerate(){tree.add(p,i as u64).unwrap();}Self{tree,points:p.to_vec()}}
 fn update(&mut self,u:&[(usize,Point)]){for &(id,p)in u{self.tree.remove(&self.points[id],&(id as u64)).unwrap();self.tree.add(p,id as u64).unwrap();self.points[id]=p;}}
 fn query(&self,p:Point,r:f64,out:&mut Vec<u64>){out.clear();out.extend(self.tree.within(&p,r*r,&kdtree::distance::squared_euclidean).unwrap().into_iter().map(|(_,id)|*id));}
}
pub struct KdTreeStatic{tree:kd_tree::KdMap<Point,u64>,points:Vec<Point>,dirty:bool}
impl Adapter for KdTreeStatic{
 fn build(p:&[Point],_:f64)->Self{Self{tree:kd_tree::KdTree::build_by_ordered_float(p.iter().enumerate().map(|(i,&p)|(p,i as u64)).collect()),points:p.to_vec(),dirty:false}}
 fn update(&mut self,u:&[(usize,Point)]){for &(i,p)in u{self.points[i]=p;}self.dirty|=!u.is_empty();}
 fn prepare(&mut self){if self.dirty{self.tree=kd_tree::KdTree::build_by_ordered_float(self.points.iter().enumerate().map(|(i,&p)|(p,i as u64)).collect());self.dirty=false;}}
 fn query(&self,p:Point,r:f64,out:&mut Vec<u64>){out.clear();out.extend(self.tree.within_radius(&p,r).into_iter().map(|x|x.1));}
}
pub struct KiddoMutable{tree:kiddo::MutableKdTree<f64,3>,points:Vec<Point>}
impl Adapter for KiddoMutable{
 fn build(p:&[Point],_:f64)->Self{let tree=kiddo::MutableKdTree::new_from_slice(p).unwrap();Self{tree,points:p.to_vec()}}
 fn update(&mut self,u:&[(usize,Point)]){for &(id,p)in u{self.tree.remove(&self.points[id],id as u32);self.tree.add(&p,id as u32).unwrap();self.points[id]=p;}}
 fn query(&self,p:Point,r:f64,out:&mut Vec<u64>){out.clear();out.extend(self.tree.query(&p).within::<kiddo::SquaredEuclidean<f64>>(r*r).unsorted().execute().into_iter().map(|x|x.item as u64));}
}
pub struct KiddoImmutable{tree:kiddo::ImmutableKdTree<f64,3>,points:Vec<Point>,dirty:bool}
impl Adapter for KiddoImmutable{
 fn build(p:&[Point],_:f64)->Self{Self{tree:kiddo::ImmutableKdTree::new_from_slice(p).unwrap(),points:p.to_vec(),dirty:false}}
 fn update(&mut self,u:&[(usize,Point)]){for &(i,p)in u{self.points[i]=p;}self.dirty|=!u.is_empty();}
 fn prepare(&mut self){if self.dirty{self.tree=kiddo::ImmutableKdTree::new_from_slice(&self.points).unwrap();self.dirty=false;}}
 fn query(&self,p:Point,r:f64,out:&mut Vec<u64>){out.clear();out.extend(self.tree.query(&p).within::<kiddo::SquaredEuclidean<f64>>(r*r).unsorted().execute().into_iter().map(|x|x.item as u64));}
}
pub struct StaticAabb{tree:static_aabb2d_index::StaticAABB2DIndex<f64>,points:Vec<Point>,dirty:bool}
impl StaticAabb{fn make(p:&[Point])->static_aabb2d_index::StaticAABB2DIndex<f64>{let mut b=static_aabb2d_index::StaticAABB2DIndexBuilder::new(p.len());for p in p{b.add(p[0],p[1],p[0],p[1]);}b.build().unwrap()}}
impl Adapter for StaticAabb{
 fn build(p:&[Point],_:f64)->Self{Self{tree:Self::make(p),points:p.to_vec(),dirty:false}}
 fn update(&mut self,u:&[(usize,Point)]){for &(i,p)in u{self.points[i]=p;}self.dirty|=!u.is_empty();}
 fn prepare(&mut self){if self.dirty{self.tree=Self::make(&self.points);self.dirty=false;}}
 fn query(&self,p:Point,r:f64,out:&mut Vec<u64>){out.clear();self.tree.visit_query(p[0]-r,p[1]-r,p[0]+r,p[1]+r,&mut|id|{if squared(&self.points[id],&p)<=r*r{out.push(id as u64);}static_aabb2d_index::Control::<()>::Continue});}
}
pub struct GeoKd{tree:geo_index::kdtree::KDTree<f64>,points:Vec<Point>,dirty:bool}
impl GeoKd{fn make(p:&[Point])->geo_index::kdtree::KDTree<f64>{let mut b=geo_index::kdtree::KDTreeBuilder::new(p.len() as u32);for p in p{b.add(p[0],p[1]);}b.finish()}}
impl Adapter for GeoKd{
 fn build(p:&[Point],_:f64)->Self{Self{tree:Self::make(p),points:p.to_vec(),dirty:false}}
 fn update(&mut self,u:&[(usize,Point)]){for &(i,p)in u{self.points[i]=p;}self.dirty|=!u.is_empty();}
 fn prepare(&mut self){if self.dirty{self.tree=Self::make(&self.points);self.dirty=false;}}
 fn query(&self,p:Point,r:f64,out:&mut Vec<u64>){use geo_index::kdtree::KDTreeIndex;out.clear();for id in self.tree.within(p[0],p[1],r){if squared(&self.points[id as usize],&p)<=r*r{out.push(id as u64);}}}
}
pub struct GeoRtree{tree:geo_index::rtree::RTree<f64>,points:Vec<Point>,dirty:bool}
impl GeoRtree{fn make(p:&[Point])->geo_index::rtree::RTree<f64>{let mut b=geo_index::rtree::RTreeBuilder::new(p.len() as u32);for p in p{b.add(p[0],p[1],p[0],p[1]);}b.finish::<geo_index::rtree::sort::HilbertSort>()}}
impl Adapter for GeoRtree{
 fn build(p:&[Point],_:f64)->Self{Self{tree:Self::make(p),points:p.to_vec(),dirty:false}}
 fn update(&mut self,u:&[(usize,Point)]){for &(i,p)in u{self.points[i]=p;}self.dirty|=!u.is_empty();}
 fn prepare(&mut self){if self.dirty{self.tree=Self::make(&self.points);self.dirty=false;}}
 fn query(&self,p:Point,r:f64,out:&mut Vec<u64>){use geo_index::rtree::RTreeIndex;out.clear();for id in self.tree.search(p[0]-r,p[1]-r,p[0]+r,p[1]+r){if squared(&self.points[id as usize],&p)<=r*r{out.push(id as u64);}}}
}
use spart::geometry::{Point3D,Point2D,EuclideanDistance,Cube,Rectangle};
fn sp(p:Point,id:u64)->Point3D<u64>{Point3D::new(p[0],p[1],p[2],Some(id))}
macro_rules! spart_dynamic{
 ($name:ident,$tree:ty,$construct:expr)=>{
 pub struct $name{tree:$tree,points:Vec<Point>}
 impl Adapter for $name{
 fn build(p:&[Point],_:f64)->Self{let mut tree:$tree=$construct;for(i,&p)in p.iter().enumerate(){let _=tree.insert(sp(p,i as u64));}Self{tree,points:p.to_vec()}}
 fn update(&mut self,u:&[(usize,Point)]){for &(i,p)in u{assert!(self.tree.delete(&sp(self.points[i],i as u64)));let _=self.tree.insert(sp(p,i as u64));self.points[i]=p;}}
 fn query(&self,p:Point,r:f64,out:&mut Vec<u64>){out.clear();out.extend(self.tree.range_search::<EuclideanDistance>(&sp(p,0),r).into_iter().map(|x|x.data.unwrap()));}
 }}
}
spart_dynamic!(SpartKd,spart::kdtree::KdTree<Point3D<u64>>,spart::kdtree::KdTree::new());
spart_dynamic!(SpartRtree,spart::rtree::RTree<Point3D<u64>>,spart::rtree::RTree::new(16).unwrap());
spart_dynamic!(SpartRstar,spart::rstar_tree::RStarTree<Point3D<u64>>,spart::rstar_tree::RStarTree::new(16).unwrap());
spart_dynamic!(SpartOctree,spart::octree::Octree<u64>,spart::octree::Octree::new(&Cube{x:-1e6,y:-1e6,z:-1e6,width:2e6,height:2e6,depth:2e6},16).unwrap());
pub struct SpartQuadtree{tree:spart::quadtree::Quadtree<u64>,points:Vec<Point>}
impl Adapter for SpartQuadtree{
 fn build(p:&[Point],_:f64)->Self{let mut tree=spart::quadtree::Quadtree::new(&Rectangle{x:-1e6,y:-1e6,width:2e6,height:2e6},16).unwrap();for(i,p)in p.iter().enumerate(){assert!(tree.insert(Point2D::new(p[0],p[1],Some(i as u64))));}Self{tree,points:p.to_vec()}}
 fn update(&mut self,u:&[(usize,Point)]){for &(i,p)in u{let old=self.points[i];assert!(self.tree.delete(&Point2D::new(old[0],old[1],Some(i as u64))));assert!(self.tree.insert(Point2D::new(p[0],p[1],Some(i as u64))));self.points[i]=p;}}
 fn query(&self,p:Point,r:f64,out:&mut Vec<u64>){out.clear();for q in self.tree.range_search::<EuclideanDistance>(&Point2D::new(p[0],p[1],None),r){let id=q.data.unwrap();if squared(&self.points[id as usize],&p)<=r*r{out.push(id);}}}
}
