# Vendored foundations

Selected ThreeX and UNGE crates were copied into ATOM with the user's authorization.
They build without access to the original directories. `manifest.json` records source paths,
HEAD revisions, dirty working-tree status, and SHA-256 for every copied file.
The copies include local modifications already present in the source repositories.

- ThreeX: `core`, `renderer`, source and tests; root workspace supplies wgpu/bytemuck versions.
- UNGE: `unge-core`, `unge-executor`, source and tests; MIT/Apache licenses are included.
- `reference/` contains upstream guidance and the copied UNGE host composition/persistence examples. The original source directories are unchanged.
- ATOM extends the vendored ThreeX `Renderer::render_region` and `WgpuSurface::set_region` for physical viewport/scissor clipping. Existing full-frame APIs still work. `region_tests.rs` compares GPU pixels with a full-frame reference. Original import hashes and ATOM override hashes are kept separately in the manifest.

ThreeX had no LICENSE file in the inspected tree; redistribution terms remain to be established.
This copy is for local ATOM development. Do not imply that upstream GPU tests passed here.
Update copies deliberately and regenerate the provenance manifest; never silently overwrite local changes.
