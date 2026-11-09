# Recorded BC rollout diagnostics

The Rust core analyzes a registered, verified evaluation to expose input support,
servo-target limit use and descriptive failure classes. It does not run Python,
change weights, rank policies or modify a scene. The evaluator supports separate **validation** and fixed **test** partitions;
diagnostics preserve the selected partition. Fixed-test diagnostics must not be
used to select training settings or best weights using test outcomes.

## GUI and CLI

In **Workflow → Dataset → Policies · history**, choose evaluation **A**, open
**Policy diagnostics** (Japanese: **方策診断**, Simplified Chinese: **策略诊断**),
and press the focus icon. A compatible evaluation B is not required. The operation
uses the shared Rust busy/revision guard and runs disk/analysis work off-thread.
The panel shows:

- Number of train observations and evaluated pre-action observations; row counts
  and rates outside the empirical train envelope and above the separate z heuristic.
- Each task's seed, outcome, classification, closest/final distance, longest
  consecutive hold versus the required hold, outside-envelope rows and clamped
  action-component count from its original rollout report.
- The 12 input features with the most outside-envelope observations, breaking ties
  by maximum absolute z; joint features identify object ID/local joint axis and
  Cartesian features identify X/Y/Z. All features remain in the saved report.
- Every joint's applied-target limit count/rate, maximum tracking error and maximum
  action change, sorted by limit use then tracking error. Expanded lists have their
  own bounded scroll areas for narrow panels.

Changing A hides an unrelated diagnostic; it does not re-label the old result.
Recompute to display another evaluation. Diagnostic status and result are shared
across windows. There is no automatic inference or model selection. The report
filename appears in the status tooltip.

```sh
atom-cli policy-diagnose ./episodes evaluation-<sha256>.json
```

The CLI calls the same Rust analysis/save functions and prints the saved filename
and complete report. Neither Python nor MuJoCo needs to be installed to analyze
existing valid artifacts.

## Train-only reference and observation alignment

The strict saved batch loader and existing BC preparer reconstruct the exact
feature order: joint position, joint velocity, endpoint, target, previous applied
action. Only the **train** partition defines per-feature minimum/maximum and the
existing train-only population mean/standard deviation. Validation/test rows never
expand the envelope or fit the normalization. The preparer's existing standard
scale fallback (1 when train std is below 1e-8) is preserved for constant features.

For feature `i`, an input is outside the envelope when it is below train minimum
or above train maximum by more than:

```
epsilon[i] = 1e-6 * (1 + max(abs(train_min[i]), abs(train_max[i])))
z[i] = abs((value[i] - train_mean[i]) / train_scale[i])
```

`z > 3` is counted separately. No Gaussian distribution or calibrated probability
is assumed. These f64 descriptive statistics are not a claim of bitwise equality
with float32 intermediate model activations. Small/constant training sets can
produce many flags; the envelope is an empirical record, not a physical safety
boundary or a proof that distribution shift caused failure.

Each recorded **pre-action** observation is examined once, including reset and
excluding the terminal observation that did not produce another action. The
previous-action input is the policy's own preceding applied command, or the
manifest's initial command on reset. Teacher reference joint goals, future frames
and subsequent teacher actions are excluded. Row percentages count a row once if
any feature is flagged; per-feature counts are separate. Longer Episodes contribute
more observations, so these rates are not task-weighted probabilities.

## Joint and failure metrics

`limit_contacts` counts **applied servo targets**, not physical collision contacts
or necessarily actual qpos at a limit. A component is counted once when its target
is within 1e-6 rad of either joint range endpoint. This differs from
`clamped_components`, the original rollout's count of raw predicted components
changed by clamping. Merely commanding a legal endpoint is not evidence of an
invalid prediction.

Maximum tracking error is absolute applied target minus the next post-action qpos,
using each channel's qpos index. Maximum action change is absolute applied target
minus the preceding applied target, including initial command → first action.

Classification uses **post-action** distances only, with the manifest's tolerance:

- `success`: the existing Reach outcome was success.
- `reached_without_hold`: timeout, but at least one post-action distance was within
  tolerance. The report also records the maximum consecutive hit count.
- `never_reached`: timeout without any post-action observation within tolerance.

The existing success/timeout outcome is never replaced. Closest distance includes
reset; hold/classification excludes reset, matching the Reach success convention.
These classes distinguish observed behavior, not underlying control/physics causes.

## Integrity, persistence and bounds

Before analysis, registry links, the original training job/checkpoint, source batch,
rollout manifest and Episode hashes are revalidated. The visited task groups must
match the original selected validation/test groups exactly; model/channels, horizon and initial action
must match their saved learning contract. Each Episode is streamed through the
analysis rather than retained in a GUI window. Existing 64 MiB batch/Episode and
256 MiB rollout caps remain in force; the diagnostic report is capped at 256 KiB.

`episodes/diagnostics/diagnostic-<sha256>.json` is a content-addressed sidecar with
schema version 1, format `atom-bc-diagnostics-v1` and algorithm
`train-envelope-z3-v1`. It records policy/evaluation IDs, source/checkpoint/evaluation
manifest hashes, comparison key, fit/evaluation partitions, feature/joint aggregates
and per-task summaries. Synced temporary-file/no-overwrite publication is idempotent;
conflicting existing bytes are refused. Original reports, batches, Episodes and
checkpoints remain untouched. The analysis version/thresholds are part of the saved
contract; they are not editable tuning controls in this version.

## Development verification (2026-11-09)

- Unit checks confirm train-only envelope fitting, held-out-value isolation,
  numerical tolerance, separate envelope/z flags and consecutive post-action
  success/timeout classification. Shared-job tests cover missing active export,
  revision/busy admission and the distinction from training/rollout jobs.
- The explicit real PyTorch/MuJoCo test trains, records, registers and diagnoses an
  actual BC run, checks the 42-input/12-joint shape and observation counts,
  idempotent persistence, and refuses diagnosis after checkpoint tampering.
- In the tiny development fixture (train seed 0, validation seed 2, test seed 20),
  25 train observations were available. The 300-step BC timeout had 300/300 rows
  outside the train envelope and 300/300 with at least one `|z| > 3` feature.
  Closest distance was 0.299704 m, final distance 1.12237 m, maximum hold 0/5,
  and 3253 predicted components were clamped. This exposes limited support and
  command behavior; it does not establish a causal explanation or policy quality.

[Validation-partition closed-loop development](VALIDATION_REACH.md) is now
available. Improvements based on train/validation evidence are the next step. Wider holdouts, other OSes and native
floating-window event checks remain pending.


The isolated native Tauri QA app successfully diagnosed the same saved run and
showed its input/joint details. The saved diagnostic matched the CLI result.
Control API responses before/after diagnosis were identical after removing only
the request UUID, including sequence, revision, scene, dirty state and Undo/Redo.
An early QA launch was slow to initialize the WebViews; the normal build eventually
rendered and passed the interaction check. Temporary startup tracing was removed;
no dependency gate, CSP or runtime behavior was weakened. The QA app exited cleanly.

Actual frontend files plus the real diagnostic fixture were tested with installed
Playwright because no Browser skill was available. English, Japanese and Simplified
Chinese at 220px/320px passed diagnosis dispatch, busy/completion handling, feature
and joint counts, page identity, nonblank content, no framework overlay, no app
console errors and no horizontal overflow. Metrics wrap at meaningful boundaries;
expanded detail lists scroll independently. This fixture-based browser check is
separate from the native Rust/IPC verification above.

Final checks: 281 standard workspace tests passed, 35 environment-dependent
tests ignored. The real PyTorch/MuJoCo registry/diagnostics test was also run
explicitly and passed. Clippy with warnings denied, Rust formatting, JavaScript
syntax, diff checks and the macOS development app build passed.
