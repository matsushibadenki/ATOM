//! Control commands attach to a running app; never lock/read its project file.
use atom_control::{
    Operation, Request,
    ipc::{Client, mcp_config},
};
use std::{io::Read, path::Path};
pub fn run(args: &[String]) -> Result<bool, Box<dyn std::error::Error>> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(false);
    };
    if command == "control-schema" {
        if args.len() != 1 {
            return Err("usage: atom-cli control-schema".into());
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&atom_control::request_schema())?
        );
        return Ok(true);
    }
    if !["control-state", "control-call", "doctor", "mcp-config"].contains(&command) {
        return Ok(false);
    }
    if args.len() < 2
        || args.len() > 3
        || (command == "control-call" && args.len() != 3)
        || (["doctor", "control-state"].contains(&command) && args.len() != 2)
    {
        return Err("invalid Control arguments; use --help".into());
    }
    let client = Client::new(&args[1]);
    match command {
        "doctor" => {
            let report = client.diagnose();
            println!("{}", serde_json::to_string_pretty(&report)?);
            if !report.connected {
                return Err("Control connection failed; see checks above".into());
            }
        }
        "mcp-config" => {
            let executable = match args.get(2) {
                Some(p) => p.into(),
                None => std::env::current_exe()?.with_file_name(if cfg!(windows) {
                    "atom-mcp.exe"
                } else {
                    "atom-mcp"
                }),
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&mcp_config(Path::new(&args[1]), &executable)?)?
            );
        }
        _ => {
            let request = if command == "control-state" {
                Request::new(Operation::State)
            } else {
                let mut bytes = vec![];
                std::fs::File::open(&args[2])?
                    .take(atom_control::MAX_REQUEST as u64 + 1)
                    .read_to_end(&mut bytes)?;
                if bytes.len() > atom_control::MAX_REQUEST {
                    return Err("Control request too large".into());
                }
                serde_json::from_slice::<Request>(&bytes)?
            };
            let response = client.request(&request)?;
            println!("{}", serde_json::to_string_pretty(&response)?);
            response.into_result()?;
        }
    }
    Ok(true)
}
