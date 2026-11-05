use atom_core::reach::dataset::batch;
use atom_core::reach::{Episode, dataset::ExportPolicy};
use std::{io::Read, path::Path};
pub fn run(args: &[String]) -> Result<bool, Box<dyn std::error::Error>> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(false);
    };
    if command == "episode-batch-config" && args.len() == 1 {
        println!(
            "{}",
            serde_json::to_string_pretty(&batch::Request::default())?
        );
        return Ok(true);
    }
    if command == "episode-batch" || command == "episode-batch-export" {
        if args.len() != if command == "episode-batch" { 2 } else { 3 } {
            return Err("invalid Episode batch arguments; use --help".into());
        }
        let mut bytes = vec![];
        std::fs::File::open(&args[1])?
            .take(65537)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 65536 {
            return Err("batch request exceeds 64 KiB".into());
        }
        let mut request: batch::Request = serde_json::from_slice(&bytes)?;
        let base = std::path::absolute(&args[1])?.parent().unwrap().to_owned();
        for path in &mut request.sources {
            if path.is_relative() {
                *path = base.join(&path);
            }
        }
        let report = request.analyze()?;
        if command == "episode-batch-export" {
            // Keep the complete audit visible even when nothing is eligible or publication fails.
            match report.save_package(Path::new(&args[2])) {
                Ok(receipt) => println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"report":report,"receipt":receipt})
                    )?
                ),
                Err(error) => {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                    return Err(error.into());
                }
            }
        } else {
            println!("{}", serde_json::to_string_pretty(&report)?);
            if report.selected() == 0 {
                return Err("batch contains no eligible Episodes".into());
            }
        }
        return Ok(true);
    }
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
