# Visited-state IK teacher

Set `"ik": true` in an existing `teacher-label` configuration to use the
`visited_position_ik_v1` rule. Omission preserves the reference servo and
the legacy artifact representation.

The IK teacher initializes the existing position solver from measured joint
positions, clipped to the channel limits. It solves the endpoint position
relative to the robot root. Joints outside the endpoint chain stay at their
clipped measured angles when IK succeeds. Successful solutions are limited to
a 0.25 rad change per channel from that measured pose. Unsolved states use
the original reference goal servo as a fallback. This is position-only IK,
without a collision, torque, or optimal-recovery guarantee.

The label package embeds the original Episode and teacher rule. The adapter
derives targets deterministically when preparing the training samples. Features
still include the actual previous applied action. Only train samples receive
these labels; validation/test rows and source Episodes remain unchanged.
The offline labeling budget applies to packaging, not subsequent adapter
preparation. Prepared artifacts and checkpoints retain their normal hash checks.

To compare recovery, pass a completed train mining report and a nonterminal
frame shared by the requested top trajectories:

```sh
atom-cli teacher-recovery /path/to/episodes mining-<sha256>.json 10 2
```

Both branches replay the applied BC prefix from reset and require exact
agreement with the recording, including the runtime metadata. This preserves
physics history and the success counter. Each teacher then controls the same
remaining episode horizon. The JSON reports per-seed outcomes, final distances,
steps and IK fallback counts, and aggregate success rate, timeouts and mean
final distance. The job allows 1–4 trajectories and checks a 120-second budget
between operations. Prefix mismatch or budget expiry yields no partial score.
This comparison runs on train probes, not on validation or test.

For a controlled retraining comparison:

1. Run `teacher-label` twice against the same mining report, with the same
   count and stride, once with `"ik": false` and once with `"ik": true`.
2. Train both exported batches with the same fresh `learning-train` config.
3. Register each job with its own batch via `policy-register`.
4. Run `policy-campaign` with the two registered IDs. It uses the original fixed
   validation groups and reports success rate, timeouts and mean final distance.
   The same registered policies can be compared in the existing desktop
   **Validation candidates** panel.

## Measured fixture results

The real MuJoCo/PyTorch integration test branched from frame 30 on train
seeds 1 and 3. Both teachers recovered on 2/2 tasks with no timeouts.
The reference servo took 105 steps on each task; IK took 27 and 69 steps
with no fallbacks. Mean final distance was 1.29 cm for reference and 2.62 cm
for IK, so faster recovery did not imply a smaller terminal error.

With the same 20-step BC settings, both retrained policies timed out on the
single fixed 300-step validation task (0/1 successes each). Mean final distance
was 11.31 cm for reference labels and 14.76 cm for IK labels. These results
verify the pipeline and show no validation gain from IK on this fixture.

## Joint mask and weights

Set `joint_weights` alongside `"ik": true` in the teacher-label configuration.
Each entry supplies a channel's object `id`, zero-based `axis` and `weight`
between 0.01 and 1. Use the probe Episode's manifest channels and the
endpoint-to-root chain to select eligible entries. When the list is populated,
omitted channels hold their measured angle clipped to their joint limits.
Duplicate channels and channels outside the endpoint chain are rejected.
Omit the list or supply an empty list to use the original IK teacher.

The weights scale coordinate updates, damped least-squares mobility, and
the per-channel displacement limit of 0.25 rad multiplied by the weight.
If IK fails, the masked teacher moves the enabled channels toward the
reference targets under the same limits. The label package retains the
weights so training and verified reload use the same configuration.

Save the same `joint_weights` array by itself as `weights.json` to compare
the teachers at several recorded frames.

```sh
atom-cli teacher-recovery-many /path/to/episodes mining-<sha256>.json 10,30 2 weights.json
```

The command accepts up to four distinct frames and four probe episodes.
Every selected frame must precede termination on every selected episode.
The resulting score uses one equally weighted pair per seed and frame.
For retraining, register both trained checkpoints and run the validation
campaign on the same fixed source batch groups. Each group contributes once.

The weighted test enabled axis 0 on IDs 2–7, with weight 0.5 on ID 7 and
1.0 elsewhere. Recovery on train seeds 1 and 3 at frames 10 and 30 yielded
4/4 successes for both teachers. Mean recovery steps were 67.25 for the
reference servo and 51.5 for weighted IK. Mean final distances were 1.74 cm
and 1.51 cm respectively. Weighted IK used zero fallback steps.

Both policies used 20 training steps and the same source rows and sampled
states. The 300-step fixed validation suite used seeds 2, 5, 31 and 59,
chosen before training from the validation partition with successful
reference demonstrations. Reference labels yielded 1/4 successes,
3 timeouts and 8.13 cm mean final distance. Weighted IK labels yielded
0/4 successes, 4 timeouts and 17.40 cm mean final distance.
