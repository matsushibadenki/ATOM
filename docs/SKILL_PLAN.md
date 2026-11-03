# Shared scene → SkillPlan → dry-run

Implemented November 3, 2026. SkillPlan v1 generates **editor candidates**, validates their types and scope in Rust, and previews their effects through the existing editor dry-run path. It never applies a candidate, runs physics, starts training or executes model-produced code.

## Workflow

1. Configure/probe a local model in **LLM** (see [runtime gateway](RUNTIME_GATEWAY.md)).
2. Select an object if you want it prioritized, expand **SkillPlan**, and enter a short instruction.
3. Use the scene icon to inspect the summary, or press play to capture a fresh summary and generate a candidate.
4. Inspect the JSON, typed step list, validation issues and predicted authored-state diff. **Validated · not applied** explicitly means the real scene is unchanged.
5. You can paste/edit JSON and click check to validate against the current scene without an LLM. Edited JSON hides the previous result/diff until revalidated. This produces a separately identified **Manual** candidate.

The interface supports English, Japanese and Simplified Chinese. Explanation and constraints are available through the information icon. JSON drafts and instruction fields are presentation inputs; the captured scene, generated report and validation result are owned by the shared Rust runtime session. All windows receive the same report. Candidates are session-local and are not saved in the project or runtime connection settings.

## Scene contract

The source is the desktop's **live shared Rust document, including unsaved edits**. No second project is loaded from disk. Previews, simulation/import jobs and Reach playback must be closed before capture/validation.

A summary carries schema version, project/revision/sequence basis, selected focus ID, gravity/floor settings, total/omitted counts and up to **8 objects / 2048 compact JSON bytes**. Focus and its ancestors are prioritized, followed by authored object order; the byte budget can reduce this further. Every omitted object still exists. The summary is not a complete world model.

Object data includes stable ID, box/mesh/robot-link kind, posed position/rotation/scale, fixed flag and available parent/joint axes, ranges and current angles. Object poses and IK targets are Y-up world meters; rotations and local joint angles are radians. Gravity is explicitly named `gravity_z_up`, in canonical Z-up m/s². Joint axes are local axes and `axis` in a skill is an index into the listed axes.

Project/object names, imported-asset metadata, file paths, credentials, mesh buffers, workflow contents and Episode records are omitted. User-entered instruction text is sent as entered. The captured basis and summary are displayed with the candidate, and the generated prompt/output hashes remain in its completion trace.

## SkillPlan v1

The model must return one strict JSON object, without Markdown fences, trailing text or extra fields:

```json
{
  "schema_version": 1,
  "summary": "Move the selected box and use lunar gravity",
  "steps": [
    {"skill": "move_object", "id": 1, "position": [1.0, 2.0, 3.0]},
    {"skill": "set_gravity", "preset": "moon"}
  ]
}
```

`summary` is a single line of 1–160 UTF-8 bytes. Plans contain 1–6 steps, at most one position IK step, and at most 8192 UTF-8 bytes. Supported steps are:

| Skill | Fields and constraints |
|---|---|
| `move_object` | `id`, `position: [x,y,z]`; non-robot objects only. Preserves rotation/scale/material. |
| `set_joint_angle` | `id`, zero-based `axis`, `angle`; the joint/axis must exist and the angle must already be within limits. Out-of-range values are rejected rather than accepted through clamping. |
| `solve_ik` | Endpoint `id`, ancestor `base`, world-space `target: [x,y,z]`; the complete chain must be represented in the summary. Uses existing bounded position IK. |
| `set_gravity` | `preset`: `earth`, `moon`, `mars` or `zero`; Rust resolves canonical vectors. |
| `set_material` | `id`, `preset`: `wood`, `steel`, `aluminum`, `rubber`, `plastic`, `glass`, `concrete` or `ice`; uses existing density/friction presets. |
| `set_floor` | `enabled: true/false`. |

Every referenced object/base must belong to the supplied summary. Gravity and floor are global environment settings. Joint changes can affect descendants not shown in the summary; this is not collision checking. Direct robot geometry moves, custom preset values, add/delete, imports, arbitrary editor commands, Undo/Redo, files, shell, scripts and simulation/training operations are not part of this schema. This release does not infer safe actions from free-form prose or support full robot task execution.

## Generation and validation

All three runtime profiles use one user-only message. The prompt contains bounded instruction data, summary and the explicit allowed shapes. There are no tools, system messages or assumed constrained decoding. Input is limited to 512 UTF-8 instruction bytes and 4096 prompt bytes; output is capped at 512 tokens and 8192 text bytes. The existing 3-second per-probe / 15-second generation deadlines and 64 KiB HTTP response limit still apply.

Before posting, the gateway checks observed context capacity with a conservative `prompt UTF-8 bytes + output tokens + 256` estimate. This is an admission bound, not exact tokenization; an over-budget request fails rather than silently dropping scene entries or truncating the instruction. The server remains responsible for its actual tokenizer/context limits. Large summaries or instructions may require a smaller scope/instruction. There is no retry, fence removal, JSON repair, default-field injection or fallback execution. A `length` finish reason rejects the candidate even if the returned text happens to parse.

The Rust pipeline parses typed enums with unknown-field rejection, validates schema/version/counts, checks scoped IDs/ancestor chains and strict joint limits, then invokes the **same editor dry-run and apply validation path on detached authoring/history copies**. Steps are evaluated sequentially, so later steps see earlier predicted changes. The copy preserves current Undo/Redo stacks, savepoint, revision/sequence and next-ID state.

If a step fails, the report identifies its zero-based step index (displayed as one-based in the GUI) and error code. No partial successful diff/step-result list is published. The parsed candidate or raw rejected output remains inspectable. On success, the report contains per-step change summaries and an aggregated, bounded diff (up to 64 entries) with history/revision predictions and warnings.

Predictions describe a sequence of ordinary editor edits, **not** a future atomic plan transaction. A sequence that changes something and then restores it can have no net object diff but still predict history/revision changes. Diff paths address authored entities by ID; they are not executable RFC 6902 patches. Robot FK poses are summarized, while editor diff fields describe authored rest geometry/joint state. No mesh frames are transferred to the WebView.

## Freshness and non-mutation

Capture requires the exact project/revision/sequence basis returned by the summary. It rejects stale requests before generating. HTTP and mathematical validation run off the UI thread without holding the shared document lock. The worker validates its frozen context, then checks the live basis again at delivery. The GUI also compares every subsequent shared-scene snapshot: edits, Undo, reload, save acknowledgements and previews invalidate an older basis. Camera movement alone is not a document edit.

A late report remains inspectable as **Scene changed · revalidate**, with its original basis. It is not presented as currently validated. Manual revalidation captures the current scope, repeats Rust validation and dry-run, and does not send another HTTP request. Changing selected focus can change which IDs are in scope on the next capture; inspect the summary when revalidating older JSON.

The original document, positions, physics, savepoint, revision/sequence, selection, playback cursor, ID allocation and Undo/Redo stacks are never edited by this pipeline. Candidate validity means type/scope/editor consistency for the captured basis. It does not establish instruction satisfaction, collision freedom, physical stability, grasping/balance, execution success or model quality. No apply button is provided in this milestone.

## Verification and remaining work

Nine new tests cover bounded/private summaries, live posed state, focused scope, strict parsing and forbidden skills, sequential dependencies/history prediction, no partial diff, joint limits, IK/chain budget, stale/busy guards, three HTTP profiles, exact user-only generation, truncated/invalid output and context admission. The workspace passes 255 tests (33 environment-dependent tests remain ignored); Clippy, formatting, JavaScript syntax and the development app build pass.

Native macOS Tauri QA used an explicitly named contract fixture to generate from two unsaved objects while the saved file contained one. The request omitted project names, and IPC comparison confirmed unchanged shared state/history/sequence/file after generation and manual validation. A deliberate edit during delayed generation produced the stale report; manual revalidation made no HTTP request. Invalid-ID errors, edited-draft result suppression, three languages and approximately 220 px Workflow width were also checked. Fixture responses are not real-model inference or planning-quality evidence. The HTTP fixture was also corrected to avoid responding to incomplete/cancelled requests; its regression check and 30 repeated parallel contract runs passed.

Real vLLM-apple/Strata model qualification remains pending under the existing environment constraints. Linux/Windows/floating-window QA, plan persistence/audit export, atomic approved application, physical execution and fixed-holdout candidate-quality evaluation remain separate milestones.
