#!/usr/bin/env python3
"""Coherent cell batching diagnostics; all runs are sequential and exploratory."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

p=argparse.ArgumentParser()
p.add_argument('--fixtures',type=Path,required=True)
p.add_argument('--stride',type=int,default=1)
p.add_argument('--repetitions',type=int,default=1)
p.add_argument('--output',type=Path,required=True)
args=p.parse_args()
root=Path(__file__).resolve().parent
binary=root/'.batch-build/release/examples/batch_probe'
sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
sources={str(path.relative_to(root)):sha(path) for path in [root/'src/lib.rs',root/'Cargo.toml',root/'Cargo.lock',root/'examples/batch_probe.rs',root/'tests/batch_oracle.rs']}
records=[]
for fixture in sorted(args.fixtures.resolve().glob('*.bin')):
    for strategy in ['hash','sorted','occupied','hybrid']:
        command=['/usr/bin/time','-l',str(binary),str(fixture),str(args.repetitions),str(args.stride)]
        result=subprocess.run(command,env=dict(os.environ,RILL_CELL_STRATEGY=strategy),text=True,capture_output=True)
        if result.returncode:raise RuntimeError(result.stderr)
        rows=[json.loads(line) for line in result.stdout.splitlines()]
        records.append({'experiment':'query_cell_candidate_reuse','fixture':str(fixture),'sha256':sha(fixture),'strategy':strategy,'command':command,'source_sha256':sources,'binary_sha256':sha(binary),'results':rows,'stderr':result.stderr})
        args.output.write_text(json.dumps(records,indent=2)+'\n')
        row=rows[-1]
        print(fixture.stem,strategy,'single_ns',row['single_ns'],'batch_ns',row['batch_ns'],'groups',row['query_cells'],'checked',row['oracle_queries'],flush=True)
