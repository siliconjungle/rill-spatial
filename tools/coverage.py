#!/usr/bin/env python3
"""Map real implementation classes to pinned source projects and evidence."""
import json
from pathlib import Path
from compare import RUST,JS,CPP
MAP={
 'rstar':['rstar'],'kdtree':['kdtree'],'kd-tree':['kd-tree'],'kiddo':['kiddo-mutable','kiddo-immutable'],
 'static_aabb2d_index':['static-aabb2d'],'geo-index':['geo-index-kd','geo-index-rtree'],
 'spart':['spart-kd','spart-rtree','spart-rstar','spart-octree','spart-quadtree'],
 'acap':['acap-kd','acap-flat-kd','acap-vp','acap-flat-vp','acap-exhaustive'],
 'vpsearch':['vpsearch'],'petal-neighbors':['petal-ball'],'neighbourhood':['neighbourhood-owned','neighbourhood-index'],
 'quadtree-f32':['quadtree-f64'],'quadtree_rs':['quadtree-rs'],'octree':['rust-octree'],'spatial':['spatial-linear','spatial-quadratic','spatial-rstar'],'nabo':['nabo'],
 '@flatten-js/core':['flatten-planar'],'mnemonist':['mnemonist-vp','mnemonist-kd'],
 'pico_tree':['pico-tree'],'libkdtree':['libkdtree'],'KDTree':['crvs-kdtree']}
NOTES={
 'quadtree-f64':'f64 feature. Fixed bounding rectangle avoids source contains_rect typo (r.max_x <= self.max_y). Coalesced rebuild avoids duplicate retention/removal failures. Deduplication and exact 3D filtering charged; source remains unmodified.',
 'mnemonist-kd':'Radius via all-N nearest-neighbour reporting; complete, untruncated, includes ranking cost because exposed API requires it.',
 'nabo':'Bounded KNN with k=population, allow-self and unsorted results; no truncation.',
 'quadtree-rs':'Conservative integer XY projection plus exact f64 XYZ filter; adapter bounded to registered coordinate range.',
 'spatial-linear':'Original linear R-tree split algorithm.', 'spatial-quadratic':'Original quadratic R-tree split algorithm.', 'spatial-rstar':'Original R-star split algorithm.',
 'box-intersect':'Original batch intersection API handles full observer batch at once; construction, callback filtering and materialization charged.'}
PROJECT_STATUS={
 'cargo:sif':('incompatible','Parameter-testing macro, not a spatial index (source description).'),
 'npm:bush':('incompatible','Unrelated package; not a spatial index.'),
 'npm:quadtree':('incompatible','Quadkey encoding package, not a radius-reporting index.'),
 'cpp:unibn-octree':('pending','Source uses float internal bounds; requires a validated conservative binary64 adapter before comparison.'),
}
d=json.loads(Path('research/inventory.json').read_text())
for source in d['implementations']:
 source['adapters']=MAP.get(source['name'],[source['name']]if source['name']in RUST+JS+CPP else[])
 if source['id']in PROJECT_STATUS:source['status'],source['status_reason']=PROJECT_STATUS[source['id']]
 elif source['adapters']:source['status']='adapter-implemented';source['status_reason']='Measurement/correctness status is per fixture and build in results; this status does not assert correctness or a performance win.'
 else:source['status']='pending';source['status_reason']='Source discovered and pinned; adapter/capability evaluation remains outstanding.'
 if source['adapters']:source['adapter_notes']={n:NOTES[n]for n in source['adapters']if n in NOTES}
Path('research/inventory.json').write_text(json.dumps(d,indent=2)+'\n')
print(json.dumps({'competitor_entries':len(RUST+JS+CPP)-2,'source_projects_with_adapters':sum(bool(s['adapters'])for s in d['implementations']),'pending_projects':[s['id']for s in d['implementations']if s['status']=='pending']},indent=2))
