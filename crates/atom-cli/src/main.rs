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
            "ATOM foundation\n  atom-cli import-usd <project.atom.json> <local.usd>\n  atom-cli import-hf <project.atom.json> <namespace/repo> <package> [revision]\n  atom-cli catalog <namespace/repo> [revision]\n  atom-cli init <project.atom.json> [name]\n  atom-cli template <project.atom.json> <humanoid|dual_arm>\n  atom-cli export-mjcf <project.atom.json> <output.xml>\n  atom-cli simulate <project.atom.json> [steps:1..1000]\n  atom-cli inspect <project.atom.json>\n  atom-cli validate <project.atom.json>\n\nvalidate checks task specifications; it does not run physics or training."
        );
        return Ok(());
    }
    if args.len() < 2 || args.len() > 5 {
        return Err("invalid arguments; use --help".into());
    }
    match args[0].as_str() {
        "catalog" if args.len() == 2 || args.len() == 3 => {
            println!(
                "{}",
                serde_json::to_string_pretty(&atom_core::assets::catalog(
                    &args[1],
                    args.get(2).map(String::as_str).unwrap_or("main")
                )?)?
            );
        }
        "import-usd" | "import-hf"
            if (args[0] == "import-usd" && args.len() == 3)
                || (args[0] == "import-hf" && (args.len() == 4 || args.len() == 5)) =>
        {
            let source = if args[0] == "import-usd" {
                atom_core::assets::ImportSource::Local {
                    path: args[2].clone(),
                }
            } else {
                atom_core::assets::ImportSource::HuggingFace {
                    repo: args[2].clone(),
                    package: args[3].clone(),
                    revision: args.get(4).cloned().unwrap_or_else(|| "main".into()),
                }
            };
            let mut store = ProjectStore::open(&args[1])?;
            let project = match store.load()? {
                Some(p) => p,
                None => {
                    let mut p = Project::new("USD Scene")?;
                    p.objects.clear();
                    p
                }
            };
            let mut core = AppCore::new(project)?;
            let s = core.snapshot();
            let command = atom_core::assets::prepare(
                source,
                &store
                    .path()
                    .parent()
                    .ok_or("project parent")?
                    .join(".asset-cache"),
            )?;
            core.apply(atom_core::EditRequest {
                expected_revision: s.revision,
                project_id: s.project_id,
                command,
            })?;
            store.save(core.project())?;
            core.mark_saved();
            println!("{}", serde_json::to_string_pretty(&core.snapshot())?);
        }
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
        "template" if args.len() == 3 => {
            let template = match args[2].as_str() {
                "humanoid" => atom_core::RobotTemplate::Humanoid,
                "dual_arm" => atom_core::RobotTemplate::DualArm,
                _ => return Err("unknown template".into()),
            };
            let mut store = ProjectStore::open(&args[1])?;
            if store.load()?.is_some() {
                return Err("project already exists".into());
            }
            let mut project = Project::new(&args[2])?;
            project.objects.clear();
            let mut core = AppCore::new(project)?;
            let s = core.snapshot();
            core.apply(atom_core::EditRequest {
                expected_revision: s.revision,
                project_id: s.project_id,
                command: atom_core::Command::AddRobot { template },
            })?;
            store.save(core.project())?;
            println!("{}", args[1]);
        }
        "export-mjcf" if args.len() == 3 => {
            use std::io::Write;
            let project = ProjectStore::open(&args[1])?
                .load()?
                .ok_or("project does not exist")?;
            let xml = atom_core::mujoco::export(&project)?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&args[2])?;
            file.write_all(xml.as_bytes())?;
        }
        "simulate" if args.len() == 2 || args.len() == 3 => {
            let project = ProjectStore::open(&args[1])?
                .load()?
                .ok_or("project does not exist")?;
            let steps = args
                .get(2)
                .map(|s| s.parse::<u32>())
                .transpose()?
                .unwrap_or(500);
            println!(
                "{}",
                serde_json::to_string_pretty(&atom_core::mujoco::run(&project, steps)?)?
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
