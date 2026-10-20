# Registered comparison protocol

Status: registered before implementation measurements. Passing has NOT been established.

## Contract

Exact closed Euclidean radius reporting in 2D or 3D over finite binary64 coordinates,
returning every matching stable ID exactly once. Result order is unspecified.
No approximate queries, nearest-N truncation, stale caches or dropped candidates.
Two-dimensional indexes may conservatively project 3D queries and use the common
exact final predicate; projection, filtering and materialization are charged.

The first performance gate concerns complete dynamic workloads, not every possible
API or every dimension. Radius search does not establish nearest-neighbour,
collision detection, rendering, persistence or multiplayer capacity.

## Workloads and controls

Deterministic public generators: dispersed uniform, multiple clusters, one dense
cluster, vertically stacked floors, boundary-crossing motion, teleport churn and
player-like observers among additional points. Include 2,000 observers, smaller
controls, and larger point populations. Reuse identical coordinate traces and
query/radius lists across implementations. Initial build, updates, required
rebuilds, exact filtering and full output-ID production are included in lifecycle
time. Report build/update/query separately too. Do not hide a construction or
update loss behind an isolated query win.

Every three update frames, issue one full observer query batch (60/20 Hz ratio).
Static indexes rebuild when required, inside the measured boundary. Dynamic
indexes receive equivalent updates; adapters may batch updates if the source API
supports that. Benchmark setup, coordinate generation, reference-oracle execution
and equality comparisons are outside timing; output production remains inside.
Consume timed outputs and validate full membership outside timing. Process startup
and dependency compilation are reported separately and excluded from hot runtime.

Retain the existing production-style cached cell-group approach as a separately
labelled, different-semantics control, never a direct exact-query winner.

## Sources

Inventory about 50 real source implementations, not 50 parameter settings of one
tree. Pin registry versions and source commits. Distinct data structures in one
project may be separate entries, but wrappers around the same backend are labelled
and are not counted as independent algorithms. Every entry receives an explicit
status: measured, incorrect, incompatible, build-failed, or pending. Incompatibility
or failure is not a performance victory. Original implementations and adaptations
are distinguished. Keep language/runtime and binding costs visible.

## Measurement

Run release optimizations without fast-math, native binary64 semantics, identical
inputs and output requirements. Randomize competitor execution order, use multiple
fresh processes and paired repeated samples, and retain raw samples. Report median,
p95/p99 where sample count supports the percentile, allocation/RSS evidence, result
cardinality, source/compiler/hardware identity and concurrent machine load.
Do not suspend or terminate other workers. Shared-machine diagnostics are not
isolated-host capacity evidence. Re-run finalists on holdout seeds, radii, densities
and population sizes. Gate evaluation uses a separate reproduction command.

## User acceptance gate

The requested superiority goal remains OPEN while any registered comparable
implementation is unmeasured or any mandatory scenario is a runtime loss/tie.
For robust dominance, the upper confidence bound of the paired candidate/baseline
complete-workload time ratio must be below 1.0 for every mandatory comparison.
Publish ties, losses, unsupported operations and missing evidence. Memory and
capability differences are reported independently; a runtime gate does not claim
dominance on every resource or on untested workloads.

## Storage

Use one small library workspace, share registry caches, and fetch minimal source
inputs. Do not clone large repository histories. Retain source pins, adapters,
fixtures/generators, correctness evidence and raw measurement data. Declare exact
completed generated paths safe to retire. Required work may proceed below the
100 GB reserve; no unrelated protected inputs or active workers are disturbed.


## Holdout registration (before measuring the revised candidate)

The development trace uses seed 2051. The first holdout uses seed 97123 and the
`holdout` generator profile: populations 47 / 4096 / 2048 / 2048 / 2048 / 2048 /
2048 / 2048 / 32768 across the same nine scenario families; radii 11 / 37 / 31 /
67 / 96 / 23 / 13 / 89 / 73. It changes cluster widths, dense volume, floor count
and spacing, boundary-grid dimensions and dispersed density. It retains 2,000
observers where the population supports them. This is an unseen validation trace,
not a replacement for any development loss. Subsequent changes informed by it
require another unseen trace before a generalization claim.
