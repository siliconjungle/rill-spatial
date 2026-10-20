// Same entity motion and20Hz per-observer relevance, spread across60Hz frames.
import{readFileSync,writeFileSync,mkdirSync}from'node:fs';
import{createHash}from'node:crypto';
const source=process.argv[2]||'benchmarks/twenty-k',out=process.argv[3]||'benchmarks/cohorts-twenty-k';
mkdirSync(out,{recursive:true});const manifest=[];
const cohort=id=>{let x=(id^0x243f6a88)>>>0;x=Math.imul(x^(x>>>16),0x7feb352d)>>>0;x=Math.imul(x^(x>>>15),0x846ca68b)>>>0;return((x^(x>>>16))>>>0)%3;};
for(const name of ['bounded64','crowd','contained']){
 const bytes=readFileSync(`${source}/${name}.json`),f=JSON.parse(bytes),observers=f.observer_count;
 if(f.simulation_hz!==60||f.relevance_hz!==20||f.frames.length!==6||observers!==20000)throw Error('registered20k trace required');
 const template=f.frames.find(frame=>frame.queries.length)?.queries;if(template?.length!==observers)throw Error('observer-index template required');
 const current=f.points.slice(),visits=new Uint8Array(observers);let count=0;
 for(let t=0;t<f.frames.length;t++){
  const frame=f.frames[t];for(const[id,p]of frame.updates)current[id]=p;
  if(frame.queries.length)for(let i=0;i<observers;i++)if(frame.queries[i].point.some((x,d)=>x!==current[i][d])||frame.queries[i].radius!==template[i].radius)throw Error('indexed-centre/static-radius assumption not proved');
  frame.queries=[];
  for(let id=0;id<observers;id++)if(cohort(id)===t%3){visits[id]++;count++;frame.queries.push({point:current[id].slice(),radius:template[id].radius,observer_id:id});}
 }
 if(count!==40000||visits.some(n=>n!==2))throw Error('per-observer rate mismatch');
 f.name+='-cohorts3';f.schedule={cohorts:3,assignment:'fixed hash32 observer-ID permutation modulo3',epoch:'current simulation frame; no stale index rows',queries_per_observer_per_trace:2,comparison_limit:'phase differs from bulk; same per-client rate, not same simultaneous epoch'};
 const result=JSON.stringify(f)+'\n',path=`${out}/${name}.json`;writeFileSync(path,result);
 manifest.push({name,path,source_sha256:createHash('sha256').update(bytes).digest('hex'),sha256:createHash('sha256').update(result).digest('hex'),bytes:Buffer.byteLength(result),queries:count,queries_per_frame:f.frames.map(fr=>fr.queries.length)});
}
writeFileSync(`${out}/manifest.json`,JSON.stringify({profile:'hashed-cohorts3-v1',source,fixtures:manifest},null,2)+'\n');console.log(JSON.stringify({fixtures:manifest.length,bytes:manifest.reduce((sum,m)=>sum+m.bytes,0)}));
