#!/usr/bin/env python3
"""Strict acceptance evaluator. Missing, failed, tied or slower comparisons fail.

Fresh-process medians are paired by process ordinal and fixture hash. For each
comparison, max(ratio) is a distribution-free one-sided upper confidence bound
for the population median ratio, at confidence 1 - 2**(-n), assuming independent
process pairs. Bonferroni protects the whole family. This deliberately requires
many fresh process pairs; thousands of in-process iterations do not replace them.
"""
import argparse,json,math,statistics
from pathlib import Path
from compare import RUST,JS,CPP
SCENARIOS='small,uniform,clusters,dense,contained,floors,boundaries,teleports,large'.split(',')
p=argparse.ArgumentParser();p.add_argument('results',nargs='+');p.add_argument('--out',default='results/gate.json');a=p.parse_args()
rows=[];issues=[];runs=[]
for f in a.results:
 d=json.loads(Path(f).read_text());runs.append({'path':f,'schema':d.get('schema'),'status':d['status']})
 if d.get('schema',0)<2:issues.append(f'{f}: missing executable provenance');continue
 if d['status']=='INCOMPLETE':issues.append(f'{f}: unfinished run')
 rows.extend(d['rows'])
 for row in d['rows']:
  if row['status']!='measured':issues.append(f"{f}: failed mandatory case {row['implementation']}/{row['scenario']}")
competitors=[x for x in RUST+JS+CPP if x!='rill-spatial'];comparisons=len(competitors)*len(SCENARIOS);required=math.ceil(math.log2(comparisons/.05));checks=[]
for scenario in SCENARIOS:
 for name in competitors:
  own=[r for r in rows if r['scenario']==scenario and r['implementation']=='rill-spatial']
  base=[r for r in rows if r['scenario']==scenario and r['implementation']==name]
  # Never silently pool several candidate binaries: optimization revisions need a
  # separate evaluation. Repeated result files likewise cannot create fresh pairs.
  if len({r['artifact_sha256']for r in base})>1:issues.append(f'{scenario}/{name}: mixed baseline binaries')
  if len({r['artifact_sha256']for r in own})>1:issues.append(f'{scenario}: mixed candidate binaries')
  pairs=[];used=set()
  for x in base:
   key=(x['process'],x['fixture_sha256']);matches=[r for r in own if(r['process'],r['fixture_sha256'])==key]
   if key in used:issues.append(f'{scenario}/{name}: duplicate process pair');continue
   used.add(key)
   if len(matches)!=1:continue
   y=matches[0]
   if x['status']!='measured'or y['status']!='measured'or not x.get('correct')or not y.get('correct'):continue
   if x['samples'][0]['results']!=y['samples'][0]['results']:issues.append(f'{scenario}/{name}: result count mismatch');continue
   pairs.append(statistics.median(s['total_ns']for s in y['samples'])/statistics.median(s['total_ns']for s in x['samples']))
  median=statistics.median(pairs)if pairs else None
  status='missing'if not pairs else'loss-or-tie'if max(pairs)>=1 else'insufficient-processes'if len(pairs)<required else'pass'
  checks.append({'implementation':name,'scenario':scenario,'status':status,'process_pairs':len(pairs),'median_ratio':median,'upper_median_ratio_bound':max(pairs)if pairs else None})
inventory=json.loads(Path('research/inventory.json').read_text());pending=[s['id']for s in inventory['implementations']if s['status']=='pending']
if pending:issues.append('Unresolved source projects: '+', '.join(pending))
report={'status':'PASS'if not issues and all(x['status']=='pass'for x in checks)else'OPEN','scope':'Complete exact-radius dynamic workload runtime; resource/capability superiority and 2,000-connection server capacity are not established','family_confidence':.95,'required_fresh_process_pairs':required,'method':'Distribution-free one-sided median bound, Bonferroni family correction; assumes independent process pairs','runs':runs,'issues':sorted(set(issues)),'counts':{s:sum(c['status']==s for c in checks)for s in ['pass','missing','loss-or-tie','insufficient-processes']},'comparisons':checks}
Path(a.out).write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:report[k]for k in ['status','required_fresh_process_pairs','counts','issues']},indent=2))
raise SystemExit(0 if report['status']=='PASS'else 1)
