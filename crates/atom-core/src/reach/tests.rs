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
