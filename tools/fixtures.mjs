import{writeFileSync,mkdirSync}from'node:fs';import{createHash}from'node:crypto';
const seed=Number(process.argv[2]||2051),out=process.argv[3]||'benchmarks/fixtures';mkdirSync(out,{recursive:true});
const configs=[['small',64,16],['uniform',2000,64],['clusters',2000,48],['dense',2000,48],['contained',2000,128],['floors',2000,16],['boundaries',2000,32],['teleports',2000,64],['large',22000,64]];
const manifest=[];
for(const[name,n,radius]of configs){let rng=seed>>>0;const rand=()=>{rng=(Math.imul(rng,1664525)+1013904223)>>>0;return rng/4294967296};const quant=x=>Math.round(x*256)/256;
 const point=i=>{
  if(name==='clusters'){const c=i%8;return[quant((c%4)*512+rand()*128),quant(Math.floor(c/4)*512+rand()*128),quant(rand()*32)];}
  if(name==='dense')return[quant(rand()*96),quant(rand()*96),quant(rand()*16)];
  if(name==='contained')return[quant(rand()*16),quant(rand()*16),quant(rand()*16)];
  if(name==='floors')return[quant(rand()*256),quant(rand()*256),(i%16)*8];
  if(name==='boundaries')return[(i%40)*32+(rand()<.5?-.125:.125),Math.floor(i/40)*32+(rand()<.5?-.125:.125),(i%3)*8];
  const span=name==='small'?256:name==='large'?8192:4096;
  return[quant(rand()*span),quant(rand()*span),quant(rand()*256)];
 };
 const points=Array.from({length:n},(_,i)=>point(i)),current=points.map(p=>p.slice()),frames=[];
 for(let t=0;t<6;t++){const updates=[];for(let k=0;k<Math.ceil(n/4);k++){
  const id=(k*3571+t*131)%n;const old=current[id];const p=name==='teleports'?point(id):old.map((v,d)=>name==='floors'&&d===2?v:quant(v+(rand()-.5)*2));current[id]=p;updates.push([id,p]);
 }
 const queries=t%3===2?Array.from({length:Math.min(n,2000)},(_,i)=>({point:current[(i*3571)%n].slice(),radius:radius*(name==='contained'?1:.5+(i%3)*.5)})):[];
 frames.push({updates,queries});}
 const fixture={name:name+'-seed'+seed,seed,cell_size:radius,points,frames};const bytes=JSON.stringify(fixture)+'\n',path=out+'/'+name+'.json';writeFileSync(path,bytes);manifest.push({name,path,sha256:createHash('sha256').update(bytes).digest('hex'),bytes:Buffer.byteLength(bytes),points:n,queries:Math.min(n,2000)*2});
}
writeFileSync(out+'/manifest.json',JSON.stringify({seed,fixtures:manifest},null,2)+'\n');console.log(JSON.stringify({seed,fixtures:manifest.length,totalBytes:manifest.reduce((s,x)=>s+x.bytes,0)}));
