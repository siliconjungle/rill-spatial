#!/usr/bin/env python3
"""Summarize observed query-frame service; never infer a server-capacity pass."""
import argparse,json,statistics
from collections import defaultdict
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('results',nargs='+');p.add_argument('--out',required=True);p.add_argument('--budget-ms',type=float,default=1000/60);a=p.parse_args()
groups=defaultdict(list);failures=[];inputs=[]
for path in a.results:
 d=json.loads(Path(path).read_text());inputs.append({'path':path,'status':d['status']})
 for r in d['rows']:
  if r['status']!='measured':failures.append({'input':path,**r});continue
  protocol=r.get('continuous_protocol','v1 serialized consumer' if r.get('continuous_full_consumption')else'paused oracle')
  key=(r['scenario'],r['implementation'],r.get('output_window_queries',d.get('output_window_queries')),r['artifact_sha256'],r['fixture_sha256'],protocol)
  groups[key].append(r)
rows=[]
for (scenario,name,window,artifact,fixture,protocol),observations in sorted(groups.items()):
 frames=[f['frame_service_ns']/1e6 for r in observations for f in r.get('frame_samples',[])if f['queries']]
 row={'scenario':scenario,'fixture_name':observations[0].get('fixture_name'),'implementation':name,'window':window,'artifact_sha256':artifact,'fixture_sha256':fixture,'protocol':protocol,'fresh_processes':len(observations),'observed_query_frames':len(frames),'median_total_ms':statistics.median(statistics.median(s['total_ns']for s in r['samples'])for r in observations)/1e6,'matches_per_query':observations[0].get('matches_per_query')}
 if frames:row.update({'median_frame_ms':statistics.median(frames),'max_observed_frame_ms':max(frames),'observed_deadline_misses':sum(f>a.budget_ms for f in frames)})
 rows.append(row)
report={'scope':'Observed kernel service only; no full server, queue latency or statistical p99 claim','budget_ms':a.budget_ms,'inputs':inputs,'failures':failures,'rows':rows,'notes':['Different binary hashes, fixture hashes and protocols stay separate','Reported frame includes updates/preparation and complete output consumption where specified','Maximum is the worst observed frame, not a guaranteed bound','Fresh processes and samples are retained; this does not satisfy the original simultaneous superiority gate']}
Path(a.out).write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'groups':len(rows),'failures':len(failures),'output':a.out}))
