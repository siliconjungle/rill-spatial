import {writeFileSync,existsSync,readFileSync} from 'node:fs';
const rust=['rstar','kiddo','kdtree','kd-tree','bosque','nabo','vpsearch','vptree','spart','spatialtree','quadtree_rs','quadtree-f32','quadtree','octree','octree-rs','spatial','spatial-index','spatial_hash','spatialhash','collision','aabb-quadtree','static_aabb2d_index','flatbush','geo-index','kdt','fnntw','neighbourhood','petal-neighbors','sif','acap','abd-clam'];
const npm=['rbush','flatbush','kdbush','static-kdtree','kd-tree-javascript','quadtree-lib','d3-quadtree','js-quadtree','@timohausmann/quadtree-js','quadtree-ts','quadtree2','quadtree','spatial-hash','spatialhash','hash-grid','box-intersect','rbush-3d','bush','rtree','rtree-2d','@flatten-js/core','mnemonist'];
const jobs=[...rust.map(name=>({name,ecosystem:'cargo'})),...npm.map(name=>({name,ecosystem:'npm'}))];
const rows=[];let cursor=0;
async function worker(){while(cursor<jobs.length){const j=jobs[cursor++];try{
 const url=j.ecosystem==='cargo'?`https://crates.io/api/v1/crates/${encodeURIComponent(j.name)}`:`https://registry.npmjs.org/${encodeURIComponent(j.name)}`;
 const response=await fetch(url,{headers:{'User-Agent':'rill-spatial-comparison/0.1 (public reproducible research)'}});
 if(!response.ok){rows.push({...j,status:'discovery-failed',http:response.status});continue;}
 const d=await response.json();
 if(j.ecosystem==='cargo'){
  const version=d.versions.find(v=>!v.yanked&&!v.num.includes('-'));
  rows.push({...j,id:`cargo:${j.name}`,version:version.num,checksum:version.checksum,repository:d.crate.repository,description:d.crate.description,license:version.license,status:'pending',source_url:`https://crates.io/api/v1/crates/${j.name}/${version.num}/download`});
 }else{
  const version=d['dist-tags'].latest;const v=d.versions[version];
  rows.push({...j,id:`npm:${j.name}`,version,integrity:v.dist.integrity,repository:typeof v.repository==='string'?v.repository:v.repository?.url,description:v.description,license:v.license,status:'pending',source_url:v.dist.tarball});
 }
}catch(e){rows.push({...j,status:'discovery-failed',error:String(e)})}}}
await Promise.all(Array.from({length:4},worker));rows.sort((a,b)=>a.ecosystem.localeCompare(b.ecosystem)||a.name.localeCompare(b.name));
writeFileSync('research/discovery.json',JSON.stringify({retrieved_at:new Date().toISOString(),sources:rows},null,2)+'\n');
const existing=existsSync('research/inventory.json')?JSON.parse(readFileSync('research/inventory.json','utf8')).implementations:[];
const implementations=rows.filter(x=>x.status==='pending').map(x=>existing.find(e=>e.id===x.id)||x);
writeFileSync('research/inventory.json',JSON.stringify({schema:1,contract:'exact-radius-reporting-f64-2d-3d-v1',gate:'OPEN',implementations},null,2)+'\n');
console.log(JSON.stringify({found:implementations.length,failed:rows.filter(x=>x.status!=='pending').map(x=>x.name)},null,2));
