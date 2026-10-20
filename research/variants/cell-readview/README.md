# Packed-cell read-view candidate

This is an independent, dependency-free experiment. The public API is
`SpatialIndex<const D: usize>` (`Index2`, `Index3`), with `new`, `from_points`,
`upsert`, `remove`, `prepare`, and `within_into`. Stable `u64` IDs need not be
dense; duplicate bulk IDs keep the last coordinate. Validation errors leave the
index and output unchanged. Mutations immediately invalidate the prepared view;
queries then scan until a subsequent `prepare` rebuilds it. Output order is
unspecified and each matching ID appears once.

`prepare` sorts integer cell keys, packs IDs and coordinates in aligned CSR arrays,
records actual per-cell point bounds, and builds an occupied-cell lookup map.
The meaningful traversal alternatives are:

* `SortedSlab` (default): binary-search the occupied-cell first-axis range, then
  filter remaining cell-key dimensions. It never enumerates empty cells.
* `HashGrid`: enumerate a conservatively expanded cell rectangle and look up
  occupied buckets; use the sorted traversal when enumeration would be excessive.
* `OccupiedScan`: visit every occupied cell, skipping query-grid construction.
* `Hybrid`: compare conservative rectangle volume against the actual sorted
  slab length before selecting hash lookup or sorted traversal.

These are traversal strategies within one candidate, not separate libraries.
Set `RILL_CELL_STRATEGY=sorted|hash|occupied|hybrid`, or call `set_strategy`.
`set_containment(false)` disables bulk acceptance for a mechanism comparison.

All traversals use the actual coordinate AABB for a cell. The nearest squared
distance rejects whole cells; the farthest squared distance allows reporting a
fully contained cell without filtering individual points. These bounds use the
same dimension order, subtraction, multiplication and addition as the scalar
membership oracle. Otherwise every candidate point receives the exact scalar
test `sum((point[i]-centre[i])²) <= radius²`.

The integer grid is only an accelerator. Unrepresentable/exceptional coordinate
ratios fall back to point scanning. Non-normal squared radii avoid grid-range
pruning and use occupied-cell AABBs; this preserves squared-distance underflow
and overflow behavior. Normal-radius boxes round outward before quantization.
No query-centre-to-point identity or common-radius assumption is used.

No external library implementation or fixture was copied. The fixtures and the
original shared probe are referenced in place. The private manifest uses its
own `[workspace]`, has no dependencies, and declares Rust 1.86 because
`f64::next_up` / `next_down` first became stable in that release. Safe bitwise
rounding helpers could preserve a 1.85 integration MSRV.

## Reproduce

From this directory, using the local Cargo wrapper:

```sh
ink-cache prepare-sources "$PWD"
CARGO_TARGET_DIR="$PWD/.build" CARGO_INCREMENTAL=0 cargo test --release -- --nocapture
CARGO_TARGET_DIR="$PWD/.build" CARGO_INCREMENTAL=0 cargo build --release --examples
python3 run_diagnostics.py --fixtures /Users/james/Documents/ChatGPT/3d-modelling/rill-spatial/benchmarks/holdout-97123 --stride 1 --output evidence/holdout-exploratory.json
python3 run_diagnostics.py --fixtures /Users/james/Documents/ChatGPT/3d-modelling/rill-spatial/benchmarks/twenty-k --stride 97 --output evidence/twenty-k-exploratory.json
```

Source preflight returned no additional direct Cargo configuration for this
dependency-free crate. Debug information and incremental output are disabled in
all local profiles. Tests independently cover 2D/3D, arbitrary stable IDs,
insertion/update/deletion, queries before and after preparation, invalid-operation
atomicity, duplicate bulk IDs, four traversal algorithms, containment on/off,
extreme finite coordinates, underflow/overflow, and nextafter boundary queries.

`examples/probe.rs` is a manifest reference to the shared parent probe, suitable
for small inputs. `stream_probe` parses the same RILLv001 binaries and retains one
output vector. Its arguments are `fixture repetitions oracle_stride`; stride 1
checks every query, 97 checks every 97th query, and 0 disables oracle validation.
Oracle checking occurs before timing and compares full sorted memberships.
The timed phase consumes every query and emits aggregate match counts. It never
stores the complete 20,000-by-20,000 result matrix.

## Evidence and limits

`evidence/oracle-release.txt` records the six independent passing test suites.
`evidence/summary.json` verifies recorded source hashes and summarizes checks.
`evidence/holdout-exploratory.json` records full per-query oracle checks on all
nine seed-97123 fixtures and all four traversals. The 20k diagnostic records use
sampled oracle checks; root's coordinated comparison performs full validation.
Diagnostic JSON includes source and binary SHA256 values, absolute fixture
references and hashes, commands, strategy, timings, match counts and macOS time
resource output. They are concurrent exploratory diagnostics, not performance
superiority or final acceptance evidence.
The full holdout run checked 128,376 query memberships across four traversals.
The 20k run checked 11,592 sampled query memberships across four traversals;
peak measured resident memory was 33,964,032 bytes. All strategies produced the
same aggregate match counts. The contained fixture reports 800 million total
matches over two frames, while storage remains bounded to one reused vector.

The algorithm trades a full sort/repack on preparation for simple read paths.
Many updates between read phases are supported; queries interleaved with every
mutation use linear scans. The first sorted axis can be a poor selectivity choice
for anisotropic data. Dense output still requires reporting every matching ID.
The implementation is intentionally generic and adds no replication-policy cap.

## Coherent query-cell batching experiment

`batch_visit_groups(&queries, max_queries, |original_indices, outputs| ...)`
is a second algorithmic experiment. It preserves every complete result vector,
including empty results, and visits each original query index exactly once.
`max_queries` must be positive and is capped at 64. All query validation occurs
before callbacks; invalid queries therefore produce no partial callback results.
The return value is `Result<BatchStats, Error>` with reporting/work counters.

The method sorts arbitrary query centres by integer cell and records their
original indices. For each occupied query cell, it builds an actual centre AABB
and the largest squared query radius. Each of the four target traversal methods
can discover a conservative shared list of occupied point cells. An AABB-distance
test removes point cells outside the whole group's maximum radius. The resulting
list is reused across all output partitions for that query cell. Each individual
query then performs its own exact point-cell rejection, containment acceptance
and scalar membership filtering. Variable radii and centres absent from the
point index need no special identity mapping. Exceptional scales use exact scalar
fallbacks. Mutations invalidate the point view as before.

Grouping, sorting, original-index bookkeeping, centre bounds, candidate-list
construction, exact filtering, expansion into complete Vecs and clearing/reusing
those Vecs all execute inside the API. A group with more than 64 queries reuses
one candidate list across several callback partitions; it never materializes all
results simultaneously. The original single-query algorithms remain intact.
No SIMD or new dependencies were introduced.

`tests/batch_oracle.rs` adds five independent full oracle suites. These exercise
both dimensions, all four target traversals, containment on/off, dense and sparse
points, arbitrary IDs, variable radii, unindexed centres, teleports/deletions,
unprepared/prepared reads, radius-square underflow/overflow, exceptional scales,
callback partitioning, empty results and validation atomicity. A mechanism check
proves that 150 unindexed query centres share one candidate list across 22
callbacks when the output width is 7. The six prior single-query suites also pass
on the extended implementation.

To reproduce the separate batch diagnostics:

```sh
ink-cache prepare-sources "$PWD"
CARGO_TARGET_DIR="$PWD/.batch-build" CARGO_INCREMENTAL=0 cargo test --release -- --nocapture
CARGO_TARGET_DIR="$PWD/.batch-build" CARGO_INCREMENTAL=0 cargo build --release --example batch_probe
python3 run_batch_diagnostics.py --fixtures /Users/james/Documents/ChatGPT/3d-modelling/rill-spatial/benchmarks/holdout-97123 --stride 1 --output evidence/batch-holdout-exploratory.json
python3 run_batch_diagnostics.py --fixtures /Users/james/Documents/ChatGPT/3d-modelling/rill-spatial/benchmarks/twenty-k --stride 97 --output evidence/batch-twenty-k-exploratory.json
```

The batch diagnostic times the entire API and opaque output consumption, then
subtracts only independently measured oracle validation inside callbacks. It also
times original-order single queries for exploration. Outputs are bounded to 64
vectors or a smaller 16 MiB logical result budget. Source hashes, command lines,
fixture hashes, RSS, group counts and both timings are retained in the labelled
JSON records. The holdout check validates 128,376 full memberships; the 20k check
validates 11,592 sampled memberships, across four strategies. Maximum measured
batch diagnostic RSS was 34,390,016 bytes. These concurrent measurements are not
acceptance rankings. Root's serial comparison supplies full 20k validation.

Reuse depends on query-cell occupancy and radius variation. The 20k bounded64
fixture has 2,167 groups for 40,000 queries, crowd 409, and teleports 2,048; uniform
has 23,153. Low coherence can leave sorting and candidate-list overhead with
little reuse. A single large-radius query can broaden its group's shared list;
all per-query filtering remains exact. Dense matches still require every ID.

The batch extension has its own current-source evidence in
`evidence/batch-summary.json` and `evidence/batch-oracle-release.txt`. Earlier
single-query diagnostic records retain their original source hashes and describe
the earlier experiment. The batch build directory is retired after every local
process completes, independently of the parent's acceptance build.

The first experiment's generated build directory was `.build` (8,094,433 bytes).
It was removed after every local test and diagnostic process completed; parent
comparative measurements build the candidate independently. Source, lockfile and
declared read-only shared fixture inputs reproduce it. Its disposition is recorded
in `storage-retention.json`; no running workers were stopped or suspended.

## Focused prepared-ID layout experiment

The original prepared layout is preserved by Git commit
`b7580ed1f698e3d14114c5a8f9c25a13dc081164`, whose `src/lib.rs` SHA256 is
`53514fbd39120f73cca034e4bc73d206e26fd34d2570f4a034ba857a68af73eb`.
The current source changes only prepared storage and the matching ID access:
`Vec<Entry<D>>` becomes aligned `Vec<[f64; D]>` and `Vec<u64>`. Both single-query
and coherent-batch containment paths use `extend_from_slice` over the ID range.
Exact per-point filtering zips coordinates with the corresponding IDs. Mutable
storage, sort permutation, cell ranges, grouping, traversal, bounds, validation
and membership arithmetic remain the same. Arbitrary IDs are preserved.

`evidence/layout-change.patch` records the entire library difference. Before and
after manifests pin all common test/probe/manifest/lockfile inputs, the compiler,
source hashes and binary hashes. Only the library source requires recovery from
the baseline commit; the unchanged diagnostic example and tests are retained
inputs. Each layout passed all 11 oracle suites, then complete membership checks
for all nine holdout fixtures (64,188 query checks across layouts). The bounded
20k diagnostic alternated invocation order over four trials per layout/fixture,
used only the sorted traversal, and checked 23,184 sampled full memberships.
Every reporting/work counter matched between layouts for each paired trial.

The concurrent diagnostic's contained-case median over two frames changed from
369.090 to 134.851 ms for single queries and from 364.288 to 132.111 ms for cell
batching. Several other scenes were mildly slower; these observations are neither
acceptance evidence nor a universal improvement. Peak measured RSS across these
runs was 34,750,464 bytes before and 44,318,720 after. No ID matrix was retained.
`evidence/layout-summary.json` has every scene and paired ratio.

The experiment built the original and changed sources sequentially in the same
private `.layout-build` directory, with the retained manifest's identical release
flags. One 481,952-byte original executable was frozen as `.layout-before-probe`
to permit alternating diagnostics. `run_layout_diagnostics.py` records the exact
invocations and manifest pins. Reproduction uses the baseline library from Git
and then the current library with the unchanged retained probe/tests and manifest;
materialize only those declared inputs in a minimal replay crate, not a repository
copy. Source preflight returned no extra configuration arguments.

After both layouts' proofs and all diagnostics completed, the private build and
frozen executable were retired. Root's earlier frozen screen binary was never
modified. Root's coordinated serial comparison remains the acceptance authority.
