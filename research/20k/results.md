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

The raw serial screen and window sweep are recorded below after completion.
