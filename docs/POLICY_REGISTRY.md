# Policy registry and fixed-test evaluation history

ATOM keeps local, immutable policy/evaluation links beside the project, under
`episodes/registry/`. The Rust core owns registration, selection and comparison;
WebViews receive metadata and summary metrics, never training tensors or Episode
frame arrays. Registry operations do not mutate the scene or its Undo history.

## Workflow

Open **Workflow → Dataset → Policies · history** (Japanese: **方策・評価履歴**;
Simplified Chinese: **策略・评估历史**).

- The plus icon registers the loaded training job's frozen **best** checkpoint and
  its exact exported batch. Registration is idempotent. Loading an offline test
  report still registers its underlying training job, not the evaluation folder.
- Select a registered policy and press the return icon. Rust verifies the source
  batch and complete checkpoint job, restores their learning context and original
  curation settings, and clears the previous curation source selection/report and
  local selection drafts.
  No reanalysis is required to evaluate the restored policy. Selecting a policy
  does not start inference or edit the scene. Registry/history viewing and policy
  selection do not require Python; starting a new rollout does.
- Starting a GUI closed-loop comparison registers its policy automatically. A
  completed result is linked to that policy. A cancelled/failed physics run has no
  committed evaluation link. Explicit registration also links matching historical
  rollouts among the latest 64 saved runs, including runs created before registry
  support. An already registered policy can be registered again to backfill runs.
- History **A** lists saved evaluation links across policies. **B** lists only
  other evaluations with the same partition and comparison key. Both different policies and
  repeated runs of one policy are supported. Dropdown labels start with a short
  checkpoint hash; tooltips show full selection identities.
- Press the check icon to verify the complete linked artifacts and compare them.
  The table shows BC successes/total and success rate, timeout count, and **mean
  final distance over all fixed tasks in the selected partition**, including failures. Rows show each
  seed's two outcomes and final distances. There is no success-only filtering or
  silent omission of failed artifact loads.
- A/B return icons open the corresponding saved rollout in **Reach · closed
  loop**. Its existing per-controller replay icons load the verified Episode into
  the native wgpu Reach player. Playback, pause/seek and return to editor retain
  their existing semantics.

Registry/history lists reload on application startup, with one refresh initiated
by the workflow WebView. Policy selection and the currently displayed pair are
session state; the application does not automatically activate a policy, run
inference or resume playback on restart.

## Stored correspondence

`policy-<sha256>.json` contains schema version 1, local job/batch basenames,
training job manifest SHA-256, best checkpoint SHA-256, exact batch SHA-256 and
prepared learning artifact SHA-256. The policy ID hashes the full record: separate
training runs remain traceable even if their validation-selected best weights
happen to be identical. The job report retains training configuration and lineage.

`evaluation-<sha256>.json` contains schema version 1, policy ID, rollout basename,
rollout manifest SHA-256 and the comparison key. The rollout manifest in turn
hashes its report and every controller Episode. Training jobs, exported batches
and rollouts stay in their existing directories; registry entries reference them
rather than copying large artifacts.

Records are bounded to 16 KiB, written through a synced temporary file and
no-overwrite publication. Existing records with different bytes are refused.
Registry IDs and referenced filenames are validated; there are no arbitrary path
arguments in the GUI. The registry allows at most 128 policy/evaluation records.
Missing or changed dependencies fail selection/comparison; hash-invalid records
are surfaced in the registry issue list. A catalog refresh is a lightweight
metadata/report-hash check, not a full dependency verification. Checkpoint, source
batch and Episode verification happens on use. Hashes detect changes; they are not
authorship signatures and this is not an arbitrary model importer.

A filesystem or registry-capacity failure after a rollout commits can leave a valid
rollout without a registry link. Its existing saved-rollout loader remains usable;
the error is reported and registration/linking can be retried after resolving the
storage issue. No previous artifact is overwritten. Rename/delete, dependency
relocation, a remote/team registry and registry garbage collection are not provided.

## Comparison identity

The comparison key is computed from verified baseline Episodes, sorted by task
group, and includes:

- The selected partition’s complete fixed-group set, seed/robot/horizon, Reach profile, model hash,
  channel order/limits, endpoint and target.
- Initial joint positions/velocities, endpoint and applied action, physics timestep,
  substeps, success tolerance and consecutive-hit requirement.
- Physics engine/Python/platform identity, policy runtime CPU/thread/Python/PyTorch
  identity, architecture, float32 preprocessing and action clamping semantics.

Training batch and checkpoint hashes are deliberately **not** part of this key:
new training data or another checkpoint may be compared if the actual fixed test
conditions match. Their separate hashes remain recorded in each policy/run link.
Different test sets, horizons, model/reset states or runtime conditions are refused
by the core even if a client bypasses the GUI's B filter. A comparison never
repartitions data, runs an optimizer or selects best weights using test results.
Repeated test inspection must not become a model-selection loop.

## CLI

```sh
# Paths below refer to the directory beside a project, not a global registry.
atom-cli policy-register ./episodes bc-<job> batch-<export>.batch.json
atom-cli policy-list ./episodes
atom-cli policy-link ./episodes policy-<sha256>.json rollout-<run>
atom-cli policy-compare ./episodes evaluation-<a-sha256>.json evaluation-<b-sha256>.json
```

The CLI uses the same Rust functions as the GUI. `policy-register` records a policy;
`policy-link` explicitly links one committed run. Historical-run backfill is a GUI
registration convenience. `policy-list` returns metadata plus integrity issues;
`policy-compare` revalidates all dependencies before returning aggregates and
seed-level results. Derived comparison tables are recomputed; the immutable
policy/evaluation links and original reports are the persisted history.

## Verification (2026-11-09)

- The real PyTorch/MuJoCo test registers two training jobs, links repeated rollouts,
  verifies idempotence and exact paired results, compares different checkpoints on
  the same fixed test group, rejects a wrong-checkpoint link, rejects a deliberately
  different-runtime fixture, and rejects selection/comparison after checkpoint
  tampering. Standard tests cover immutable registry records, tamper reporting,
  missing dependencies, path rejection and shared revision/busy admission.
- Isolated native Tauri QA: loaded a saved policy without an active export, restored
  its batch/checkpoint, registered it idempotently, compared two real model reports,
  confirmed the incompatible-runtime fixture was absent from B, opened evaluation
  A and replayed its BC Episode. A new GUI rollout automatically added its registry
  evaluation link. Two policies and four evaluation links survived restart; clean
  startup no longer produced competing refresh errors. The QA app exited cleanly.
- In the tiny seed-20 fixture both BC policies timed out (0/1 success, one timeout).
  Final distances were 1.12237 m and 0.601214 m. The GUI showed these values rather
  than implying successful control or a general performance improvement.
- Control API checks after playback returned to the same authored revision, scene,
  dirty flag and Undo/Redo state. The transient sequence advanced during playback,
  as expected; request UUIDs were excluded from that comparison.
- Rendered UI checks used the actual frontend files, real registry/report fixtures
  and a simulated Tauri bridge with installed Playwright because no Browser skill
  was available. English/Japanese/Simplified Chinese at 220px and 320px passed
  selection, registration dispatch, compatible-history filtering, aggregate/seed
  rows and opening saved results, with no horizontal overflow, overlay or app
  console errors. Native IPC/physics verification was performed separately above.

Other OSes and floating-window native event QA remain pending. The fixture size
is insufficient for performance or generalization claims.

Final checks: 278 standard workspace tests passed, 35 environment-dependent tests
ignored. The extended real PyTorch/MuJoCo registry/comparison test was run explicitly
and passed. Clippy with warnings denied, Rust formatting, JavaScript syntax, diff
checks and the macOS development app build passed.

Saved evaluation A can also be analyzed without another comparison run using
[recorded BC diagnostics](BC_DIAGNOSTICS.md). This adds train-only support and
servo-limit/timeout summaries without updating weights or selecting a model.

Evaluation history now carries an explicit validation/test partition in its views
and source report. Comparison requires both the same partition and the same fixed
conditions; original test keys remain compatible. See [validation Reach](VALIDATION_REACH.md).
