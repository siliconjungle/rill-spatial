# Selected 20k experiment results

Acceptance remains **OPEN**. No 20,000-connection server has been tested.

The initial bounded screen (`results/twenty-k-screen-v1.json`) completed 98 jobs
with full query membership/duplicate/omission checks. These were concurrent
exploratory runs with oracle pauses between bounded output groups. Walking the
reference point set during those pauses can disturb caches and frequency state;
these service sums are not continuous frame latency.

A separate independently proved uninterrupted main-index diagnostic did not
reproduce the suspected refit regression. Keeping the existing refit policy was
faster overall than reconstructing at every query frame. See the source-pinned
[refit experiment](../variants/dual-tree/REFIT.md).

Cell read views now store aligned coordinate and ID arrays. Contained-cell
acceptance bulk-copies contiguous IDs. Four alternating concurrent trials gave
contained-scene medians 369→135ms (single) and 364→132ms (cell batch), over two query
frames. Some other scenes regressed 2–6%; this is a useful mechanism result, not
universal superiority. Both layouts passed 11 independent oracle suites and
64,188 full holdout checks. See
[paired layout evidence](../variants/cell-readview/evidence/layout-summary.json).

The public `within_batch` API composes spatial ordering, one immutable index,
scoped workers, reusable complete-query result vectors and serialized consumers.
It preserves arbitrary u64 IDs, 2D/3D, variable radii, arbitrary centres, immediate
mutation visibility, and the existing extreme-distance predicate. Invalid input
emits no callbacks. Worker and output-window choices are explicit resource
parameters; more workers do not imply less CPU or memory use.

The fresh continuous protocol first proves every query, then reconstructs the
same trace for separate uninterrupted timing. It charges updates, preparation,
spatial ordering, allocation, scheduling, thread creation/join, full materialized
u64 outputs and every-ID consumption. It records each query frame. No oracle time
is subtracted from overlapping timed workers. Input parsing, initial fixture
loading, networking, simulation, serialization and a permanent worker pool are
outside that frame scope. This is service latency without an offered-load queue;
it must not be called scheduled-to-ready latency or a server p99.

The strongest selected external indexes also receive the same coherent scoped
scheduler at one/four workers. This prevents attributing a general multicore
benefit uniquely to the new index. Output windows 1/8/64 and worker counts 1/2/4
are separate experimental dimensions. Results from different output/timing
contracts are not pooled into the original strict superiority gate.

The original serial screen completed 84/84 jobs and the randomized output-window
sweep completed 96/96 jobs, all with full membership proofs. These v1 runs have a
serialized consumer. Dense results showed that tiny groups can increase overhead;
there is no universal best output-window choice. The new explicit concurrent
consumer path is measured separately using protocol v2, with identical synthetic
per-ID consumption on every implementation. This checksum is a compiler-visible
consumer, not game serialization or realistic network work.

The v2 proof exercises the same parallel-consumer path as timing. It validates
all IDs under a proof-only mutex, then recreates the trace for uninterrupted
concurrent consumption using per-group atomic summaries. Every timed query is
visited once; count and order-independent checksum agree with the independent
proof. Callback order never substitutes for original query identity.

Protocol v2 completed 35/35 additional full-path proof jobs. Parallel consumers
improved the dense case, but one crowded-scene run had large outliers (up to
146 ms) on the shared host. They remain retained rather than discarded. The
benchmark also wrote a shared AtomicBool per query and did a scalar multiply per
returned ID. Those are synthetic measurement costs, not required index work.

Protocol v3 keeps the same exact library and checksum value, factors the checksum
using modular distributivity, and removes per-query shared timed counters. Every
query/ID/duplicate/omission is still checked in the separate full proof; timed
runs check aggregate count and the identical order-independent checksum. Group
counters remain atomic. This reduces instrumentation contention and makes the
consumer's reduction SIMD-friendly. A direct wrapping-arithmetic equivalence
test passes, including overflowing u64 inputs. All implementations receive the
same consumer. A v2→v3 timing difference is not an index-algorithm speedup and
must not be presented as one. Binary/protocol identities remain separate.


## Uninterrupted v3 baseline and independent holdout

The v3 baseline/window/seed97531 holdout studies completed 143 jobs, all passing
full parallel-path query identity and membership proofs. On this shared Apple M2
8 GiB host, four-worker public Rill with a 64-query output window gave:

| 20k profile | Mean matches/query | Median query frame (ms) | Worst observed (ms) |
|---|---:|---:|---:|
| Uniform | 7.6 | 4.50 | 5.38 |
| Ordinary density | 69.3 | 6.79 | 7.67 |
| Crowd | 544.8 | 19.32 | 21.61 |
| All visible | 20,000 | 50.12 | 55.19 |
| Floors | 14.7 | 5.64 | 6.49 |
| Teleports | 69.6 | 7.55 | 8.03 |
| 100k targets, 20k observers | 85.7 | 12.37 | 16.65 |

Rill led the three selected external indexes (Kiddo, static AABB, packed R-tree)
at equal four-worker resources on these seven profiles and on the four independent
holdout profiles. This is a selected screen, not the original all-library gate:
only two fresh processes per baseline pair, incomplete competitor coverage, and a
changed output contract. The holdout crowded frame median was 20.71 ms, so a
20,000-query burst still exceeds a 60 Hz simulation-frame allowance. The all-visible
window sweep favored eight-query groups (33.91 ms median), while ordinary/crowded
scenes favored 64. Different binaries and protocols remain separate in raw reports.

## Tree topology and observer cohorts

A global radius-hinted active-axis topology candidate passed the existing 15
library tests plus three targeted thin-world suites in scalar/release modes.
The subsequent primary, independent holdout, original nine-workload and cohort
studies completed another 120 full-proof jobs. It improved several crowded scenes
but gave mixed results elsewhere; the public tree topology remains unchanged.
It is retained as an experimental fork rather than promoted as a universal gain.

The cohort fixture assigns each observer a deterministic phase in a three-frame
cycle. Every observer still receives two complete relevance queries over six
60 Hz simulation frames (20 Hz per observer), with all frame updates charged.
This changes client phase and the position epoch observed; it is a separate
infrastructure choice, not the same-epoch benchmark accelerated by division.

| Cohort profile, public baseline | Median service/frame (ms) | Worst observed (ms) | Observed 16.67 ms misses |
|---|---:|---:|---:|
| Ordinary density, mean 69 matches | 2.18 | 5.19 | 0 / 36 |
| Crowd, mean 545 matches | 6.64 | 8.93 | 0 / 36 |
| All visible, 20,000 matches | 21.64 | 26.28 | 36 / 36 |

These are six-frame traces, three samples in each of two fresh processes. No
clock pacing, offered-load queue, sockets, game simulation or encoding is present.
Motion is tested; structural insert/remove throughput is not yet a performance
proof. An adaptive output-window candidate is the next isolated experiment:
retain the public geometry, sample output density, and reduce worker-local rows
only where complete materialized output would otherwise exceed a soft cache
budget. Sampling overhead must remain charged.


## Adaptive output experiment v1

All 56 primary/holdout/cohort jobs passed full membership and query-visit proofs.
On the paired primary all-visible profile, median service/frame fell from 48.74
to 28.01 ms. On staggered all-visible cohorts it fell from 23.26 to 9.99 ms,
with observed worst frames 30.67 versus 11.30 ms and 36/36 versus 0/36 misses.
No IDs were capped; all 20,000 results per observer were materialized and consumed.

This candidate is **not promoted** on that evidence alone. Primary crowd median
rose 17.23→18.85 ms and cohort crowd rose 5.73→7.30 ms, even though the sample-based
window stayed at 64. Holdout crowd was essentially unchanged (17.03→16.94 ms).
The probe fraction is small; compiler inlining/layout effects and shared-host
variation remain hypotheses, not established causes. A second source variant
moves the identical sampling policy into an out-of-line setup helper, preserving
all validation, output and statistics semantics. It must be remeasured separately.


## Out-of-line setup v2 and public opt-in

The identical sampling policy moved to a private out-of-line helper. All 84
three-process primary/holdout/cohort jobs passed full proofs; its 24 candidate
tests passed release/scalar and harness lint checks. Crowd median service was
17.06 versus fixed-window 17.52 ms in the primary set and 5.80 versus 6.15 ms
in cohorts. This removes the earlier observed crowd regression in this screen,
not proof that compiler layout was its cause. Other profiles remain mixed:
primary floors 5.17 versus 4.64 ms, and 100k-target service 10.93 versus 10.37 ms.
Shared-host outliers remain included; different executable hashes stay separate.

Dense cohort medians were 10.23 versus 31.48 ms. Crucially, adaptive output still
had 2/54 observed 16.67 ms misses (worst 27.57 ms), while fixed output missed
54/54 (worst 35.70 ms). This is not a strict deadline or p99 pass. The earlier
v1 cohort study's 0/36 misses does not erase the later misses.

The public library therefore exposes the measured policy through opt-in
`adaptive-batches`, retaining the default fixed-window choice. The maximum row
configuration remains an upper bound; probes add charged work but neither
callbacks nor statistics. Exact complete materialization is unchanged. Six
adaptive oracle suites join the existing 15 public tests when the feature is
enabled; 21 tests pass SIMD release and scalar validation. Workspace/all-feature
clippy is clean. Feature flags are recorded alongside final public build hashes.
