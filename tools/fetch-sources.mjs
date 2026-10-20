import {readFileSync,writeFileSync,mkdirSync,existsSync}from'node:fs';
import {createHash}from'node:crypto';import{spawnSync}from'node:child_process';
const inventory=JSON.parse(readFileSync('research/inventory.json','utf8'));let cursor=0;const failures=[];
mkdirSync('research/vendor',{recursive:true});
async function worker(){while(cursor<inventory.implementations.length){const e=inventory.implementations[cursor++];if(e.ecosystem!=='cargo')continue;
 const folder='research/vendor/'+e.name;if(existsSync(folder+'/Cargo.toml'))continue;
 try{const r=await fetch(e.source_url,{headers:{'User-Agent':'rill-spatial-research/0.1'}});if(!r.ok)throw new Error('HTTP '+r.status);const data=Buffer.from(await r.arrayBuffer());
 if(createHash('sha256').update(data).digest('hex')!==e.checksum)throw new Error('checksum mismatch');
 mkdirSync(folder,{recursive:true});
 const p=spawnSync('tar',['-xz','--strip-components=1','-C',folder],{input:data,encoding:'utf8'});if(p.status!==0)throw new Error(p.stderr);
 }catch(error){failures.push({id:e.id,error:String(error)})}
}}
await Promise.all(Array.from({length:4},worker));console.log(JSON.stringify({failures},null,2));
