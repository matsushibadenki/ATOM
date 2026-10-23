# ATOM

<img src="docs/images/atom-image.png" alt="ATOM robot simulation workspace">

A desktop environment for robot simulation, language-based planning, and learning and evaluating robot policies.

**Implemented:** Tauri 2 / Rust, a native wgpu 3D viewport, docking and floating panels, physics settings, robot templates, and persistent static-mesh imports from Hugging Face / OpenUSD.

Rust owns documents, shared state, and rendering. WebViews send commands and display bounded state snapshots. The interface supports English, Japanese, and Simplified Chinese.

Last updated: October 23, 2026 (JST). LLM connections, training, and continuous simulation playback are not implemented. vLLM-apple and Strata are not connected yet. Selected ThreeX and UNGE sources are copied into `vendor/`, so builds do not depend on the original directories.

## Run

```sh
cd /Users/Shared/Program/Tauri/ATOM
cargo run --locked -p atom-desktop
```

Install Rust and the platform-specific Tauri development prerequisites. There is no Node.js build step.

By default, ATOM uses `workspace.atom.json` in its application data directory. To choose another location, use:

```sh
cargo run --locked -p atom-desktop -- --project /absolute/path/project.atom.json
```

Create the parent directory first.

```sh
cargo run --locked -p atom-cli -- init /tmp/example.atom.json "My Lab"
cargo run --locked -p atom-cli -- inspect /tmp/example.atom.json
cargo run --locked -p atom-cli -- validate /tmp/example.atom.json
```

`validate` generates and validates eight task specifications. It does not run physics or training.

- [Implementation scope and validation results](docs/FOUNDATION.md)
- [Docking controls, structure, and limitations](docs/DOCKING.md)
- For a macOS development app, run `python3 scripts/build-dev-app.py` to create `target/debug/ATOM.app`. This is not a signed distribution build.

## Workspace and 3D controls

Scene, Workflow, and 3D View support tab groups, nested horizontal and vertical splits, resizable dividers, and floating windows.

Drop a tab in the center of another panel to merge it, on an edge to split it, or outside the window to float it. The 3D View can also float. Use the group icon to float an entire tab group; groups can return together. Layout, active tabs, selection, and native window positions and sizes persist across restarts.

Directional and ambient lighting, warm neutral materials, selection outlines, an adaptive XZ grid, and world axes help inspect the scene.

| Action | Control |
|---|---|
| Orbit | Left drag |
| Pan | Right or middle drag, or Shift + left drag |
| Zoom | Mouse wheel |
| Focus selection | F or the focus icon |
| Move object | W |
| Rotate object | E |
| Scale object | R |
| Camera / selection mode | V |
| Undo / Redo | Command + Z / Command + Shift + Z; use Ctrl on Windows and Linux |

Select objects in the viewport or Scene list. Transform objects with axis handles or the XYZ inspector. Undo/Redo is shared across windows and retains up to 128 edits per session.

## Gravity, materials, and robot templates

In the Scene panel, open Environment to choose Earth, Moon, Mars, zero gravity, or numeric XYZ gravity. Physics gravity uses Z-up coordinates; the viewport and object placement use Y-up coordinates.

Select an object to edit its material, density, and sliding, torsional, and rolling friction under Physical material. Choose Floor under Apply to to edit floor friction. Numeric changes select Custom and support saving, reloading, and Undo/Redo. Floor contact, fixed objects, and robot templates are also editable and persistent.

Use the template selector and + button to add a robot:

| Template | Structure |
|---|---|
| Humanoid | 99 links, 98 articulated connections, and 182 rotational degrees of freedom, plus 6 floating-base degrees of freedom |
| Dual arm | 13 links, 6 joints per arm, and a fixed base |

The detailed humanoid includes a segmented spine, neck, jaw, shoulders, arms, hands and fingers, hips, legs, feet, and toes. Connection counts and rotational degrees of freedom are separate quantities. This is an engineering approximation, not a medically complete anatomical model or a universal human joint count. Existing simplified humanoids are preserved; add a new humanoid template to use the detailed model.

These generic box-link models have bounded position servos that hold the initial pose. They do not include learned walking, balance, or grasp policies. Selecting the root allows focusing the entire robot; deleting the root removes the whole robot. The inspector displays the robot's connection and rotational DoF counts and the selected link's axes and angular limits. Joint-angle and pivot editing remain planned.

Use the MuJoCo play icon in Workflow for a bounded physics preview, then reset to restore the authored scene. Preview durations are 0.002, 1, or 2 seconds; the viewport shows the final pose without changing the saved document.

The Python `mujoco` package is required for simulation. ATOM uses `python3` by default; set `ATOM_MUJOCO_PYTHON=/absolute/path/to/python` to use another environment. Editing, saving, and MJCF export remain available without MuJoCo. Models and dependencies are not installed automatically.

```sh
# Create new template projects without overwriting existing files.
cargo run --locked -p atom-cli -- template /tmp/humanoid.atom.json humanoid
cargo run --locked -p atom-cli -- template /tmp/dual-arm.atom.json dual_arm
cargo run --locked -p atom-cli -- export-mjcf /tmp/dual-arm.atom.json /tmp/dual-arm.xml
cargo run --locked -p atom-cli -- simulate /tmp/dual-arm.atom.json 500
```

The repository includes a [humanoid project](templates/humanoid.atom.json) and [MJCF](templates/humanoid.xml), plus a [dual-arm project](templates/dual-arm.atom.json) and [MJCF](templates/dual-arm.xml). These hand-authored box models do not depend on external robot assets. See the [implementation and validation record](docs/FOUNDATION.md) for details.

## Hugging Face / OpenUSD

Choose a source under OpenUSD · SimReady in the Scene panel. For Hugging Face, enter a dataset repository (default: `nvidia/simready-assets`), revision, and package name. Use the list icon to fetch the catalog and + to import. For local USD, enter an absolute path.

USD, USDA, and USDC are supported. The official OpenUSD SDK composes references, payloads, and their dependencies.

Create the dedicated ATOM SDK environment with the following command. It is already installed in the current development environment.

```sh
sh scripts/setup-usd.sh
# Use a different OpenUSD SDK environment.
ATOM_USD_PYTHON=/absolute/path/to/python cargo run --locked -p atom-desktop

cargo run --locked -p atom-cli -- catalog nvidia/simready-assets
cargo run --locked -p atom-cli -- import-hf /tmp/asset.atom.json nvidia/simready-assets box_cardboard_a01
cargo run --locked -p atom-cli -- import-usd /tmp/local.atom.json /absolute/path/asset.usda
```

ATOM imports public SimReady v1 packages containing one root USD asset. It pins the commit, verifies the bill of materials and file hashes, and retains licensing information. Original USD files and dependencies are cached in `.asset-cache` beside the project. Display meshes are embedded in the project, allowing saved documents to reopen offline. CLI imports append to a project and save it.

Current support is **static-mesh display at frame 0**. Rust/wgpu retains and renders meshes with coordinate and unit conversion, standard instances, and base colors. Imports support transforms, persistence, and Undo/Redo.

Imported articulations remain fixed in their displayed pose. Texture and MDL rendering, full USD physics conversion, articulated control, and USDZ support remain planned. MuJoCo contacts use a convex-hull approximation. The asset's information icon lists approximations and unsupported features. Source, revision, and license metadata are saved; original license text remains in the cache.

## Architecture and planned training pipeline

Tauri 2 provides the UI shell, Rust owns Document / State, and wgpu handles native 3D rendering. ThreeX supplies rendering code; UNGE supplies workflow execution and macOS panel-composition references. The current foundation integrates a native ThreeX viewport and UNGE task-specification validation.

The planned training pipeline starts with small tabletop-arm tasks in MuJoCo on macOS. Simulation records will support LoRA fine-tuning of small language models and evaluation on held-out conditions. vLLM-apple is intended for lightweight local inference, while Strata will provide teacher and planning candidates. Separate MLX or PyTorch workers will update model weights. Job management and these runtime integrations remain planned.

The following design documents are primarily in Japanese:

- [System architecture](docs/ARCHITECTURE.ja.md)
- [Runtime integration design](docs/RUNTIME-INTEGRATION.ja.md)
- [ThreeX / UNGE integration design](docs/REFERENCE-INTEGRATION.ja.md)
- [Implementation roadmap](docs/ROADMAP.md)
- [Research sources and open questions](docs/RESEARCH.md)
