use atom_runtime::{Config, Gateway, Profile};
pub fn run(args: &[String]) -> Result<bool, Box<dyn std::error::Error>> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(false);
    };
    if !["runtime-probe", "runtime-smoke"].contains(&command) {
        return Ok(false);
    }
    if (command == "runtime-probe" && args.len() != 3)
        || (command == "runtime-smoke" && !(4..=5).contains(&args.len()))
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
    if command == "runtime-probe" {
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
