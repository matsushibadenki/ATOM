use super::*;
fn source() -> Project {
    let mut p = Project::new("Reach tests").unwrap();
    let (r, objects, _) = robots::build(RobotTemplate::DualArm, 1).unwrap();
    p.objects = objects;
    p.robots = vec![r];
    p
}
fn fixture() -> Episode {
    fixture_seed(7)
}
fn fixture_seed(seed: u32) -> Episode {
    let p = source();
    let m = prepare(
        &p,
        Config {
            seed,
            robot: 1,
            horizon: 5,
        },
    )
    .unwrap();
    let mut goal = m.project.clone();
    for (c, a) in m.channels.iter().zip(&m.reference_action) {
        goal.set_joint_angle(c.id, c.axis, *a as f32).unwrap();
    }
    let mut frames = vec![];
    for index in 0..=5 {
        let objects = if index == 0 {
            m.project.posed_objects()
        } else {
            goal.posed_objects()
        };
        let endpoint = objects
            .iter()
            .find(|o| o.id == m.endpoint)
            .unwrap()
            .position
            .map(f64::from);
        let observation = Observation {
            engine: "MuJoCo fixture".into(),
            python: "fixture".into(),
            platform: "fixture".into(),
            time: index as f64 * DT,
            qpos: if index == 0 {
                m.initial_action.clone()
            } else {
                m.reference_action.clone()
            },
            qvel: vec![0.; m.channels.len()],
            endpoint,
            poses: objects
                .iter()
                .map(|o| mujoco::Pose {
                    id: o.id,
                    position: o.position,
                    rotation: o.rotation,
                })
                .collect(),
        };
        frames.push(Frame {
            index,
            action: if index == 0 {
                None
            } else {
                Some(m.reference_action.clone())
            },
            distance: distance(endpoint, m.target),
            observation,
            outcome: if index == 5 {
                Outcome::Success
            } else {
                Outcome::Running
            },
        });
    }
    Episode {
        schema_version: 1,
        id: Uuid::new_v4(),
        manifest: m,
        frames,
    }
}
#[test]
fn seed_task_definition_is_stable_and_does_not_modify_authoring() {
    let p = source();
    let before = serde_json::to_vec(&p).unwrap();
    let a = prepare(
        &p,
        Config {
            seed: 7,
            robot: 1,
            horizon: 300,
        },
    )
    .unwrap();
    let b = prepare(
        &p,
        Config {
            seed: 7,
            robot: 1,
            horizon: 300,
        },
    )
    .unwrap();
    assert_eq!(
        serde_json::to_vec(&a).unwrap(),
        serde_json::to_vec(&b).unwrap()
    );
    assert_ne!(
        a.target,
        prepare(
            &p,
            Config {
                seed: 8,
                robot: 1,
                horizon: 300
            }
        )
        .unwrap()
        .target
    );
    assert_eq!(before, serde_json::to_vec(&p).unwrap());
    assert_eq!(a.project.physics.gravity.preset, GravityPreset::Zero);
    assert!(action_valid(&a, &a.reference_action));
    assert!(!action_valid(&a, &[f64::NAN]));
}
#[test]
fn episode_atomic_roundtrip_partial_records_and_tamper_rejection() {
    let e = fixture();
    e.validate().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reach-test.episode.json");
    e.save(&path).unwrap();
    e.save(&path).unwrap();
    let loaded = Episode::load(&path).unwrap();
    assert_eq!(
        serde_json::to_vec(&e).unwrap(),
        serde_json::to_vec(&loaded).unwrap()
    );
    let mut partial = e.clone();
    partial.frames.truncate(3);
    partial.validate().unwrap();
    assert!(matches!(partial.save(&path), Err(Error::ExternalChange)));
    let partial_path = dir.path().join("reach-partial.episode.json");
    partial.save(&partial_path).unwrap();
    assert_eq!(
        Episode::load(&partial_path)
            .unwrap()
            .frames
            .last()
            .unwrap()
            .outcome,
        Outcome::Running
    );
    let mut envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    envelope["episode"]["manifest"]["config"]["seed"] = 99.into();
    std::fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    assert!(Episode::load(&path).is_err());
    std::fs::write(&path, b"{\"episode\":").unwrap();
    assert!(Episode::load(&path).is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}
#[test]
fn episode_rejects_invalid_timing_actions_pose_and_terminal_sequences() {
    let e = fixture();
    for mutation in 0..10 {
        let mut bad = e.clone();
        match mutation {
            0 => bad.frames[1].observation.time += 0.001,
            1 => bad.frames[1].action.as_mut().unwrap()[0] = 100.,
            2 => bad.frames[1].index = 2,
            3 => bad.frames[1].outcome = Outcome::Success,
            4 => bad.frames.push(bad.frames.last().unwrap().clone()),
            5 => bad.frames[2].observation.poses.pop().map(|_| ()).unwrap(),
            6 => bad.frames[2].distance = f64::NAN,
            7 => bad.frames[0].action = Some(vec![]),
            8 => bad.manifest.target[0] += 1.,
            _ => bad.frames[2].observation.qvel.clear(),
        }
        assert!(bad.validate().is_err(), "mutation {mutation}");
    }
    assert_eq!(e.frames.last().unwrap().outcome, Outcome::Success); // success wins at horizon
    assert_eq!(outcome(5, 4, 5), Outcome::Timeout);
}
#[test]
fn replay_is_shared_read_only_and_restores_the_authored_scene() {
    let e = fixture();
    let dir = tempfile::tempdir().unwrap();
    e.save(&dir.path().join("reach-test.episode.json")).unwrap();
    let mut core = AppCore::new(source()).unwrap();
    let original = core.snapshot();
    fn command(core: &mut AppCore, c: Command, dir: &Path) -> Result<Snapshot> {
        let s = core.snapshot();
        core.reach_command(
            Request {
                project_id: s.project_id,
                expected_revision: s.revision,
                expected_sequence: s.sequence,
                command: c,
            },
            dir,
        )
    }
    command(
        &mut core,
        Command::Load {
            name: "reach-test.episode.json".into(),
        },
        dir.path(),
    )
    .unwrap();
    let id = core.snapshot().reach.unwrap().id;
    assert!(core.snapshot().previewing);
    assert!(core.snapshot().robots.is_empty());
    assert!(!core.snapshot().dirty);
    assert_eq!(core.snapshot().revision, original.revision);
    command(&mut core, Command::Seek { id, index: 5 }, dir.path()).unwrap();
    assert_eq!(core.snapshot().objects, e.objects_at(5).unwrap());
    assert!(
        core.apply(EditRequest {
            project_id: core.project().id,
            expected_revision: core.snapshot().revision,
            command: crate::Command::AddBox
        })
        .is_err()
    );
    assert!(
        command(
            &mut core,
            Command::Step {
                id,
                expected_step: 5,
                action: None
            },
            dir.path()
        )
        .is_err()
    );
    command(&mut core, Command::Play { id }, dir.path()).unwrap();
    let (_, token) = core.reach_play_token().unwrap();
    core.reach_play_tick(id, token).unwrap().unwrap();
    assert_eq!(core.snapshot().reach.unwrap().cursor, 1);
    command(&mut core, Command::Pause { id }, dir.path()).unwrap();
    assert!(core.reach_play_tick(id, token).unwrap().is_none());
    command(&mut core, Command::Play { id }, dir.path()).unwrap();
    let (_, fresh) = core.reach_play_token().unwrap();
    assert_ne!(token, fresh);
    assert!(core.reach_play_tick(id, token).unwrap().is_none());
    assert!(command(&mut core, Command::Close { id: Uuid::new_v4() }, dir.path()).is_err());
    command(&mut core, Command::Close { id }, dir.path()).unwrap();
    assert_eq!(core.snapshot().objects, original.objects);
    assert_eq!(core.snapshot().undo_depth, 0);
    assert!(!core.snapshot().previewing);
    assert!(
        command(
            &mut core,
            Command::Load {
                name: "../outside.episode.json".into()
            },
            dir.path()
        )
        .is_err()
    );
}
#[test]
#[ignore = "Requires real MuJoCo Python; reset/step determinism and physical success/timeout"]
fn real_mujoco_seeded_reset_step_success_timeout_record_and_playback() {
    let p = source();
    let config = Config {
        seed: 7,
        robot: 1,
        horizon: 300,
    };
    let mut a = Runner::reset(&p, config.clone()).unwrap();
    let mut b = Runner::reset(&p, config).unwrap();
    assert_eq!(
        serde_json::to_vec(&a.episode.frames).unwrap(),
        serde_json::to_vec(&b.episode.frames).unwrap()
    );
    let initial = serde_json::to_vec(&a.episode.frames).unwrap();
    assert!(a.step(Some(vec![100.; 12])).is_err());
    assert_eq!(initial, serde_json::to_vec(&a.episode.frames).unwrap());
    assert!(a.worker.running_for_test());
    a.run().unwrap();
    b.run().unwrap();
    assert!(!a.worker.running_for_test());
    assert!(!b.worker.running_for_test());
    assert_eq!(a.episode.frames.last().unwrap().outcome, Outcome::Success);
    assert_eq!(
        serde_json::to_vec(&a.episode.frames).unwrap(),
        serde_json::to_vec(&b.episode.frames).unwrap()
    );
    a.episode.validate().unwrap();
    assert!(a.step(None).is_err());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reach-real.episode.json");
    a.episode.save(&path).unwrap();
    let restored = Episode::load(&path).unwrap();
    assert_eq!(restored.objects_at(0).unwrap(), p.posed_objects());
    for i in 0..restored.frames.len() {
        assert_eq!(restored.objects_at(i).unwrap().len(), 13);
    }
    let mut timeout = Runner::reset(
        &p,
        Config {
            seed: 7,
            robot: 1,
            horizon: 1,
        },
    )
    .unwrap();
    timeout.run().unwrap();
    assert_eq!(
        timeout.episode.frames.last().unwrap().outcome,
        Outcome::Timeout
    );
    timeout.episode.validate().unwrap();
}

#[test]
fn invalid_task_without_a_joint_and_failed_load_preserve_playback_cursor() {
    let mut p = source();
    p.robots[0].links.truncate(1);
    p.objects.truncate(1);
    assert!(
        prepare(
            &p,
            Config {
                seed: 7,
                robot: 1,
                horizon: 10
            }
        )
        .is_err()
    );
    let e = fixture();
    let dir = tempfile::tempdir().unwrap();
    e.save(&dir.path().join("reach-test.episode.json")).unwrap();
    let mut core = AppCore::new(source()).unwrap();
    let send = |core: &mut AppCore, command| {
        let s = core.snapshot();
        core.reach_command(
            Request {
                project_id: s.project_id,
                expected_revision: s.revision,
                expected_sequence: s.sequence,
                command,
            },
            dir.path(),
        )
    };
    send(
        &mut core,
        Command::Load {
            name: "reach-test.episode.json".into(),
        },
    )
    .unwrap();
    send(&mut core, Command::Seek { id: e.id, index: 2 }).unwrap();
    assert!(
        send(
            &mut core,
            Command::Seek {
                id: e.id,
                index: 999
            }
        )
        .is_err()
    );
    assert_eq!(core.snapshot().reach.unwrap().cursor, 2);
    assert!(
        send(
            &mut core,
            Command::Load {
                name: "reach-missing.episode.json".into()
            }
        )
        .is_err()
    );
    assert_eq!(core.snapshot().reach.unwrap().cursor, 2);
}
#[test]
#[ignore = "Requires MuJoCo Python; worker crash must preserve a valid partial record"]
fn real_worker_failure_keeps_partial_episode_and_blocks_further_steps() {
    let mut runner = Runner::reset(
        &source(),
        Config {
            seed: 7,
            robot: 1,
            horizon: 300,
        },
    )
    .unwrap();
    runner.step(None).unwrap();
    let before = serde_json::to_vec(&runner.episode.frames).unwrap();
    runner.worker.terminate_for_test();
    assert!(runner.step(None).is_err());
    assert!(runner.failed);
    assert!(runner.step(None).is_err());
    assert_eq!(before, serde_json::to_vec(&runner.episode.frames).unwrap());
    runner.episode.validate().unwrap();
    let dir = tempfile::tempdir().unwrap();
    runner
        .episode
        .save(&dir.path().join("partial.episode.json"))
        .unwrap();
}
#[test]
#[ignore = "Requires MuJoCo Python; live command identity and stale-step protection"]
fn real_live_requests_reject_duplicates_and_stale_run_ids() {
    let mut core = AppCore::new(source()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let send = |core: &mut AppCore, command| {
        let s = core.snapshot();
        core.reach_command(
            Request {
                project_id: s.project_id,
                expected_revision: s.revision,
                expected_sequence: s.sequence,
                command,
            },
            dir.path(),
        )
    };
    let s = send(
        &mut core,
        Command::Reset {
            config: Config {
                seed: 7,
                robot: 1,
                horizon: 300,
            },
        },
    )
    .unwrap();
    let id = s.reach.unwrap().id;
    send(
        &mut core,
        Command::Step {
            id,
            expected_step: 0,
            action: None,
        },
    )
    .unwrap();
    assert!(
        send(
            &mut core,
            Command::Step {
                id,
                expected_step: 0,
                action: None
            }
        )
        .is_err()
    );
    assert_eq!(core.snapshot().reach.unwrap().frames, 2);
    send(&mut core, Command::Seek { id, index: 0 }).unwrap();
    assert!(
        send(
            &mut core,
            Command::Step {
                id,
                expected_step: 1,
                action: None
            }
        )
        .is_err()
    );
    assert_eq!(core.snapshot().reach.unwrap().frames, 2);
    send(&mut core, Command::Seek { id, index: 1 }).unwrap();
    send(
        &mut core,
        Command::Step {
            id,
            expected_step: 1,
            action: None,
        },
    )
    .unwrap();
    assert_eq!(core.snapshot().reach.unwrap().frames, 3);
    assert!((core.snapshot().reach.unwrap().time - 0.04).abs() < 1e-9);
    let current = core.snapshot();
    assert!(
        core.reach_command(
            Request {
                project_id: current.project_id,
                expected_revision: current.revision,
                expected_sequence: current.sequence - 1,
                command: Command::Reset {
                    config: Config {
                        seed: 7,
                        robot: 1,
                        horizon: 300
                    }
                }
            },
            dir.path()
        )
        .is_err()
    );
    send(
        &mut core,
        Command::Reset {
            config: Config {
                seed: 7,
                robot: 1,
                horizon: 300,
            },
        },
    )
    .unwrap();
    let fresh = core.snapshot().reach.unwrap().id;
    assert_ne!(fresh, id);
    assert!(
        send(
            &mut core,
            Command::Step {
                id,
                expected_step: 0,
                action: None
            }
        )
        .is_err()
    );
    assert_eq!(core.snapshot().reach.unwrap().frames, 1);
    send(&mut core, Command::Close { id: fresh }).unwrap();
    assert!(core.snapshot().reach.is_none());
    assert!(!core.snapshot().dirty);
}

#[test]
#[ignore = "Requires MuJoCo Python; 10 seeds x 2 physical reruns"]
fn real_ten_seed_reruns_match_every_transition() {
    let p = source();
    let mut success = 0;
    let mut timeout = 0;
    for seed in 0..10 {
        let mut a = Runner::reset(
            &p,
            Config {
                seed,
                robot: 1,
                horizon: 300,
            },
        )
        .unwrap();
        a.run().unwrap();
        let mut b = Runner::reset(
            &p,
            Config {
                seed,
                robot: 1,
                horizon: 300,
            },
        )
        .unwrap();
        b.run().unwrap();
        assert_eq!(
            serde_json::to_vec(&a.episode.manifest).unwrap(),
            serde_json::to_vec(&b.episode.manifest).unwrap(),
            "seed {seed}"
        );
        assert_eq!(
            serde_json::to_vec(&a.episode.frames).unwrap(),
            serde_json::to_vec(&b.episode.frames).unwrap(),
            "seed {seed}"
        );
        a.episode.validate().unwrap();
        b.episode.validate().unwrap();
        match a.episode.frames.last().unwrap().outcome {
            Outcome::Success => success += 1,
            Outcome::Timeout => timeout += 1,
            _ => panic!("unterminated"),
        }
    }
    println!(
        "10 seeds x 2: exact transition equality; per first run: {success} success, {timeout} timeout"
    );
}

#[test]
fn dataset_aligns_pre_action_observation_and_keeps_teacher_goal_out_of_inputs() {
    use dataset::ExportPolicy;
    let e = fixture();
    let before = serde_json::to_vec(&e).unwrap();
    let data = e.dataset(ExportPolicy::SuccessOnly).unwrap();
    assert_eq!(data.transitions.len(), e.frames.len() - 1);
    for (i, t) in data.transitions.iter().enumerate() {
        assert_eq!(t.index, i);
        assert_eq!(t.time_seconds, e.frames[i].observation.time);
        assert_eq!(t.next_time_seconds, e.frames[i + 1].observation.time);
        assert_eq!(&t.action_rad, e.frames[i + 1].action.as_ref().unwrap());
        assert_eq!(
            t.observation.previous_action_rad,
            if i == 0 {
                e.manifest.initial_action.clone()
            } else {
                e.frames[i].action.clone().unwrap()
            }
        );
        for (j, c) in e.manifest.channels.iter().enumerate() {
            assert_eq!(
                t.observation.joint_position_rad[j],
                e.frames[i].observation.qpos[c.qpos_index]
            );
            assert_eq!(
                t.next_observation.joint_position_rad[j],
                e.frames[i + 1].observation.qpos[c.qpos_index]
            );
        }
        assert_eq!(t.next_observation.previous_action_rad, t.action_rad);
        assert_eq!(t.terminated, i + 1 == data.transitions.len());
        assert!(!t.truncated);
    }
    let json = serde_json::to_value(&data).unwrap();
    assert!(json["metadata"].get("reference_action").is_none());
    assert!(json["metadata"].get("initial_action").is_none());
    for t in json["transitions"].as_array().unwrap() {
        for key in ["observation", "next_observation"] {
            let o = t[key].as_object().unwrap();
            assert_eq!(o.len(), 5);
            assert!(!o.contains_key("reference_action"));
            assert!(!o.contains_key("initial_action"));
        }
    }
    assert!(data.quality.complete && data.quality.success_exportable);
    assert_eq!(before, serde_json::to_vec(&e).unwrap());
}
#[test]
fn dataset_filters_partial_and_timeout_and_checks_integrity_before_metrics() {
    use dataset::ExportPolicy;
    let mut e = fixture();
    e.frames.pop();
    let q = e.quality().unwrap();
    assert!(!q.complete && !q.completed_exportable);
    assert!(q.warnings.contains(&"partial_episode".into()));
    assert!(e.dataset(ExportPolicy::Completed).is_err());
    e.frames.truncate(1);
    assert_eq!(e.quality().unwrap().transitions, 0);
    assert!(e.dataset(ExportPolicy::SuccessOnly).is_err());
    let mut e = fixture();
    // Time out before five consecutive in-tolerance samples.
    e.manifest.config.horizon = 1;
    e.frames.truncate(2);
    e.frames[1].outcome = Outcome::Timeout;
    assert!(e.dataset(ExportPolicy::SuccessOnly).is_err());
    let d = e.dataset(ExportPolicy::Completed).unwrap();
    assert!(d.transitions[0].truncated);
    assert!(!d.transitions[0].terminated);
    assert!(
        d.quality
            .warnings
            .contains(&"timeout_not_success_demonstration".into())
    );
    e.frames[1].observation.time += 0.001;
    assert!(e.quality().is_err());
    let mut e = fixture();
    e.frames[1].action.as_mut().unwrap()[0] = f64::NAN;
    assert!(e.quality().is_err());
}
#[test]
fn dataset_group_and_atomic_output_are_stable_and_conflicts_are_preserved() {
    use dataset::ExportPolicy;
    let e = fixture();
    let q = e.quality().unwrap();
    let mut partial = e.clone();
    partial.id = Uuid::new_v4();
    partial.frames.truncate(2);
    assert_eq!(q.split_group, partial.quality().unwrap().split_group);
    assert_ne!(q.episode_sha256, partial.quality().unwrap().episode_sha256);
    partial.manifest.source_sha256 = "a".repeat(64);
    assert_eq!(q.split_group, partial.quality().unwrap().split_group);
    partial.manifest.config.horizon = 1;
    partial.frames[1].outcome = Outcome::Timeout;
    assert_eq!(q.split_group, partial.quality().unwrap().split_group);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("output.json");
    let d = e.dataset(ExportPolicy::SuccessOnly).unwrap();
    d.save(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    d.save(&path).unwrap();
    assert_eq!(bytes, std::fs::read(&path).unwrap());
    let envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        envelope["sha256"],
        digest(&serde_json::to_vec(&envelope["dataset"]).unwrap())
    );
    assert_eq!(
        envelope["dataset"]["transitions"].as_array().unwrap().len(),
        5
    );
    assert!(
        partial
            .dataset(ExportPolicy::Completed)
            .unwrap()
            .save(&path)
            .is_err()
    );
    assert_eq!(bytes, std::fs::read(&path).unwrap());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn dataset_remaps_nontrivial_qpos_indices_and_reports_physical_warnings() {
    use dataset::ExportPolicy;
    let mut e = fixture();
    // Reorder source links (legal sibling traversal) so action channel order and qpos differ.
    e.manifest.project.robots[0].links = [0, 1, 7, 2, 8, 3, 9, 4, 10, 5, 11, 6, 12]
        .map(|i| e.manifest.project.robots[0].links[i].clone())
        .to_vec();
    let m = prepare(&e.manifest.project, e.manifest.config.clone()).unwrap();
    assert!(
        m.channels
            .iter()
            .enumerate()
            .any(|(i, c)| i != c.qpos_index)
    );
    for f in &mut e.frames {
        let old = &e.manifest;
        let mut qpos = vec![0.; m.channels.len()];
        let mut action = vec![0.; m.channels.len()];
        for (j, c) in m.channels.iter().enumerate() {
            let oi = old
                .channels
                .iter()
                .position(|o| o.id == c.id && o.axis == c.axis)
                .unwrap();
            qpos[c.qpos_index] = f.observation.qpos[old.channels[oi].qpos_index];
            action[j] = f.action.as_ref().unwrap_or(&old.initial_action)[oi];
        }
        f.observation.qpos = qpos;
        if f.index > 0 {
            f.observation.qvel = (0..m.channels.len())
                .map(|j| 0.1 * (j + 1) as f64)
                .collect();
            f.action = Some(action);
        }
    }
    e.manifest = m;
    e.validate().unwrap();
    let d = e.dataset(ExportPolicy::SuccessOnly).unwrap();
    for t in &d.transitions {
        for (j, c) in e.manifest.channels.iter().enumerate() {
            assert_eq!(
                t.next_observation.joint_position_rad[j],
                e.frames[t.index + 1].observation.qpos[c.qpos_index]
            );
            assert_eq!(
                t.next_observation.joint_velocity_rad_s[j],
                e.frames[t.index + 1].observation.qvel[c.qpos_index]
            );
        }
    }
    let c = &e.manifest.channels[0];
    e.frames[1].observation.qpos[c.qpos_index] = f64::from(c.range[1]) + 0.05;
    e.frames[1].observation.qvel[c.qpos_index] = 2.5;
    let q = e.quality().unwrap();
    assert!((q.max_limit_excess_rad - 0.05).abs() < 1e-10);
    assert_eq!(q.max_abs_velocity_rad_s, 2.5);
    assert!(q.warnings.contains(&"observed_joint_limit_excess".into()));
}

#[test]
fn dataset_shared_recording_guard_and_analysis_leave_editor_and_cursor_unchanged() {
    let e = fixture();
    let dir = tempfile::tempdir().unwrap();
    e.save(&dir.path().join("reach-analysis.episode.json"))
        .unwrap();
    let mut core = AppCore::new(source()).unwrap();
    let s = core.snapshot();
    core.reach_command(
        Request {
            project_id: s.project_id,
            expected_revision: s.revision,
            expected_sequence: s.sequence,
            command: Command::Load {
                name: "reach-analysis.episode.json".into(),
            },
        },
        dir.path(),
    )
    .unwrap();
    let s = core.snapshot();
    let before = serde_json::to_value(&s).unwrap();
    let request = dataset::Request {
        project_id: s.project_id,
        expected_revision: s.revision,
        expected_sequence: s.sequence,
        episode_id: e.id,
    };
    let frozen = core.recorded_episode(&request).unwrap();
    frozen.quality().unwrap();
    frozen
        .dataset(dataset::ExportPolicy::SuccessOnly)
        .unwrap()
        .save(&dir.path().join("training.json"))
        .unwrap();
    assert_eq!(before, serde_json::to_value(core.snapshot()).unwrap());
    let mut bad = request.clone();
    bad.episode_id = Uuid::new_v4();
    assert!(core.recorded_episode(&bad).is_err());
    bad = request.clone();
    bad.project_id = Uuid::new_v4();
    assert!(core.recorded_episode(&bad).is_err());
    bad = request.clone();
    bad.expected_revision += 1;
    assert!(core.recorded_episode(&bad).is_err());
    bad = request.clone();
    bad.expected_sequence += 1;
    assert!(core.recorded_episode(&bad).is_err());
    core.reach_command(
        Request {
            project_id: s.project_id,
            expected_revision: s.revision,
            expected_sequence: s.sequence,
            command: Command::Play { id: e.id },
        },
        dir.path(),
    )
    .unwrap();
    let s = core.snapshot();
    let request = dataset::Request {
        expected_sequence: s.sequence,
        ..request
    };
    assert!(core.recorded_episode(&request).is_err());
}

#[test]
fn batch_quality_dedup_and_checkpoint_versions_are_auditable() {
    use dataset::batch;
    let dir = tempfile::tempdir().unwrap();
    let full = fixture();
    full.save(&dir.path().join("a.episode.json")).unwrap();
    full.save(&dir.path().join("b.episode.json")).unwrap();
    let mut partial = full.clone();
    partial.frames.truncate(2);
    partial
        .save(&dir.path().join("partial.episode.json"))
        .unwrap();
    std::fs::write(dir.path().join("broken.episode.json"), b"{}").unwrap();
    let mut request = batch::Request {
        sources: [
            "partial.episode.json",
            "b.episode.json",
            "broken.episode.json",
            "a.episode.json",
        ]
        .map(|s| dir.path().join(s))
        .to_vec(),
        ..Default::default()
    };
    let report = request.analyze().unwrap();
    assert_eq!(report.selected(), 1);
    assert!(
        report
            .entries()
            .iter()
            .any(|e| e.reasons.iter().any(|r| r == "partial_episode"))
    );
    assert!(
        report
            .entries()
            .iter()
            .any(|e| e.reasons.iter().any(|r| r == "duplicate_episode"))
    );
    assert!(
        report
            .entries()
            .iter()
            .any(|e| e.quality.is_none() && !e.reasons.is_empty())
    );
    let split = report.entries()[0].split;
    for e in report.entries().iter().filter(|e| e.quality.is_some()) {
        assert_eq!(e.split, split);
    }
    let before = serde_json::to_vec(&report).unwrap();
    request.sources.reverse();
    assert_eq!(
        before,
        serde_json::to_vec(&request.analyze().unwrap()).unwrap()
    );
    let mut version = full.clone();
    version.manifest.source_revision = Some(8);
    version
        .save(&dir.path().join("version.episode.json"))
        .unwrap();
    request
        .sources
        .push(dir.path().join("version.episode.json"));
    let versions = request.analyze().unwrap();
    assert_eq!(versions.selected(), 1);
    assert!(
        versions
            .entries()
            .iter()
            .any(|e| e.reasons.iter().any(|r| r == "superseded_episode_version"))
    );
    request.limits.max_action_delta_rad = Some(0.);
    assert_eq!(request.analyze().unwrap().selected(), 0);
}
#[test]
fn batch_partition_is_group_stable_and_not_rebalanced_when_appending() {
    use dataset::batch;
    let dir = tempfile::tempdir().unwrap();
    let e = fixture();
    e.save(&dir.path().join("a.episode.json")).unwrap();
    let mut rerun = e.clone();
    rerun.id = Uuid::new_v4();
    rerun.save(&dir.path().join("rerun.episode.json")).unwrap();
    let mut request = batch::Request {
        sources: vec![dir.path().join("a.episode.json")],
        ..Default::default()
    };
    let split = request.analyze().unwrap().entries()[0].split;
    request.sources.push(dir.path().join("rerun.episode.json"));
    let report = request.analyze().unwrap();
    assert_eq!(report.selected(), 2);
    for row in report.entries() {
        assert_eq!(row.split, split);
    }
    assert_eq!(report.splits().values().map(|s| s.groups).sum::<usize>(), 1);
    assert_eq!(
        report
            .splits()
            .values()
            .map(|s| s.transitions)
            .sum::<usize>(),
        10
    );
    let mut assignment = std::collections::BTreeSet::new();
    for i in 0..200 {
        assignment.insert(
            request
                .split
                .assign(&format!("independent-task-{i}"))
                .unwrap(),
        );
    }
    assert_eq!(assignment.len(), 3);
    request.split.validation_bps = 9000;
    request.split.test_bps = 1000;
    assert!(request.analyze().is_err());
    request.split.test_bps = 0;
    request.limits.max_final_distance_m = Some(f64::NAN);
    assert!(request.analyze().is_err());
}
#[test]
fn batch_atomic_package_contains_only_selected_splits_and_refuses_changed_sources() {
    use dataset::batch;
    let dir = tempfile::tempdir().unwrap();
    let e = fixture();
    let path = dir.path().join("run.episode.json");
    e.save(&path).unwrap();
    let request = batch::Request {
        sources: vec![path.clone()],
        ..Default::default()
    };
    let report = request.analyze().unwrap();
    let out = dir.path().join("curated.batch.json");
    let receipt = report.save_package(&out).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(receipt.sha256, digest(&bytes));
    assert_eq!(receipt.bytes, bytes.len() as u64);
    assert_eq!(report.save_package(&out).unwrap().sha256, receipt.sha256);
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["datasets"].as_array().unwrap().len(), 1);
    assert_eq!(
        value["datasets"][0]["dataset"]["transitions"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert!(
        value["datasets"][0]["dataset"]["transitions"][0]["observation"]
            .get("reference_action")
            .is_none()
    );
    assert_eq!(
        value["datasets"][0]["split"],
        value["report"]["entries"][0]["split"]
    );
    std::fs::write(&out, b"keep existing").unwrap();
    assert!(report.save_package(&out).is_err());
    assert_eq!(std::fs::read(&out).unwrap(), b"keep existing");
    let mut changed = e.clone();
    changed.id = Uuid::new_v4();
    std::fs::remove_file(&path).unwrap();
    changed.save(&path).unwrap();
    let absent = dir.path().join("not-published.batch.json");
    assert!(report.save_package(&absent).is_err());
    assert!(!absent.exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}
#[test]
fn batch_timeout_filter_conflicting_identity_and_input_budget_are_enforced() {
    use dataset::{ExportPolicy, batch};
    let dir = tempfile::tempdir().unwrap();
    let full = fixture();
    let mut timeout = full.clone();
    timeout.frames.truncate(2);
    timeout.manifest.config.horizon = 1;
    timeout.frames[1].outcome = Outcome::Timeout;
    let a = dir.path().join("timeout.episode.json");
    timeout.save(&a).unwrap();
    let mut request = batch::Request {
        sources: vec![a.clone()],
        ..Default::default()
    };
    assert_eq!(request.analyze().unwrap().selected(), 0);
    request.policy = ExportPolicy::Completed;
    assert_eq!(request.analyze().unwrap().selected(), 1);
    // Use a second valid task from a separately prepared deterministic seed.
    let m = prepare(
        &source(),
        Config {
            seed: 8,
            robot: 1,
            horizon: 1,
        },
    )
    .unwrap();
    let reset = Observation {
        engine: "MuJoCo fixture".into(),
        python: "fixture".into(),
        platform: "fixture".into(),
        time: 0.,
        qpos: m.initial_action.clone(),
        qvel: vec![0.; m.channels.len()],
        endpoint: timeout.frames[0].observation.endpoint,
        poses: timeout.frames[0].observation.poses.clone(),
    };
    let mut other = Episode {
        schema_version: 1,
        id: full.id,
        manifest: m,
        frames: vec![],
    };
    other.frames.push(Frame {
        index: 0,
        action: None,
        distance: distance(reset.endpoint, other.manifest.target),
        observation: reset,
        outcome: Outcome::Running,
    });
    let b = dir.path().join("other-task.episode.json");
    other.save(&b).unwrap();
    request.sources.push(b);
    let report = request.analyze().unwrap();
    assert_eq!(report.selected(), 0);
    assert!(
        report
            .entries()
            .iter()
            .all(|e| e.reasons.iter().any(|r| r == "episode_id_conflict"))
    );
    let huge = dir.path().join("huge.episode.json");
    std::fs::File::create(&huge)
        .unwrap()
        .set_len(batch::MAX_INPUT_BYTES + 1)
        .unwrap();
    request.sources = vec![huge];
    assert!(request.analyze().is_err());
}

fn learning_fixture(dir: &Path) -> std::path::PathBuf {
    use dataset::batch::{Request, Split};
    let mut request = Request::default();
    let mut found = std::collections::BTreeSet::new();
    for seed in 0..100 {
        let episode = fixture_seed(seed);
        let split = request
            .split
            .assign(&episode.quality().unwrap().split_group)
            .unwrap();
        if found.insert(split) {
            let p = dir.join(format!("seed-{seed}.episode.json"));
            episode.save(&p).unwrap();
            request.sources.push(p);
        }
        if found.len() == 3 {
            break;
        }
    }
    assert_eq!(found, [Split::Train, Split::Validation, Split::Test].into());
    let path = dir.join("source.batch.json");
    request.analyze().unwrap().save_package(&path).unwrap();
    path
}
#[test]
fn learning_roundtrip_preserves_splits_channels_and_train_only_normalization() {
    use crate::learning::Validated;
    let dir = tempfile::tempdir().unwrap();
    let path = learning_fixture(dir.path());
    let data = Validated::load(&path).unwrap();
    let summary = data.summary();
    assert!(summary.fit_data_ready && summary.test_data_ready);
    assert_eq!((summary.feature_dim, summary.action_dim), (42, 12));
    let prepared = data.prepare().unwrap();
    let value = serde_json::to_value(&prepared).unwrap();
    assert_eq!(value["normalization"]["count"], 5);
    assert_eq!(value["contract"]["channels"].as_array().unwrap().len(), 12);
    let package: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for item in package["datasets"].as_array().unwrap() {
        let split = item["split"].as_str().unwrap();
        assert_eq!(value["splits"][split].as_array().unwrap().len(), 5);
        assert_eq!(
            value["splits"][split][0]["action"],
            item["dataset"]["transitions"][0]["action_rad"]
        );
        assert_eq!(
            value["splits"][split][0]["features"][0],
            item["dataset"]["transitions"][0]["observation"]["joint_position_rad"][0]
        );
    }
    // Rebuild after changing every held-out velocity. Train statistics must stay identical.
    let mut request = dataset::batch::Request::default();
    for entry in package["report"]["entries"].as_array().unwrap() {
        let source = Path::new(entry["source"].as_str().unwrap());
        let mut episode = Episode::load(source).unwrap();
        if entry["split"] != "train" {
            for frame in episode.frames.iter_mut().skip(1) {
                frame.observation.qvel.fill(123.);
            }
        } else {
            // The final next observation is never an input sample or normalization datum.
            episode
                .frames
                .last_mut()
                .unwrap()
                .observation
                .qvel
                .fill(321.);
        }
        let replacement = dir.path().join(format!(
            "new-{}",
            source.file_name().unwrap().to_string_lossy()
        ));
        episode.save(&replacement).unwrap();
        request.sources.push(replacement);
    }
    let other = dir.path().join("modified.batch.json");
    request.analyze().unwrap().save_package(&other).unwrap();
    let changed =
        serde_json::to_value(Validated::load(&other).unwrap().prepare().unwrap()).unwrap();
    assert_eq!(value["normalization"], changed["normalization"]);
    assert_eq!(value["splits"]["train"], changed["splits"]["train"]);
    assert_ne!(value["splits"]["test"], changed["splits"]["test"]);
    // A portable package no longer depends on the source Episode paths.
    for source in request.sources {
        std::fs::remove_file(source).unwrap();
    }
    let out = dir.path().join("prepared.bc.json");
    let digest = prepared.save(&out).unwrap();
    assert_eq!(digest, prepared.save(&out).unwrap());
    std::fs::write(&out, b"preserve").unwrap();
    assert_eq!(
        prepared.save(&out).err().unwrap().code,
        "learning_output_conflict"
    );
    assert_eq!(std::fs::read(&out).unwrap(), b"preserve");
}
#[test]
fn learning_rejects_split_relabeling_future_fields_timing_and_audit_tampering() {
    use crate::learning::Validated;
    let dir = tempfile::tempdir().unwrap();
    let path = learning_fixture(dir.path());
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mutations: [(&str, serde_json::Value); 5] = [
        ("/datasets/0/split", serde_json::json!("unknown")),
        (
            "/datasets/0/dataset/transitions/0/observation/reference_action",
            serde_json::json!([0.]),
        ),
        (
            "/datasets/0/dataset/transitions/0/time_seconds",
            serde_json::json!(0.01),
        ),
        ("/report/selected", serde_json::json!(20)),
        (
            "/datasets/0/dataset/quality/final_distance_m",
            serde_json::json!(20.),
        ),
    ];
    for (pointer, value) in mutations {
        let mut bad = original.clone();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        bad.pointer_mut(parent).unwrap()[key] = value;
        std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(Validated::load(&path).is_err(), "{pointer}");
    }
    let mut bad = original.clone();
    let old = bad["datasets"][0]["split"].as_str().unwrap();
    let new = if old == "train" { "test" } else { "train" };
    let id = bad["datasets"][0]["dataset"]["quality"]["episode_id"].clone();
    bad["datasets"][0]["split"] = serde_json::json!(new);
    for entry in bad["report"]["entries"].as_array_mut().unwrap() {
        if entry["quality"]["episode_id"] == id {
            entry["split"] = serde_json::json!(new);
        }
    }
    std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
    assert_eq!(
        Validated::load(&path).err().unwrap().code,
        "learning_split_leakage"
    );
}
#[test]
fn learning_empty_training_partition_never_borrows_holdout_rows() {
    use crate::learning::Validated;
    let dir = tempfile::tempdir().unwrap();
    let path = learning_fixture(dir.path());
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let request = dataset::batch::Request {
        sources: value["report"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["split"] != "train")
            .map(|e| std::path::PathBuf::from(e["source"].as_str().unwrap()))
            .collect(),
        ..Default::default()
    };
    let out = dir.path().join("holdout-only.batch.json");
    request.analyze().unwrap().save_package(&out).unwrap();
    let data = Validated::load(&out).unwrap();
    assert!(!data.summary().fit_data_ready);
    assert_eq!(data.prepare().err().unwrap().code, "learning_no_train");
}
#[test]
fn learning_connection_reports_unavailable_executable_and_malformed_protocol() {
    use crate::learning::{Validated, diagnose_using};
    let dir = tempfile::tempdir().unwrap();
    let path = learning_fixture(dir.path());
    let data = Validated::load(&path).unwrap();
    assert_eq!(
        diagnose_using(
            &data,
            std::ffi::OsStr::new("/nonexistent/atom-learning-python")
        )
        .err()
        .unwrap()
        .code,
        "learning_python_unavailable"
    );
    #[cfg(unix)]
    assert_eq!(
        diagnose_using(&data, std::ffi::OsStr::new("/bin/echo"))
            .err()
            .unwrap()
            .code,
        "learning_worker_protocol"
    );
}

#[test]
#[cfg(unix)]
fn learning_diagnostic_deadline_kills_and_waits_for_worker() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = learning_fixture(dir.path());
    let data = crate::learning::Validated::load(&path).unwrap();
    let worker = dir.path().join("slow-interpreter");
    std::fs::write(&worker, b"#!/bin/sh\nexec /bin/sleep 10\n").unwrap();
    std::fs::set_permissions(&worker, std::fs::Permissions::from_mode(0o700)).unwrap();
    let start = std::time::Instant::now();
    let error = crate::learning::diagnose_using_budget(
        &data,
        worker.as_os_str(),
        std::time::Duration::from_millis(50),
    )
    .err()
    .unwrap();
    assert_eq!(error.code, "learning_worker_timeout");
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
}

#[test]
fn bc_job_rejects_invalid_budgets_and_uncommitted_checkpoints() {
    use crate::learning::training;
    let mut cfg = training::Config {
        max_steps: 0,
        ..Default::default()
    };
    assert!(cfg.validate().is_err());
    cfg.max_steps = 10001;
    assert!(cfg.validate().is_err());
    cfg.max_steps = 20;
    cfg.max_seconds = 301;
    assert!(cfg.validate().is_err());
    cfg.max_seconds = 1;
    cfg.learning_rate = f64::NAN;
    assert!(cfg.validate().is_err());
    let dir = tempfile::tempdir().unwrap();
    let path = learning_fixture(dir.path());
    let data = crate::learning::Validated::load(&path).unwrap();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let empty = dir.path().join("partial-job");
    std::fs::create_dir(&empty).unwrap();
    std::fs::write(empty.join("last.pt"), b"not a committed job").unwrap();
    assert!(training::evaluate(&data, &empty, &dir.path().join("evaluation"), &cancel).is_err());
    assert!(!dir.path().join("evaluation").exists());
}
#[test]
#[ignore = "Requires isolated PyTorch learning environment; real optimizer/checkpoint/holdout contract"]
fn bc_real_train_resume_and_test_isolation() {
    use crate::learning::{self, training};
    let dir = tempfile::tempdir().unwrap();
    let path = learning_fixture(dir.path());
    let data = learning::Validated::load(&path).unwrap();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let cfg = training::Config {
        max_steps: 20,
        max_seconds: 20,
        ..Default::default()
    };
    let first = training::train(&data, &cfg, &dir.path().join("first"), None, &cancel).unwrap();
    assert_eq!(first.result["weights_updated"], true);
    assert_eq!(first.result["checkpoint_reload_verified"], true);
    let cfg = training::Config {
        max_steps: 40,
        ..cfg
    };
    let resume = training::train(
        &data,
        &cfg,
        &dir.path().join("resume"),
        Some(&first.directory),
        &cancel,
    )
    .unwrap();
    let fresh = training::train(&data, &cfg, &dir.path().join("fresh"), None, &cancel).unwrap();
    assert_eq!(resume.result["test"], fresh.result["test"]);
    let evaluation =
        training::evaluate(&data, &resume.directory, &dir.path().join("test"), &cancel).unwrap();
    assert_eq!(evaluation.result["test"], resume.result["test"]);
    assert_eq!(evaluation.result["trained"], false);
    // Changing held-out action labels must not change learned or validation-selected weights.
    let package: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut request = dataset::batch::Request::default();
    for entry in package["report"]["entries"].as_array().unwrap() {
        let source = Path::new(entry["source"].as_str().unwrap());
        let mut episode = Episode::load(source).unwrap();
        if entry["split"] == "test" {
            for frame in episode.frames.iter_mut().skip(1) {
                for (a, c) in frame
                    .action
                    .as_mut()
                    .unwrap()
                    .iter_mut()
                    .zip(&episode.manifest.channels)
                {
                    *a = (*a + 0.02).min(f64::from(c.range[1]));
                }
            }
        }
        let changed = dir.path().join(format!(
            "changed-{}",
            source.file_name().unwrap().to_string_lossy()
        ));
        episode.save(&changed).unwrap();
        request.sources.push(changed);
    }
    let changed = dir.path().join("changed.batch.json");
    request.analyze().unwrap().save_package(&changed).unwrap();
    let changed = learning::Validated::load(&changed).unwrap();
    let other = training::train(&changed, &cfg, &dir.path().join("other"), None, &cancel).unwrap();
    assert_ne!(fresh.result["test"], other.result["test"]);
    let script = "import torch,sys; a=torch.load(sys.argv[1],weights_only=True)['model']; b=torch.load(sys.argv[2],weights_only=True)['model']; assert all(torch.equal(a[k],b[k]) for k in a)";
    for (a, b) in [
        (&resume.directory, &fresh.directory),
        (&fresh.directory, &other.directory),
    ] {
        assert!(
            std::process::Command::new(learning::python_executable())
                .args(["-I", "-c", script])
                .arg(a.join("best.pt"))
                .arg(b.join("best.pt"))
                .status()
                .unwrap()
                .success()
        );
    }
    let mut bad = cfg.clone();
    bad.learning_rate = 0.01;
    assert!(
        training::train(
            &data,
            &bad,
            &dir.path().join("bad-config"),
            Some(&first.directory),
            &cancel
        )
        .is_err()
    );
    assert!(!dir.path().join("bad-config").exists());
}
