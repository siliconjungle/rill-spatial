# Auto-output-only batch window candidate

Despite the historical folder/package name, this candidate uses **unchanged root
geometry**, not radius-aware topology. It materializes only root `src/lib.rs` and
the stable root batch module. Its hierarchy is a read-only path reference to
`../../../../src/hierarchy.rs` from this crate's `src/lib.rs`. No root files are
changed. This is an internal batch-policy candidate, not a new competitor.

The only behavioral change is in `src/batch.rs`, after all query/configuration
validation and the empty-query return, before partitioning:

1. Start with `effective_limit = min(configured_max, query_count)`.
2. If `query_count >= 1024` and `index_len * effective_limit * 8 > 1 MiB`, run four
   exact `within_into` queries at original indices `0`, `Q/4`, `Q/2`, and `3Q/4`,
   using one reusable scratch vector. Multiplication in the byte estimate saturates
   to avoid overflow; sample indices use integer division after multiplication.
3. If any sample matches, reduce the limit to at most
   `max(1, floor(1 MiB / (largest_sample_match_count * 8)))`.
4. Drop the scratch vector before allocating worker outputs. Use the effective
   limit for both spatial partitioning and per-worker result-vector allocation.

The four probes are additional charged query/allocation work inside the batch
call. They emit no visitor callbacks and contribute no query or match statistics.
Every original query still reaches exactly one complete result row. Callbacks
remain within the configured maximum, and `BatchStats.groups/workers` describe the
actual resulting scheduler. The existing serialized `FnMut + Send` API and
concurrent `Fn + Sync` API share the same modified scheduler. Validation, panic
behavior, stable u64 IDs and exact membership predicates are otherwise unchanged.

The 1 MiB value is a **soft cache target per worker**, not a hard byte bound.
Samples can miss arbitrarily dense queries. Vector capacity can exceed length;
one exact row may itself exceed 1 MiB; multiple workers retain multiple groups.
The existing hard upper bound on live row count remains configured workers times
configured maximum rows, plus ordering/scheduler storage. No ID is capped,
discarded, approximated or deferred to achieve the target.

The hypothesis is that dense all-visible phases can benefit from smaller output
windows while ordinary scenes retain larger groups. Root subsequently validated
and measured v1. This working version is v2: no builds, tests, probes or timings
have been executed for v2, and no v2 performance claim is made.

## V2: source-only probe helper isolation

V2 moves exactly the same sampling policy into private
`#[inline(never)] fn adaptive_group_limit(&self, queries, configured_limit)`.
`run_batch` still validates every query/configuration, handles the empty batch,
then calls that helper before partitioning. There is no `cold` attribute. The
helper is independent of the consumer's callback type and retains the exact same
four probes, sample-count limit, byte arithmetic, scratch lifetime and soft target.
No API, geometry, query, result or scheduler algorithm changes accompany it.

Root reported that v1's 56-job comparison passed all exact proofs and improved
dense primary median service from 48.74 to 28.01 ms, while crowd regressed roughly
10–25% in primary/cohort runs despite an unchanged effective limit of 64. The
compiler inlining/layout explanation is a hypothesis; v2 isolates the probe body
from the callback-specific scheduler to investigate it. V2 requires fresh proof
and measurement before drawing any conclusion.

V1 remains available at commit `9a2c5821ed91eaa023107d641f3ea8e425eb6c9a`.
`source-change-v1.patch` and `source-provenance-v1.json` preserve the original
selected source baseline; current provenance/patch describe v2.

`tests/adaptive.rs` adds independent point-snapshot/full-membership checks without
a retained result matrix. It rejects duplicate IDs and repeated/missing original
query visits, and reconciles actual groups/matches with statistics. Source cases:

- 16,384 targets and 1,024 dense queries reduce configured 64 rows to 8 rows;
- empty sparse samples preserve 64-row windows and complete empty query rows;
- fewer than 1,024 queries preserve the configured window;
- four empty samples miss dense hotspots: all results remain exact, and observed
  group output is explicitly allowed to exceed the soft target;
- Q=1,027 with a hot query only at `floor(3Q/4)` catches premature division;
- a late invalid query in a large adaptive batch emits no callbacks for either API.

The manifest also references root's unchanged correctness/batch suites and the
radius-aware candidate's independent thin-world membership suite read-only.
Sharing those semantic cases does not activate radius-aware geometry. `wide` is
still pinned to `=0.7.33`; the lock includes only its required closure. Dev/test
debug and incremental output are disabled. No dependencies, knowledge stores,
fixture copies, build targets or compiler archives were copied into this tree.

After root permits validation, reproduce from
`/Users/james/Documents/ChatGPT/3d-modelling/rill-spatial` using local Cargo:

```sh
ink-cache prepare-sources "$PWD/research/variants/radius-auto" --offline
cargo test --manifest-path research/variants/radius-auto/Cargo.toml --release --test adaptive -- --test-threads=1
cargo test --manifest-path research/variants/radius-auto/Cargo.toml --release -- --test-threads=1
cargo test --manifest-path research/variants/radius-auto/Cargo.toml --no-default-features --release -- --test-threads=1
```

V2 preflight and all Cargo commands are deliberately deferred. Exact copied/shared
source hashes, base revision and source-only status are in
`source-provenance.json`; `source-change.patch` is the reviewable batch delta.
Recheck shared-source hashes before future measurements. The existing free-disk
reserve deficit remains: this source-only tree does not meet the requested
100 GB reserve or alter protected inputs or active workers.
