# Validation-partition closed-loop Reach evaluation

The same frozen BC checkpoint can now be evaluated on the original saved
**validation** groups for development, separately from the fixed **test** groups.
No groups are borrowed, relabeled or repartitioned. This milestone adds execution,
recording, stop, registry/history and diagnostic routing; it does not automatically
change training settings, optimize weights or select another checkpoint.

## Workflow

In **Workflow → Dataset → Reach · closed loop**, the partition selector offers
**Validation** and **Test** (Japanese: **検証／テスト**, Simplified Chinese:
**验证／测试**). New GUI sessions default to validation. Load/train a checkpoint or
select a registered policy to restore its exact source batch, then:

1. Choose validation and press play. Rust snapshots the authored scene and runs
   both controllers only on the original validation groups, with their original
   seeds and horizons. All windows share one running job and its selected partition;
   the selector is disabled during execution.
2. Stop uses the existing cancellation path, including inference-wait polling and
   worker kill/wait cleanup. Cancellation publishes no partial rollout or registry
   evaluation link. An empty validation partition fails explicitly; it never falls
   back to test. Train is not an accepted execution partition.
3. Completed results show **Fixed validation groups**, controller success counts,
   seed-level outcomes, distance and clamping metrics. The report's `split` is
   `validation`. Both controllers' normal Episodes are saved for playback.
4. The policy registry automatically links the result. History labels show the
   evaluation partition before the checkpoint/run identity. B lists only matching
   partition **and** fixed task/execution conditions. The Rust API also rejects
   validation-versus-test comparisons if a client bypasses the filter.
5. Diagnose evaluation A as usual. The diagnostic report and GUI retain
   `split: validation`, while the feature envelope/mean/std still fit **train only**.
   Its task groups must exactly match the original validation groups. No test
   performance is scored or used to fit diagnostic support.
6. Open a history result and use its per-controller replay icons. Playback uses the
   saved Episode through the existing native player, including pause/seek/return
   to editor. Loading another saved result aligns the partition selector with that
   result for the next invocation.

The source batch remains one hash-verified package containing all original
partitions. Structural/package validation still checks that complete contract;
selecting validation does not execute or score the fixed test tasks.

## API and CLI compatibility

`learning::rollout::Partition` accepts only `Validation` and `Test`. New Rust
callers use `compare_partition(..., Partition::Validation)`. The existing
`compare(...)` function remains a **test-only** wrapper.

The desktop request is `{ "op": "rollout", "partition": "validation" }`.
For compatibility, a legacy request containing only `{ "op": "rollout" }` retains
its previous test behavior. The new GUI always sends an explicit partition.

```sh
atom-cli learning-rollout curated.batch.json /tmp/bc-job project.atom.json /tmp/val-run validation
atom-cli learning-rollout curated.batch.json /tmp/bc-job project.atom.json /tmp/test-run test
```

The optional CLI partition defaults to **test**, preserving existing scripts.
Invalid names, including `train`, are refused. Output directories must be new.
Budgets, checkpoint signature checks, action bounds and success/timeout semantics
are identical to the original [closed-loop contract](BC_ROLLOUT.md).

## Partition identity and saved artifacts

The existing `atom-bc-rollout-v1` report's `split` field now accepts `validation`
as well as `test`. Every result still hashes its report and Episodes in its commit
manifest. Registry evaluation views derive their partition from that hash-linked
report, so existing immutable registry record IDs do not need rewriting.

Validation comparison keys have an explicit validation namespace. The previous
fixed-test key algorithm is preserved byte-for-byte for test results; older test
records/checkpoint links remain loadable. Full linking/comparison checks match the
declared partition against the exact original source groups, seeds and horizons,
so merely changing `split` and rehashing a test report cannot register it as a
validation result. Existing model/reset/runtime checks remain in the comparison
key; validation and test cannot be mixed even when their runtime/model is equal.

Validation is the development partition. It is acceptable to use validation
results to inform development choices; fixed test should remain reserved for
final evaluation. This release does not automate those choices or introduce a new
validation-driven optimizer/model-selection loop. Repeated development can also
overfit validation; broader preregistered holdouts remain future work.

## Verification (2026-11-09)

- Standard tests cover empty validation with a nonempty test partition, no worker
  startup/fallback in that case, invalid partition rejection, legacy test command
  defaults and shared busy/revision/partition admission.
- The real PyTorch/MuJoCo test executes validation twice, verifies identical
  summaries, validation registry linking/history comparison/diagnosis/playback,
  rejects test-versus-validation comparison, rejects a rehashed/relabelled test
  artifact and tests cancellation without publication. Existing test execution,
  checkpoint integrity, train-only support and resume checks remain covered.
- CLI QA loaded a pre-existing test registry entry and compared two validation
  runs on seed 2. The reference servo succeeded after 24 steps (final distance
  0.0225928 m); the tiny BC policy timed out at 300 steps (1.03257 m). Test seed 20
  was not part of these runs. Both validation executions and diagnosis retained
  the same partition, and the cross-partition CLI comparison was refused.
- Isolated native Tauri QA: selected the registered policy, started/cancelled
  validation, then completed another validation evaluation. Cancellation left two
  prior validation artifacts and one test artifact; completion added exactly one
  validation artifact and kept test at one. The GUI compared validation histories,
  diagnosed them with train-only support and played the seed-2 BC Episode to frame
  300. Before playback, Control API state before/after evaluation/cancellation/
  diagnosis matched exactly excluding request UUIDs, including sequence, revision,
  dirty flag and Undo/Redo. The QA app returned to editor and exited cleanly.
- Actual UI files with real validation reports and a simulated Tauri bridge were
  checked using installed Playwright (no Browser skill available): English,
  Japanese and Simplified Chinese, 220px/320px. Explicit validation dispatch,
  start/stop, disabled running selector, partition-aware history filter, diagnosis,
  saved-result/replay dispatch, page identity, nonblank content, no overlay, no app
  console errors and no horizontal overflow passed. Native IPC/physics checks were
  performed separately above.

These tiny fixture results verify partition isolation and plumbing, not policy
quality. Other OSes, native floating-panel event checks and larger holdouts remain
pending.

Final checks: 283 standard workspace tests passed, 35 environment-dependent
tests ignored. The extended real PyTorch/MuJoCo validation/test isolation test was
explicitly run and passed. Clippy with warnings denied, formatting, JavaScript
syntax, diff checks and the macOS development app build passed.
