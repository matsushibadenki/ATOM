# Bounded CPU BC jobs

Implemented November 6, 2026. ATOM now executes real PyTorch tensor loading and
behavior-cloning optimization, saves/reloads checkpoints and evaluates a fixed
recorded-data holdout. This baseline predicts joint-servo actions from the existing
Reach observations. It is not closed-loop rollout evaluation, robot deployment,
LLM fine-tuning or a task-success measurement.

## Environment

The isolated `.venv-learning` was created with Python 3.12.16. Verified installed
versions are PyTorch 2.15.0, NumPy 2.5.4 and MuJoCo 3.1.6 on macOS arm64. Exact
packages are pinned in `scripts/requirements-learning.lock`; this lock describes
this development platform, not a tested cross-platform environment.

```sh
python3 scripts/setup-learning.py
```

The setup script needs an existing `python3.12`, creates only `.venv-learning` and
installs binary wheels from official PyPI. MuJoCo is included to reproduce recording
checks; training an existing batch does not start MuJoCo. Application launch never
installs dependencies. Rust first uses `ATOM_LEARNING_PYTHON` if set, otherwise this
checkout's `.venv-learning/bin/python` (Windows path variant supported by lookup),
then `python3`. A copied release app needs an explicitly configured compatible
interpreter when the original checkout is unavailable. Other OSes/clean-machine
packaging remain pending.

PyTorch installation follows the [official setup guidance](https://pytorch.org/get-started/locally/).
Checkpoints use state dictionaries plus optimizer state, CPU loading,
`weights_only=True` and strict parameter loading, following the
[official checkpoint API](https://docs.pytorch.org/tutorials/beginner/saving_loading_models.html).

## CLI

```sh
cargo build --locked -p atom-cli
# Checks the real tensor/DataLoader path in the chosen environment.
target/debug/atom-cli learning-diagnose curated.batch.json
# Default config: seed 7, 200 total steps, 60 seconds, batch 32, Adam 0.001.
target/debug/atom-cli learning-job-config > /tmp/bc-config.json
# Output directory must not exist; its parent must exist.
target/debug/atom-cli learning-train curated.batch.json /tmp/bc-first /tmp/bc-config.json
# Increase max_steps in a new config to continue beyond a completed step target.
target/debug/atom-cli learning-train curated.batch.json /tmp/bc-resumed /tmp/bc-more.json /tmp/bc-first
# Reload frozen best weights and evaluate exactly the original test partition.
target/debug/atom-cli learning-evaluate curated.batch.json /tmp/bc-resumed /tmp/bc-evaluation
```

`max_steps` is cumulative, 1–10000. `max_seconds` is per invocation, 1–300.
Batch size is 1–256; learning rate is finite in `[0.000001, 0.1]`. Unsupported values
are refused before starting a worker. All three partitions must be nonempty;
missing data is never borrowed or resplit. Resume permits a larger total-step
budget/new time budget but requires the saved seed, batch size, learning rate,
architecture, preprocessing, source/artifact hashes, normalization, channels,
Python/PyTorch versions and CPU profile. Extending after a time stop may use the
same total target if it has not yet been reached. A completed target requires a
larger target. Different batch bytes (including changed audit/provenance) are
refused, even if some samples happen to match.

## GUI

Workflow → Dataset → analyze → export → **BC · CPU**. Set cumulative steps/time and
use the start, resume, fixed-test evaluation and stop icons. New GUI jobs use seed 7,
batch 32 and Adam 0.001; resume preserves the loaded job's settings. Saved jobs can
be refreshed and loaded from the adjacent checkpoint selector, including after
reopening the app and recreating the identical export context. Loading verifies
committed files/hashes; resume/evaluation subsequently reload actual tensors.
Evaluation directories are not offered as resumable training checkpoints.

The Rust shared state owns the active job, cancellation token, last checkpoint and
result. Other windows see the same busy/completion state and cannot launch a second
batch job. Worker I/O does not hold editor/state locks. Job history/weights stay in
files; WebViews receive bounded summaries. Editing dataset settings invalidates the
handoff until analysis/export; saved checkpoint data hashes must match. Authoring
Document, history, savepoint and replay cursor do not change. Session selection is
not persisted, but completed job directories can be selected again. CLI accepts
explicit saved directories independently of GUI session state.

## Optimizer and split semantics

- Fixed CPU MLP: `input → Linear(64) → ReLU → Linear(64) → ReLU → action`.
  Torch intra/inter-op threads are 1 and deterministic algorithms are requested.
- Current-observation features and train-only normalization come from the strict
  adapter. Both the actual Dataset/DataLoader and training tensor path normalize
  in float32; the first loader sample is checked against the training tensor.
- A seeded permutation is cycled deterministically by cumulative step and batch
  size. Only train rows call backward/Adam. Loss is mean squared raw action error
  in radians; finite-gradient checking and norm-10 clipping are enabled.
- Validation selects the lowest-MSE weights at initialization, step 1 and every
  tenth step. This schedule does not change at resume boundaries. A final last
  step between validation ticks is saved for continuation but does not silently
  become the selected best checkpoint.
- Best weights are frozen, saved, reloaded strictly and rechecked against their
  validation MSE before test tensors are constructed. Test is not used for
  gradients, normalization, selection, stopping or scheduling. Explicit evaluation
  does not call optimizer.step or choose another model.
- MSE/RMSE/MAE are weighted over rows and action elements, not averaged equally
  over task groups. Predictions are not applied to a physics controller and are
  not clipped into a task-success claim. Small holdout counts do not establish
  generalization. Repeated explicit evaluations remain visible jobs; the tool
  does not prevent a human from tuning against previously seen test reports.

## Budgets and stop behavior

The soft time budget includes worker computation after import/data loading begins,
checked at optimizer boundaries; one validation operation can finish after the
soft deadline. Startup/preparation and checkpoint/final evaluation need extra time.
Rust enforces a hard worker lifetime of `max_seconds + 30` seconds after spawn and
kills/waits for a nonresponsive worker. Input/prepared data remain capped at 64 MiB,
checkpoints at 8 MiB each, and worker/report output at 256 KiB. These are step/time/
artifact limits, not an OS RAM reservation. CPU is the explicit baseline; no MPS or
CUDA fallback/claim is made.

GUI stop signals a cooperative flag. After updates, the worker saves last/best,
checks reload, evaluates the frozen model and commits a `cancelled` result. If
cancelled before the first update, there is no new trained checkpoint. An
unresponsive cancellation is killed/waited after five seconds; an interrupted
staging directory is never a valid completed job. Existing completed jobs remain
unchanged. CLI jobs are synchronous and bounded; interactive cooperative stop is
provided by GUI, not a separate cross-process CLI job manager.

## Files and reload

Training writes a private staging area and produces:

- `last.pt`: current model and Adam state, cumulative step, best step/validation,
  initial validation and a strict data/environment/model signature.
- `best.pt`: validation-selected model, step and the same signature.
- `report.json`: budgets, provenance, version/device, update/reload evidence,
  validation/test metrics, bounded history and parent-manifest hash on resume.
- `manifest.json`: `atom-bc-job-v1`, hashes of all three files and source/artifact
  hashes. This is written into the new output directory **last**, as commit marker.

Output directories are created exclusively; existing output is never overwritten.
Every file is synced, then the directory/parent are synced. A publication failure
can leave an incomplete new directory without a commit marker; it is not loadable
and does not replace prior jobs. Resume/evaluation first verify bounded regular
files and hashes, copy them privately to avoid source changes during loading,
then use safe tensor-state loading. Only ATOM's own versioned checkpoint format is
supported; arbitrary third-party pickles/models are not an import route. There is
no authenticity signature or sandbox for arbitrary user-edited checkpoint content.

An evaluation directory instead contains `evaluation.json` and an
`atom-bc-evaluation-v1` manifest. It links source/artifact and best-checkpoint hashes,
retains the frozen best step and metrics, and records `trained: false`.

## Verified results

Real MuJoCo 3.1.6 recorded seeds 0–23 with horizon 300. The default success-only
curator selected 21 successful Episodes and excluded 3 timeouts. Fixed groups:

| Split | Episodes/groups | Transitions |
|---|---:|---:|
| Train | 17 | 757 |
| Validation | 2 | 50 |
| Test | 2 | 130 |

Real CPU PyTorch runs on these recordings:

| Run | Selected step | Validation RMSE (rad) | Fixed test RMSE (rad) |
|---|---:|---:|---:|
| 200 steps | 200 | 0.0309457 | 0.0725329 |
| 200 → 400 resume | 340 | 0.0293428 | 0.07536 |

The initialized model's validation RMSE was 0.1356965 rad. More training did **not**
improve this small test holdout; it was not used to choose the model. These are
engineering checks of recorded-action prediction, not closed-loop policy success.
Resume-to-400 weights exactly matched a fresh 400-step run in the same environment;
independent checkpoint reload/evaluation reproduced the test metrics. A one-second
budget stopped before 10000 steps, and altered checkpoints/existing outputs were
refused. A real optimizer test that perturbs only test labels changes test metrics
but leaves learned/validation-selected tensors unchanged.

Native Tauri on an isolated project exercised 200-step train, 400-step resume,
fixed-test evaluation, stored-job load and cancellation at step 1098 (best 1000),
with checkpoint publication. Shared core state before/during/after remained equal
apart from request UUIDs. Native QA was closed. Browser checks use actual job-report
fixtures separately from native IPC, covering budget/cancel/load dispatch, invalid
caps, three languages, 220/320px, console health and no horizontal overflow.

Pending: distribution-shift diagnostics, broader
physical performance evaluation, MPS/CUDA training, other OSes and independent
clean-machine packaging. Bitwise
reproducibility is verified only in the measured environment. LLM/MLX-LM fine-tuning
and sensor/VLA training remain separate roadmap work.

Training milestone checkpoint (2026-11-06): 273 standard workspace tests passed, 34 environment-dependent
tests ignored. The real PyTorch optimizer/resume/test-isolation test was also run
explicitly and passed. Clippy with warnings denied, formatting, JavaScript syntax,
native job operation and three-language 220/320px layout checks passed.

Closed-loop physics evaluation is now available separately through `learning-rollout`; see [BC rollout](BC_ROLLOUT.md). The training GUI and `learning-evaluate` continue to report offline action-prediction metrics.

Saved policy selection and fixed-condition history comparison are available through
[the policy registry](POLICY_REGISTRY.md); these operations do not optimize weights.
