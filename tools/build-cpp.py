#!/usr/bin/env python3
import subprocess,json,argparse
from pathlib import Path
sources=[('nanoflann',1,[]),('pico-tree',2,['-Iresearch/vendor/cpp-pico_tree/src/pico_tree']),('libkdtree',3,['build/libkdtree.o']),('kdtree-cpp',4,['research/vendor/cpp-kdtree-cpp/kdtree.cpp']),('crvs-kdtree',5,['research/vendor/cpp-KDTree/KDTree.cpp']),('gishi-kdtree',6,[]),('kdtree-plusplus',7,[])]
p=argparse.ArgumentParser();p.add_argument('--only');a=p.parse_args()
if a.only:sources=[s for s in sources if s[0] in a.only.split(',')]
assert sources
Path('build').mkdir(exist_ok=True)
if any(n==3 for _,n,_ in sources):subprocess.run(['clang','-O3','-ffp-contract=off','-c','research/vendor/cpp-libkdtree/kdtree.c','-o','build/libkdtree.o'],check=True)
rows=[]
for name,number,extra in sources:
 cmd=['clang++','-std=c++17','-O3','-ffp-contract=off','-DNDEBUG',f'-DIMPLEMENTATION={number}',f'-DIMPLEMENTATION_NAME="{name}"','benchmarks/native.cpp',*extra,'-o','build/cpp-'+name]
 r=subprocess.run(cmd,capture_output=True,text=True)
 row={'implementation':name,'command':cmd,'status':'built'if r.returncode==0 else'build-failed','error':r.stderr};rows.append(row);print(name,row['status'],r.stderr[-300:],flush=True)
prior=json.loads(Path('results/cpp-builds.json').read_text())if a.only and Path('results/cpp-builds.json').exists()else[]
prior=[r for r in prior if r['implementation']not in {r['implementation']for r in rows}]
Path('results/cpp-builds.json').write_text(json.dumps(prior+rows,indent=2)+'\n')
