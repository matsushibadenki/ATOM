fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] {
        eprintln!(
            "atom-mcp --project /absolute/path/to/project.atom.json\nRead-only MCP stdio bridge to the running ATOM desktop."
        );
        return;
    }
    if args.len() != 2 || args[0] != "--project" {
        eprintln!("usage: atom-mcp --project /absolute/path/to/project.atom.json");
        std::process::exit(2);
    }
    if let Err(error) = atom_mcp::serve(
        std::io::stdin(),
        std::io::stdout(),
        atom_control::ipc::Client::new(&args[1]),
    ) {
        eprintln!("ATOM MCP: {error}");
        std::process::exit(1);
    }
}
