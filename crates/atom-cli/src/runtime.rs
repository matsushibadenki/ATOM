use atom_runtime::{Config, Gateway, Profile, evaluation};
use std::io::Read;
pub fn run(args: &[String]) -> Result<bool, Box<dyn std::error::Error>> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(false);
    };
    if command == "plan-eval-suite" && args.len() == 1 {
        println!("{}", serde_json::to_string_pretty(&evaluation::manifest())?);
        return Ok(true);
    }
    if command == "plan-evaluate" && args.len() == 2 {
        let mut bytes = vec![];
        std::fs::File::open(&args[1])?
            .take(evaluation::MAX_BUNDLE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > evaluation::MAX_BUNDLE_BYTES {
            return Err("evaluation file exceeds 128 KiB".into());
        }
        let bundle: evaluation::Bundle = serde_json::from_slice(&bytes)?;
        let report = evaluation::evaluate(&bundle)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        if report.passed != report.total {
            return Err("evaluation has failed or missing cases".into());
        }
        return Ok(true);
    }
    if !["runtime-probe", "runtime-smoke", "runtime-plan-evaluate"].contains(&command) {
        return Ok(false);
    }
    if (command == "runtime-probe" && args.len() != 3)
        || (command == "runtime-smoke" && !(4..=5).contains(&args.len()))
        || (command == "runtime-plan-evaluate" && args.len() != 4)
    {
        return Err("invalid runtime arguments; use --help".into());
    }
    let profile = match args[1].as_str() {
        "vllm_apple_preview" => Profile::VllmApplePreview,
        "vllm_apple_daemon" => Profile::VllmAppleDaemon,
        "strata" => Profile::Strata,
        _ => return Err("unknown runtime profile".into()),
    };
    let g = Gateway::new(
        Config {
            profile,
            origin: args[2].clone(),
        },
        std::env::var("ATOM_RUNTIME_TOKEN").ok(),
    )?;
    if command == "runtime-plan-evaluate" {
        let (bundle, evidence) = evaluation::generate(&g, &args[3]);
        let report = evaluation::evaluate(&bundle)?;
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"bundle":bundle,"evaluation":report,"completions":evidence})
            )?
        );
        if report.passed != report.total {
            return Err("evaluation has failed cases".into());
        }
    } else if command == "runtime-probe" {
        let c = g.probe();
        println!("{}", serde_json::to_string_pretty(&c)?);
        if !c.connected {
            return Err("runtime probe failed; see checks above".into());
        }
    } else {
        let limit = args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(32);
        println!(
            "{}",
            serde_json::to_string_pretty(&g.smoke(&args[3], limit)?)?
        );
    }
    Ok(true)
}
