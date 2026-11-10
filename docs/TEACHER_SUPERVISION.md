# Offline teacher labels on policy-visited train states

Workflow → Dataset → Train data collection → Train difficulty → **Visited-state
teacher labels** builds a supplementary training batch from completed BC probe
trajectories. It labels the states the learner actually encountered instead of
collecting another reference rollout from the initial pose. This is offline
teacher-data aggregation; it does not automate DAgger iterations or establish a
policy improvement.

## Workflow

1. Select the source policy/batch and load a complete, matching train difficulty
   report. Probe groups must be train-only, with immutable source/checkpoint links.
2. Choose the number of top trajectories (1–16, not exceeding the completed pool),
   state interval (1–50) and labeling budget (1–300 seconds). Defaults use the top
   two trajectories and one state every ten control steps.
3. Build labels. States are sampled before actions, including reset and excluding
   the terminal frame. Successful and timeout BC trajectories can both contribute.
   The teacher is the existing Reach **reference-goal position servo**, whose
   target is reconstructed from the validated Episode manifest. It is not a
   learned teacher, optimal controller or demonstrated recovery guarantee.
4. Review per-seed state counts and the saved result. Stop/budget expiry saves
   metadata about processed trajectories but publishes **no partial batch**.
5. Load/verify the saved result, then explicitly choose **Use teacher-labeled
   batch for fresh training**. This clears the old checkpoint/policy selection.
   Diagnose the adapter or start a fresh BC job. Resuming an old checkpoint with
   changed data remains forbidden. Label creation never starts optimization.
6. Compare newly trained policies on the same fixed validation tasks. Reserve
   fixed test for final assessment, not data or model selection.

Label creation performs file validation and deterministic target generation only;
it does not launch physics, policy inference or a Python optimizer. Windows share
one Rust-owned job, progress, cancellation and report. The GUI receives metadata,
never embedded Episodes or tensors. The authored scene, native renderer preview,
revision, sequence, dirty flag and Undo/Redo stay unchanged.

## Applied actions and teacher targets are separate

The original Episode retains every **actually applied BC action**, observed
qpos/qvel, pose, distance and outcome. Those observations are not rewritten to
pretend the teacher drove the recorded physics. Original batch datasets and their
curation report remain intact.

For a selected pre-action frame `i`, the new BC sample is:

- **Input:** current channel-ordered joint positions/velocities, endpoint/target
  positions and the previous **applied** action, using the same feature function
  as real policy inference. Reset uses the authored initial servo targets.
- **Target:** `manifest.reference_action`, the existing reference-goal servo's
  absolute joint-position target in radians.
- **Identity:** the original Episode UUID, deterministic task group and frame
  index. Indices may have gaps when the interval exceeds one.

The teacher target is never substituted into the input's `previous_action` and
never appended as an extra feature. Future observations, terminal outcomes and
teacher joint goals do not become policy inputs. The existing target world
position remains the normal task input.

Only train gets supplementary samples. Normalization is recomputed from the
combined **train** inputs, including labeled states. Original raw validation/test
samples stay exactly equal; they never contribute to normalization or optimization.
Their normalized values may change because the train-derived mean/std changes.

Supplementary labels intentionally allow failed trajectories. The original
success-only/export policy and quality maxima continue to validate the original
curated datasets; they are **not** applied as success/quality filters to these
failed-state labels. Instead, label Episodes must be complete, structurally valid,
checksum-consistent, model/channel/profile/reset compatible and assigned to train.
The teacher targets follow the validated deterministic servo rule and joint bounds.
This does not certify dynamics, collision safety or recovery from every state.

## Saved format and compatibility

The existing `atom-reach-batch-v1` package gains an optional top-level `labels`
array. Each entry includes:

- Teacher tag `reference_goal_servo_v1`, stride, mining filename, policy ID,
  checkpoint hash and parent source hash.
- The original complete BC Episode and its checksum. Its manifest proves the
  deterministic reference-goal target; applied-action provenance remains intact.

The loader validates all entries, rejects unsupported teachers, held-out groups,
changed manifests/actions, duplicate Episode IDs/checksums, mismatched models or
invalid strides, and materializes supplementary targets only in train. The combined
original audit/label capacity is 64 Episodes and the package stays within 64 MiB;
prepared splits retain the 32,000-row cap. Existing labeled groups are treated as
existing tasks by subsequent mining/reference collection.

The original audit and datasets describe original records. They are not silently
expanded or relabeled as successful reference trajectories. Learning summaries
separately expose `teacher_rows` and `teacher_episodes`; total train rows/groups
include the supplement. The Python adapter validates the actual tensor counts and
recomputes train normalization as before.

Empty `labels` and zero teacher counters are omitted during serialization. Legacy
sequential collection packages, prepared artifacts, registry/checkpoint hashes and
inference paths preserve their prior representation. Old readers that do not know
the extension may reject a labeled package; they must not ignore its labels.
New labeled batches receive a different source/artifact identity and require fresh
training. Existing supplementary labels survive later reference collection.

Artifacts are stored beside the project under `episodes/`:

- `exports/supervision-<uuid>.batch.json`: the self-contained augmented package.
  It embeds the new Episodes, so training needs no external mining/checkpoint or
  original Episode files once the package has passed format validation.
- `supervision/supervision-<sha256>.json`: configuration, teacher/source/policy/
  checkpoint identity, selected seeds/groups/checksums, processed state counts,
  status and optional exported filename/hash.

The builder uses a bounded, synced temporary package, validates it, then publishes
without overwriting another file. Cancellation/budget is checked at operation
boundaries and before publication. File/manifest validation, serialization or
cleanup in progress can finish after the requested second; it is not an exact
wall-clock cutoff. Metadata publication failure can leave an independently valid
export without a report, but never silently activates it.

Saved-result verification checks report hash/schema/counts, the complete matching
mining pool and source, all original datasets/audit/prior-label prefixes, exact
new Episode checksums and teacher provenance, selected order/stride, augmented
hash and total labels. Preserve original source, mining/policy/checkpoint links
and probe Episodes for full provenance reinspection. The augmented package itself
is independently loadable for training. The GUI catalog lists up to 64 reports.

## CLI

After creating a complete report with `train-mine`, use its exact filename:

```json
{
  "mining": "mining-<64-character-sha256>.json",
  "count": 2,
  "stride": 10,
  "max_seconds": 60
}
```

```sh
atom-cli teacher-label /path/to/episodes teacher-config.json
atom-cli supervision-inspect /path/to/episodes supervision-<sha256>.json
atom-cli learning-diagnose /path/to/episodes/exports/supervision-<uuid>.batch.json
atom-cli learning-train /path/to/episodes/exports/supervision-<uuid>.batch.json /path/to/episodes/training/bc-teacher training-config.json
```

No model download, dependency installation or external Control API/MCP mutation
is added. The existing training worker still reports frozen-checkpoint test
metrics after validation-based checkpoint selection; labeling does not use those
metrics to choose samples.

## Verification (2026-11-10)

- Standard tests verify that the input tail contains the actual prior BC action
  while the label is the reference target, exclude terminal frames, exercise
  sampling intervals and train-only normalization counts, preserve raw holdout
  samples and reject held-out, duplicate, unsupported-teacher, invalid-stride,
  manifest and applied-action tampering. Desktop admission requires a source but
  no active checkpoint, and uses the shared cancellable job slot.
- The extended real PyTorch/MuJoCo integration test labels real BC probe states,
  verifies exact validation/test samples and inference-feature equality, trains a
  new CPU checkpoint and reloads it, rejects old-checkpoint resume, cancels after
  one trajectory without an export, expires the budget, retains labels through
  subsequent reference collection and rejects modified labeled data.
- CLI QA labeled seeds **6 and 1**, both timeout trajectories, every ten states:
  **60 teacher states**. Total rows changed from **25/24/106** to **85/24/106**
  (train/validation/test). Original audit and datasets matched exactly. Real
  PyTorch tensors, fresh 20-step training, checkpoint reload and a validation-only
  comparison with the source policy passed. Both policies timed out on the single
  fixed validation task; final distance changed from approximately **1.033 m to
  0.736 m**. This is a tiny fixture result, not an improvement in task success rate
  or demonstrated generalization.
- An isolated native macOS Tauri app restored the original policy, loaded and
  verified the completed mining report, created 60 labels, reloaded/verified them,
  explicitly selected the new batch, diagnosed real CPU tensor loading and
  completed fresh 20-step training with checkpoint reload. Old resume was disabled
  at data selection. Control API state before and after labeling, selection and
  training matched exactly excluding request UUIDs, including scene, revision,
  sequence, dirty and history. The disposable QA app exited cleanly.
- Browser plugin/skill was unavailable, so installed Playwright tested actual UI
  files with real metadata and a simulated Tauri bridge in English, Japanese and
  Simplified Chinese at **220px/320px**. Invalid interval, start/stop, disabled
  running controls, no partial batch, per-seed counts, saved verification, explicit
  data switch, tensor/teacher count display and fresh-training dispatch passed.
  Page identity, nonblank content, no overlay, no app console errors and no
  horizontal overflow passed; screenshots were inspected. Native Rust IPC, actual
  tensors and training were verified separately above.

State-dependent IK teachers, task-relevant channel masks/weighting, recovery
validation, automatic DAgger rounds, teacher/learner intervention mixtures and sample/trajectory weighting and broader preregistered
validation remain next. Other OSes and native floating-panel event delivery remain
pending. The reference-goal teacher and this offline aggregate are not a robotics
safety or optimal-control guarantee.

Final checks: 292 standard workspace tests passed, with 37 environment-dependent
tests explicitly ignored. The extended real PyTorch/MuJoCo test was run separately
and passed, including a second manual mining/label aggregation round with existing
labels retained. Clippy with warnings denied, formatting, JavaScript syntax, diff
checks and the final macOS development app build passed.
