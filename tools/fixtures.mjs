import{writeFileSync,mkdirSync}from'node:fs';import{createHash}from'node:crypto';
const seed=Number(process.argv[2]||2051),out=process.argv[3]||'benchmarks/fixtures',holdout=process.argv[4]==='holdout';mkdirSync(out,{recursive:true});
const configs=[['small',64,16],['uniform',2000,64],['clusters',2000,48],['dense',2000,48],['contained',2000,128],['floors',2000,16],['boundaries',2000,32],['teleports',2000,64],['large',22000,64]];
if(holdout){const changes=[[47,11],[4096,37],[2048,31],[2048,67],[2048,96],[2048,23],[2048,13],[2048,89],[32768,73]];configs.forEach((row,i)=>{row[1]=changes[i][0];row[2]=changes[i][1]});}
const manifest=[];
for(const[name,n,radius]of configs){let rng=seed>>>0;const rand=()=>{rng=(Math.imul(rng,1664525)+1013904223)>>>0;return rng/4294967296};const quant=x=>Math.round(x*256)/256;
 const point=i=>{
  if(name==='clusters'){const c=i%8;return[quant((c%4)*512+rand()*(holdout?64:128)),quant(Math.floor(c/4)*512+rand()*(holdout?64:128)),quant(rand()*(holdout?48:32))];}
  if(name==='dense')return[quant(rand()*(holdout?192:96)),quant(rand()*(holdout?192:96)),quant(rand()*(holdout?48:16))];
  if(name==='contained')return[quant(rand()*16),quant(rand()*16),quant(rand()*16)];
  if(name==='floors')return[quant(rand()*256),quant(rand()*256),(i%(holdout?8:16))*(holdout?16:8)];
  if(name==='boundaries')return[(i%(holdout?43:40))*(holdout?24:32)+(rand()<.5?-.125:.125),Math.floor(i/(holdout?43:40))*(holdout?24:32)+(rand()<.5?-.125:.125),(i%(holdout?5:3))*(holdout?7.5:8)];
  const span=name==='small'?256:name==='large'?8192:holdout?8192:4096;
  return[quant(rand()*span),quant(rand()*span),quant(rand()*256)];
 };
 const points=Array.from({length:n},(_,i)=>point(i)),current=points.map(p=>p.slice()),frames=[];
 for(let t=0;t<6;t++){const updates=[];for(let k=0;k<Math.ceil(n/4);k++){
  const id=(k*3571+t*131)%n;const old=current[id];const p=name==='teleports'?point(id):old.map((v,d)=>name==='floors'&&d===2?v:quant(v+(rand()-.5)*2));current[id]=p;updates.push([id,p]);
 }
 const queries=t%3===2?Array.from({length:Math.min(n,2000)},(_,i)=>({point:current[(i*3571)%n].slice(),radius:radius*(name==='contained'?1:.5+(i%3)*.5)})):[];
 frames.push({updates,queries});}
 const fixture={name:name+(holdout?'-holdout':'')+'-seed'+seed,seed,cell_size:radius,points,frames};const bytes=JSON.stringify(fixture)+'\n',path=out+'/'+name+'.json';writeFileSync(path,bytes);manifest.push({name,path,sha256:createHash('sha256').update(bytes).digest('hex'),bytes:Buffer.byteLength(bytes),points:n,queries:Math.min(n,2000)*2});
}
writeFileSync(out+'/manifest.json',JSON.stringify({seed,fixtures:manifest},null,2)+'\n');console.log(JSON.stringify({seed,fixtures:manifest.length,totalBytes:manifest.reduce((s,x)=>s+x.bytes,0)}));
