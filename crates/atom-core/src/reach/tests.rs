use super::*;
fn source() -> Project {
    let mut p = Project::new("Reach tests").unwrap();
    let (r, objects, _) = robots::build(RobotTemplate::DualArm, 1).unwrap();
    p.objects = objects;
    p.robots = vec![r];
    p
}
fn fixture() -> Episode {
    let p = source();
    let m = prepare(
        &p,
        Config {
            seed: 7,
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
