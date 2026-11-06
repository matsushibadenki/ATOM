# Split-preserving PyTorch BC data adapter v1

Implemented November 5, 2026. This connects curated Reach Episodes to a concrete
behavior-cloning **data adapter**. It validates saved batch packages, produces an
adapter artifact, and diagnoses a bundled Python/PyTorch reader. It does not
optimize weights, start a training job, download a model or install dependencies.
Numeric joint-servo recordings are not automatically converted into LLM chat/SFT
examples. SkillPlan/MLX-LM LoRA datasets remain a separate workflow.

The adapter uses PyTorch's documented map-style `Dataset` (`__len__`, `__getitem__`)
and `DataLoader` interfaces. See the [official data-loading documentation](https://docs.pytorch.org/docs/stable/data.html).
The original November 5 verification had no installed PyTorch. On November 6,
real tensor loading and CPU training were verified in an isolated environment; see
[bounded BC jobs](BC_TRAINING.md). Data diagnosis and training remain separate actions.

## GUI

In Workflow → Dataset, analyze and **export** the curated batch first. The PyTorch · BC
row offers:

- Check: validate the exported learning format, dimensions and preserved partitions.
- Reload/diagnose: validate again, prepare a private temporary artifact and launch the
  bundled Python reader. The result distinguishes valid data, a running reader,
  available PyTorch, verified CPU tensor loading, and readiness for a future fit job.

Settings edits disable these actions until reanalysis/export. One Rust state owns
results across views. Reading/preparing/diagnosing run outside the UI/editor/state
mutex. WebViews receive only summaries and diagnostics, never training arrays.
The GUI retains the SHA-256 from the export receipt and refuses a changed package,
even if the replacement is otherwise valid. No document edit, history/savepoint
change or replay movement occurs. Results are session state, not a saved training job.

## CLI

```sh
cargo build --locked -p atom-cli
# Format validation does not require Python or PyTorch.
target/debug/atom-cli learning-validate curated.batch.json
# Optional expected SHA-256 is the raw package hash from the batch-export receipt.
target/debug/atom-cli learning-validate curated.batch.json EXPECTED_SHA256
# Publish an adapter artifact without overwriting a different existing file.
target/debug/atom-cli learning-prepare curated.batch.json prepared.bc.json [EXPECTED_SHA256]
# Diagnose the actual interpreter, artifact reader and optional PyTorch loader.
target/debug/atom-cli learning-diagnose curated.batch.json [EXPECTED_SHA256]
```

Brackets above indicate optional arguments; omit the brackets in a real command.
A successful validation can still report empty train/validation/test partitions.
`fit_data_ready` requires nonempty train **and** validation; `test_data_ready` is
independent. Preparation requires nonempty train. No record is moved between splits
or borrowed from holdout to fill an empty partition. Diagnosis returns exit status 1
unless the actual adapter reports `fit_ready`; JSON on stdout retains the reason.
A missing framework is not reported as a failed data-format check. Transport or
interpreter errors preserve `valid: true` and the format summary, with a separate
`diagnostic_error` and no successful diagnostic object. Invalid input
returns `{ "valid": false, "code": ... }` and status 1. Prepared output is atomic,
synced, capped at 64 MiB and no-clobber; identical repeated export is idempotent.

## Validation contract

Input is a bounded (64 MiB), strict typed `atom-reach-batch-v1` package with at most
64 Episodes and 500 transitions per Episode. It can be moved independently of the
source Episode files: validation never opens paths contained in the batch report.
The report is read as an **audit**, never deserialized into an executable batch
approval or used to apply commands. The loader checks:

- Exact schema/format, expected profile and units/timestep/action semantics; finite
  numeric values, channel IDs/axes/index permutation, range and action dimensions.
- Pre-action times, consecutive observation equality, next observation's previous
  action, target consistency, action limits, distance, consecutive success and
  timeout/terminal placement. Typed policy observations reject extra fields such
  as teacher reference targets. This does not rerun dynamics or prove safety.
- Completed-policy eligibility; recomputed duration, distance, velocity, tracking,
  limit-excess and action-change metrics against serialized quality and selection limits.
- One-to-one selected audit entries and datasets, Episode identity/content-hash
  deduplication, policy/quality/split agreement, and partition Episode/group/row totals.
- Recomputed exact-task group hashes and original salt/basis-point assignment.
  Cross-partition group reuse or relabeling is rejected; there is no random resplit.
- One model hash and identical ordered channel specifications across all records.
  Heterogeneous models/channels fail explicitly rather than being padded or relabeled.

The original package contains no embedded self-checksum. The computed raw SHA-256
anchors each subsequent operation; provide the receipt hash to CLI commands when
checking a transferred artifact. GUI commands always pin the exported hash. These
checks validate the package's declared contracts, not the authenticity of a producer,
the reference controller, or actual simulated dynamics. Original Episodes retain
complete physics/model provenance for independent reconstruction.

## `atom-bc-dataset-v1`

The artifact has `schema_version`, `format`, `summary`, `contract`, `feature_order`,
`normalization`, and `splits`. It contains no local source paths, mesh/project payload
or teacher reference joint goal. The summary records the exact source package hash,
feature/action dimensions and per-partition row/group counts. The contract retains
ordered channels (IDs, axes, ranges, qpos indices), model hash, Y-up world frame,
0.02-second control interval and absolute-radian position-servo action semantics.

Each sample keeps Episode UUID, task group, transition index, `features` and `action`:

| Feature segment, in order | Count | Units / time |
|---|---:|---|
| `joint_position_rad` | N | radians, current observation |
| `joint_velocity_rad_s` | N | radians/second, current observation |
| `endpoint_world_m` | 3 | Y-up meters, current observation |
| `target_world_m` | 3 | Y-up meters, task input |
| `previous_action_rad` | N | preceding control interval, radians |
| supervised `action` (label) | N | position targets for the current interval |

Thus 12 channels produce 42 inputs and 12 labels. The final next observation is not
an input, and next actions/next observations are not substituted for current inputs.
Actions retain their original radians; there is no fabricated reward or text prompt.
Episode boundaries remain explicit. All three partitions retain their original group
membership and sample order.

Welford population mean/standard deviation are fitted **only on train feature rows**.
A standard deviation below `1e-8` uses scale 1. The artifact records `fit_split: train`,
count, mean and scale. The Python reader independently recomputes these statistics
and checks group separation and finite shapes after verifying the artifact hash.
Changing held-out observations or a terminal next observation cannot change train
normalization. Fitting consumers get only train/validation Dataset objects from
`fit_datasets`; test is not a fit DataLoader or an optimization input. All bytes,
including holdout, are read for integrity/format verification; this is a data API
separation, not a sandbox hiding test bytes from arbitrary user code. A dedicated
final-evaluation job remains planned.

## Connection diagnosis

Rust uses `ATOM_LEARNING_PYTHON` as one executable name/path (default `python3`),
never a shell command. It passes `-I` and the bundled script, a private temporary
artifact path and expected artifact SHA-256. There is no arbitrary worker-script
argument or remote upload. `ATOM_RUNTIME_TOKEN` is removed from the child environment.
No user-site/PYTHONPATH fallback or dependency auto-install is performed.

The actual worker has a 20-second deadline **after preparation/spawn**, a 16 KiB
stdout response limit and strict version/adapter/hash/count/flag validation. Failure
kills and waits for the direct worker. Temporary artifacts are cleaned up. Startup
logs do not become protocol JSON or user-visible server error bodies. No scene or
state mutex is held during the subprocess operation.

Diagnosis stages are observable fields rather than one ambiguous connected badge:

| Field | Evidence |
|---|---|
| `connected`, `data_loaded` | Bundled Python reader replied and verified the prepared artifact |
| `framework_available`, version | Actual `torch` import succeeded |
| `tensor_batch_verified` | A CPU `DataLoader`, `num_workers=0`, materialized one train and each nonempty validation batch with finite expected shapes |
| `fit_ready` | Data readiness, actual framework and tensor-batch checks all passed |
| `trained` | Always false for this adapter operation |

Missing Python, timeout, malformed protocol, invalid format, changed source, missing
train/validation and unavailable PyTorch are distinct codes. Default-environment
verification on this machine found Python 3.15.2 and **no PyTorch**: the reader
connected and loaded all partitions, while `framework_available`,
`tensor_batch_verified` and `fit_ready` stayed false. No framework version or
successful tensor-loader result is inferred from documentation or a fixture.

## Verification and remaining work

Five new Rust tests cover three-way roundtrip, typed/time/audit corruption, split
relabel rejection, train-only statistics (including future/held-out perturbation),
empty-train refusal, atomic/idempotent/conflicting output, unavailable executable,
malformed protocol and deadline kill/wait behavior. CLI checks use three synthetic
Episodes, 5 rows in each partition; they exercise format validation, hash-pinned
preparation, independent Python loading/normalization/group checks and unchanged
source hashes.

Native macOS Tauri checks exercise selection → export → format check → Python
reader diagnosis; a different structurally valid package is rejected by the GUI
export-hash pin. Live core state/history stay identical. A uniquely named QA app
and temporary project were used and closed. Playwright checks actual UI code with
a local Rust-CLI-backed adapter: two-view result sharing, dirty gating, three
languages, 220/320px layouts, no blank/overlay/console errors or horizontal overflow.
That browser adapter is not native IPC; native IPC was checked separately.

Pending: actual installed PyTorch tensor loading, optimizer/training execution,
checkpoint/reload and held-out policy metrics; LeRobot/RLDS compatibility, remote
training workers, new physics rollouts and other-OS/native-floating UI QA. None of
these are marked implemented by data preparation or a successful Python handshake.

Final verification: 272 workspace tests passed; 33 environment-dependent tests stayed
ignored. Clippy with warnings denied, formatting, JavaScript syntax and the unsigned
development app build passed. No PyTorch installation, real tensor-success claim,
optimizer run or new MuJoCo rollout was performed.

November 6 update: [bounded CPU BC training](BC_TRAINING.md) now implements actual optimization, checkpoint/reload/resume and fixed recorded-data holdout evaluation. The earlier unavailable-framework observations above are historical; current local diagnosis uses the new isolated environment.
