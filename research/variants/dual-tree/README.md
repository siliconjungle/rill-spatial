# Exact dual-tree batch candidate

This is an isolated, dependency-free Rust prototype. It changes no main library,
benchmark dependency, Git state, fixture or shared input. Stable IDs are arbitrary
`u64`; dimensions are 2 or 3; finite arbitrary centres and nonnegative finite
variable radii are accepted. Results use the parent's closed squared-f64 predicate
and unordered membership semantics. Duplicate point IDs replace prior positions.

The target and query trees use median kd partitions with at most 16 points per
leaf. Each query node stores centre bounds and minimum/maximum squared radii.
Traversal rejects a node pair if its minimum box distance exceeds every radius;
it accepts a whole Cartesian product if its maximum box distance is within every
radius. Otherwise it splits one nonleaf node or tests the leaf product. This shares
target traversal among spatially coherent queries, without any approximation,
result cap, world layout assumption or stale answer.

## Output interfaces and costs

- `within_into(centre, radius, &mut Vec<u64>)`: existing materialized-result API.
- `batch_blocks(queries, |query_indices, ids| ...)`: exact Cartesian blocks.
  An emitted block means **every** listed query matches **every** listed ID.
  This is a compact representation; consumers must pay to expand the product.
- `batch_each(queries, |original_query_index, id| ...)`: expands every membership
  and dispatches one callback per ID, storing no full result matrix.
- `batch_visit_groups(queries, max_queries, |original_indices, outputs| ...)`:
  complete materialized `Vec<u64>` results for at most `max_queries` queries per
  callback. Each `outputs[i]` corresponds to `original_indices[i]`. Every query
  visits once, even when its result is empty. Build one full query tree, partition
  it into bounded spatial subtrees, traverse each subtree against target root,
  then dispatch complete results and reuse vectors. This charges all ID expansion
  and vector writes. `batch_visit` wraps it with individual-query callbacks.

`BatchStats` counts node pairs, rejected/accepted pairs, exact point tests, blocks
and expanded membership cardinality. Blocks never suppress self/centre matches.
All queries validate before any callback or output mutation. `max_queries` must
be positive. Callbacks borrow slices; retaining output requires an explicit copy.

An upsert or removal invalidates the target tree immediately. Until `prepare()`
rebuilds it, every query API scans current entries and gives the complete result.
Invalid mutations preserve the index. Preparation rebuilds, rather than refits;
this is a prototype limitation and part of measured update cost.

## Correctness argument

Every node bounds all points in its contiguous permutation range. Median children
partition that range exactly. Query permutations retain original input indices;
target permutations retain stable IDs. The lower box distance in each coordinate
is no greater than any pair's absolute coordinate difference. The farthest box
corner distance in each coordinate is no less. Squaring nonnegative distances and
adding them in axis order preserves these inequalities under ordinary IEEE f64
rounding. Implementation and oracle use the same left-to-right squared-distance
predicate. Coordinates are finite; intermediate overflow yields positive infinity,
and neither bound introduces NaN. Underflow and infinity equality therefore
preserve the parent's floating-point semantics, rather than silently switching
to an ideal-real or hypot metric.

Thus `lower > max_radius_squared` excludes every pair, while
`upper <= min_radius_squared` includes every pair, including closed boundaries.
Every recursive split partitions the current Cartesian product into disjoint
products. Accepted products and exact leaf cases report each matching pair once;
rejected products report none. This establishes full membership and uniqueness.
The spatial visitor additionally partitions the query tree into disjoint query
subtrees, so visiting each subtree against the full target root preserves the
same argument. Dirty scanning uses the exact leaf predicate directly.

The proof oracle in `tests/oracle.rs` independently evaluates every potential
query/point membership with a scalar all-pairs predicate. It checks both present
and absent bits and explicitly rejects duplicates. It is not a count or hash
comparison. It covers both 20,000-by-20,000 dense and sparse cases, 18 shared
fixtures across two seeds, arbitrary IDs, mutations, arbitrary centres, radii,
closed boundaries, 2D/3D and extreme finite floating-point values. Visitor tests
check limits 1, 7, 16, 63, 64 and 1000 before and after preparation.

## Space and complexity

Target preparation is O(n log n), query-tree construction is O(q log q), with
O(n+q) index/tree state. Streaming blocks and individual-pair callbacks add only
a leaf-sized scratch vector and logarithmic traversal stack. Worst-case traversal
and expanded reporting remain O(nq); there is no unconditional subquadratic claim.
The spatial visitor retains at most `max_queries` result vectors, so its logical
ID storage is bounded by `max_queries*n*8` bytes. Vector capacity growth can add
overhead. The diagnostic CLI caps that logical storage at 16 MiB and 64 queries.
The caller can retain outputs, which is outside the implementation's memory bound.

20k observers seeing 20k targets produce 400,000,000 IDs: 3.2 GB of u64 ID writes
per frame before downstream handling. A single accepted block is a useful exact
representation of that relation, but its discovery time cannot be compared with
complete ID reporting time without including consumer expansion.

The dense 400M-membership oracle retains a 50,000,000-byte bitset and peaked at
57,147,392 bytes RSS in `/usr/bin/time -l`; no 20k-squared vector of IDs was created.
The disk reserve already had a deficit on entry (~23 GiB free versus requested
100 GB). This required proof uses small local generated build outputs and does
not alter or suspend other workers.

## Reproduction

From `/Users/james/Documents/ChatGPT/3d-modelling/rill-spatial`:

```sh
ink-cache prepare-sources "$PWD/research/variants/dual-tree" --offline
cargo test --manifest-path research/variants/dual-tree/Cargo.toml --release -- --test-threads=1
cargo build --release --manifest-path research/variants/dual-tree/Cargo.toml --examples
/usr/bin/time -l research/variants/dual-tree/target/release/examples/batch_probe benchmarks/twenty-k/bounded64.bin 1
/usr/bin/time -l research/variants/dual-tree/target/release/examples/batch_probe benchmarks/twenty-k/contained.bin 1
```

Preflight returned `source_config: null` and `direct_cargo_arguments: []`: this
crate has no external dependencies. Use the local `cargo` wrapper. Dev/test debug
and incremental output are disabled; release settings are unchanged defaults.
`probe` references `../../../examples/probe.rs` read-only for compatibility.

The CLI reads RILLv001 binary fixtures directly from the parent. It verifies full
memberships for single queries, raw input-order chunks, and the spatial visitor
using independent bitsets. All oracle time is outside query timings; visitor
validation is subtracted once per complete-results group. Tree construction,
traversal, expansion, writes, dispatch and vector bookkeeping remain charged.
The materialized consumption fields separately time an opaque operation for every
ID. The whole-batch block field excludes expansion; the whole-batch streaming
field includes one opaque operation per individual pair. Whole-batch count checks
in this CLI are diagnostics; the independent full-membership whole-batch proofs
are the oracle tests described above.

Single-query and raw-batch paths use independent equally cold output buffers and
alternate execution order per chunk/repetition. Both include clearing and result
allocation. Preparation runs only in query-bearing frames. These fields are sums
of timed service intervals with oracle work excluded, rather than wall latency.
The CLI holds separate single/raw-batch buffers in addition to the visitor's
bounded buffers; measured RSS for the selected 20k dense CLI run was 37,289,984 bytes.

All local timings in `diagnostics.jsonl` are exploratory concurrent measurements,
not serial acceptance evidence. Query order matters: raw 64-query chunks can
scatter centres and lose most pruning. The spatial visitor avoids that loss but
does not yet establish a general performance win over single-query traversal.
Final performance decisions belong to the root's serial, equivalent-output gate.

## Primary research

Curtin, March, Ram, Anderson, Gray and Isbell, *Tree-Independent Dual-Tree
Algorithms*, ICML 2013, section 6 and Algorithms 5–6, provide the range-search
base-case/pruning framework; section 3 explains conservative kd bounding boxes.
This prototype adapts the framework to variable radii and explicit accepted
Cartesian blocks, with the correctness argument above.
[Publisher and paper](https://proceedings.mlr.press/v28/curtin13.html),
[PDF](https://proceedings.mlr.press/v28/curtin13.pdf).

Curtin, Lee, March and Ram, *Plug-and-Play Dual-Tree Algorithm Runtime Analysis*,
JMLR 16(101):3269–3297, 2015, analyzes cover-tree dual algorithms and gives a
conditional linear range-search guarantee. Those cover-tree assumptions do not
automatically apply to this kd-tree prototype or remove explicit output cost.
[Publisher and paper](https://jmlr.org/papers/v16/curtin15a.html).
