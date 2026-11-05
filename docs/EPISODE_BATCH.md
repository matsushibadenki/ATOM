# Episode batch curation and group holdouts

Implemented November 5, 2026. This offline Rust pipeline selects existing Reach v1
recordings and prepares a bounded batch of the existing transition format. It does
not run physics, train a model, certify safety or implement LeRobot/RLDS compatibility.
The desktop now also provides a shared Dataset panel in Workflow; the existing
per-Episode tools remain available. The CLI needs neither a running desktop nor MuJoCo.

## Usage

```sh
cargo build --locked -p atom-cli
# Edit this configuration to list your recorded Episodes.
target/debug/atom-cli episode-batch-config > /tmp/curation.json
# Inspect every source and its selection reason; this does not create a dataset.
target/debug/atom-cli episode-batch /tmp/curation.json > /tmp/curation-report.json
# Inspect the report, then publish one package of selected records and their splits.
target/debug/atom-cli episode-batch-export /tmp/curation.json /tmp/curated.batch.json
```

Example request (all object shapes reject unknown fields):

```json
{
  "schema_version": 1,
  "policy": "success_only",
  "split": {
    "salt": "atom-reach-split-v1",
    "validation_bps": 1000,
    "test_bps": 1000
  },
  "limits": {
    "max_abs_velocity_rad_s": null,
    "max_tracking_error_rad": null,
    "max_limit_excess_rad": 0.000001,
    "max_action_delta_rad": null,
    "max_final_distance_m": null
  },
  "sources": ["episodes/run-1.episode.json", "episodes/run-2.episode.json"]
}
```

Sources are explicit files, with paths relative to the **request file directory**.
There is no directory crawler. The CLI resolves them to local absolute paths;
reports/packages retain these paths for local audit and should be reviewed before
sharing. No network requests occur. Sources are sorted to make input ordering
irrelevant. The request is at most 64 KiB, with 1–64 sources, each path at most 4096
UTF-8 bytes, and at most 64 MiB of aggregate source bytes (counting listed copies).
Non-regular files are rejected as individual sources. The loader is bounded by each
observed file size, preventing growth after metadata inspection from bypassing the
analysis budget. Invalid overall configuration or aggregate size aborts the request.

## Selection

Every file passes the existing Episode checksum and structural validation before
quality metrics. Missing, malformed or invalid files remain in the report with a
`source_*` reason; valid files retain their full quality report. Partial recordings
and reset-only records cannot be exported. `success_only` is the default;
`completed` explicitly includes completed timeouts. Success/timeout means the
existing recorded task outcome, not a new physical evaluation.

Limits are inclusive maxima, finite and nonnegative, in the named units. `null`
means no additional maximum. Default selection rejects observed joint-limit excess
above `1e-6` radians; the other optional limits are unset. These defaults are data
curation choices, not universally appropriate robot limits. Reports distinguish
`velocity_limit`, `tracking_error_limit`, `joint_limit_excess`, `action_delta_limit`
and `final_distance_limit`. An unset tracking threshold does not mean perfect tracking.

Exact duplicate **typed Episode content hashes** are included once, choosing the
first lexicographic source path. Different file formatting does not create a new
record. Identical trajectories with different Episode UUIDs are not semantic duplicates
and may still be selected; they share a split if their task group is identical.

Versions with the same Episode UUID and task group (including checkpoints) are
selected at most once: choose the longest eligible recording, then the
lexicographically greatest content hash as a deterministic tie-breaker. Other eligible
versions receive `superseded_episode_version`; incomplete/filtered versions keep
those reasons. A UUID reused across different task groups excludes **all** those
versions as `episode_id_conflict`. This is deterministic version selection, not proof
of common trajectory ancestry or a quality ranking of branches.

The report includes one entry per input, all rejection reasons, selected/rejected
counts and per-split Episode/group/transition totals. The CLI exits 1 if no Episode
is eligible, while preserving the report on stdout. A mix of selected and rejected
inputs exits 0; always inspect counts. Invalid requests return an error instead of a
partial report. Export failures also print the analysis before returning status 1.

## Stable train / validation / test assignment

The existing `quality.split_group` hashes exact model/profile/seed/robot/endpoint/target,
excluding Episode UUID, source naming/revision, horizon and checkpoint length.
Assignment is SHA-256 of the compact JSON array
`["atom-group-split-v1", salt, split_group]`; read the first eight digest bytes as
an unsigned big-endian integer and take modulo 10000. Buckets below `test_bps` are
test, the next `validation_bps` are validation, and the rest are train. Basis points
must sum to less than 10000, reserving a nonzero train probability. The salt is
1–64 UTF-8 bytes without control characters. Keep this configuration fixed across
all batches in one experiment.

All versions and reruns of the same exact task stay together, including rejected
records whose group is available. Adding/removing other tasks, changing input order
or changing quality thresholds never reassigns a group's split. No forced balancing
moves a task across partitions. Small batches can have empty partitions or very
uneven sample counts; warnings expose this. The default 80/10/10 values are **hash
bucket probabilities**, not exact dataset proportions. Do not reroll the salt to
improve evaluation scores. Different seeds/models or similar tasks can still be
correlated; this exact-task holdout is not proof of broader generalization or semantic
deduplication. Keep public SkillPlan evaluation prompts separate from training data.

## Package and publication

`atom-reach-batch-v1` has `schema_version: 1`, `format`, the complete frozen `report`,
and `datasets`, an array of `{ "split": "train|validation|test", "dataset": ... }`.
Each dataset is an unchanged `atom-reach-transitions-v1` payload, with source quality,
channels, units and pre-action/next-observation alignment. The teacher's reference
joint goal is still excluded from policy observations. Rows preserve Episode boundaries;
there are no cross-Episode transitions. Keep splits at the dataset/whole-group level.

The package is serialized one selected Episode at a time to a temporary file beside
the output. Before each serialization, reload/checksum/validate its source and compare
the typed content hash with the analyzed hash. Reloading is also bounded by the
recorded source file size; oversized replacements cannot bypass the input budget. A changed/missing/invalid selected
source aborts publication, so old selection results cannot export new content.
The file is capped at 64 MiB during streaming, synced and published with no-clobber.
Failures clean up the temporary file; no incomplete destination is published. Existing
different output is preserved; identical output is idempotent. Parent directories
must exist. The receipt includes selected count, byte count and SHA-256 of **all raw
package bytes** (there is no embedded self-checksum). Retain the receipt externally
when transferring the package. Reports are generated afresh from the request;
`save_package` only accepts an immutable in-process analyzed report, never a
user-edited/deserialized approval manifest. Source files and GUI state are unchanged.

Example consumer:

```python
import json
with open("curated.batch.json", encoding="utf-8") as f:
    batch = json.load(f)
train = [row["dataset"] for row in batch["datasets"] if row["split"] == "train"]
validation = [row["dataset"] for row in batch["datasets"] if row["split"] == "validation"]
test = [row["dataset"] for row in batch["datasets"] if row["split"] == "test"]
# Only train may supply optimization samples. Preserve grouping when shuffling.
```

## Validation scope

Tests cover deterministic selection/order, copies, checkpoint versions, reruns,
whole-group splits, all three partitions, timeout/partial filters, invalid files,
quality limits, conflicting task identities, input/output bounds, atomic/idempotent
publication, unchanged existing outputs, changed sources, temporary-file cleanup
and policy-input separation. These are record/contract fixtures, not new physical
rollouts. Actual model training, framework loaders, improved holdout performance remain unverified. Native batch UI verification is
described below.

Verified on November 5, 2026: 265 workspace tests passed, 33 environment-dependent
tests ignored; the final buffered serializer also passed all core unit tests. Clippy
with warnings denied, formatting and the development app build passed. Actual CLI
checks used synthetic, structurally valid record fixtures: default policy selected
one of six inputs; `completed` exported two Episodes with six transitions, all in
one task group. Ordering, receipt hash, repeated export, conflict preservation,
zero-eligible rejection, invalid configuration and unchanged source hashes passed.
No new real-physics rollout was run: neither the default Python nor the existing
`.venv-usd` has the MuJoCo module installed. No dependency installation was performed.


## Workflow GUI

Open **Dataset** in Workflow. Refresh the saved Episode list, select records using
checkboxes (the check icon selects the first 64; the clear icon clears the list),
then choose Success only or Success + timeout. The five numeric quality maxima
accept blank values for no limit. Expand Partitions to set validation/test
percentages and the fixed salt; Train displays the remainder. Validation + test
must be less than 100%. Keep salt fixed across runs as described above.

Click the analyze/check icon to commit those settings and run the existing Rust
curator. The three cards show selected Episodes, distinct task groups and transition
counts. Filter the result list by train/validation/test or excluded. Each file shows
its assignment or exclusion reasons; hover a valid result for transition count and
group ID. Empty-partition and approximate-proportion warnings remain visible.
Click export to write a batch package into `episodes/exports/` beside the project;
the result displays the filename. Repeat export of the same analyzed batch is
idempotent. A new analysis creates a distinct package name and never overwrites a
previous batch.

Editing any setting/selection marks the displayed result as requiring reanalysis
and disables export. No eligible records also disables export. Source changes
between analysis and export are rejected by the frozen Rust report's hash check;
a failed export clears its approval/result so reanalysis is required. No automatic
filesystem watcher is provided: refresh/reanalyze after external edits. Missing or
invalid sources remain explicit exclusions on analysis. Only names from the local
project's Episode catalog are accepted; path traversal, duplicate input names and
symlink Episode files are refused. Output/report transitions remain in Rust/files;
the WebView receives counts, quality summaries, reasons and filenames only.

One Rust state owns committed settings, selected names, the frozen report, job
revision, error and exported filename for all docked/floating views. Form edits are
local uncommitted drafts. Concurrent jobs and stale revision requests are rejected;
completion broadcasts the shared snapshot. Disk reading/analysis/serialization run
on a blocking worker without holding the editor/document or batch-state mutex.
No batch action changes the shared scene, Undo/Redo, replay cursor or savepoint.
Batch settings/results are session state and are not restored after app restart.

### GUI verification (November 5, 2026)

- Native macOS Tauri: five synthetic records, success-only 1 selected, completed
  2 selected / 6 transitions, zero action-change limit excludes all, dirty-settings
  export gating, actual package publication, and source replacement export rejection.
  Existing package hashes and the live Control API core snapshot stayed unchanged
  (excluding the request UUID, which is unique to each read). The isolated QA app
  was closed; no user project was opened or modified.
- Playwright on actual UI code with a temporary local adapter to the Rust CLI:
  selection, exclusions, partition filters, limits, percentage validation, export,
  two view snapshots, three languages and 220/320px widths; no horizontal overflow,
  page/console errors or blank page. This adapter is not native Tauri IPC. Native
  IPC was exercised separately by the native checks above.
- All 267 workspace tests passed (33 environment-dependent tests ignored); Clippy
  with warnings denied, formatting, JavaScript syntax and the development app build
  passed. New Rust unit checks cover shared revision/busy gates, unanalyzed export refusal,
  source scope/symlinks and IPC summary stripping. Other OSes, native floating-panel
  batch operations, actual model training and newly generated physics rollouts
  remain unverified.

The next implemented handoff is the [split-preserving PyTorch BC data adapter](LEARNING_ADAPTER.md): strict saved-package validation, train-only normalization, prepared artifacts and GUI/CLI connection diagnostics. It does not yet optimize model weights.
