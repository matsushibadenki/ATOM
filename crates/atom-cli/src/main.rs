use atom_core::{AppCore, Project, ProjectStore};
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--help"] {
        println!(
            "ATOM foundation\n  atom-cli init <project.atom.json> [name]\n  atom-cli inspect <project.atom.json>\n  atom-cli validate <project.atom.json>\n\nvalidate checks task specifications; it does not run physics or training."
        );
        return Ok(());
    }
    if args.len() < 2 || args.len() > 4 {
        return Err("invalid arguments; use --help".into());
    }
    match args[0].as_str() {
        "init" if args.len() <= 3 => {
            let mut store = ProjectStore::open(&args[1])?;
            if store.load()?.is_some() {
                return Err("project already exists".into());
            }
            let project = Project::new(args.get(2).map(String::as_str).unwrap_or("ATOM Lab"))?;
            store.save(&project)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&AppCore::new(project)?.snapshot())?
            );
        }
        "inspect" | "validate" if args.len() == 2 => {
            let project = ProjectStore::open(&args[1])?
                .load()?
                .ok_or("project does not exist")?;
            if args[0] == "inspect" {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&AppCore::new(project)?.snapshot())?
                );
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&futures::executor::block_on(
                        atom_core::workflow::validate_tasks(&project.workflow)
                    )?)?
                );
            }
        }
        _ => return Err("invalid arguments; use --help".into()),
    }
    Ok(())
}
