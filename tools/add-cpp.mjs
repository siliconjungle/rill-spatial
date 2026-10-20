import{readFileSync,writeFileSync,mkdirSync,existsSync}from'node:fs';import{execFileSync}from'node:child_process';import{createHash}from'node:crypto';
const sources=[['nanoflann','jlblancoc/nanoflann'],['pico_tree','Jaybro/pico_tree'],['unibn-octree','jbehley/octree'],['libkdtree','jtsiomb/kdtree'],['kdtree-cpp','cdalitz/kdtree-cpp'],['KDTree','crvs/KDTree'],['gishi-kdtree','gishi523/kd-tree'],['kdtree-plusplus','nvmd/libkdtree']];
const d=JSON.parse(readFileSync('research/inventory.json','utf8'));
for(const[name,repo]of sources){try{
 let entry=d.implementations.find(x=>x.id==='cpp:'+name);
 if(!entry){
  const info=JSON.parse(execFileSync('gh',['api','repos/'+repo],{encoding:'utf8'}));
  const commit=JSON.parse(execFileSync('gh',['api',`repos/${repo}/commits/${info.default_branch}`],{encoding:'utf8'})).sha;
  entry={id:'cpp:'+name,name,ecosystem:'cpp',repository:'https://github.com/'+repo,commit,license:info.license?.spdx_id,status:'pending',description:info.description,source_url:`https://api.github.com/repos/${repo}/tarball/${commit}`};d.implementations.push(entry);
 }
 if(!/^[0-9a-f]{40}$/.test(entry.commit))throw new Error('Full commit pin required');
 const folder='research/vendor/cpp-'+name;entry.source_directory=folder;
 if(existsSync(folder+'/.rill-source-pin')){if(readFileSync(folder+'/.rill-source-pin','utf8').trim()!==entry.commit)throw new Error('Existing source pin differs');console.log(name,'already pinned',entry.commit);continue;}
 const r=await fetch(entry.source_url,{headers:{'User-Agent':'rill-spatial-research/0.1'}});if(!r.ok)throw new Error('HTTP '+r.status);
 const data=Buffer.from(await r.arrayBuffer());entry.archive_sha256=createHash('sha256').update(data).digest('hex');mkdirSync(folder,{recursive:true});
 execFileSync('tar',['-xz','--strip-components=1','-C',folder],{input:data});writeFileSync(folder+'/.rill-source-pin',entry.commit+'\n');
 console.log(name,entry.commit);
}catch(e){console.log(name,'ERROR',String(e).slice(0,150));process.exitCode=1;}}
writeFileSync('research/inventory.json',JSON.stringify(d,null,2)+'\n');
