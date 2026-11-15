# Paired DAgger rounds

Run the same initial registered BC policy through two independent branches.
Each branch collects new train trajectories, labels sampled visited states,
appends them to its cumulative batch, and fits a fresh BC checkpoint. The
reference branch uses the reference position servo. The IK branch uses the
visited-state IK teacher with an explicit joint mask and weights.

```sh
atom-cli dagger-run /path/to/episodes project.atom.json dagger.json
```

In the desktop app, open **Workflow → Dataset → Automatic DAgger**.
Choose a registered initial policy and enter the teacher percentage for each
round, such as `100,50`. Enter the IK channel weights as a JSON array and set
the collection and training limits. The play icon starts the shared job and
the pause icon requests cooperative stop. Completed rounds stay visible while
the next stage runs. Use the saved experiment list and load icon to verify a
report and restore its settings and paired metrics.

A configuration for the default dual-arm fixture might use the following settings.
Replace the policy ID and use channel IDs from your own endpoint chain.

```json
{
  "initial_policy": "policy-<sha256>.json",
  "teacher_bps": [10000, 5000],
  "seed": 17,
  "start_seed": 0,
  "episodes_per_round": 2,
  "horizon": 60,
  "stride": 10,
  "joint_weights": [
    {"id": 2, "axis": 0, "weight": 1.0},
    {"id": 3, "axis": 0, "weight": 1.0},
    {"id": 4, "axis": 0, "weight": 1.0},
    {"id": 5, "axis": 0, "weight": 1.0},
    {"id": 6, "axis": 0, "weight": 1.0},
    {"id": 7, "axis": 0, "weight": 0.5}
  ],
  "training": {
    "seed": 7,
    "max_steps": 20,
    "max_seconds": 60,
    "batch_size": 32,
    "learning_rate": 0.001
  }
}
```

The schedule specifies teacher probability in basis points for each of one
to four rounds. At 0 the BC policy controls every step. At 10000 the teacher
controls every step. Intermediate values use deterministic per-step draws
keyed by the experiment seed, round, task seed and frame. Both branches use
the same draws for shared frames. Different terminal times can produce
different realized fractions.

Every sampled observation receives its branch's teacher target. The input
retains the previous actually applied command, including interventions.
Both branches use the same new train groups in each round. Checkpoints are
fit from scratch on cumulative data and initialize the next round's behavior
policy. Checkpoint selection uses validation loss. Each round runs both
frozen policies on the original fixed closed-loop validation group and
reports success rate, timeouts and mean final distance. The automatic
training path skips test scoring.

Read `episodes/dagger/<id>/round-N.json` for the paired metrics and realized
teacher/total step counts. `report.json` contains the configuration and
rounds completed up to the end of the run. Mining, labels, checkpoints,
registry entries and validation campaigns retain their existing artifact
paths. Validation campaigns can also be opened in the desktop panel.

Collection allows two to eight trajectories per branch per round and
retains the usual package limits. Collection, labeling and each paired
validation campaign have separate 300-second budgets. Training uses the
specified step and time limits. An incomplete job ends the experiment and
preserves published artifacts. Restarting the CLI creates a new experiment.

## Measured fixture

Both branches started from the same checkpoint and trained for 20 steps
per round. The fixed validation seeds were 2, 5, 31 and 59.

| Round | Teacher probability | Branch | Teacher / total steps | Success | Timeout | Mean final distance |
|---|---:|---|---:|---:|---:|---:|
| 1 | 100% | Reference servo | 55/55 | 0/4 | 4 | 120.98 cm |
| 1 | 100% | Weighted IK | 59/59 | 0/4 | 4 | 20.15 cm |
| 2 | 50% | Reference servo | 60/120 | 3/4 | 1 | 4.68 cm |
| 2 | 50% | Weighted IK | 44/87 | 1/4 | 3 | 29.84 cm |

The integration test verifies policy lineage, realized intervention counts,
four fixed tasks per branch and the absence of test scores in training jobs.


## Compare schedules in the desktop app

Enter one schedule per line in **Teacher % · one schedule per line**. For example,
enter `100,50` on the first line and `80,40` on the second. The app accepts up to
four distinct schedules with the same number of rounds. Each schedule starts
from the selected registered policy and uses the shared collection, seed, IK and
training settings. Within each schedule, each teacher branch retains its own
cumulative training data. Validation results are checked against the same task
definition and execution conditions across schedules.

The comparison displays a table for each schedule and round. Each table shows
teacher steps, success rate, timeouts and mean final distance in adjacent columns
for the reference servo and weighted IK. The results and inputs can be restored
from the saved experiment selector. The comparison report links to the individual
experiment reports and their validation campaigns.
