import{readFileSync}from'node:fs';import{createRequire}from'node:module';import{performance}from'node:perf_hooks';import assert from'node:assert/strict';
const require=createRequire(new URL('../research/npm/package.json',import.meta.url));
const name=process.argv[2],fixture=JSON.parse(readFileSync(process.argv[3],'utf8')),sampleCount=Number(process.argv[4]||5);
const squared=(a,b)=>{const x=a[0]-b[0],y=a[1]-b[1],z=a[2]-b[2];return x*x+y*y+z*z};
const toBox=(p,id)=>({minX:p[0],minY:p[1],maxX:p[0],maxY:p[1],minZ:p[2],maxZ:p[2],id});
const square=(p,r)=>({minX:p[0]-r-1e-9,minY:p[1]-r-1e-9,maxX:p[0]+r+1e-9,maxY:p[1]+r+1e-9,minZ:p[2]-r-1e-9,maxZ:p[2]+r+1e-9});
const moduleImport=async name=>import(new URL('../research/npm/node_modules/'+name+'/index.js',import.meta.url));
let make;
if(name==='rbush'||name==='rbush-3d'){
 const mod=name==='rbush'?await moduleImport(name):require(name);const Tree=mod.default||mod.RBush3D||mod;
 make=points=>{let current=points.slice(),items=current.map(toBox),tree=new Tree().load(items);return{
  update(updates){for(const[id,p]of updates){tree.remove(items[id]);current[id]=p;items[id]=toBox(p,id);tree.insert(items[id]);}},
  query(p,r,out){out.length=0;for(const item of tree.search(square(p,r)))if(squared(current[item.id],p)<=r*r)out.push(item.id);}
 }};
}else if(name==='flatbush'||name==='kdbush'){
 const {default:Tree}=await moduleImport(name);
 make=points=>{let current=points.slice(),tree,dirty=false;const rebuild=()=>{tree=new Tree(current.length);for(const p of current)name==='flatbush'?tree.add(p[0],p[1],p[0],p[1]):tree.add(p[0],p[1]);tree.finish();};rebuild();return{
  update(updates){for(const[id,p]of updates)current[id]=p;dirty||=updates.length>0;},prepare(){if(dirty){rebuild();dirty=false;}},
  query(p,r,out){out.length=0;const b=square(p,r),ids=name==='flatbush'?tree.search(b.minX,b.minY,b.maxX,b.maxY):tree.within(p[0],p[1],r+1e-9);for(const id of ids)if(squared(current[id],p)<=r*r)out.push(id);}
 }};
}else if(name==='static-kdtree'){
 const create=require(name);make=points=>{let current=points.slice(),tree=create(current),dirty=false;return{
  update(u){for(const[id,p]of u)current[id]=p;dirty||=u.length>0;},prepare(){if(dirty){tree.dispose();tree=create(current);dirty=false;}},
  query(p,r,out){out.length=0;tree.rnn(p,r+1e-9,id=>{if(squared(current[id],p)<=r*r)out.push(id);});},dispose(){tree.dispose();}
 }};
}else if(name==='d3-quadtree'){
 const{quadtree}=await moduleImport(name+'/src');make=points=>{let current=points.slice(),items=current.map((p,id)=>({p,id})),tree=quadtree(items,d=>d.p[0],d=>d.p[1]);return{
  update(u){for(const[id,p]of u){tree.remove(items[id]);current[id]=p;items[id]={p,id};tree.add(items[id]);}},
  query(p,r,out){out.length=0;tree.visit((node,x0,y0,x1,y1)=>{if(x0>p[0]+r+1e-9||x1<p[0]-r-1e-9||y0>p[1]+r+1e-9||y1<p[1]-r-1e-9)return true;if(!node.length)do{const q=node.data;if(squared(q.p,p)<=r*r)out.push(q.id);}while(node=node.next);return false;});}
 }};
}else if(name==='kd-tree-javascript'){
 const{kdTree}=require(name);make=points=>{let current=points.slice(),tree,dirty=false;const rebuild=()=>{const items=current.map((p,id)=>Object.assign(p.slice(),{id}));tree=new kdTree(items,squared,[0,1,2]);};rebuild();return{
  // Upstream remove searches only one side of tied split coordinates and can
  // leave stale IDs. Preserve the native tree/query through coalesced rebuilds.
  update(u){for(const[id,p]of u)current[id]=p;dirty||=u.length>0;},prepare(){if(dirty){rebuild();dirty=false;}},
  query(p,r,out){out.length=0;for(const[q]of tree.nearest(p,current.length,r*r+1e-9))if(squared(q,p)<=r*r)out.push(q.id);}
 }};
}else if(name==='js-quadtree'){
 const{QuadTree,Box,Point}=require(name);make=points=>{let current=points.slice(),items=current.map((p,id)=>new Point(p[0],p[1],id));const tree=new QuadTree(new Box(-1e6,-1e6,2e6,2e6),{capacity:16,arePointsEqual:(a,b)=>a.data===b.data},items);return{
  update(u){for(const[id,p]of u){tree.remove(items[id]);current[id]=p;items[id]=new Point(p[0],p[1],id);tree.insert(items[id]);}},
  query(p,r,out){out.length=0;for(const q of tree.query(new Box(p[0]-r-1e-9,p[1]-r-1e-9,2*r+2e-9,2*r+2e-9)))if(squared(current[q.data],p)<=r*r)out.push(q.data);}
 }};
}else if(name==='@timohausmann/quadtree-js'){
 const Tree=require(name);make=points=>{let current=points.slice(),tree,dirty=false;const rebuild=()=>{tree=new Tree({x:-1e6,y:-1e6,width:2e6,height:2e6},16,16);current.forEach((p,id)=>tree.insert({x:p[0],y:p[1],width:1e-9,height:1e-9,id}));};rebuild();return{
  update(u){for(const[id,p]of u)current[id]=p;dirty||=u.length>0;},prepare(){if(dirty){rebuild();dirty=false;}},
  query(p,r,out){out.length=0;const seen=new Set();for(const q of tree.retrieve({x:p[0]-r-1e-9,y:p[1]-r-1e-9,width:2*r+2e-9,height:2*r+2e-9}))if(!seen.has(q.id)&&squared(current[q.id],p)<=r*r){seen.add(q.id);out.push(q.id);}}
 }};
}else if(name==='quadtree-lib'){
 const Tree=require(name);make=points=>{let current=points.slice(),tree,dirty=false;const rebuild=()=>{tree=new Tree({x:-1e6,y:-1e6,width:2e6,height:2e6,maxElements:16});tree.pushAll(current.map((p,id)=>({x:p[0],y:p[1],width:1e-9,height:1e-9,id})));};rebuild();return{
  update(u){for(const[id,p]of u)current[id]=p;dirty||=u.length>0;},prepare(){if(dirty){rebuild();dirty=false;}},
  query(p,r,out){out.length=0;for(const q of tree.colliding({x:p[0]-r-1e-9,y:p[1]-r-1e-9,width:2*r+2e-9,height:2*r+2e-9}))if(squared(current[q.id],p)<=r*r)out.push(q.id);}
 }};
}else if(name==='rtree'){
 const create=require(name),rect=p=>({x:p[0],y:p[1],w:0,h:0});make=points=>{let current=points.slice(),items=current.map((p,id)=>({id})),tree=create(16);current.forEach((p,id)=>tree.insert(rect(p),items[id]));return{
  update(u){for(const[id,p]of u){tree.remove(rect(current[id]),items[id]);current[id]=p;tree.insert(rect(p),items[id]);}},
  query(p,r,out){out.length=0;for(const q of tree.search({x:p[0]-r-1e-9,y:p[1]-r-1e-9,w:2*r+2e-9,h:2*r+2e-9}))if(squared(current[q.id],p)<=r*r)out.push(q.id);}
 }};
}else if(name==='mnemonist-vp'){
 const Tree=require('mnemonist/vp-tree.js');make=points=>{let current=points.slice(),tree,dirty=false;const rebuild=()=>{tree=new Tree((a,b)=>Math.sqrt(squared(a.p,b.p)),current.map((p,id)=>({p,id})));};rebuild();return{
  update(u){for(const[id,p]of u)current[id]=p;dirty||=u.length>0;},prepare(){if(dirty){rebuild();dirty=false;}},
  query(p,r,out){out.length=0;for(const q of tree.neighbors(r+1e-9,{p}))if(squared(q.item.p,p)<=r*r)out.push(q.item.id);}
 }};
}else if(name==='spatial-hash'){
 const Tree=require(name);make=points=>{let current=points.slice(),items=current.map((p,id)=>({range:{x:p[0],y:p[1],width:1e-9,height:1e-9},id})),tree=new Tree({x:-16384,y:-16384,width:Math.ceil(32768/fixture.cell_size)*fixture.cell_size,height:Math.ceil(32768/fixture.cell_size)*fixture.cell_size},fixture.cell_size);items.forEach(x=>tree.insert(x));return{
  update(u){for(const[id,p]of u){current[id]=p;items[id].range.x=p[0];items[id].range.y=p[1];tree.update(items[id]);}},
  query(p,r,out){out.length=0;for(const q of tree.query({x:p[0]-r-1e-9,y:p[1]-r-1e-9,width:2*r+2e-9,height:2*r+2e-9}))if(squared(current[q.id],p)<=r*r)out.push(q.id);}
 }};
}else if(name==='flatten-planar'){
 const F=require('@flatten-js/core');make=points=>{let current=points.slice(),items=current.map((p,id)=>Object.assign(new F.Point(p[0],p[1]),{id})),tree=new F.PlanarSet();items.forEach(x=>tree.add(x));return{
  update(u){for(const[id,p]of u){tree.delete(items[id]);current[id]=p;items[id]=Object.assign(new F.Point(p[0],p[1]),{id});tree.add(items[id]);}},
  query(p,r,out){out.length=0;for(const q of tree.search(new F.Box(p[0]-r-1e-9,p[1]-r-1e-9,p[0]+r+1e-9,p[1]+r+1e-9)))if(squared(current[q.id],p)<=r*r)out.push(q.id);}
 }};
}else if(name==='box-intersect'){
 const intersect=require(name),box=p=>p.concat(p);make=points=>{let current=points.slice(),boxes=current.map(box);return{
  update(u){for(const[id,p]of u){current[id]=p;boxes[id]=box(p);}},
  queryBatch(queries,outputs){outputs.forEach(out=>out.length=0);const qboxes=queries.map(q=>[...q.point.map(x=>x-q.radius-1e-9),...q.point.map(x=>x+q.radius+1e-9)]);intersect(boxes,qboxes,(id,q)=>{if(squared(current[id],queries[q].point)<=queries[q].radius**2)outputs[q].push(id);});}
 }};
}else if(name==='quadtree2'){
 const Tree=require(name),Vec2=require('vec2');make=points=>{let current=points.slice(),tree=new Tree({size:new Vec2(32768,32768),objectLimit:16,levelLimit:12}),items=current.map((p,id)=>({id:id+1,pos:new Vec2(p[0]+16384,p[1]+16384),rad:1e-9}));tree.addObjects(items);return{
  update(u){for(const[id,p]of u){current[id]=p;items[id].pos.x=p[0]+16384;items[id].pos.y=p[1]+16384;tree.updateObject(items[id]);}},
  query(p,r,out){out.length=0;const query={id:current.length+1,pos:new Vec2(p[0]+16384,p[1]+16384),rad:r+1e-9};tree.addObject(query);const found=tree.getCollidables(query);for(const key of Object.keys(found)){const id=found[key].id-1;if(squared(current[id],p)<=r*r)out.push(id);}tree.removeObject(query);}
 }};
}else if(name==='mnemonist-kd'){
 const Tree=require('mnemonist/kd-tree.js');make=points=>{let current=points.slice(),tree,dirty=false;const rebuild=()=>{tree=Tree.from(current.map((p,id)=>[id,p]),3);};rebuild();return{
  update(u){for(const[id,p]of u)current[id]=p;dirty||=u.length>0;},prepare(){if(dirty){rebuild();dirty=false;}},
  query(p,r,out){out.length=0;for(const id of tree.kNearestNeighbors(current.length,p))if(squared(current[id],p)<=r*r)out.push(id);}
 }};
}else throw new Error('No adapter for '+name);

function boundedExpected(points,p,r,lo,hi){
 const far=p.map((x,d)=>Math.max(Math.abs(x-lo[d]),Math.abs(x-hi[d])));if(squared(far,[0,0,0])<=r*r)return{all:true,count:points.length};
 const words=Math.ceil(points.length/32);let ids=[],bits=null,count=0;
 for(let i=0;i<points.length;i++)if(squared(points[i],p)<=r*r){count++;if(bits)bits[i>>>5]|=1<<(i&31);else{ids.push(i);if(ids.length>words/2){bits=new Uint32Array(words);for(const id of ids)bits[id>>>5]|=1<<(id&31);ids=null;}}}
 return{ids,bits,count};
}
function boundedValidate(expected,actual,seen,n){
 assert.equal(actual.length,expected.count,'result count');seen.fill(0);
 for(const id of actual){assert(Number.isSafeInteger(id)&&id>=0&&id<n,'ID range');const mask=1<<(id&31),word=id>>>5;assert.equal(seen[word]&mask,0,'duplicate ID');seen[word]|=mask;if(expected.bits)assert.notEqual(expected.bits[word]&mask,0,'unexpected ID');}
 if(expected.ids)for(const id of expected.ids)assert.notEqual(seen[id>>>5]&(1<<(id&31)),0,'missing ID');
}
function runBounded(){
 const window=Number(process.env.RILL_OUTPUT_WINDOW||64);assert(Number.isInteger(window)&&window>=1&&window<=64);
 assert(fixture.points.length<=100000&&fixture.frames.every(f=>f.queries.length<=20000),'bounded runner admission');
 assert(fixture.points.flat().every(x=>Number.isFinite(x)&&Math.abs(x)<=1e6));
 assert(fixture.frames.every(f=>f.updates.every(([id,p])=>id>=0&&id<fixture.points.length&&p.every(x=>Number.isFinite(x)&&Math.abs(x)<=1e6))&&f.queries.every(q=>q.point.every(x=>Number.isFinite(x)&&Math.abs(x)<=1e6)&&q.radius>=1e-100&&q.radius<=1e100)));
 const samples=[],matchCounts=[];let peakLogicalOutput=0,validationMs=0;
 for(let sample=0;sample<sampleCount;sample++){
  const reference=fixture.points.slice(),seen=new Uint32Array(Math.ceil(reference.length/32));let start=performance.now(),index=make(fixture.points),build_ns=Math.round((performance.now()-start)*1e6),update_ns=0,query_ns=0,results=0;
  for(const frame of fixture.frames){start=performance.now();index.update(frame.updates);if(frame.queries.length)index.prepare?.();update_ns+=Math.round((performance.now()-start)*1e6);for(const[id,p]of frame.updates)reference[id]=p;if(!frame.queries.length)continue;
   const lo=[Infinity,Infinity,Infinity],hi=[-Infinity,-Infinity,-Infinity];for(const p of reference)for(let d=0;d<3;d++){lo[d]=Math.min(lo[d],p[d]);hi[d]=Math.max(hi[d],p[d]);}
   start=performance.now();const outputs=Array.from({length:window},()=>[]);let excluded=0;
   for(let begin=0;begin<frame.queries.length;begin+=window){const queries=frame.queries.slice(begin,begin+window),out=outputs.slice(0,queries.length);
    if(index.queryBatch)index.queryBatch(queries,out);else for(let i=0;i<queries.length;i++)index.query(queries[i].point,queries[i].radius,out[i]);
    const pause=performance.now();for(let i=0;i<queries.length;i++){const q=queries[i];boundedValidate(boundedExpected(reference,q.point,q.radius,lo,hi),out[i],seen,reference.length);results+=out[i].length;if(!sample)matchCounts.push(out[i].length);}peakLogicalOutput=Math.max(peakLogicalOutput,out.reduce((sum,v)=>sum+v.length*8,0));excluded+=performance.now()-pause;
   }
   query_ns+=Math.round((performance.now()-start-excluded)*1e6);validationMs+=excluded;
  }
  index.dispose?.();samples.push({build_ns,update_ns,query_ns,total_ns:build_ns+update_ns+query_ns,results});
 }
 matchCounts.sort((a,b)=>a-b);console.log(JSON.stringify({implementation:name,scenario:fixture.name,points:fixture.points.length,queries:matchCounts.length,correct:true,runtime:process.version,output_window_queries:window,materialization:'bounded reusable full-query number arrays; every ID produced and full membership validated',timing_scope:'summed kernel service time, oracle pauses excluded; GC may cross pauses; not uninterrupted tick wall latency',output_logical_peak_bytes:peakLogicalOutput,excluded_validation_ns:Math.round(validationMs*1e6),matches_per_query:{mean:matchCounts.reduce((a,b)=>a+b,0)/matchCounts.length,p99:matchCounts[Math.floor(matchCounts.length*.99)],max:matchCounts.at(-1)},samples}));
}

if(fixture.bounded){runBounded();}else{
const expected=[];{const points=fixture.points.slice();for(const frame of fixture.frames){for(const[id,p]of frame.updates)points[id]=p;for(const q of frame.queries){const ids=[];for(let i=0;i<points.length;i++)if(squared(points[i],q.point)<=q.radius*q.radius)ids.push(i);expected.push(ids);}}}
const samples=[];
for(let sample=0;sample<sampleCount;sample++){
 const outputs=Array.from({length:expected.length},()=>[]);
 let start=performance.now(),index=make(fixture.points),build_ns=Math.round((performance.now()-start)*1e6),update_ns=0,query_ns=0,cursor=0;
 for(const frame of fixture.frames){start=performance.now();index.update(frame.updates);if(frame.queries.length)index.prepare?.();update_ns+=Math.round((performance.now()-start)*1e6);start=performance.now();if(index.queryBatch&&frame.queries.length){index.queryBatch(frame.queries,outputs.slice(cursor,cursor+frame.queries.length));cursor+=frame.queries.length;}else for(const q of frame.queries)index.query(q.point,q.radius,outputs[cursor++]);query_ns+=Math.round((performance.now()-start)*1e6);}
 let results=0;for(let i=0;i<outputs.length;i++){outputs[i].sort((a,b)=>a-b);assert.deepEqual(outputs[i],expected[i],`${name} ${fixture.name} query ${i}`);results+=outputs[i].length;}
 index.dispose?.();samples.push({build_ns,update_ns,query_ns,total_ns:build_ns+update_ns+query_ns,results});
}
console.log(JSON.stringify({implementation:name,scenario:fixture.name,points:fixture.points.length,queries:expected.length,correct:true,runtime:process.version,samples}));

}
