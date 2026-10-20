#!/usr/bin/env python3
import json,struct,sys
from pathlib import Path
for path in Path(sys.argv[1]if len(sys.argv)>1 else'benchmarks/fixtures').glob('*.json'):
 if path.name=='manifest.json':continue
 f=json.loads(path.read_text());name=f['name'].encode();b=bytearray(b'RILLv001');b+=struct.pack('<I',len(name))+name+struct.pack('<dII',f['cell_size'],len(f['points']),len(f['frames']))
 for p in f['points']:b+=struct.pack('<3d',*p)
 for frame in f['frames']:
  b+=struct.pack('<I',len(frame['updates']))
  for i,p in frame['updates']:b+=struct.pack('<I3d',i,*p)
  b+=struct.pack('<I',len(frame['queries']))
  for q in frame['queries']:b+=struct.pack('<4d',*q['point'],q['radius'])
 path.with_suffix('.bin').write_bytes(b)
