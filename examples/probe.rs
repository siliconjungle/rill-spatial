//! Small dependency-free profiling harness. Same binary fixtures and full oracle
//! as the comparison runner; use the comparison runner for acceptance evidence.
use rill_spatial::Index3;
use std::{hint::black_box,time::Instant};
type Point=[f64;3];
struct Reader{bytes:Vec<u8>,at:usize}
impl Reader{
 fn u32(&mut self)->usize{let v=u32::from_le_bytes(self.bytes[self.at..self.at+4].try_into().unwrap());self.at+=4;v as usize}
 fn f64(&mut self)->f64{let v=f64::from_le_bytes(self.bytes[self.at..self.at+8].try_into().unwrap());self.at+=8;v}
 fn point(&mut self)->Point{[self.f64(),self.f64(),self.f64()]}
}
fn main(){
 let args:Vec<_>=std::env::args().collect();let mut reader=Reader{bytes:std::fs::read(&args[1]).unwrap(),at:8};assert_eq!(&reader.bytes[..8],b"RILLv001");
 let name_len=reader.u32();reader.at+=name_len;let cell=reader.f64();let count=reader.u32();let frame_count=reader.u32();let points:Vec<_>=(0..count).map(|_|reader.point()).collect();
 let frames:Vec<_>=(0..frame_count).map(|_|{let count=reader.u32();let updates:Vec<_>=(0..count).map(|_|(reader.u32(),reader.point())).collect();let count=reader.u32();let queries:Vec<_>=(0..count).map(|_|(reader.point(),reader.f64())).collect();(updates,queries)}).collect();
 let mut current=points.clone();let mut expected=Vec::new();for(updates,queries)in &frames{for &(id,p)in updates{current[id]=p;}for &(centre,radius)in queries{expected.push(current.iter().enumerate().filter(|(_,p)|p.iter().zip(centre).map(|(x,y)|(x-y)*(x-y)).sum::<f64>()<=radius*radius).map(|(id,_)|id as u64).collect::<Vec<_>>());}}
 for _ in 0..args.get(2).and_then(|s|s.parse().ok()).unwrap_or(5){
  let mut outputs:Vec<Vec<u64>>=expected.iter().map(|_|Vec::new()).collect();let t=Instant::now();let mut index=Index3::from_points(cell,points.iter().enumerate().map(|(id,&p)|(id as u64,p))).unwrap();let build=t.elapsed().as_nanos();let(mut update,mut query,mut at)=(0,0,0);
  for(updates,queries)in &frames{let t=Instant::now();for &(id,p)in updates{index.upsert(id as u64,black_box(p)).unwrap();}if !queries.is_empty(){index.prepare();}update+=t.elapsed().as_nanos();let t=Instant::now();for &(p,r)in queries{index.within_into(black_box(p),black_box(r),&mut outputs[at]).unwrap();at+=1;}query+=t.elapsed().as_nanos();}
  black_box(&outputs);for(a,b)in outputs.iter_mut().zip(&expected){a.sort_unstable();assert_eq!(a,b);}println!("{{\"build_ns\":{build},\"update_ns\":{update},\"query_ns\":{query},\"total_ns\":{}}}",build+update+query);
 }
}
