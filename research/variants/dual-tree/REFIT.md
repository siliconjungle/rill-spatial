# Main-library refit versus full reconstruction

The frozen main library did not show the suspected bounded64 refit regression in
this separate uninterrupted-timing diagnostic. Reconstructing from current points
at every query phase gave nearly identical query times and increased total service
time on all four selected traces. This does not support changing the main refit
policy based on the earlier interrupted-oracle screen.

`examples/refit_diagnostic.rs` imports the actual root library through a dev-only
`main_rill` path alias, with default SIMD features preserved. It makes no root
source modifications and does not use this variant's own spatial index. The root
library sources were frozen and hashed before and after the experiment:

The exact frozen main sources are retained in Git commit
`b7580ed1f698e3d14114c5a8f9c25a13dc081164`; both hashes below were verified against
that commit. Later main API changes are a different experiment version.

- `src/lib.rs`: `73c7390eb0f665d51b0c3049a72ae4a0b96673d5469d5e5bb8a35a73c8186a21`
- `src/hierarchy.rs`: `299cd36c34e398b51832a44d83d92153b13488ddbe61293b3d8a8b5273d618ca`

Both modes maintain the same index through all six simulation frames, applying
the same upserts. Normal mode calls `prepare()` only at the two query phases.
Reconstruction mode additionally discards the old index and calls
`from_points(current_points)` at every query phase, then calls `prepare()` on that
new index. Its reconstruction timer includes allocations, ID lookup construction
and old-index destruction. It still pays the same upsert protocol, even though
reconstruction makes those prior index updates redundant. Initial `from_points`
does not prepare either mode: initial tree construction is charged to the first
query-phase preparation, not hidden in the initial-build row.

The fixture's wire positions map bijectively to arbitrary sparse u64 identities
using odd multiplication and xor. Both modes preserve these stable identities
through updates and queries; no dense-ID shortcut substitutes for the contract.
The independent oracle maps returned IDs back to reference points and checks
every present and absent membership plus duplicate rejection. It runs for both
modes with output windows 1 and 64 before any timing. All 16 complete trace/mode/
window proof configurations passed. Timed runs replay identical inputs without
oracle work between windows and consume every ID. Complete result vectors are
also exposed to `black_box` before per-ID consumption.

Three repetitions reverse window/mode order on odd repetitions. Each independent
run has equally cold result vectors, with clearing/allocation charged inside
`within_into` and capacity reused across windows/frames. Full outputs are bounded
by 1 or 64 reusable vectors; no full-query result matrix exists. The measured
process RSS was 21.35 MB for bounded64 and at most 17.47 MB for the other traces.

The totals below are medians across three repetitions, in milliseconds, including
initial build, all six frames' upserts, preparation, full reconstruction where
selected, all query work and every-ID consumption. Query-only fields sum the two
20,000-query phases. These are concurrent diagnostic service intervals, not
end-to-end server wall latency or controlled acceptance measurements.

| Trace | Window | Normal total | Reconstruct total | Normal query | Reconstruct query |
| --- | ---: | ---: | ---: | ---: | ---: |
| bounded64 | 1 | 39.26 | 41.95 | 35.21 | 35.24 |
| bounded64 | 64 | 39.35 | 41.60 | 35.62 | 35.31 |
| uniform | 1 | 19.55 | 22.15 | 16.36 | 16.51 |
| uniform | 64 | 19.65 | 22.19 | 16.92 | 16.94 |
| floors | 1 | 29.19 | 31.61 | 25.81 | 25.74 |
| floors | 64 | 29.34 | 31.85 | 26.46 | 26.56 |
| teleports | 1 | 40.19 | 41.47 | 35.09 | 34.94 |
| teleports | 64 | 39.93 | 41.75 | 35.09 | 35.31 |

For bounded64/window64, normal preparation in query frame 5 took 0.068 ms, versus
1.181 ms for fresh preparation plus 0.703 ms reconstruction. Their query times were
17.738 and 17.596 ms respectively. The first query phase was also similar: 17.855
versus 17.722 ms. There is no large post-refit query degradation here. Low normal
preparation cost is consistent with retaining a refit; teleports' higher normal
preparation cost is consistent with rebuilding. This example does not instrument
the private keep decision, span ratio or node-visit counts.

The earlier large bounded64/teleports gap is not reproduced: normal window64
totals are 39.35 versus 39.93 ms. This rules out neither profile effects nor other
traces. It only fails to support the specific refit explanation under this frozen
source and uninterrupted-timing protocol. The root's earlier screen performed
full oracle scans between paused timing windows, and shared workload also varied;
those are plausible confounders rather than established causes. The root's new
common proof-then-uninterrupted-time comparison should decide performance.

The isolated workspace uses its existing default release profile, rather than
claiming the root benchmark's thin-LTO profile. The alias resolves `wide 0.7.33`,
`safe_arch 0.7.4` and `bytemuck 1.25.2`; exact lock/source/input hashes and all
repetitions and per-query-frame fields are in `refit-results.json` and the four
`refit-*.jsonl` files. The compiler was rustc 1.96.0. Preparation returned no source
configuration or direct Cargo arguments; local Cargo was used throughout.

Reproduce from `/Users/james/Documents/ChatGPT/3d-modelling/rill-spatial`:

The parent main-library checkout must contain the frozen commit's source version
above. Running these commands against a later main API exercises that later code;
it does not reproduce the retained measurements.

```sh
ink-cache prepare-sources "$PWD/research/variants/dual-tree" --offline
cargo build --manifest-path research/variants/dual-tree/Cargo.toml --release --example refit_diagnostic
/usr/bin/time -l research/variants/dual-tree/target/release/examples/refit_diagnostic benchmarks/twenty-k/bounded64.bin 3
/usr/bin/time -l research/variants/dual-tree/target/release/examples/refit_diagnostic benchmarks/twenty-k/uniform.bin 3
/usr/bin/time -l research/variants/dual-tree/target/release/examples/refit_diagnostic benchmarks/twenty-k/floors.bin 3
/usr/bin/time -l research/variants/dual-tree/target/release/examples/refit_diagnostic benchmarks/twenty-k/teleports.bin 3
```

Append `--verify-only` for complete proof without timing. Previous dual-tree and
threaded evidence remains retained. Before adding this dev dependency, the original
dependency-free Cargo manifest/lock were preserved as the selected historical
reproduction baseline in `repro/original-Cargo.{toml,lock}`. Those reproduce the
older manifest hashes when used with the unchanged original dual-tree sources;
new diagnostics use the current manifest/lock and their separately recorded hashes.
For a minimal historical dependency-free reproduction, materialize only the prior
declared sources/examples/tests, excluding the new `refit_diagnostic.rs` example
that requires `main_rill`.
