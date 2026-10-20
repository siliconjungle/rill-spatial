//! Prepared packed hierarchy. Spatial order makes accepted subtrees contiguous.
use super::in_radius;

#[derive(Clone)]
struct Node<const D:usize>{lo:[f64;D],hi:[f64;D],start:usize,end:usize,left:usize,right:usize,half_diagonal_sq:f64}
#[derive(Clone)]
pub(crate) struct Hierarchy<const D:usize>{points:Vec<[f64;D]>,ids:Vec<u64>,nodes:Vec<Node<D>>}

impl<const D:usize> Hierarchy<D>{
 pub(crate) fn new(mut rows:Vec<(u64,[f64;D])>)->Self{
  let mut nodes=Vec::new();if !rows.is_empty(){Self::build(&mut rows,0,&mut nodes);}
  let points=rows.iter().map(|x|x.1).collect();let ids=rows.iter().map(|x|x.0).collect();Self{points,ids,nodes}
 }
 fn build(rows:&mut[(u64,[f64;D])],start:usize,nodes:&mut Vec<Node<D>>)->usize{
  let mut lo=[f64::INFINITY;D];let mut hi=[f64::NEG_INFINITY;D];
  for(_,p)in rows.iter(){for d in 0..D{lo[d]=lo[d].min(p[d]);hi[d]=hi[d].max(p[d]);}}
  let mut axis=0;for d in 1..D{if hi[d]-lo[d]>hi[axis]-lo[axis]{axis=d;}}
  let at=nodes.len();let half_diagonal_sq=(0..D).map(|d|{let x=(hi[d]-lo[d])*0.5;x*x}).sum();
  nodes.push(Node{lo,hi,start,end:start+rows.len(),left:usize::MAX,right:usize::MAX,half_diagonal_sq});
  if rows.len()>24{
   let middle=rows.len()/2;rows.select_nth_unstable_by(middle,|a,b|a.1[axis].total_cmp(&b.1[axis]));
   let(a,b)=rows.split_at_mut(middle);let left=Self::build(a,start,nodes);let right=Self::build(b,start+middle,nodes);nodes[at].left=left;nodes[at].right=right;
  }at
 }
 #[inline]pub(crate) fn query(&self,p:&[f64;D],r:f64,out:&mut Vec<u64>){if self.nodes.is_empty(){return;}self.visit(0,p,r,r*r,out);}
 fn visit(&self,index:usize,p:&[f64;D],r:f64,r2:f64,out:&mut Vec<u64>){
  let n=&self.nodes[index];let mut near=0.0;
  for d in 0..D{let delta=if p[d]<n.lo[d]{n.lo[d]-p[d]}else if p[d]>n.hi[d]{p[d]-n.hi[d]}else{0.0};near+=delta*delta;}
  if near>r2{return;}
  if r2>=n.half_diagonal_sq{
   let far=std::array::from_fn(|d|(p[d]-n.lo[d]).abs().max((p[d]-n.hi[d]).abs()));
   if in_radius(&far,&[0.0;D],r){out.extend_from_slice(&self.ids[n.start..n.end]);return;}
  }
  if n.left==usize::MAX{
   for i in n.start..n.end{if in_radius(&self.points[i],p,r){out.push(self.ids[i]);}}
  }else{self.visit(n.left,p,r,r2,out);self.visit(n.right,p,r,r2,out);}
 }
}
