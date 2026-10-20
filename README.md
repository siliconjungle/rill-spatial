# rill-spatial

A standalone Rust library for exact dynamic 2D/3D radius
reporting. Stable `u64` IDs, immediate insert/update/removal, caller-owned result
buffers and an optional packed read view. No game engine, database, renderer,
network service or application-specific behaviour is required.

**Experimental. The requested performance acceptance gate is OPEN.** This project
is being developed against a pinned multi-implementation comparison. Initial
measurements include losses to established libraries. It does not currently claim
to outperform every competitor or workload. See [the registered comparison
protocol](research/protocol.md) and [raw results](results/).

```rust
use rill_spatial::Index3;

let mut index = Index3::new(16.0)?;
index.upsert(101, [1.0, 2.0, 3.0])?;
index.upsert(102, [50.0, 2.0, 3.0])?;

// Optional: explicitly pay preparation cost before a query-heavy phase.
index.prepare();
let mut ids = Vec::new();
index.within_into([0.0, 0.0, 0.0], 8.0, &mut ids)?;
assert_eq!(ids, vec![101]);

// Movement requires refitting before using the packed view; queries stay current.
index.upsert(102, [0.0, 0.0, 0.0])?;
index.remove(101);
# Ok::<(), rill_spatial::Error>(())
```

## Contract

- `Index2` and `Index3` use finite `f64` coordinates and nonnegative finite radii.
- All matching IDs are returned once, in unspecified order. Boundary points are
  included; coincident points with distinct IDs remain distinct.
- IDs are arbitrary stable `u64` values. Upsert replaces an existing ID atomically.
- Invalid coordinates/radii fail without changing the index or query output.
- Normal-scale distance uses ordered binary64 squared Euclidean arithmetic without
  fast-math. Extreme radii use a scaled `hypot` path to avoid overflow/underflow.
  This is an exact reporting contract under these floating-point operations, not
  arbitrary-precision real geometry or approximate-neighbour search.
- Read-only queries may run concurrently on shared references. Mutations use
  ordinary exclusive Rust access; there is no hidden thread pool or global state.
- Queries always observe current mutations. `prepare()` builds a packed hierarchy
  and releases the grid. Movement refits that hierarchy on the next preparation;
  queries during a dirty phase scan current points. Structural changes rebuild
  the hierarchy on preparation. `clear()` returns to the initial grid mode.

`from_points(cell_size, rows)` bulk-loads `(u64, [f64; D])` rows directly into
packed mode, avoiding temporary grid construction. Duplicate IDs keep the last
position. Call `prepare()` before a query-heavy phase.

The default `simd` feature uses the portable `wide` crate for small packed scans.
`--no-default-features` selects a scalar, dependency-free library. SIMD preserves
ordered squared-distance arithmetic; it does not enable fast-math.

Cell width is a workload tuning parameter, not a restriction on query radius.
Extreme coordinate/cell ratios fall back to a scan. Conservative bounds can be
tightened with `refresh_bounds()` after long churn. Hashing is optimized for
spatial coordinates, not adversarial collision resistance; applications must bound
their resource consumption at untrusted ingress.

## Reproduce

```sh
cargo test --locked -p rill-spatial
cargo build --locked --release -p rill-spatial-benchmarks
node tools/fixtures.mjs
python3 tools/binary-fixtures.py
node tools/fetch-sources.mjs
node tools/add-cpp.mjs
npm ci --prefix research/npm --ignore-scripts --no-audit --no-fund
python3 tools/build-cpp.py
python3 tools/coverage.py
python3 tools/compare.py --samples 3 --processes 14 --out results/comparison.json
python3 tools/gate.py results/comparison.json
```

In the development workspace, use the local Cargo source-preparation entry point
and pin `RUSTUP_TOOLCHAIN=stable` when compiling dependencies that carry their own
toolchain file. Compilation is outside runtime measurement. C++ adapters use
`clang++ -O3 -ffp-contract=off`; Rust uses the checked-in release profile.

Run `node tools/discover.mjs` only to refresh discovery, not as part of reproducing
a pinned measurement. Discovery refreshes are not benchmark source-pin updates: review changes before
accepting a new dependency inventory or lockfile. Third-party code stays outside
the public library and retains its own licenses. Registry versions/checksums and
Git commit pins are in the source inventory; benchmark adapters are original code.

## Comparison status

The harness currently has 52 competitor entries from 38 source projects, plus a
brute-force control. Several projects expose genuinely different data structures;
these are separate implementations, not 52 independent algorithms. Additional
source projects remain pending. The source inventory records adapter limitations.
A single screening sample checks correctness and finds optimization targets; it
cannot pass the gate. The evaluator requires at least 14 fresh process pairs per
comparison for its simultaneous 95% median-runtime bound, and rejects missing
cases, failed correctness, losses/ties, mixed binaries or unresolved sources.
The initial machine is shared with other workers; this is not isolated-host
capacity evidence. No 2,000-connection server claim follows from this kernel.

## Scope

Radius reporting is the first generic kernel. Nearest-neighbour ranking,
collision resolution, general geometry, persistence and multiplayer capacity are
not established by these experiments. Runtime, memory, capabilities and correctness
are separate dimensions; the project reports negative results explicitly.

License: MIT OR Apache-2.0.
