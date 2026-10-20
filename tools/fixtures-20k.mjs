import{writeFileSync,mkdirSync}from'node:fs';import{createHash}from'node:crypto';
const seed=Number(process.argv[2]||20671),out=process.argv[3]||'benchmarks/twenty-k';mkdirSync(out,{recursive:true});
const selected=(process.argv[4]||'uniform,bounded64,crowd,contained,floors,teleports,many-objects').split(',');
const manifest=[];
for(const name of selected){
 let state=seed>>>0;const rand=()=>{state=(Math.imul(state,1664525)+1013904223)>>>0;return state/4294967296};const quant=x=>Math.round(x*256)/256;
 const n=name==='many-objects'?100000:20000,observers=20000,movable=20000;
 const radius=({uniform:64,bounded64:256,crowd:48,contained:128,floors:32,teleports:256,'many-objects':128})[name];if(!radius)throw Error('unknown scenario '+name);
 const point=i=>{
  if(name==='contained')return[quant(rand()*16),quant(rand()*16),quant(rand()*16)];
  if(name==='crowd')return[quant(rand()*512),quant(rand()*512),quant(rand()*32)];
  if(name==='floors')return[quant(rand()*1024),quant(rand()*1024),(i%32)*8];
  const span=name==='uniform'?4096:8192,z=name==='uniform'?256:64;
  return[quant(rand()*span),quant(rand()*span),quant(rand()*z)];
 };
 const points=Array.from({length:n},(_,i)=>point(i)),current=points.map(p=>p.slice()),frames=[];
 for(let t=0;t<6;t++){
  const updates=[];for(let k=0;k<movable/4;k++){
   const id=(k*3571+t*131)%movable,old=current[id];
   const p=name==='teleports'?point(id):old.map((v,d)=>name==='floors'&&d===2?v:quant(v+(rand()-.5)*2));current[id]=p;updates.push([id,p]);
  }
  const queries=t%3===2?Array.from({length:observers},(_,i)=>({point:current[i].slice(),radius:radius*(name==='contained'?1:.5+(i%3)*.5)})):[];
  frames.push({updates,queries});
 }
 const fixture={name:name+'-20k-seed'+seed,bounded:true,seed,cell_size:radius,simulation_hz:60,relevance_hz:20,observer_count:observers,movable_points:movable,points,frames};
 const bytes=JSON.stringify(fixture)+'\n',path=out+'/'+name+'.json';writeFileSync(path,bytes);
 manifest.push({name,path,sha256:createHash('sha256').update(bytes).digest('hex'),bytes:Buffer.byteLength(bytes),points:n,observers,queries:observers*2});
}
writeFileSync(out+'/manifest.json',JSON.stringify({seed,profile:'20k-v1',fixtures:manifest},null,2)+'\n');console.log(JSON.stringify({seed,fixtures:manifest.length,bytes:manifest.reduce((s,x)=>s+x.bytes,0)}));
