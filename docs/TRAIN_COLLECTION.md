# Train-only reference data collection

Workflow → Dataset → **Train data collection** extends an exported Reach batch
with new reference-servo demonstrations. The collector preserves the original
embedded train, validation and test records, split salt/ratios and quality limits.
It never regenerates holdouts or uses their task outcomes to select new seeds.

## Workflow

1. Export a curated batch or select an existing registered policy to restore its
   exact source batch. A checkpoint is not required for collection. The source must
   contain train and validation data, with enough room for the requested attempts
   within the 64-entry audit limit.
2. Set the first seed, **train attempts** (1–16), step limit (1–500) and total
   budget (1–300 seconds). Attempts count new train tasks actually started, not the
   number eventually accepted. Starting at the same seed gives the same scan order.
3. Start collection. Rust snapshots the authored scene and verifies its Reach
   model, channels, endpoint, profile and initial servo targets against the source
   batch before creating output or starting physics. Later editor changes do not
   change the running snapshot. Collection leaves the shared document, preview,
   dirty flag, revision, sequence and Undo/Redo unchanged.
4. The collector scans at most 1024 consecutive seeds. It computes the existing
   group hash and original split assignment **before** physics. Validation/test
   groups and existing source audit groups are skipped without starting a worker.
   Existing rejected groups are also skipped. Only new train groups run the normal
   reference servo, with independent resets and unchanged Reach success/timeout
   rules. This is not BC inference or teacher labeling of BC-visited states.
5. Every completed demonstration is saved as an ordinary Episode. Only successes
   passing the source batch's quality maxima are appended. Timeouts and quality
   exclusions remain visible with reasons; skipped holdouts have no Episode file.
   No accepted records means no augmented batch is published.
6. Stop retains completed records and an accepted subset, if any. The interrupted
   task has no saved partial Episode. Budget expiry and worker failure similarly
   produce an incomplete collection report. A completed collection can still have
   zero accepted records; completion describes attempts, not data quality.
7. Inspect accepted/attempted counts, group totals and seed-level exclusions. The
   saved-result controls reload and verify the report. **Use augmented batch for
   training** explicitly switches the active batch and clears the old policy,
   checkpoint and curation result. Fresh BC training is then available. Resuming
   an old checkpoint with changed data is refused. Collection itself never starts
   training or activates a policy.

The compact controls and tooltip descriptions support English, Japanese and
Simplified Chinese. The UI receives metadata and commands; trajectories and
physics remain outside the WebView. New Episodes appear in the ordinary Episode
catalog and can be loaded through the existing playback controls.

## Bounds and saved artifacts

The total deadline starts before source loading and is checked at operation
boundaries and between physics steps. Each task also has a 30-second budget.
An in-progress worker startup/reset (up to 15 seconds), step (3 seconds), file
operation or cleanup may finish after the total deadline. This is a bounded
operation budget, not an exact wall-clock cutoff. Workers are stopped and waited
on through the existing Reach runner's cleanup path.

Config permits seeds from 0 through `u32::MAX - 1023`. Both source and augmented
batch retain the existing 64 MiB limit. Serialization uses a bounded writer and
synced temporary files; publication never overwrites different existing content.
The collector does not install dependencies, download models or change runtimes.

Files are placed beside the project under `episodes/`:

- `reach-collection-<uuid>-<seed>.episode.json`: completed reference runs, including
  rejected timeouts/quality failures. These are normal replayable Episodes.
- `exports/collection-<uuid>.batch.json`: the original embedded datasets and audit
  followed by accepted new train records, using the original quality and split
  settings. No source Episode needs reopening to preserve the original batch.
- `collections/collection-<sha256>.json`: source filename/hash, configuration,
  status/error, ordered seeds, exclusions, Episode hashes, accepted counts,
  augmented filename/hash, group counts and a hash of the complete original
  validation/test items. Identical metadata reports are idempotent.

Reload checks report hash/schema, the original batch hash, preserved holdouts and
original dataset/audit prefix, unchanged split/quality settings, train-only new
success records and exact agreement with their recorded Episode datasets. New
Episode hashes, provenance, seeds and horizons are checked. Keep the original
batch, augmented batch, report and newly recorded Episodes for full verification.
The old source Episode files may be moved without breaking this verification.
The saved-result catalog lists up to 64 reports. Storage/persistence failures can
leave independently valid completed files without a published report; these do
not silently become the active training batch.

## CLI

```sh
atom-cli train-collect-config > collection-config.json
atom-cli train-collect /path/to/episodes fixture.batch.json project.atom.json collection-config.json
atom-cli collection-inspect /path/to/episodes collection-<sha256>.json
```

The source argument is a batch **filename under `episodes/exports/`**, not an
arbitrary path. A configuration example:

```json
{"start_seed":0,"count":4,"horizon":300,"max_seconds":60}
```

Use the returned augmented batch with the existing `learning-validate`,
`learning-diagnose` and fresh `learning-train` commands. Register the new job and
batch together, then use the existing validation candidate comparison to compare
with the source policy on unchanged validation conditions. Reserve fixed test for
final assessment, rather than using it to choose collection/training settings.
The existing training worker still performs its configured post-training frozen
checkpoint test report; the collector never executes or scores test tasks.

## Verification (2026-11-09)

- Standard tests cover invalid count/horizon/budget/seed, source filename and hash
  rejection, changed-scene refusal before physics, cancellation before startup,
  idempotent saved metadata, missing old source Episodes and report tampering.
  Desktop admission requires a batch, allows no checkpoint, uses the shared busy
  slot and allows saved-result loading without an active batch.
- An explicitly run MuJoCo test collects real train demonstrations, verifies no
  held-out seed starts physics, preserves validation/test rows and groups, extends
  an augmented batch without recollecting existing train groups, cancels after a
  completed run, expires the budget before startup, rejects real demonstrations
  with a zero final-distance maximum and detects changed Episode files. Source
  bytes and the authored project remain unchanged.
- CLI QA added four accepted train records: groups changed from **1/1/1** to
  **5/1/1** (train/validation/test). Complete embedded validation/test items matched
  exactly. PyTorch 2.15.0 loaded real tensors (125/24/106 rows); a fresh 20-step CPU
  job saved/reloaded its checkpoint. Resume with the old source checkpoint failed
  with `learning_checkpoint_data_mismatch` and published no job.
- A validation-only campaign compared the original and augmented-data jobs with
  the same seed-7, 20-step training configuration. On the single fixed seed-2
  validation task, the original BC checkpoint timed out at 1.032572 m; the new
  checkpoint succeeded at 0.022315 m. This one-task fixture demonstrates a useful
  closed loop through data collection, training and validation, **not** a measured
  generalization improvement. No test result selected either candidate.
- Isolated native macOS Tauri QA restored the source policy, completed an eight-
  attempt collection (seven accepted), stopped another collection during 16
  attempts (three accepted out of four started), verified its saved report,
  explicitly selected the partial augmented batch and completed fresh 20-step BC
  training with checkpoint reload. The old resume action was disabled at data
  selection. Control API state before and after matched exactly excluding the
  request UUID, including authored scene, revision, sequence, dirty and history.
  The disposable QA app exited cleanly.
- Browser plugin/skill was not available, so installed Playwright exercised the
  actual UI files with real result metadata and a simulated Tauri bridge. English,
  Japanese and Simplified Chinese at 220px/320px passed invalid-number rejection,
  start/stop, running-control disabling, exclusion display, reload, explicit data
  selection, disabled old resume and fresh-training dispatch. Page identity,
  nonblank content, no framework overlay, no application console errors and no
  horizontal overflow passed; narrow screenshots were inspected. Native IPC,
  simulation and training were verified separately above.

Broader preregistered validation tasks, failure-directed collection or DAgger-style
labeling of policy-visited states, robust success-rate gains, other OSes and native
floating-panel event delivery remain unfinished. Additional data alone does not
establish policy quality, and repeated validation-driven development can overfit
validation.

Final checks: 287 standard workspace tests passed, with 36 environment-dependent
tests explicitly ignored. The real MuJoCo collection test was separately executed
and passed, including the stricter saved-data/Episode verification. Clippy with
warnings denied, formatting, JavaScript syntax, diff checks and the final macOS
development app build passed. The final CLI binary also reverified the saved QA
collection and its exact new Episode/dataset correspondence.
