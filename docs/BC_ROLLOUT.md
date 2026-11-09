# Frozen BC policy: closed-loop Reach evaluation

The Rust core, CLI and Workflow GUI can run a saved BC checkpoint against the
existing MuJoCo Reach runner and compare it with the reference goal servo. The GUI
supports start/stop, saved comparison loading and recorded Episode playback. The
training panel continues to report offline prediction errors separately. The [policy registry](POLICY_REGISTRY.md) now persists checkpoint/data/result
links and compares matching evaluation history in the GUI.

## Run

```sh
cargo build --locked -p atom-cli
export ATOM_MUJOCO_PYTHON="$PWD/.venv-learning/bin/python"
# Uses the normal ATOM_LEARNING_PYTHON / local .venv-learning lookup for PyTorch.
target/debug/atom-cli learning-rollout curated.batch.json /tmp/bc-first project.atom.json /tmp/bc-rollout
```

The output directory must not exist and its parent must exist. Use the exact batch
used to train the job and a project reproducing its robot model, joint channels,
initial configuration and seeded targets. The command opens the project read-only;
it does not change the shared GUI scene or history. Episodes embed their isolated
project and can be inspected with `episode-replay` or copied into the project's
`episodes` directory and loaded in the existing Reach player.

## Contract

- Uses **every original group in the selected validation or test partition exactly once**, sorted by group hash, with its
  recorded seed and horizon. Other partitions are not executed, borrowed or resplit. Repeated records in a group with conflicting horizons are refused.
- Validates the training job's commit marker and file hashes, copies verified best
  checkpoint bytes into private staging, and checks the exact source/artifact,
  channel/model, normalization, architecture, preprocessing and Python/PyTorch CPU
  signature before inference. Last weights are never substituted for best weights.
- Starts independent reset/step sessions for the reference servo and learned policy.
  The engine identity and complete initial observation must match within each pair.
  Both use the original Reach v1 success radius, consecutive-hit count, control dt,
  substeps and horizon. Success and timeout come from the real Rust Reach runner.
- Policy input is channel-ordered qpos, qvel, endpoint, target and **its own previous
  applied action** (initial action on reset). No reference joint goal, future frame
  or recorded teacher action is sent to the inference worker.
- Uses the same float32 train-only normalization and MLP architecture as training.
  CPU inference uses `model.eval()` and `torch.inference_mode()` separately, as
  specified by the [PyTorch inference API](https://docs.pytorch.org/docs/stable/generated/torch.autograd.grad_mode.inference_mode.html).
- Rejects malformed/nonfinite/oversized actions; clamps valid outputs to each joint
  limit and records the number of clamped action components. The saved Episode
  contains the actual applied targets, never the unclamped predictions.
- Limits inference startup to 20 seconds, each inference request to 3 seconds,
  each rollout to 30 seconds, total evaluation to 300 seconds and staged Episode
  output to 256 MiB. Pending physics operations retain the Reach worker's own
  deadlines. Budgets are checked between operations, so a pending operation may
  exceed the outer deadline. Cancellation is available to Rust callers through an
  AtomicBool and to GUI users through Stop; this synchronous CLI has no interactive
  stop command. Inference waits poll cancellation every 50 ms and drop kills/waits
  the worker. Pending physics requests retain their own deadline.
- Worker failures, cancellation, inconsistent reset, and budget exhaustion fail
  the comparison without publishing a successful report. They are not silently
  dropped from a success-rate denominator. Child processes are killed and waited
  on at scope exit. There is no fallback to the reference controller.

## Output

`report.json` contains task group/seed/horizon, controller, success or timeout,
control steps, final distance, clamp count, runtime identity, checkpoint SHA-256,
batch SHA-256 and separate policy/reference success counts and rates. Both rates
use the full fixed group count in the selected partition as their denominator.

Each controller/task also produces a normal checksum-validated
`NN-reference_servo.episode.json` or `NN-bc_policy.episode.json` for replay and
numerical comparison. A final `manifest.json` hashes the report and all Episodes.
Output creation is exclusive; existing directories are preserved. Files are
synced before writing the final commit marker. An I/O failure during publication
may leave an incomplete directory without a manifest; it is not a committed run.
The manifest is an integrity record, not a signature or general policy registry.

## Development verification (2026-11-09)

The explicit `bc_real_closed_loop_reproducible_holdout` test records real MuJoCo
Episodes into three fixed split groups, trains 20 optimizer steps, freezes best
weights and compares both controllers twice on the original test seed (20), with
horizon 300. In the tested macOS arm64 / Python 3.12.16 / PyTorch 2.15.0 /
MuJoCo 3.1.6 environment:

- Reference servo: success after 106 control steps, final distance 0.0120264 m.
- Small BC model: timeout after 300 steps, final distance 1.12237 m; 3253 clamped
  action components. Both paired runs produced exactly identical applied actions
  and qpos, as well as matching summary results.
- Saved Episodes validated and replay poses loaded. Existing output, cancellation,
  incompatible model and modified checkpoint were refused without new publication.
- A standard unit test verifies channel ordering, reset action and previous applied
  action inputs while replacing the teacher reference goal with unrelated values.

This tiny fixture validates execution and failure visibility; it is not a learned
performance benchmark. Repeated test evaluation must not become model selection.
Larger preregistered holdouts, distribution-shift analysis, improved policy quality,
other hardware/OSes and cross-version reproducibility remain pending.

```sh
ATOM_MUJOCO_PYTHON="$PWD/.venv-learning/bin/python" \
  cargo test -p atom-core --lib bc_real_closed_loop_reproducible_holdout -- --ignored
```

The workspace regression suite passed (including local IPC tests outside the
restricted socket sandbox), as did the added pre-worker task/scene guard test,
Clippy with warnings denied, formatting, Python syntax and CLI build/help checks.
The original headless milestone did not change GUI behavior; GUI verification is
described below.


## Workflow GUI

Open **Workflow → Dataset → Reach · closed loop** (Japanese: **Reach · 閉ループ**,
Simplified Chinese: **Reach · 闭环**).

1. Select, analyze and export Episodes as usual. In **BC · CPU**, train a model or
   load a saved checkpoint belonging to that exact exported batch.
2. Press the comparison play icon. Rust clones the authored project at start and
   evaluates the checkpoint on the selected original validation/test groups.
   New GUI sessions default to validation; explicitly choose test for final evaluation. Other windows observe
   the same busy state and result through the shared Rust batch session. No scene
   lock is held while reading data or running workers. Editing after launch does
   not alter the isolated evaluation snapshot.
3. The stop icon requests cancellation, shows **Stopping…** and disables repeated
   stop requests. Cancellation publishes no partial comparison. A completed
   evaluation displays separate reference/BC success counts and rates; each seed
   row shows outcome, steps/horizon, final distance and clamped components.
4. Saved comparisons are listed under `episodes/rollouts/rollout-*`. Refresh and
   load icons reopen a committed comparison even without an active batch or
   checkpoint selection. Loading verifies the manifest and every Episode hash;
   incomplete, modified or malformed results are refused. The tooltip on the
   fixed-test label shows the comparison directory and full checkpoint/batch hashes.
5. Each controller row has a replay icon. Rust verifies and loads that Episode,
   then saves an idempotent `reach-<uuid>.episode.json` copy beside the project's
   other Episodes. Only its local filename crosses IPC. The existing Reach player
   loads it, focuses the robot and starts recorded playback in the native wgpu
   view. Use the normal Reach pause, frame slider and return-to-editor controls.
   Playback does not run PyTorch or MuJoCo and does not create Undo entries.

Batch commands retain revision/busy guards across windows; a stale replay request
is refused. Start requires a current export/checkpoint and is disabled for local
unreviewed batch edits. Saved comparisons remain independently inspectable and
carry their own source/checkpoint identity. Selecting a comparison does not change
the loaded training checkpoint. The inference worker now shares the learning
interpreter lookup as the default MuJoCo Reach fallback; an explicit
`ATOM_MUJOCO_PYTHON` still takes precedence. No dependency is installed on launch.

### GUI verification (2026-11-09)

- Isolated native Tauri QA project, Python 3.12.16 / PyTorch 2.15.0 / MuJoCo 3.1.6:
  selected three actual recordings (seeds 0/2/20 in train/validation/test), exported,
  trained 20 steps, ran a completed comparison and cancelled a second invocation.
  Exactly one committed comparison remained. The status passed through running,
  stopping and stopped. Reference success was 1/1 (106 steps); BC was 0/1 (timeout,
  300 steps). These fixture results do not certify policy quality.
- Reloaded the saved comparison and replayed **both** controller Episodes through
  native IPC/wgpu. The Reach player reached frame 106 for reference and 300 for BC;
  returning to the editor restored the authored scene. Control API snapshots
  before/after evaluation matched exactly after excluding request UUIDs, including
  scene revision, sequence, dirty flag and Undo/Redo state. The QA app exited cleanly.
- Playwright on the actual UI files at a temporary loopback HTTP URL, with real
  saved report fixtures and a simulated Tauri bridge: English/Japanese/Simplified
  Chinese at 220px and 320px widths. Start/stop/reload/replay dispatch, competing
  training disabled during evaluation, success counts, initial content, page
  identity, no overlay, no app console errors and no horizontal overflow passed.
  Browser plugin skill was unavailable, so the installed Playwright was used.
  This browser check validates rendering/dispatch separately from the native test.
- Rust regression tests cover cross-window revisions/busy admission, missing
  export/checkpoint, saved-result loading without a batch, artifact tamper rejection
  and verified Episode loading. The actual closed-loop test was explicitly rerun.

Other OSes, native floating-window event QA for these new controls, larger holdouts
and cross-version reproducibility remain pending. Saved comparison viewing and
replay require no learning environment; starting evaluation requires both workers.

Final GUI milestone checks: 276 standard workspace tests passed, 35
environment-dependent tests ignored; the real PyTorch/MuJoCo closed-loop test was
run explicitly and passed. Clippy with warnings denied, Rust formatting, JavaScript
syntax, diff whitespace checks and the macOS development app build passed.

Validation-partition execution is now supported alongside the fixed test path. See
[partition selection, compatibility and isolation](VALIDATION_REACH.md). The legacy
CLI/Rust wrapper remains test-only; the GUI explicitly selects its partition.
