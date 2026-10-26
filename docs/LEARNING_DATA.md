# Episode quality and transition export

Implemented October 25, 2026. This is a bounded, single-Episode export for the existing fixed-base, zero-gravity dual-arm Reach v1 task. It prepares recorded data; it does not train a policy, generate an LLM dataset, or implement LeRobot/RLDS compatibility.

## Use

In Workflow, load an Episode or finish a Reach run. Click the check icon below the Episode selector to inspect the **whole recording**, independently of the replay cursor. Hover the result for velocity, joint-limit excess and warning details. Click the export icon to write learning transitions. The filter defaults to **Success only**; choose **Success + timeout** explicitly to include a finished timeout trial. Partial/reset-only recordings cannot be exported. Playback must be paused.

GUI exports go to `episodes/exports/` beside the active project, using the Episode UUID, transition count and filter in the filename. Identical repeated exports succeed without rewriting; existing different content is preserved. Analysis runs on a frozen Rust recording outside the shared document lock. Only a bounded report and filename go to the WebView. No document edit, runtime sequence increment, cursor movement, savepoint change or Undo/Redo entry occurs.

CLI commands do not require a running desktop or MuJoCo installation:

```sh
cargo build --locked -p atom-cli
target/debug/atom-cli episode-quality /absolute/path/run.episode.json
target/debug/atom-cli episode-export /absolute/path/run.episode.json /absolute/path/run.transitions.json
# Explicitly allow completed timeout trials:
target/debug/atom-cli episode-export /absolute/path/timeout.episode.json /absolute/path/timeout.transitions.json completed
```

`episode-quality` returns a report for valid partial records as well as completed ones; use its eligibility fields instead of treating exit code zero as training approval. Invalid files/checksums fail before metrics. `episode-export` defaults to `success_only`, accepts `completed`, and returns `episode_export_ineligible` when the filter is not met. Output parent directories must already exist in CLI mode. Publication is atomic, no-clobber and limited to 64 MiB.

## Quality contract

The existing Episode validator checks its deterministic task definition, model hash, initial state, finite values, shapes, action ranges, observation times, distances, consecutive-success and terminal rules. The file loader also verifies its checksum. The report adds:

- Whole-record completeness, outcome, frame/transition count, duration and export eligibility.
- Initial, final and minimum target distance in meters.
- Maximum absolute joint velocity, post-action tracking error, observed joint-limit excess and consecutive action delta, in radians or radians/second.
- Whether every recorded action equals the reference servo goal; custom actions are marked as having unknown policy provenance.
- Source Episode content hash and a deterministic split group.

Observed joint-limit excess above `1e-6` rad is reported as a warning, not silently clamped or rejected as malformed. Export eligibility means structurally valid, completed and matching the chosen outcome filter. It does **not** certify a demonstration as physically safe, collision-free, balanced, optimal, or generalizable. No physics rerun is performed during analysis. Reference-servo records are not learned-policy evaluation results.

## `atom-reach-transitions-v1`

The JSON envelope contains `sha256` and `dataset`. The digest covers the compact serialization of `dataset` as a sorted-object-key `serde_json::Value`; it is not a hash of the entire file or a generic RFC 8785 canonicalization. Keep the original Episode for complete physics/model reconstruction. `quality.episode_sha256` uses the original typed Episode serialization, consistent with the Episode checksum convention.

`dataset` contains `schema_version: 1`, `format`, `policy`, `quality`, `metadata` and `transitions`. Metadata retains the profile, seed, horizon, robot/endpoint IDs, source/model hashes, source revision, engine/Python/platform, joint channels and limits, control/physics timesteps, substeps, tolerance, hold count and units. It deliberately excludes the manifest's teacher `reference_action` and full project/geometry.

For each consecutive frame pair `(f[i], f[i+1])`, one transition contains:

| Field | Meaning |
|---|---|
| `observation` | State at `f[i]`, before the exported action |
| `action_rad` | `f[i+1].action`: absolute joint position-servo targets held over the next control interval |
| `next_observation` | State at `f[i+1]`, after that action |
| `time_seconds`, `next_time_seconds` | Interval endpoints; normally 0.02 seconds apart |
| `next_distance_m` | Post-action endpoint-to-target distance |
| `terminated` | Success at the next frame |
| `truncated` | Horizon timeout at the next frame |

There is no synthetic reset action, terminal-to-reset transition, fabricated reward or automatic resampling. All exported joint positions, velocities and actions follow **`metadata.channels` order**. Raw recorded qpos/qvel arrays can have a different ordering; the exporter uses each channel's `qpos_index` to map them. This mapping is valid for the fixed-base hinge-only Reach v1 profile.

Each policy observation has exactly five fields: `joint_position_rad`, `joint_velocity_rad_s`, `endpoint_world_m`, `target_world_m` and `previous_action_rad`. The first observation's previous action is the authored initial servo target; subsequent observations use the command actually applied in the preceding interval. Reference teacher joint goals are not copied into observation inputs. Positions use Y-up world coordinates in meters; angular quantities use radians. Preserve this distinction when constructing model inputs.

```python
import json
with open("run.transitions.json", encoding="utf-8") as f:
    dataset = json.load(f)["dataset"]
for transition in dataset["transitions"]:
    inputs = transition["observation"]
    action_targets = transition["action_rad"]
    # Build training features from inputs, never from next_observation.
```

## Splits and reproducibility

`quality.split_group` hashes the profile, exact exported model hash, seed, robot/endpoint and target. It excludes Episode UUID, source project identity/name, GUI revision, horizon and frame count. Checkpoints and repeated runs of the same exact model/seed task therefore remain together, even if the source project is renamed. Keep an entire group in one train/validation/test partition; do not randomly split its transitions. This release supplies grouping metadata, not a batch splitter or semantic near-duplicate detector. Different models and derived tasks can still be correlated and require a broader holdout policy.

Output is deterministic for the same Episode and filter. This does not guarantee bit-identical physics across machines or engines. Source records and generated files are never modified by inspection/export. Generated `*.transitions.json` files are ignored by Git, with fixture/template exceptions.

## Validation

The full workspace suite passes 238 tests (33 environment-dependent tests remain ignored); Clippy, formatting, JavaScript syntax and the development app build pass. Five new Rust tests cover pre/post-action alignment, teacher-input separation, channel remapping (including velocities), partial/timeout filters, malformed data, physical warnings, stable grouping, atomic/idempotent publication, conflict preservation and shared-core guards/non-mutation. Real MuJoCo 3.1.6 produced a seed-7 success with 24 transitions in 0.48 seconds and a one-step timeout with one truncated transition. Native macOS Tauri QA exercised both exports, filter gating, reset-only rejection, cache invalidation on Episode change, three languages and a narrow Workflow panel. IPC state comparison confirmed unchanged revision, sequence, dirty/history and replay cursor after inspection and repeated export.

Linux/Windows native UI, floating-window export interaction, direct framework dataset import, batch splitting, model training and holdout improvements remain unverified or planned.
