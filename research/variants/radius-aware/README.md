# Global active-axis topology experiment

Status: source ready. This lane has not built, tested or benchmarked it. Root
reported the existing 15 release tests passed before the targeted thin-world
test source was added; those new tests still require root validation. There are
no performance claims.

This is a minimal frozen fork of the two declared core source files from
`7c9f2018324454bd987c2efd0517c20e341d7849`: `src/lib.rs` and `src/hierarchy.rs`.
The public `Index2`, `Index3`, generic `SpatialIndex`, `BatchOptions` and batch
APIs remain available. The root batch implementation, correctness tests, batch
tests and profiling example are read-only references. `source-provenance.json`
pins every input by SHA256; shared references must still match those pins when
root begins validation. `source-change.patch` records the source-only difference.

`tests/thin.rs` is a separate candidate-specific independent oracle source. It
constructs 1,280 wide-XY points with Z span below the hint, 1,200 points in a thin
2D corridor, and 1,331 points with every axis below the hint. Fixture assertions
guarantee the tree-building and disabled-axis/all-axis-fallback preconditions.
Arbitrary stable u64 IDs and unindexed centres are used throughout. Queries cover
slab faces, their adjacent representable f64 coordinates, adjacent representable
radius boundaries, zero radius, and radii far smaller/larger than the hint.

The thin slab and corridor each progress through prepared reads, movement-only
dirty reads, reprepare/refit, structural churn and rebuild, then a point teleport
plus forced rebuild that activates the formerly disabled axis. Restoring the
geometry forces the opposite transition. Expected complete memberships come only
from an independently maintained BTreeMap snapshot and scalar closed-radius
arithmetic. Four-worker `within_batch_parallel` checks validate every original
query index and complete row on both disabled-axis fixtures, including the
reprepared and teleported states. The all-axis fallback also has a four-worker
check and dirty/reprepare reads. No targeted test has been run in this lane.

The hypothesis is generic: a thin coordinate dimension can attract local BVH
median splits even when the configured cell width approximates a common query
radius that spans nearly that entire dimension. Those splits may duplicate
traversal through the other dimensions. This experiment changes only topology
selection in a freshly constructed prepared hierarchy.

For the existing tree-building path (more than 128 points), construction computes
one global axis mask:

```
active[d] = global_max[d] - global_min[d] >= cell_size
```

The threshold multiplier is exactly 1.0; no other threshold variants are present.
If every axis is inactive, all axes become active. Every recursive median split
then chooses the longest local span among those globally active axes. The mask
is passed through construction only; it is not applied to node bounds, point
coordinates, distance calculations or query radii.

Every node retains its full D-dimensional bounding box. Scalar/SIMD leaf filtering
and closed-radius arithmetic retain all original dimensions. Stable arbitrary
IDs, point permutations, position mapping, invalid-input handling, small-index
paths, stale-view behavior, updates, deletion, refitting, rebuild criteria and
exceptional-coordinate/radius paths are unchanged. A refit preserves its existing
topology; a later rebuild computes a new mask from current point spans.

`cell_size` is only a performance hint. Queries may use arbitrary finite radii
and arbitrary centres, including radii much smaller than the hint. The prototype
does not assume a game, fixed query radius, indexed query centres or a dropped
coordinate dimension. If all global spans meet or exceed the hint, the split rule
agrees with the baseline. If none reaches it, the fallback agrees with the baseline.

The plausible useful case has at least one global span below the hint and another
above it, with typical radii comparable to the hint. A poor hint or many queries
smaller than the thin dimension can make topology worse. Construction adds a
global O(D*n) bounds pass; root's eventual measurements must include preparation
and mutation costs. Avoiding an axis is never a correctness condition or a
membership approximation. Full oracle validation is still required.

The independent manifest has `[workspace]`, `[lib] name = "rill_spatial"`, the
same Rust 1.85 minimum, the same default SIMD feature and exact `wide =0.7.33`
pin. Its small lockfile materializes only `wide`, `safe_arch` and `bytemuck` from
the pinned root lock, with the original versions and registry checksums. No
dependency sources, targets, stores or repository trees were copied.

After root's current serial phase has finished, the intended validation from
this directory is:

```sh
ink-cache prepare-sources "$PWD"
CARGO_TARGET_DIR="$PWD/.build" CARGO_INCREMENTAL=0 cargo test --locked
CARGO_TARGET_DIR="$PWD/.build" CARGO_INCREMENTAL=0 cargo test --locked --no-default-features
CARGO_TARGET_DIR="$PWD/.build" CARGO_INCREMENTAL=0 cargo test --locked --release
CARGO_TARGET_DIR="$PWD/.build" CARGO_INCREMENTAL=0 cargo test --locked --release --test thin
CARGO_TARGET_DIR="$PWD/.build" CARGO_INCREMENTAL=0 cargo test --locked --release --no-default-features --test thin
```

These commands have not been run. The local Cargo entry point and source preflight
remain required before isolated builds; any returned direct-toolchain `--config`
arguments must be passed through if root chooses direct toolchain invocation.
Debug information and incremental output are disabled for untimed validation.
Do not run `cargo fmt` across this crate's shared references; any formatting must
name only its two owned source files. Future generated outputs require an exact
retention disposition after completion. No target or other generated build output
exists at this handoff, and no workers were stopped, paused or made to wait.
