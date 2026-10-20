import{readFileSync,writeFileSync,mkdirSync}from'node:fs';import{execFileSync}from'node:child_process';
const sources=[['nanoflann','jlblancoc/nanoflann'],['pico_tree','Jaybro/pico_tree'],['unibn-octree','jbehley/octree'],['libkdtree','jtsiomb/kdtree'],['kdtree-cpp','cdalitz/kdtree-cpp'],['KDTree','crvs/KDTree'],['gishi-kdtree','gishi523/kd-tree'],['kdtree-plusplus','nvmd/libkdtree']];
const d=JSON.parse(readFileSync('research/inventory.json','utf8'));
for(const[name,repo]of sources){try{
 const info=JSON.parse(execFileSync('gh',['api','repos/'+repo],{encoding:'utf8'}));
 const commit=JSON.parse(execFileSync('gh',['api',`repos/${repo}/commits/${info.default_branch}`],{encoding:'utf8'})).sha;
 const entry={id:'cpp:'+name,name,ecosystem:'cpp',repository:'https://github.com/'+repo,commit,license:info.license?.spdx_id,status:'pending',description:info.description,source_url:`https://api.github.com/repos/${repo}/tarball/${commit}`};
 const r=await fetch(entry.source_url,{headers:{'User-Agent':'rill-spatial-research/0.1'}});if(!r.ok)throw new Error('HTTP '+r.status);
 const data=Buffer.from(await r.arrayBuffer());const folder='research/vendor/'+name;mkdirSync(folder,{recursive:true});
 execFileSync('tar',['-xz','--strip-components=1','-C',folder],{input:data});
 if(!d.implementations.some(x=>x.id===entry.id))d.implementations.push(entry);
 console.log(name,commit);
}catch(e){console.log(name,'ERROR',String(e).slice(0,150));}}
writeFileSync('research/inventory.json',JSON.stringify(d,null,2)+'\n');
