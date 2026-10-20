#!/usr/bin/env python3
"""Run original adapters serially; retain failures and exact raw observations."""
import argparse,json,subprocess,time,random,re,platform,hashlib
from pathlib import Path
RUST=['rill-spatial','brute-force','rstar','kdtree','kd-tree','kiddo-mutable','kiddo-immutable','static-aabb2d','geo-index-kd','geo-index-rtree','spart-kd','spart-rtree','spart-rstar','spart-octree','spart-quadtree']
JS=['rbush','rbush-3d','flatbush','kdbush','static-kdtree','d3-quadtree','kd-tree-javascript','js-quadtree','@timohausmann/quadtree-js','quadtree-lib','rtree','mnemonist-vp','spatial-hash']
p=argparse.ArgumentParser();p.add_argument('--implementations',default=','.join(RUST+JS));p.add_argument('--scenarios',default='small,uniform,clusters,dense,contained,floors,boundaries,teleports,large');p.add_argument('--samples',type=int,default=5);p.add_argument('--processes',type=int,default=3);p.add_argument('--out',default='results/comparison.json');p.add_argument('--fixtures',default='benchmarks/fixtures');p.add_argument('--seed',type=int,default=17);a=p.parse_args()
out=Path(a.out);out.parent.mkdir(parents=True,exist_ok=True)
doc={'schema':1,'status':'INCOMPLETE','host':platform.platform(),'machine':platform.machine(),'notes':['Other user workers left running; shared-machine diagnostics','Process RSS includes reference oracle and retained result vectors; not isolated index bytes'],'samples_per_process':a.samples,'requested_processes':a.processes,'rows':[]}
jobs=[(i,s,n)for i in range(a.processes)for s in a.scenarios.split(',')for n in a.implementations.split(',')];random.Random(a.seed).shuffle(jobs)
for ordinal,(process,scenario,name)in enumerate(jobs):
 fixture=Path(a.fixtures)/(scenario+'.json')
 if name in RUST:cmd=['build/target/release/rill-spatial-benchmarks',name,str(fixture),str(a.samples)]
 elif name in JS:cmd=['node','benchmarks/javascript.mjs',name,str(fixture),str(a.samples)]
 else:cmd=['build/cpp-'+name,str(fixture.with_suffix('.bin')),str(a.samples)]
 start=time.monotonic()
 try:
  r=subprocess.run(['/usr/bin/time','-l']+cmd,text=True,capture_output=True,timeout=300)
  if r.returncode:row={'implementation':name,'scenario':scenario,'status':'failed','exit_code':r.returncode,'error':r.stderr[-3500:]}
  else:row=json.loads(r.stdout);row['status']='measured';row['process_peak_rss_bytes']=int(re.search(r'(\d+)\s+maximum resident set size',r.stderr)[1])
 except Exception as e:row={'implementation':name,'scenario':scenario,'status':'failed','error':str(e)}
 row.update({'process':process,'command':cmd,'fixture_sha256':hashlib.sha256(fixture.read_bytes()).hexdigest(),'process_wall_seconds':time.monotonic()-start});doc['rows'].append(row)
 out.write_text(json.dumps(doc,indent=2)+'\n')
 if row['status']=='measured':
  values=sorted(x['total_ns']for x in row['samples']);print(f'{ordinal+1}/{len(jobs)} {name} {scenario}: {values[len(values)//2]/1e6:.3f} ms',flush=True)
 else:print(f'{ordinal+1}/{len(jobs)} {name} {scenario}: FAILED {str(row.get("error",""))[:100]}',flush=True)
doc['status']='COMPLETE_WITH_FAILURES'if any(r['status']!='measured'for r in doc['rows'])else'COMPLETE';out.write_text(json.dumps(doc,indent=2)+'\n')
