# Scoped-thread coherent-cohort experiment

This follow-up adds examples and tests; the stable library source/API is unchanged
(SHA-256 `895d16e7425af0a7ed016a45f343769188d4682055e0385d762d01b0f0f59b90`).
It is a query/index scalability diagnostic on the shared Apple M2, rather than a
full 20,000-user server result or a permanent worker-pool implementation.

`examples/support/cohort.rs` globally partitions original query indices with
median kd splits into disjoint spatial subtrees. Scoped workers share one immutable
prepared index and pull disjoint cohorts through an atomic counter. Each worker
uses the existing exact `batch_blocks` implementation to populate complete local
result vectors, then exposes the complete vectors and consumes every original
query/ID pair through `black_box`. No compact block discovery substitutes for
materialized output. Target updates happen only between joined query phases.

At 20,000 target points the cohort limit is 52; the actual tree has 512 cohorts of
39 or 40 queries. The identical cohorts are used at 1, 2 and 4 workers. Output
storage is budgeted for four workers at 32 MiB logical IDs, with additional vector
capacity growth, target index, input trace and small scratch allocations. The
selected dense process peaked at 45,137,920 bytes RSS, below the 128 MB experiment
limit. No 20k-by-20k array of u64 results is retained.

Every CLI invocation first proves full fixture membership at all three worker
counts. For every original query it rejects duplicate output IDs, compares both
present and absent memberships against an independent scalar all-pairs predicate,
and rejects missing/repeated query visits. A reusable per-worker bitset is only one
bit per target point. The timed phase reruns the identical binary trace with an
empty oracle callback and full output consumption. Oracle time is never subtracted
from overlapping parallel work: proof runs and timing runs are separate.

The new tests additionally cover arbitrary stable u64 IDs, variable radii, arbitrary
centres, dirty mutation fallback, prepared reads, limits 1/7/16/64, empty queries,
empty targets and extreme floating-point boundaries, at 1/2/4 workers. Both pass.
The earlier seven independent library oracle proofs remain in `validation.log`.

## Timing protocol

Query wall service includes spatial ordering and allocations, cohort scheduling,
scoped thread creation/join, per-cohort query-tree construction, exact target
traversal, every ID expansion and vector write, complete-output exposure, every-ID
consumption, bookkeeping and buffer destruction. There is no thread pool warmup.
Each worker reuses its own vectors within a frame. Initial index construction and
input parsing/I/O are outside the query-service interval and reported separately.

Query-frame service additionally includes that frame's updates and preparation.
Only query-bearing frames contribute to summary `sum_frame_service_ns`; all six
simulation-frame records, including update-only frames, are retained in the raw
JSONL. Each trace has two query-bearing frames. The stated deadline is 16.666667 ms
(60 Hz), configurable on the CLI. Physics, other game logic, network serialization
and transmission are outside this experiment's service deadline.

`sum_worker_elapsed_ns_not_cpu` sums elapsed worker lifetimes, including
descheduling. It is deliberately separate from wall latency and is **not** a
CPU-time measurement. Each worker includes its buffer destruction before stopping
that timer. All wall timings are measured across the enclosing thread scope.

Three repetitions rotate worker-count order: 1/2/4, 2/4/1, 4/1/2. All runs were
concurrent development diagnostics, while the root's separate exploratory work
continued. They cannot establish controlled acceptance or a universal speedup.
Final source/input hashes, every repetition and resource data are retained in
`threaded-results.json` and `threaded-{bounded64,contained}.jsonl`.

## Selected outcomes

All timings below are medians across three repetitions. Total query wall is summed
over the trace's two query-bearing frames. Max frame is each repetition's maximum
query-frame service, including updates/preparation. Deadline counts cover all six
query frames across repetitions.

| Fixture | Workers | Total query wall, ms | Max query frame, ms | Summed worker elapsed, ms | Deadline misses |
| --- | ---: | ---: | ---: | ---: | ---: |
| bounded64 | 1 | 48.63 | 26.56 | 46.32 | 6/6 |
| bounded64 | 2 | 24.97 | 14.67 | 45.53 | 0/6 |
| bounded64 | 4 | 16.87 | 10.49 | 58.52 | 0/6 |
| contained | 1 | 380.38 | 192.82 | 378.19 | 6/6 |
| contained | 2 | 220.66 | 114.11 | 436.82 | 6/6 |
| contained | 4 | 143.55 | 75.28 | 564.49 | 6/6 |

Parallelism improved wall query latency in both selected traces. Four workers
reduced median query wall by about 2.88x on bounded64 and 2.65x on contained,
relative to this same cohort runner with one scoped worker. The bounded trace's
query/index component fit the stated service deadline at two and four workers in
these samples. The dense trace still missed every query-frame deadline, even at
four workers: it reports 400,000,000 IDs per query frame.

There is a regression in the elapsed-worker metric: four workers increased its
sum by about 26% on bounded64 and 49% on contained. Two workers also increased this
metric on contained. These elapsed sums include descheduling, so they do not prove
an equivalent CPU-time increase. The data shows lower frame wall latency at the
cost of greater concurrent worker occupancy; scaling is sublinear. It does not
establish full-server feasibility or generalize beyond the selected traces.

## Reproduction

From `/Users/james/Documents/ChatGPT/3d-modelling/rill-spatial`, with local Cargo:

```sh
ink-cache prepare-sources "$PWD/research/variants/dual-tree" --offline
cargo test --manifest-path research/variants/dual-tree/Cargo.toml --release --test threaded_cohort -- --test-threads=1 --nocapture
cargo build --manifest-path research/variants/dual-tree/Cargo.toml --release --example threaded_cohort
/usr/bin/time -l research/variants/dual-tree/target/release/examples/threaded_cohort benchmarks/twenty-k/bounded64.bin 3
/usr/bin/time -l research/variants/dual-tree/target/release/examples/threaded_cohort benchmarks/twenty-k/contained.bin 3
```

The compiler was `rustc 1.96.0 (ac68faa20 2026-05-25)`. Preflight again returned no
source configuration or direct Cargo arguments: the crate has no dependencies.
For proof only, append `--verify-only`; full proof always precedes timings.
The example accepts `fixture.bin [repetitions=3] [deadline_ms=16.666667]` before
that flag. Max active scoped query workers is four; no other workers were paused,
terminated or made to wait. The preexisting disk reserve deficit remains reported
in the local retention contract.
