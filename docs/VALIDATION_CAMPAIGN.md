# Bounded validation candidate comparisons

Workflow → Dataset → **Validation candidates** compares 2–4 registered, frozen
BC policies on their original validation tasks. This adds a repeatable way to
compare development candidates before collecting more data or changing training
settings. It does not update weights, score test tasks or activate a winner.

## Workflow

1. Register checkpoints in **Policies · history**. Each candidate retains its own
   exact training job and source batch; an active batch or training selection is
   not required to start a campaign.
2. Select 2–4 candidates and a total budget of 1–300 seconds. Rust snapshots the
   authored scene, verifies every candidate and checks identical original
   validation groups, seeds, horizons, model/reset state and task rules before
   starting physics. Empty or incompatible validation sets fail explicitly.
3. Start the comparison. A shared Rust job owns progress, cancellation and results
   across windows. Candidate order follows the submitted configuration. Each
   candidate runs the existing reference-servo/BC comparison with independent
   resets, frozen checkpoint loading and bounded inference/physics workers.
4. Stop preserves a campaign marked incomplete and any **completed** individual
   evaluations. The active interrupted rollout is not published. Budget exhaustion
   and failure also preserve the completed subset. Pending candidates remain in
   the report; incomplete campaigns have **no ranking**.
5. After every candidate completes under identical actual execution conditions,
   results rank by BC success count descending, mean final distance ascending,
   then policy ID for deterministic ties. All candidates have the same task
   denominator. A first-place policy can still have zero successes.
6. Load a saved comparison to verify its metadata and referenced artifacts again.
   Each completed row opens its individual evaluation and existing Episode player.
   Individual results also appear in partition-aware history for diagnosis and
   A/B comparisons. The select icon explicitly restores a chosen policy's context;
   ranking itself never changes the active policy.

Candidate labels, status, controls and tooltips support English, Japanese and
Simplified Chinese. Long job names wrap as units when they fit the panel width.
The UI receives command/status/report metadata; tensors, recorded trajectories,
physics and native rendering remain in Rust/Python/GPU paths.

## Contracts and limits

The global deadline starts before candidate verification and is checked between
bounded operations and within the existing rollout runner. It is not a strict
wall-clock kill at the selected second: an in-progress startup (up to 20 seconds),
physics reset (15 seconds), step (3 seconds), file verification or cleanup can
finish afterward. Cancellation also waits for bounded operations/worker cleanup.
No dependency installation, model download or optimization occurs automatically.

Every candidate must have the same validation execution specification in the
current authored scene, even if its training data differs. Completed evaluations
must additionally share the actual runtime comparison key. Changed source batches,
checkpoints or job manifests are rejected. Training normalization remains train
only. Package integrity checks inspect the full source contract, but the campaign
executes and scores validation tasks only; it never fills gaps with train or test.

Results are stored under the project's `episodes/` directory:

- `campaigns/campaign-<sha256>.json`: configuration, validation task specification
  hash, status, candidate checkpoint/batch/job hashes, evaluation manifest links,
  scores and ranking. Synced temporary files publish without overwriting existing
  different content. Identical reports are idempotent.
- `rollouts/rollout-campaign-<uuid>-<index>/`: ordinary committed individual
  comparisons, including both controllers' replayable Episodes.
- `registry/`: existing immutable policy/evaluation records. Keep these and their
  original source batches/training jobs for later full verification.

Reload checks the campaign content hash, schema/configuration, candidate identities,
source artifacts and every completed evaluation's partition, conditions, manifest
and recomputed score. It recomputes the ranking. Partial rows cannot carry scores
or evaluation links. Missing or changed references fail verification; a campaign
is a linked record, not a self-contained portable checkpoint archive. The saved
campaign catalog exposes up to 64 files; history retains its existing limits.
If persistence or registry linking fails after an individual rollout commits, that
individual artifact may remain independently loadable, but no complete campaign
ranking is claimed.

## CLI

Create a configuration using exact registered policy filenames:

```json
{
  "policies": ["policy-<64-character-sha256>.json", "policy-<another-sha256>.json"],
  "max_seconds": 60
}
```

```sh
atom-cli policy-list /path/to/episodes
atom-cli policy-campaign /path/to/episodes project.atom.json campaign-config.json
atom-cli campaign-inspect /path/to/episodes campaign-<sha256>.json
```

`policy-campaign` outputs the saved filename and report. The desktop uses the same
core runner with progress and cancellation; CLI invocation runs to its budget or
completion. No new external Control API mutation or MCP execution endpoint is
introduced by this milestone.

## Verification (2026-11-09)

- Standard tests cover candidate count/uniqueness/budget admission, deterministic
  ranking, the complete-denominator requirement and cancellation admission without
  an active export or training selection.
- The real PyTorch/MuJoCo integration test compares two frozen candidates, verifies
  validation-only registry links and reload, cancels after one completed candidate
  without ranking, tests budget expiry before worker launch, checks source-batch
  bytes remain unchanged, rejects campaign tampering, and rejects an additional
  incompatible validation group before any new rollout is published. Existing
  test/validation isolation, inference reproducibility and diagnostic checks remain.
- CLI QA completed and reloaded a real two-candidate comparison. On the one-task
  seed-2 validation fixture, both BC candidates timed out at 300 steps; final
  distances were approximately 1.033 m and 0.309 m. Ranking reflects distance here,
  **not** a successful policy or established generalization improvement.
- An isolated native Tauri app selected two candidates, started/stopped a campaign,
  displayed **stopped / no ranking**, completed a second campaign, verified the
  saved result and replayed a linked BC Episode to frame 300. Authored revision
  remained zero and the editor was restored before quitting. This test used only
  disposable QA files, not the user's project.
- Actual UI files with real result metadata and a simulated Tauri bridge were
  exercised through installed Playwright because a Browser testing skill was not
  available. English/Japanese/Simplified Chinese at 220px and 320px passed candidate
  selection, invalid-budget rejection, start/stop, disabled running controls,
  partial-result display, ranking, saved reload and individual evaluation dispatch.
  Page identity, nonblank rendering, no error overlay, no application console
  errors and no horizontal overflow were checked; screenshots were inspected.

Larger preregistered holdouts, demonstrated closed-loop policy improvement,
other OSes and native floating-panel event delivery remain pending. Repeated
validation-driven development can overfit validation; reserve fixed test for final
assessment rather than tuning.

Final checks: the standard workspace suite passed (environment-dependent tests
retain their explicit ignored status). The extended real PyTorch/MuJoCo test was
run explicitly and passed, including the campaign budget and incompatibility
checks. Clippy with warnings denied, formatting, JavaScript syntax, diff checks
and the macOS development app build also passed.
