#![cfg(unix)]
use atom_control::ipc::{Client, Server, descriptor_path};
use atom_control::*;
use atom_core::{AppCore, Command, EditRequest, Project};
use std::{
    io::{Read, Write},
    os::unix::{fs::PermissionsExt, net::UnixStream},
    sync::{Arc, Mutex},
};
fn host(path: &std::path::Path, core: Arc<Mutex<AppCore>>) -> Server {
    let service = Mutex::new(Service::default());
    Server::start(path, move |request| {
        service
            .lock()
            .unwrap()
            .handle(&core.lock().unwrap(), request)
    })
    .unwrap()
}
#[test]
fn live_ipc_reconnect_cleanup_permissions_and_no_second_document() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gui.atom.json");
    let core = Arc::new(Mutex::new(
        AppCore::new(Project::new("Live GUI").unwrap()).unwrap(),
    ));
    let server = host(&path, core.clone());
    let descriptor = descriptor_path(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&descriptor).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let client = Client::new(&path);
    assert!(client.diagnose().connected);
    {
        let mut c = core.lock().unwrap();
        let s = c.snapshot();
        c.apply(EditRequest {
            project_id: s.project_id,
            expected_revision: s.revision,
            command: Command::Rename {
                name: "Unsaved inside GUI".into(),
            },
        })
        .unwrap();
    }
    assert_eq!(
        client
            .call(Operation::State)
            .unwrap()
            .into_result()
            .unwrap()["name"],
        "Unsaved inside GUI"
    );
    assert!(!path.exists()); // IPC did not open/create a project file.
    assert!(Server::start(&path, |r| Response::error(r.request_id, "unexpected")).is_err());
    drop(server);
    assert!(!descriptor.exists());
    assert!(!client.diagnose().connected);
    let _server = host(&path, core);
    assert_eq!(
        client
            .call(Operation::State)
            .unwrap()
            .into_result()
            .unwrap()["name"],
        "Unsaved inside GUI"
    );
}
#[test]
fn credential_mismatch_and_unsafe_descriptor_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gui.atom.json");
    let core = Arc::new(Mutex::new(
        AppCore::new(Project::new("Auth").unwrap()).unwrap(),
    ));
    let _server = host(&path, core);
    let location = descriptor_path(&path).unwrap();
    let bytes = std::fs::read(&location).unwrap();
    let mut descriptor: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    descriptor["token"] = serde_json::json!("0".repeat(64));
    std::fs::write(&location, serde_json::to_vec(&descriptor).unwrap()).unwrap();
    assert_eq!(
        Client::new(&path)
            .call(Operation::State)
            .unwrap()
            .into_result()
            .unwrap_err()
            .code,
        "unauthorized"
    );
    std::fs::write(&location, bytes).unwrap();
    std::fs::set_permissions(&location, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        Client::new(&path).call(Operation::State).unwrap_err().code,
        "insecure_endpoint"
    );
    std::fs::set_permissions(&location, std::fs::Permissions::from_mode(0o600)).unwrap();
}
#[test]
fn oversized_and_unterminated_peers_do_not_block_other_clients() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gui.atom.json");
    let core = Arc::new(Mutex::new(
        AppCore::new(Project::new("Bounds").unwrap()).unwrap(),
    ));
    let server = host(&path, core);
    let descriptor: serde_json::Value =
        serde_json::from_slice(&std::fs::read(descriptor_path(&path).unwrap()).unwrap()).unwrap();
    let socket = descriptor["socket"].as_str().unwrap();
    let _stalled = UnixStream::connect(socket).unwrap();
    let mut huge = UnixStream::connect(socket).unwrap();
    // Configure before sending: macOS may reject socket options after the server
    // has already closed this oversized peer.
    huge.set_read_timeout(Some(std::time::Duration::from_secs(4)))
        .unwrap();
    let _ = huge.write_all(&vec![b'x'; MAX_REQUEST + 1]);
    let mut answer = vec![];
    match huge.read_to_end(&mut answer) {
        Ok(_) => {}
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
            ) => {}
        Err(e) => panic!("oversized peer was not promptly closed: {e}"),
    }
    assert!(answer.is_empty());
    assert!(Client::new(&path).diagnose().connected);
    drop(server);
}
