# Rill JavaScript snapshot index

A portable JavaScript implementation of Rill's packed leaves, separate coordinate
arrays and longest-axis median hierarchy. Node and browser ESM use the same file;
there are no dependencies, native addons, GPU APIs or runtime code generation.

```js
import {PackedPointIndex} from './index.mjs';
const values = [{id: 1n, pos: [10, 0, 20]}, {id: 2n, pos: [12, 4, 20]}];
const index = new PackedPointIndex(values);
index.nearestXZ([11, 0, 20], 4); // original value and horizontal distance
index.within3D([10, 0, 20], 5); // original values, in input order
```

Construction captures coordinates once; rebuild after movement. Values retain
identity, including arbitrary IDs/BigInts. Invalid/nonfinite positions are skipped.
`position` defaults to `value.pos ?? value`; `unique: true` retains the first value
at each identical coordinate triple. `leafSize` defaults to 24 (1–1024), and
`splitAxes` defaults to all three axes; choosing `[0,2]` favors horizontal queries.

- `nearestXZ(origin, maxHeight=Infinity)`: horizontal `Math.hypot` distance,
  strict vertical difference below `maxHeight`, first input among ties.
- `nearest3D(origin)`: first input minimizing ordered squared 3-D distance.
  Its returned `distance` is **squared**, matching the snapshot compatibility API.
- `nearXZ(origin,radius)`: any point in the closed squared horizontal radius.
- `withinXZ` and `within3D`: strict `Math.hypot` radius, in original input order.
- `withinBox`: inclusive axis-aligned box half-width, in original input order.

These JavaScript predicates and snapshot APIs are distinct from Rust Rill's
mutable `u64` radius-query API. There is no approximate filtering or result cap.
The Rust SIMD/multithread implementation is not silently used in JavaScript.

Run `npm test` here, or `node --test javascript/test/*.test.mjs` at repository root.
The initial Node22/26 game-component experiment showed lower build-plus-query and
fauna-update cost; this is not whole-game FPS or hosted capacity evidence. The
original all-competitor acceptance gate remains open. This package is source-only
in this repository; it has not been published to npm.
