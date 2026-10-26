# Local Control API and MCP

ATOM exposes the **running desktop's shared Rust state**, including unsaved edits. The bridge does not open another document or read the project file as a substitute for live state. This release supports reading and editor dry-runs; it does not apply edits, start simulation, execute shell commands, or call LLM runtimes.

## Connect

```sh
cargo build --locked -p atom-desktop -p atom-cli -p atom-mcp
# Keep the desktop running with this project.
target/debug/atom-desktop --project /absolute/path/project.atom.json
# In another terminal:
target/debug/atom-cli doctor /absolute/path/project.atom.json
target/debug/atom-cli control-state /absolute/path/project.atom.json
target/debug/atom-cli mcp-config /absolute/path/project.atom.json
```

The final command prints a generic `mcpServers` configuration with absolute executable and project arguments. Add that configuration to a compatible client's settings. It does not modify any AI client's settings or export a token. An optional final argument selects another `atom-mcp` executable. `python3 scripts/build-dev-app.py` bundles all three binaries in the macOS development app. Restart an older desktop process to enable its new endpoint.

## Tools and commands

| MCP tool | Result |
|---|---|
| `atom_capabilities` | Implemented operations, constraints and limits |
| `atom_project_get` | Live project summary, dirty/history state, activity mode and guard |
| `atom_scene_list` | Displayed world-space objects, paginated with offset and limit (1–64) |
| `atom_edit_dry_run` | Validation and bounded authored-state diff for one editor command |
| `atom_diagnose` | Discovery, transport, authentication, API-version and shared-state checks |

All tools have input/output JSON Schemas and read-only annotations. Project state and scene results omit mesh buffers, full documents, credentials and local paths. World/object coordinates use Y up and meters; physics gravity uses Z up; API joint angles use radians. A dry-run diff describes authored rest state rather than a transient rendered pose.

`control-schema` exports the public Control API v1 request JSON Schema. `control-call <project> <request.json>` sends a typed request to the same dispatcher used by the GUI and MCP. For example:

```json
{
  "api_version": 1,
  "request_id": "04f509de-cea4-4cbc-8490-3096aac93f38",
  "operation": {"method": "state"}
}
```

For `scene_list` or `dry_run`, copy the exact `data.guard` from a fresh state response into `operation.guard`. A dry-run operation has this shape:

```json
{
  "method": "dry_run",
  "guard": {"project_id": "COPY-FROM-STATE", "revision": 1, "sequence": 1},
  "command": {"kind": "rename", "name": "Candidate name"}
}
```

The guard includes project identity, document revision and runtime sequence. Refresh it after edits or preview changes. Errors include a stable code and retryability; inspect the current state before retrying a stale guard. Reusing a request UUID and identical body replays a cached response; reusing it with different arguments returns `request_conflict`. The cache retains the last 64 requests. Use a **new UUID for a fresh read**.

## Dry-run semantics

A disposable core copies the current authored document, Undo/Redo stacks, savepoint and next object ID, then runs the same editor validation/application path as the GUI. The real document, runtime, history and files remain unchanged. Supported commands cover rename, box creation/removal, transforms, gravity, materials, fixed/floor settings, robot templates, joint angles/ranges, position IK and Undo/Redo. Consult the exported schema for exact arguments. Imports and simulation/training commands are not exposed.

Active previews/jobs reject dry-runs. A locked core returns `busy` promptly. The result includes up to 64 changes, total count and truncation state. Change paths address authored entities by ID; they are **not executable RFC 6902 patches**. Physical edits carry a validation-only warning: no physics, collision, balance or stability test is performed. Cancellation suppresses the MCP reply and does not claim to stop physics or alter the GUI.

## Transport and diagnostics

Local stdio MCP attaches through a private Unix socket. A same-owner `0600` discovery file named `<project>.control.json` contains the endpoint and random token; the socket directory is `0700` and socket `0600`. A retained `<project>.control.lock` serializes endpoint ownership. Both files are ignored by Git. Do not share the discovery file. Normal shutdown removes the owned descriptor/socket; restarting replaces stale discovery after a crash. Each bridge call rediscovers the endpoint, allowing reconnection after desktop restart. An absent app reports `not_running`; disconnecting a bridge leaves the desktop alive.

Limits include 64 KiB request frames, 256 KiB Control responses, three-second socket frame deadlines and eight concurrent connections. Authentication is local process access protection, not a remote identity/authorization system. No HTTP listener is opened.

Workflow's Control · MCP check button and `doctor` test the actual endpoint. Diagnosis reports each connection stage and does not probe MuJoCo, vLLM-apple or Strata. GUI diagnosis is a manually refreshed observation, not a continuously monitored status.

The stdio implementation supports [MCP 2026-07-28 per-request version metadata](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning), `server/discover`, and the legacy [2025-11-25 initialization lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle). Messages follow the [stdio transport](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio). The pinned wire implementation does not advertise resources, prompts, subscriptions or tasks.

## Validation and remaining scope

Rust tests cover live unsaved state, history-preserving dry-runs, stale guards, duplicate/conflicting requests, pagination, invalid inputs, busy states, token/permission checks, stalled/oversized clients, reconnects, both MCP versions and cancellation. Native macOS Tauri QA changed a project without saving, read it through a separate MCP process, ran dry-runs, verified unchanged state/history/file, and checked the GUI diagnosis in English, Japanese and Simplified Chinese. Both protocol responses and tool outputs were checked against official MCP JSON Schemas.

Linux execution, Windows transport, individual commercial AI client configurations, remote HTTP authorization, actual external writes/jobs, and function-calling/OpenAPI bridges remain unverified or planned. Windows currently reports `unsupported_transport`. MCP conformance here does not imply every AI product supports both protocol versions.
