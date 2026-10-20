#!/usr/bin/env python3
"""One focused layout change, frozen binaries, alternating invocation order."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

p=argparse.ArgumentParser()
p.add_argument('--fixtures',type=Path,required=True)
p.add_argument('--passes',type=int,default=4)
p.add_argument('--stride',type=int,default=97)
p.add_argument('--output',type=Path,required=True)
args=p.parse_args()
root=Path(__file__).resolve().parent
sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
binaries={'before':root/'.layout-before-probe','after':root/'.layout-build/release/examples/batch_probe'}
manifests={key:json.loads((root/f'evidence/layout-{key}-manifest.json').read_text()) for key in binaries}
for key,binary in binaries.items():assert sha(binary)==manifests[key]['binary_sha256']
records=[]
for trial in range(args.passes):
    for at,fixture in enumerate(sorted(args.fixtures.resolve().glob('*.bin'))):
        keys=['before','after'] if (trial+at)%2==0 else ['after','before']
        for layout in keys:
            command=['/usr/bin/time','-l',str(binaries[layout]),str(fixture),'1',str(args.stride)]
            result=subprocess.run(command,env=dict(os.environ,RILL_CELL_STRATEGY='sorted'),text=True,capture_output=True)
            if result.returncode:raise RuntimeError(result.stderr)
            row=json.loads(result.stdout)
            records.append({'experiment':'prepared_id_layout','timing_status':'exploratory_concurrent_not_acceptance','layout':layout,'trial':trial,'invocation_order':keys,'fixture':str(fixture),'fixture_sha256':sha(fixture),'manifest':manifests[layout],'command':command,'result':row,'stderr':result.stderr})
            args.output.write_text(json.dumps(records,indent=2)+'\n')
            print(fixture.stem,trial,layout,'single_ns',row['single_ns'],'batch_ns',row['batch_ns'],'checked',row['oracle_queries'],flush=True)
