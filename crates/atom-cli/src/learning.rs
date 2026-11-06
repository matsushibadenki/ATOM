use atom_core::learning::{self, Validated};
use std::{io::Read, path::Path};
pub fn run(args: &[String]) -> Result<bool, Box<dyn std::error::Error>> {
    let Some(cmd) = args.first().map(String::as_str) else {
        return Ok(false);
    };
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
