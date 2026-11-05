# SkillPlan evaluation v1

The Rust evaluation runner measures candidate plans against six fixed public,
evaluation-only tasks. It uses the existing scene summary, RuntimeGateway and
SkillPlan validator. It never opens the user's project or applies an edit to the
shared GUI document. This is an editor-intent regression suite, not a robotics
benchmark or evidence of physical safety, learned policy quality or generalization.

## Commands

```sh
# Task instructions, bounded scenes, exact prompts and suite fingerprint; no reference answers.
cargo run -p atom-cli -- plan-eval-suite > /tmp/plan-suite.json

# Send the six fixed tasks sequentially through an already running local model.
cargo run -p atom-cli -- runtime-plan-evaluate strata http://127.0.0.1:8080 MODEL > /tmp/plan-run.json

# Extract the candidate bundle, then reevaluate without a server.
python3 -c 'import json; r=json.load(open("/tmp/plan-run.json")); json.dump(r["bundle"],open("/tmp/candidates.json","w"))'
cargo run -p atom-cli -- plan-evaluate /tmp/candidates.json
```

The runtime command accepts the same `vllm_apple_preview`, `vllm_apple_daemon` and
`strata` profiles and `ATOM_RUNTIME_TOKEN` environment variable as the existing
runtime commands. It starts no server, downloads no model and changes no dependency
gates. There are exactly six sequential attempts with no retries; each attempt
uses the existing per-request probe and 15-second generation limits. The full
dual-arm summary can exceed Preview’s conservative 4096-token admission budget;
that case is recorded as `plan_context_budget`, not silently shortened or skipped. An unavailable
server produces six generation failures, never an empty successful report. Stdout
contains JSON even when failed cases cause exit status 1; stderr carries the short
failure message. Redirect stdout to retain the run, including partial failures.

The runtime output contains `bundle`, `evaluation` and `completions`. Completion
metadata includes requested/returned model, profile, prompt/output SHA-256, elapsed
generation time, token budget and reported usage. These are observations, not a
model-weight fingerprint or an end-to-end latency measurement. Candidate text is
retained for inspection. Neither token values nor server error bodies are included.

## Offline input

```json
{
  "schema_version": 1,
  "suite_sha256": "copy the exact manifest fingerprint",
  "candidates": [
    {
      "case_id": "move-en",
      "outcome": {
        "kind": "generated",
        "raw": "{\"schema_version\":1,\"summary\":\"Candidate\",\"steps\":[{\"skill\":\"set_floor\",\"enabled\":true}]}",
        "finish_reason": "stop"
      }
    },
    {
      "case_id": "gravity-ja",
      "outcome": {"kind": "failure", "code": "runtime_timeout"}
    }
  ]
}
```

Only `stop` and `length` finish reasons are accepted. Raw candidates are limited to
8 KiB; CLI input to 128 KiB. Unknown/duplicate case IDs, unknown fields, wrong
suite/schema and malformed failure codes reject the bundle instead of being silently
ignored. Missing known cases remain in the denominator. Offline metadata is
user-supplied, so it does not prove a model generated the candidates.

## Scoring and reproducibility

`atom-skillplan-holdout-v1` contains three-language instructions for object movement,
Moon gravity, rubber material, combined movement/floor changes, an already-enabled
floor, and a dual-arm joint angle. Every task starts from its own frozen fixture.
A stable project UUID removes random prompt variation. The suite SHA-256 includes
suite/scorer ID, exact prompts (including initial summaries) and reference plans;
any change requires a new matching bundle. Candidate SHA-256 is independent of
candidate ordering. Reports retain case IDs and response hashes.

Each reference and candidate passes through the same typed validator and detached
editor dry-run. Scoring compares the **complete final authoring diff**, including
unrequested changes, exactly. Alternate step sequences that reach the same final
state pass; summary prose and Undo count do not affect this score. Numeric values
are compared exactly after Rust deserialization, with no tolerance or semantic
LLM judge. Truncated diffs cannot pass. No-op tasks require a valid nonempty plan
with no final document change. This deliberately small suite does not cover all
skills, IK quality, collisions, dynamics or arbitrary natural-language intent.

Categories are `passed`, `missing`, `generation_failure`, `truncated`,
`validation_failure` and `intent_mismatch`. Validation failures preserve original
error codes and the zero-based failed step when available. All six tasks remain
in `total`, including missing responses, timeout and malformed JSON. The report
provides counts rather than a misleading success rate over valid candidates only.

The suite is public and may be inspected; do not train on its prompts, references
or outputs. It is a reproducible regression holdout, not a secret test set. Keep
future training data and genuinely unseen evaluation tasks separate. Episode batch
curation and automatic train/validation/test grouping are still a separate next step.

## Verification

The macOS/BSD HTTP fixture explicitly resets accepted sockets to blocking mode,
including a regression check where headers arrive after TCP connection. This avoids
misclassifying an inherited nonblocking `WouldBlock` as EOF. Production Gateway
transport behavior is unchanged.

Unit tests cover reference scoring, stable manifests/hashes, candidate reordering,
all failure categories, suite drift, unknown/duplicate IDs, budgets, equivalent
sequences and unrequested side effects. A loopback HTTP contract fixture exercises
all six actual Gateway requests, evidence capture and offline scoring; its deliberately
wrong floor-only replies pass one task and fail five. Fixture scores are not model
quality measurements. Real-model evaluation remains pending under the existing
runtime environment constraints.

Verified on 2026-11-04 (macOS): full workspace 260 passed / 33 environment-dependent
tests ignored, Clippy with warnings denied, 15 consecutive evaluation HTTP fixture
runs after the socket correction, and actual CLI checks for deterministic manifests,
6/6 reference scoring, six missing failures, stale fingerprints and file limits.
The reference and contract-fixture scores are implementation checks only.
