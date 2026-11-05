use atom_core::learning::{self, Validated};
use std::path::Path;
pub fn run(args: &[String]) -> Result<bool, Box<dyn std::error::Error>> {
    let Some(cmd) = args.first().map(String::as_str) else {
        return Ok(false);
    };
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
