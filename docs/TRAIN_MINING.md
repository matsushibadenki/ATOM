# Frozen-policy train difficulty probes

Workflow → Dataset → **Train data collection → Train difficulty** probes new
train tasks with an existing frozen BC policy, then lets the user explicitly
prioritize reference demonstrations for the tasks on which that policy struggled.
This adds task-level failure-directed collection. It does not label policy-visited
states, perform DAgger, optimize weights or certify task difficulty or safety.

## Workflow

1. Select a registered policy in **Policies · history** to restore its exact source
   batch. The difficulty policy selector lists only policies matching the active
   batch hash. No active training job is required beyond the registered artifact.
2. Set the first seed, step limit and total budget in the collection controls.
   Choose a probe pool of **2–16 tasks**. Start runs the policy on new train groups
   only. The original split salt/ratios decide group membership before physics;
   validation/test and groups already present in the source audit are skipped.
   Existing rejected source groups are skipped too. At most 1024 seeds are scanned.
3. All tasks use the authored scene snapshot, independent resets and the original
   Reach profile. Inputs use current observations and previous applied actions,
   with train-only normalization. Teacher reference joint goals are not policy
   inputs. Inference uses the checked best checkpoint without an optimizer.
4. When every requested probe completes, order tasks by **timeout first**, then
   the **share of decision observations outside the train feature ranges** in
   descending order, then **final distance descending** and **seed ascending**.
   A decision observation counts once if any feature falls outside its empirical
   train range, with the same floating-point tolerance as policy diagnostics.
   The GUI shows this count and the number of decisions. Reload recomputes the
   count from the saved Episode and the policy's exact source batch. Earlier
   reports retain their original distance-based order. A feature that stays
   outside the train range throughout a rollout (such as a constant target
   coordinate) can make the share 100%. Equal shares fall through to distance.
   If all tasks succeed, the order is still a relative heuristic, not proof that
   any task needs more data. No partial pool receives priorities.
5. Stop, budget expiry, worker failure or seed-scan exhaustion keeps completed BC
   probe Episodes and an incomplete metadata report, without priorities. The active
   interrupted probe is not published. Counts of scanned, reserved and existing
   groups and any failure reason are shown in the panel.
6. Reload a saved report to verify it and all references. Each row opens the
   existing recorded Episode player. Playback requires no inference or MuJoCo.
7. Press **Use ranked seeds for reference collection**. This copies the first seed
   and horizon and caps the requested reference-attempt count to the completed
   pool. Starting collection now invokes the ordinary reference servo on the top
   requested seeds, in priority order. Clear priorities to return to sequential
   seeds. Changing source batch clears the local priority draft.
8. Only successful reference demonstrations passing the original quality maxima
   enter the augmented batch. BC probe actions are never appended by this flow.
   The original embedded train/validation/test items, split settings and quality
   rules are preserved. Inspect and explicitly select the augmented batch before
   fresh training. Existing checkpoints cannot resume on changed data.

The source, first seed and horizon must match the selected mining report and the
requested count cannot exceed its completed pool. These checks also run in Rust;
changing a GUI draft or bypassing the GUI cannot silently substitute other tasks.
Source audit capacity and the batch-size limit remain enforced by the collector.
Using priorities does not itself start collection, training or policy activation.
Manual curation of ordinary BC probe Episodes remains a separate existing flow;
those records are not expert demonstrations.

## Execution and ownership

Rust owns the shared running job, cancellation, progress and saved results across
WebViews. Scene capture briefly clones the authored project; filesystem, inference
and simulation run off-thread without holding the document lock. The probe job
never changes the shared scene, renderer preview, dirty flag, revision, runtime
sequence or Undo/Redo. Tensors and trajectories stay in the Rust/Python paths;
only command/status/report metadata is sent to the UI.

The total budget is 1–300 seconds, starts before source verification and is checked
between bounded operations and steps. Each probe has a 30-second physics budget.
An in-progress policy startup (20 seconds), inference response (3 seconds), physics
startup/reset (15 seconds), step (3 seconds), filesystem operation or cleanup may
finish after the total deadline. Cancellation uses inference polling and worker
kill/wait cleanup. No package installation, model download, new external Control
API mutation or MCP execution endpoint is introduced.

Probe actions are clamped to the existing joint-position command bounds. The
report retains the number of clamped components; the raw unclamped predictions
are not recorded or recomputed during reload. Task success and timeouts use the
normal Reach rules. Runtime/model versions and seeds must be kept aligned for
reproducibility; a seed alone does not guarantee cross-environment equivalence.

## Artifacts and verification

Files live in the project's `episodes/` directory:

- `reach-mining-<uuid>-<seed>.episode.json`: completed BC probe Episodes, never
  silently relabeled as reference-servo records.
- `mining/mining-<sha256>.json`: configuration, policy ID, source filename/hash,
  checkpoint and training-manifest hashes, actual CPU inference runtime metadata,
  scan/skip counts, per-task Episode hashes/outcomes/distances/steps/clamps,
  completion status/error and priorities. Synced temporary files publish without
  overwriting different content; identical metadata is idempotent.
- Existing `reach-collection-*`, `collections/` and `exports/` artifacts hold the
  separately collected reference demonstrations and augmented data.

Reload verifies the metadata content hash, configuration, current registered
policy/source/checkpoint/job manifest, actual recorded Episodes, task groups,
model/channels/initial-state compatibility, original train assignment, exclusion
of existing groups, seed/horizon, runtime consistency, complete outcomes, scores
and recomputed priorities. A rehashed forged ranking is rejected. Changing or
removing a referenced artifact invalidates the link rather than silently loading
another policy. The catalog lists up to 64 reports.

Collection configuration has an optional `mining` filename. Omission preserves
legacy sequential behavior and serialization; old collection records keep their
original hashes. With a mining reference, collection reload additionally verifies
that its attempted seeds are a prefix of the immutable priorities, even when the
order is not numerically increasing. Keep the mining report, its policy/source/
checkpoint, completed probe Episodes and collection artifacts for later full
verification. Persistence failures can leave independently valid completed files
without a published report; these do not become the active training batch.

## CLI

First register an existing checkpoint and source batch:

```sh
atom-cli policy-register /path/to/episodes bc-example fixture.batch.json
```

A mining configuration uses that exact returned policy filename:

```json
{
  "policy": "policy-<64-character-sha256>.json",
  "start_seed": 0,
  "count": 4,
  "horizon": 300,
  "max_seconds": 60
}
```

```sh
atom-cli train-mine /path/to/episodes project.atom.json mining-config.json
atom-cli mining-inspect /path/to/episodes mining-<sha256>.json
```

To collect reference demonstrations for the top two tasks, use the exact saved
mining filename in the ordinary collection configuration:

```json
{
  "start_seed": 0,
  "count": 2,
  "horizon": 300,
  "max_seconds": 60,
  "mining": "mining-<sha256>.json"
}
```

```sh
atom-cli train-collect /path/to/episodes fixture.batch.json project.atom.json collection-config.json
atom-cli collection-inspect /path/to/episodes collection-<sha256>.json
```

No validation/test task runs or scores are used by difficulty mining. Structural
validation still checks the source package's full contract. Use the existing
validation candidate comparison after fresh training, and reserve fixed test for
final assessment instead of task or model selection.

## Verification (2026-11-10)

- Standard tests cover pool/config/path bounds, complete-pool ranking, timeout/
  distance/seed tie rules, legacy collection JSON serialization and shared desktop
  admission/busy behavior without an active training selection.
- The explicitly run PyTorch/MuJoCo test probes only new train groups, rejects a
  mismatched source before workers, handles pre-cancel and budget expiry before
  inference startup, repeats outcomes/distances/steps/clamps/priorities, cancels
  after one completed probe without priorities, collects top-ranked reference
  tasks in priority order, preserves holdouts and rejects oversized/stale selections,
  rehashed forged priorities and changed Episode files. Checkpoint bytes, source
  bytes and authored project stay unchanged. Old source Episode files were removed
  before mining to verify the self-contained source package path.
- Real CLI QA probed seeds **1, 3, 4, 6** with the frozen 20-step CPU BC fixture.
  All four timed out. Final distances were approximately **1.083, 1.052, 1.032 and
  1.124 m** respectively, producing priorities **6 → 1 → 3 → 4**. The top two seeds
  **6, 1** produced two accepted reference demonstrations. Train groups grew from
  one to three, while complete embedded validation/test items matched exactly.
  These results verify selection and data plumbing, not learned-policy improvement.
- An isolated native macOS Tauri app restored the source policy, started/cancelled
  mining (no priorities), completed a four-task pool, reloaded/verified it, selected
  priorities and collected two accepted reference records in order. Control API
  responses before and after mining/collection matched exactly excluding request
  UUIDs, including scene, revision, sequence, dirty and history. A saved BC probe
  was loaded and played through the existing native controls to frame 300; the
  editor was restored and the disposable QA app exited cleanly.
- Browser plugin/skill was unavailable, so installed Playwright exercised actual
  UI files with real result metadata and a simulated Tauri bridge. English,
  Japanese and Simplified Chinese at **220px/320px** passed matching-source policy
  filtering, invalid-count rejection, start/stop, running-control disabling, no
  partial priorities, saved reload, ranked display, playback dispatch, explicit
  priority selection/clearing, priority collection, explicit augmented-data use and
  stale-priority clearing after source changes. Page identity, nonblank content,
  no error overlay, no application console errors and no horizontal overflow
  passed. Narrow screenshots were inspected; native IPC/physics/playback were
  verified separately above.

Offline visited-state teacher aggregation is implemented in [Teacher supervision](TEACHER_SUPERVISION.md).
Automatic DAgger, support-aware sampling, broader preregistered validation tasks
and measured policy gains remain next. Native floating panels,
other OSes and larger deployments remain pending. Do not treat this relative task
heuristic as a safety boundary or a generalization benchmark.

Final checks: 290 standard workspace tests passed, with 37 environment-dependent
tests explicitly ignored. The real PyTorch/MuJoCo mining test was separately run
and passed, including invalid counter/ranking rejection and priority collection.
Clippy with warnings denied, formatting, JavaScript syntax, diff checks and the
final macOS development app build passed. The final CLI binary reverified both
saved mining and priority collection artifacts.

## Compare collection rankings

Run `atom-cli train-collection-compare <episodes-dir> <project> <config.json>`
with a completed train-distribution mining report. The source batch needs at
least eight fixed validation groups. The comparison uses the batch's entire
validation partition and excludes test scores from fitting and evaluation.

```json
{
  "mining": "mining-<sha256>.json",
  "attempts": 4,
  "collection_seconds": 120,
  "validation_seconds": 300,
  "training": {
    "max_steps": 40,
    "max_seconds": 60,
    "seed": 7,
    "batch_size": 32,
    "learning_rate": 0.001
  }
}
```

The completed pool must contain more candidates than the number of collection
attempts. Each arm collects and quality-checks the same number of attempted
reference demonstrations, starting with the original source batch. One arm
ranks timeouts and final distance. The other ranks timeouts, train range
coverage, and final distance. Both start from the frozen policy's best
checkpoint with a fresh optimizer. The first layer is adjusted to preserve
predictions in raw observation coordinates under each updated normalization.

The JSON report is printed and saved under
`<episodes-dir>/collection-comparisons/`. It records the shared checkpoint,
pool, saturation count and denominator, both sets of selected seeds,
accepted demonstrations, and closed-loop validation scores. The linked
validation campaign records the exact task specification and evaluation
artifacts. Equal seed selections mean that the two ranking rules produced
identical collection inputs for this run.

### Fixed 12-task experiment

The integration test uses validation seeds
`2, 5, 31, 59, 62, 67, 72, 79, 84, 86, 101, 115`.
Both arms start from the same 20-step BC checkpoint and use 40 optimizer steps.
The initial fixture has one train demonstration.

| Collection ranking | Accepted demonstrations | Success | Timeouts | Mean final distance |
|---|---:|---:|---:|---:|
| Final distance | 4 | 1/12 (8.3%) | 11 | 0.9778 m |
| Train distribution | 4 | 1/12 (8.3%) | 11 | 0.9778 m |

All 12 new train probes had an outside-train observation share of 100%.
Both rankings selected seeds `1, 11, 3, 12` in the same order. This run
measured saturation and produced identical collection inputs.

Reproduce with the PyTorch and MuJoCo runtimes configured:

```sh
cargo test -p atom-core --lib real_compare_collection_rankings_on_expanded_validation -- --ignored --nocapture
```
