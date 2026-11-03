# Local runtime gateway v1

Implemented November 3, 2026. `atom-runtime` provides bounded connection diagnostics and a fixed short text-generation test for vLLM-apple and Strata. Connection diagnostics do not apply scene commands, train models, or launch servers/models automatically. The companion [SkillPlan pipeline](SKILL_PLAN.md) now generates typed editor candidates from bounded scene summaries and validates them through dry-run; physical robot execution remains planned.

## Connect

Start a separately configured, supported local server, then expand **LLM** in Workflow. Select a profile, enter the server **origin** (for example `http://127.0.0.1:8000`), and click the check icon to apply/save it. Click reload to inspect models/readiness. Choose an observed text model and click play to run the connection test. The output budget is 1–128 tokens, default 32.

The CLI works independently of the desktop:

```sh
cargo build --locked -p atom-cli
# Preview example; the server must already be running.
target/debug/atom-cli runtime-probe vllm_apple_preview http://127.0.0.1:8000
target/debug/atom-cli runtime-smoke vllm_apple_preview http://127.0.0.1:8000 default_model 32
# Strata example; use an actual ID from the probe result.
target/debug/atom-cli runtime-probe strata http://127.0.0.1:8080
target/debug/atom-cli runtime-smoke strata http://127.0.0.1:8080 YOUR_MODEL_ID 32
```

Origins must be HTTP loopback addresses with an explicit non-default port and no credentials, query, fragment or API path. Literal `127.0.0.1` and `[::1]` are supported; `localhost` is normalized to `127.0.0.1` without DNS lookup. Do not append `/v1`. Redirects and environment proxies are disabled. This release does not support remote HTTPS, UDS, mTLS, token files or public cloud endpoints.

If the server requires Bearer authentication, set `ATOM_RUNTIME_TOKEN` in the environment of the CLI or the launched desktop process. Tokens are not GUI settings and are never serialized into capability/results or written to the configuration file. Authentication/HTTP error bodies are discarded. The gateway does not change the server's authentication policy.

## Profiles and capability evidence

| Profile | Probe requests | Interpretation |
|---|---|---|
| `vllm_apple_preview` | `GET /v1/models` | Model discovery only; readiness remains unknown. Text-only/4096 context/concurrency 1 limits describe the declared local-preview contract, not a hardware qualification or measured server property. |
| `vllm_apple_daemon` | Models plus `GET /health` | Reads `control_ready` and `inference_ready` separately. Other capabilities remain unknown unless exposed by a future adapter. |
| `strata` | Models plus `GET /v1/status` | Requires `service: strata` and a boolean `loaded`. Reads vision availability, structured-output formats, constrained decoding, context and serving concurrency when present. |

`connected` means all required probe stages returned usable responses. An empty model list can still be a connected service. `generation_verified` is **always false in a probe**: listing a model never proves generation. Optional capability values use JSON `null` for unknown, distinct from `false`. Strata's `prompt_and_validate` JSON support is not constrained decoding.

The play/`runtime-smoke` operation reprobes the server and checks the selected model immediately before posting. Explicit not-ready states, absent models and image-only models are rejected without a generation request. Strata models that are listed but unloaded are not autoloaded by this gateway. Known `alias_of` mappings permit the server to return its canonical model ID; the result records requested and returned IDs separately.

The generation request contains exactly one user message, `Reply with ATOM OK only.`, plus model, `max_tokens`, `temperature: 0` and `stream: false`. There is no system role, tools, JSON mode, arbitrary prompt, seed, document summary, geometry, Episode data or robot command. A successful result requires one assistant text choice, a recognized stop/length finish reason, no nonempty tool calls, and a matching model/declared alias. Reported completion usage cannot exceed the requested budget. The text is displayed with `textContent` rather than interpreted as HTML.

A result records output text, prompt/output hashes, requested budget, optional reported token usage and generation latency. `exact_smoke_reply` distinguishes an exact `ATOM OK` reply from a different or truncated answer. **Response received** means a valid protocol response was received, not that the answer is correct or the runtime is generally qualified. Token usage is reported by the server; ATOM does not retokenize output or certify generation determinism.

## Budgets and shared state

Each probe HTTP request has a three-second total deadline; generation has a fifteen-second deadline. A smoke operation includes its fresh probe, so the current two-request profiles can take up to roughly 6 + 15 seconds. Responses are limited to 64 KiB, displayed text to 4096 UTF-8 bytes and model lists to 64 entries with an explicit truncation flag. Requests do not retry automatically. A timeout ends the client wait; it does **not** prove that server-side generation stopped or GPU resources were released.

Every Tauri window uses one Rust runtime session, with revision checks, one active client operation and job IDs that reject late/duplicate completion. HTTP runs off the UI thread without holding the runtime-session or simulation-core mutex. No simulation document, history, revision, runtime sequence, cursor or savepoint is changed. This runtime session is separate from the shared simulation document, and all windows subscribe to its state updates. WebViews hold only form drafts and rendered snapshots.

Only profile/origin settings persist, using an atomic replacement of `dock-layout.runtime.json` beside the layout configuration by default. `ATOM_RUNTIME_CONFIG_PATH` can select another file. Invalid or oversized settings are preserved, with an explicit warning and reviewable default fields; they are not replaced until Apply is used. No connection starts on launch, and capability/response evidence is cleared on restart or configuration change. The runtime revision is session-local and independent of document Undo/Redo.

The Control API/MCP diagnosis still checks its own IPC connection, not these LLM servers. External MCP callers cannot invoke this smoke operation through the current read-only tool set.

## Validation and qualification

The local source contracts were checked at:

- vLLM-apple `9a12be3466aba14c081b1f405156db8286c13b2d`: local-preview documentation/launcher, daemon health/models handlers, and the installed MLX-LM response model field.
- Strata `c62d5e3a15b40ea51c96aae9e8d36928965f53ab`: `serve/server.py` models, aliases and status handlers.

The full workspace passes 246 tests (33 environment-dependent tests remain ignored); Clippy, formatting, JavaScript syntax and the development app build pass. Eight new tests cover three profiles, unknown-vs-false capabilities, exact request shape, readiness/model/budget gates, auth redaction, redirects, malformed/oversized responses, delayed-body deadlines, aliases, image models, session races/late replies, and settings restoration/preservation. Native macOS Tauri QA used a clearly named **contract-fixture-only** HTTP server to exercise settings → probe → test response, unavailable-server handling, three languages, approximately 220 px Workflow width and restart restoration. Request capture and Control IPC state comparison confirmed that no scene data was sent and document/history/sequence stayed unchanged. These are contract/UI tests, not real-model inference results.

Real-model qualification is pending in this development environment. The vLLM-apple launcher `--check` rejected installed MLX 0.32.2 and Transformers 5.19.0 as `unreviewed_mlx` / `unreviewed_transformers`. Its gate was not bypassed, dependencies/models were not changed, and no model was launched. No existing server answered on the documented 8000/8080 ports. Actual vLLM-apple/Strata inference, authenticated production configuration, GPU resource behavior, Linux/Windows UI and floating-window interaction require separate qualification.

Bounded scene summaries and typed/validated editor SkillPlan candidates connected to dry-run are now implemented separately. Next milestones include approved plan transactions, execution budgets shared with simulation jobs, explicit server cancellation support and real-model holdout evaluation. Plan text must never become a scene action without Rust validation.
