#!/usr/bin/env python3
"""Run pinned adapters serially; retain failures and exact raw observations."""
import argparse,json,subprocess,time,random,re,platform,hashlib,shutil,datetime,os
from pathlib import Path
RUST=['rill-spatial','brute-force','rstar','kdtree','kd-tree','kiddo-mutable','kiddo-immutable','static-aabb2d','geo-index-kd','geo-index-rtree','spart-kd','spart-rtree','spart-rstar','spart-octree','spart-quadtree']
RUST+=['acap-kd','acap-flat-kd','acap-vp','acap-flat-vp','acap-exhaustive','vpsearch','petal-ball','neighbourhood-owned','neighbourhood-index','quadtree-f64','quadtree-rs','rust-octree','spatial-linear','spatial-quadratic','spatial-rstar','nabo']
JS=['rbush','rbush-3d','flatbush','kdbush','static-kdtree','d3-quadtree','kd-tree-javascript','js-quadtree','@timohausmann/quadtree-js','quadtree-lib','rtree','mnemonist-vp','spatial-hash','flatten-planar','box-intersect','quadtree2','mnemonist-kd']
VARIANTS=['rill-auto-1','rill-auto-4','rill-radius-1','rill-radius-4','rill-parallel-1','rill-parallel-2','rill-parallel-4','study-kiddo-1','study-kiddo-4','study-static-aabb-1','study-static-aabb-4','study-geo-rtree-1','study-geo-rtree-4','rill-batch-1','rill-batch-2','rill-batch-4','rill-coherent-1','rill-coherent-2','rill-coherent-4','rill-cell-sorted','rill-cell-hybrid','rill-cell-hash','rill-cell-occupied','rill-dual','rill-cell-batch-sorted','rill-cell-batch-hybrid','rill-cell-batch-hash','rill-cell-batch-occupied']
CPP=['kdtree-plusplus','nanoflann','pico-tree','libkdtree','kdtree-cpp','crvs-kdtree','gishi-kdtree']
def digest(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def output(cmd):return subprocess.check_output(cmd,text=True).strip()
def identity():
 files=output(['git','ls-files','--cached','--others','--exclude-standard']).splitlines()
 files=[f for f in files if Path(f).is_file() and not f.startswith('results/')]
 return {'git_commit':output(['git','rev-parse','HEAD']),'git_status':output(['git','status','--short']),'source_sha256':{f:digest(f)for f in files},'toolchains':{n:output(c)for n,c in {'rust':['rustc','--version'],'node':['node','--version'],'clang':['clang++','--version'],'cpu':['sysctl','-n','machdep.cpu.brand_string']}.items()}}

def main():
 p=argparse.ArgumentParser();p.add_argument('--implementations',default=','.join(RUST+JS+CPP));p.add_argument('--scenarios',default='small,uniform,clusters,dense,contained,floors,boundaries,teleports,large');p.add_argument('--samples',type=int,default=5);p.add_argument('--processes',type=int,default=3);p.add_argument('--out',default='results/comparison.json');p.add_argument('--fixtures',default='benchmarks/fixtures');p.add_argument('--seed',type=int,default=17);p.add_argument('--timeout',type=int,default=300);p.add_argument('--window',type=int,default=64);p.add_argument('--continuous',action='store_true');p.add_argument('--windows',help='comma-separated randomized output-window sweep');a=p.parse_args()
 names=a.implementations.split(',');scenarios=a.scenarios.split(',');windows=[int(w)for w in a.windows.split(',')]if a.windows else[a.window]
 assert len(windows)==len(set(windows)) and all(1<=w<=64 for w in windows)
 assert a.samples>0 and a.processes>0 and 1<=a.window<=64
 assert not(set(names)-set(RUST+JS+CPP+VARIANTS)), 'Unknown implementation'
 assert not a.continuous or not(set(names)-set(RUST+VARIANTS)), 'Continuous full-consumption protocol currently Rust only'
 out=Path(a.out);out.parent.mkdir(parents=True,exist_ok=True)
 # Freeze executable artifacts before measurements. A concurrent later build cannot
 # replace the code being measured. The JS copy stays by its imports, whose hashes
 # are checked after every job. Registry lockfiles identify transitive code.
 frozen=Path('build/frozen')/out.stem;frozen.mkdir(parents=True,exist_ok=False)
 artifacts={}
 if set(names)&set(RUST+VARIANTS):
  path=frozen/'rust';shutil.copy2('build/target/release/rill-spatial-benchmarks',path);artifacts['rust']={'path':str(path),'sha256':digest(path)}
 if set(names)&set(JS):
  path=Path('benchmarks')/('.frozen-'+out.stem+'.mjs');assert not path.exists();shutil.copy2('benchmarks/javascript.mjs',path);artifacts['js']={'path':str(path),'sha256':digest(path)}
 for name in set(names)&set(CPP):
  path=frozen/name;shutil.copy2('build/cpp-'+name,path);artifacts[name]={'path':str(path),'sha256':digest(path)}
 doc={'schema':2,'status':'INCOMPLETE','created_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'host':platform.platform(),'machine':platform.machine(),'identity':identity(),'artifacts':artifacts,'implementations':names,'scenarios':scenarios,'fixture_directory':a.fixtures,'notes':['Other user workers left running; shared-machine diagnostics','Process RSS includes reference oracle and retained result vectors; not isolated index bytes'],'output_window_queries':windows[0] if len(windows)==1 else None,'output_windows':windows,'continuous_full_consumption':a.continuous,'samples_per_process':a.samples,'requested_processes':a.processes,'rows':[]}
 jobs=[(i,s,n,w)for i in range(a.processes)for s in scenarios for n in names for w in windows];random.Random(a.seed).shuffle(jobs)
 for ordinal,(process,scenario,name,window)in enumerate(jobs):
  fixture=Path(a.fixtures)/(scenario+'.json');input_path=fixture
  bounded=json.loads(fixture.read_text()).get('bounded',False)
  if bounded and name not in RUST+VARIANTS+JS+CPP:raise RuntimeError('Unknown bounded driver')
  if name in RUST+VARIANTS:artifact=artifacts['rust'];cmd=[artifact['path'],name,str(fixture),str(a.samples)]
  elif name in JS:artifact=artifacts['js'];cmd=['node',artifact['path'],name,str(fixture),str(a.samples)]
  else:artifact=artifacts[name];input_path=fixture.with_suffix('.bin');cmd=[artifact['path'],str(input_path),str(a.samples)]+(['bounded']if bounded else[])
  assert digest(artifact['path'])==artifact['sha256'],'Measured executable changed'
  start=time.monotonic()
  try:
   r=subprocess.run(['/usr/bin/time','-l']+cmd,text=True,capture_output=True,timeout=a.timeout,env={**os.environ,"RILL_OUTPUT_WINDOW":str(window),"RILL_CONTINUOUS":"1" if a.continuous else "0"})
   if r.returncode:row={'implementation':name,'scenario':scenario,'status':'failed','exit_code':r.returncode,'error':r.stderr if len(r.stderr)<7000 else r.stderr[:3500]+'\n[stderr middle omitted]\n'+r.stderr[-3500:]}
   else:row=json.loads(r.stdout);row['fixture_name']=row.get('scenario');row['scenario']=scenario;row['status']='measured';row['process_peak_rss_bytes']=int(re.search(r'(\d+)\s+maximum resident set size',r.stderr)[1])
   if r.returncode==0:
    cpu=re.search(r'([\d.]+)\s+real\s+([\d.]+)\s+user\s+([\d.]+)\s+sys',r.stderr)
    if cpu:row['whole_process_cpu_seconds']={'user':float(cpu[2]),'system':float(cpu[3]),'scope':'entire process including proof, parsing, allocation and timing; not timed kernel CPU alone'}
  except Exception as e:row={'implementation':name,'scenario':scenario,'status':'failed','error':str(e)}
  row.update({'process':process,'output_window_queries':window,'command':cmd,'fixture_sha256':digest(fixture),'input_sha256':digest(input_path),'artifact_sha256':artifact['sha256'],'load_average':output(['sysctl','-n','vm.loadavg']),'process_wall_seconds':time.monotonic()-start});doc['rows'].append(row)
  out.write_text(json.dumps(doc,indent=2)+'\n')
  if row['status']=='measured':
   values=sorted(x['total_ns']for x in row['samples']);print(f'{ordinal+1}/{len(jobs)} {name} {scenario} window={window}: {values[len(values)//2]/1e6:.3f} ms',flush=True)
  else:print(f'{ordinal+1}/{len(jobs)} {name} {scenario} window={window}: FAILED {str(row.get("error",""))[:100]}',flush=True)
 doc['status']='COMPLETE_WITH_FAILURES'if any(r['status']!='measured'for r in doc['rows'])else'COMPLETE';out.write_text(json.dumps(doc,indent=2)+'\n')
if __name__=='__main__':main()
