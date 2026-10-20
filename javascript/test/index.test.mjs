import test from 'node:test';
import assert from 'node:assert/strict';
import {PackedPointIndex} from '../index.mjs';

function check(values,queries,options={}){
  const index=new PackedPointIndex(values,options),position=options.position??(v=>v?.pos??v),seen=new Set();
  const rows=values.filter(v=>{const p=position(v);if(!p||p.length!==3||!p.every(Number.isFinite))return false;if(options.unique){const key=p.join(',');if(seen.has(key))return false;seen.add(key);}return true;});
  assert.equal(index.size,rows.length);
  for(const [o,r,h]of queries){
    let near=null,distance=Infinity,near3=null,squared=Infinity;
    for(const v of rows){const p=position(v),d=Math.hypot(p[0]-o[0],p[2]-o[2]),q=(o[0]-p[0])**2+(o[1]-p[1])**2+(o[2]-p[2])**2;
      if(Math.abs(p[1]-o[1])<h&&d<distance){near=v;distance=d;}
      if(q<squared){near3=v;squared=q;}
    }
    const found=index.nearestXZ(o,h),found3=index.nearest3D(o);
    assert.equal(found?.value??null,near);if(found)assert.equal(found.distance,distance);
    assert.equal(found3?.value??null,near3);if(found3)assert.equal(found3.distance,squared);
    assert.equal(index.nearXZ(o,r),rows.some(v=>{const p=position(v);return(p[0]-o[0])**2+(p[2]-o[2])**2<=r*r;}));
    for(const [method,predicate]of [
      ['withinXZ',p=>Math.hypot(p[0]-o[0],p[2]-o[2])<r],
      ['within3D',p=>Math.hypot(p[0]-o[0],p[1]-o[1],p[2]-o[2])<r],
      ['withinBox',p=>p.every((v,i)=>Math.abs(v-o[i])<=r)]
    ]){const expected=rows.filter(v=>predicate(position(v))),actual=index[method](o,r);assert.equal(actual.length,expected.length,method);for(let i=0;i<expected.length;i++)assert.equal(actual[i],expected[i],method+' identity/order');}
  }
}

test('packed leaves and median partitions match independent ordered scans',()=>{
  let s=71923;const R=()=>{s=(Math.imul(s,1664525)+1013904223)>>>0;return s/4294967296;};
  for(const n of[0,1,33,128,2000])for(const leafSize of[1,4,16,24,32]){
    const points=Array.from({length:n},(_,i)=>({id:BigInt(i)+2n**60n,pos:[Math.round((R()-.5)*200),Math.floor(R()*6)*4,Math.round((R()-.5)*200)]}));
    const queries=Array.from({length:35},()=>[[(R()-.5)*200,R()*24,(R()-.5)*200],R()*80,R()<.5?4:Infinity]);check(points,queries,{leafSize});
  }
});
test('strict spherical edges, inclusive boxes and nearest ties preserve original identity',()=>{
  const next=r=>{const a=new BigUint64Array(new Float64Array([r]).buffer);a[0]++;return new Float64Array(a.buffer)[0];};
  const p=[[3,0,4],[-3,0,-4],[0,4,0],[0,-4,0],[3,0,4],[0,0,0]];
  for(const leafSize of[1,4,24])for(const unique of[false,true])check(p,[[[0,0,0],5,4],[[0,0,0],next(5),next(4)],[[3,0,4],0,Infinity]],{leafSize,unique});
});
test('finite extremes and underflow retain JavaScript predicates',()=>{
  const points=[[Number.MAX_VALUE,0,0],[-Number.MAX_VALUE,0,0],[1e300,1e300,1e300],[1e-300,0,0],[Number.MIN_VALUE,0,0],[0,0,0]];
  const q=[];for(const p of[[0,0,0],[Number.MAX_VALUE,0,0],[-Number.MAX_VALUE,0,0]])for(const r of[0,Number.MIN_VALUE,1e-300,1,1e300,Infinity])q.push([p,r,Infinity]);
  for(const leafSize of[1,4,24])check(points,q,{leafSize});
});
test('invalid rows, typed coordinates, signed zeros and duplicate filtering are captured once',()=>{
  const values=[{at:new Float64Array([0,0,0])},{at:[-0,0,0]},{at:[NaN,0,0]},{at:[Infinity,0,0]},{at:[1,2]},{at:[3,0,4]},null];
  const position=v=>v?.at;for(const unique of[false,true])check(values,[[[0,0,0],5,4],[[0,0,0],6,Infinity]],{position,unique});
  const point={pos:[1,0,0]},index=new PackedPointIndex([point]);point.pos[0]=100;
  assert.equal(index.nearestXZ([0,0,0]).distance,1);assert.equal(index.withinXZ([0,0,0],2)[0],point);
  const out=index.withinBox([0,0,0],5);out.pop();assert.equal(index.withinBox([0,0,0],5).length,1);
});
test('axis choices and input permutations preserve nearest ties by input order',()=>{
  const p=Array.from({length:90},(_,i)=>[i%3-1,(i%6)*4,Math.floor(i/3)%3-1]);
  for(const splitAxes of[[0,1,2],[0,2],[1]])for(const points of[p,p.slice().reverse()])check(points,[[[0,0,0],3,4],[[.5,4,.5],1,Infinity]],{splitAxes,leafSize:4});
});
test('invalid resource options fail explicitly',()=>{
  for(const leafSize of[0,1.5,1025,NaN])assert.throws(()=>new PackedPointIndex([],{leafSize}),RangeError);
  for(const splitAxes of[[],[3],[-1],[.5]])assert.throws(()=>new PackedPointIndex([],{splitAxes}),RangeError);
});

test('height pruning retains coincident heights when the tolerance is below an origin ULP',()=>{
 const points=Array.from({length:70},(_,i)=>[i,1e16+(i%3)*4,0]);
 for(const leafSize of[1,16,24])check(points,[[[0,1e16,0],5,.1],[[3,1e16,0],5,.1]],{leafSize});
});

test('bounded ordered spheres equal independent filtered prefixes with exclusions',()=>{
 let seed=337;const R=()=>(seed=(Math.imul(seed,1664525)+1013904223)>>>0)/4294967296;
 for(const n of[0,1,32,1000])for(const leafSize of[1,16,24]){
  const values=Array.from({length:n},(_,id)=>({id,pos:[Math.round(R()*30)-15,Math.round(R()*12)-6,Math.round(R()*30)-15]}));
  const index=new PackedPointIndex(values,{leafSize}),exclude=v=>v.id%3===0;
  for(let query=0;query<20;query++){
   const o=[R()*30-15,R()*12-6,R()*30-15],radius=query%5===0?Infinity:query,rows=values.filter(v=>!exclude(v)&&Math.hypot(...v.pos.map((x,i)=>x-o[i]))<radius);
   for(const limit of[0,1,4,17,2000])assert.deepEqual(index.firstWithin3D(o,radius,limit,{exclude}),rows.slice(0,limit));
  }
 }
 const values=[{id:0,pos:[3,0,4]},{id:1,pos:[0,0,0]},{id:2,pos:[0,0,0]}],index=new PackedPointIndex(values,{leafSize:1});
 assert.deepEqual(index.firstWithin3D([0,0,0],5,1),[values[1]]);
 assert.deepEqual(index.firstWithin3D([0,0,0],5,10),values.slice(1));
 for(const limit of[-1,NaN,1.5])assert.deepEqual(index.firstWithin3D([0,0,0],5,limit),[]);
 for(const radius of[0,-1,NaN])assert.deepEqual(index.firstWithin3D([0,0,0],radius,2),[]);
});
