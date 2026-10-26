use atom_core::reach::{Episode, dataset::ExportPolicy};
use std::path::Path;
pub fn run(args: &[String]) -> Result<bool, Box<dyn std::error::Error>> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(false);
    };
    if !["episode-quality", "episode-export"].contains(&command) {
        return Ok(false);
    }
    if (command == "episode-quality" && args.len() != 2)
        || (command == "episode-export" && !(3..=4).contains(&args.len()))
    {
        return Err("invalid Episode dataset arguments; use --help".into());
    }
    let policy = match args.get(3).map(String::as_str).unwrap_or("success_only") {
        "success_only" => ExportPolicy::SuccessOnly,
        "completed" => ExportPolicy::Completed,
        _ => return Err("export policy must be success_only or completed".into()),
    };
    let episode = Episode::load(Path::new(&args[1]))?;
    if command == "episode-quality" {
        println!("{}", serde_json::to_string_pretty(&episode.quality()?)?);
    } else {
        let dataset = episode.dataset(policy)?;
        dataset.save(Path::new(&args[2]))?;
        println!("{}", serde_json::to_string_pretty(&dataset.quality)?);
    }
    Ok(true)
}
