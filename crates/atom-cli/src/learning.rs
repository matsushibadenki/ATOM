use atom_core::learning::{self, Validated};
use std::{io::Read, path::Path};
pub fn run(args: &[String]) -> Result<bool, Box<dyn std::error::Error>> {
    let Some(cmd) = args.first().map(String::as_str) else {
        return Ok(false);
    };
    if cmd == "train-collect-config" && args.len() == 1 {
        println!(
            "{}",
            serde_json::to_string_pretty(&learning::collection::Config::default())?
        );
        return Ok(true);
    }
    if cmd == "train-collect" {
        if args.len() != 5 {
            return Err(
                "train-collect <episodes-dir> <source.batch.json filename> <project> <config.json>"
                    .into(),
            );
        }
        let mut bytes = vec![];
        std::fs::File::open(&args[4])?
            .take(4097)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 4096 {
            return Err("collection config size".into());
        }
        let config = serde_json::from_slice(&bytes)?;
        let project = atom_core::ProjectStore::open(&args[3])?
            .load()?
            .ok_or("project missing")?;
        let saved = learning::collection::run(
            Path::new(&args[1]),
            &args[2],
            &project,
            &config,
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        )?;
        println!("{}", serde_json::to_string_pretty(&saved)?);
        return Ok(true);
    }
    if cmd == "collection-inspect" {
        if args.len() != 3 {
            return Err("collection-inspect <episodes-dir> <filename>".into());
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&learning::collection::inspect(
                Path::new(&args[1]),
                &args[2]
            )?)?
        );
        return Ok(true);
    }
    if cmd == "policy-campaign" {
        if args.len() != 4 {
            return Err("policy-campaign <episodes-dir> <project> <config.json>".into());
        }
        let mut bytes = vec![];
        std::fs::File::open(&args[3])?
            .take(16385)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 16384 {
            return Err("campaign config size".into());
        }
        let config = serde_json::from_slice(&bytes)?;
        let project = atom_core::ProjectStore::open(&args[2])?
            .load()?
            .ok_or("project missing")?;
        let saved = learning::campaign::run(
            Path::new(&args[1]),
            &project,
            &config,
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        )?;
        println!("{}", serde_json::to_string_pretty(&saved)?);
        return Ok(true);
    }
    if cmd == "campaign-inspect" {
        if args.len() != 3 {
            return Err("campaign-inspect <episodes-dir> <filename>".into());
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&learning::campaign::inspect(
                Path::new(&args[1]),
                &args[2]
            )?)?
        );
        return Ok(true);
    }
    if cmd == "policy-diagnose" {
        if args.len() != 3 {
            return Err("policy-diagnose <episodes-dir> <evaluation-id>".into());
        }
        let root = Path::new(&args[1]);
        let report = learning::diagnostics::analyze(root, &args[2])?;
        println!(
            "{}",
            serde_json::to_string_pretty(&learning::diagnostics::save(root, report)?)?
        );
        return Ok(true);
    }
    if [
        "policy-list",
        "policy-register",
        "policy-link",
        "policy-compare",
    ]
    .contains(&cmd)
    {
        use learning::registry;
        let expected = if cmd == "policy-list" { 2 } else { 4 };
        if args.len() != expected {
            return Err("invalid policy arguments; use --help".into());
        }
        let root = Path::new(&args[1]);
        let result = match cmd {
            "policy-list" => serde_json::to_value(registry::listing(root)?)?,
            "policy-register" => {
                serde_json::to_value(registry::register(root, &args[2], &args[3])?)?
            }
            "policy-link" => serde_json::to_value(registry::attach(root, &args[2], &args[3])?)?,
            _ => serde_json::to_value(registry::compare(root, &args[2], &args[3])?)?,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(true);
    }
    if cmd == "learning-rollout" {
        if !(5..=6).contains(&args.len()) {
            return Err(
                "learning-rollout <batch> <job-dir> <project> <new-output-dir> [validation|test]"
                    .into(),
            );
        }
        let data = Validated::load(Path::new(&args[1]))?;
        let project = atom_core::ProjectStore::open(&args[3])?
            .load()?
            .ok_or("project does not exist")?;
        let partition = match args.get(5).map(String::as_str).unwrap_or("test") {
            "test" => learning::rollout::Partition::Test,
            "validation" => learning::rollout::Partition::Validation,
            _ => return Err("partition must be validation or test".into()),
        };
        let report = learning::rollout::compare_partition(
            &data,
            Path::new(&args[2]),
            &project,
            Path::new(&args[4]),
            &std::sync::atomic::AtomicBool::new(false),
            partition,
        )?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(true);
    }
    if cmd == "learning-job-config" && args.len() == 1 {
        println!(
            "{}",
            serde_json::to_string_pretty(&learning::training::Config::default())?
        );
        return Ok(true);
    }
    if cmd == "learning-train" || cmd == "learning-evaluate" {
        if (cmd == "learning-train" && !(3..=5).contains(&args.len()))
            || (cmd == "learning-evaluate" && args.len() != 4)
        {
            return Err("invalid learning job arguments; use --help".into());
        }
        let result = (|| -> Result<learning::training::Job, Box<dyn std::error::Error>> {
            let data = Validated::load(Path::new(&args[1]))?;
            let cancel = std::sync::atomic::AtomicBool::new(false);
            Ok(if cmd == "learning-train" {
                let cfg = if let Some(path) = args.get(3) {
                    let mut b = vec![];
                    std::fs::File::open(path)?.take(4097).read_to_end(&mut b)?;
                    if b.len() > 4096 {
                        return Err("job config size".into());
                    }
                    serde_json::from_slice(&b)?
                } else {
                    learning::training::Config::default()
                };
                learning::training::train(
                    &data,
                    &cfg,
                    Path::new(&args[2]),
                    args.get(4).map(Path::new),
                    &cancel,
                )?
            } else {
                learning::training::evaluate(
                    &data,
                    Path::new(&args[2]),
                    Path::new(&args[3]),
                    &cancel,
                )?
            })
        })();
        match result {
            Ok(job) => println!("{}", serde_json::to_string_pretty(&job)?),
            Err(e) => {
                println!("{}", serde_json::json!({"ok":false,"code":e.to_string()}));
                return Err(e);
            }
        }
        return Ok(true);
    }
    if !["learning-validate", "learning-prepare", "learning-diagnose"].contains(&cmd) {
        return Ok(false);
    }
    let required = if cmd == "learning-prepare" { 3 } else { 2 };
    if !(required..=required + 1).contains(&args.len()) {
        return Err("invalid learning arguments; use --help".into());
    }
    let result = (|| -> Result<serde_json::Value, learning::Error> {
        let data = Validated::load(Path::new(&args[1]))?;
        if args
            .get(required)
            .is_some_and(|expected| expected != &data.summary().source_sha256)
        {
            return Err(learning::Error {
                code: "learning_source_changed".into(),
            });
        }
        Ok(match cmd {
            "learning-prepare" => {
                serde_json::json!({"valid":true,"summary":data.summary(),"artifact_sha256":data.prepare()?.save(Path::new(&args[2]))?})
            }
            "learning-diagnose" => match learning::diagnose(&data) {
                Ok(diagnostic) => {
                    serde_json::json!({"valid":true,"summary":data.summary(),"diagnostic":diagnostic})
                }
                Err(error) => {
                    serde_json::json!({"valid":true,"summary":data.summary(),"diagnostic":null,"diagnostic_error":error.code})
                }
            },
            _ => serde_json::json!({"valid":true,"summary":data.summary()}),
        })
    })();
    match result {
        Ok(value) => {
            println!("{}", serde_json::to_string_pretty(&value)?);
            if cmd == "learning-diagnose" && value["diagnostic"]["fit_ready"] != true {
                return Err("learning adapter not ready; see diagnostic".into());
            }
        }
        Err(e) => {
            println!("{}", serde_json::json!({"valid":false,"code":e.code}));
            return Err(e.into());
        }
    }
    Ok(true)
}
