use super::*;
use std::{
    fs::{File, OpenOptions, Permissions},
    io::{Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
const DEADLINE: Duration = Duration::from_secs(3);
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    api_version: u32,
    instance_id: Uuid,
    pid: u32,
    socket: PathBuf,
    token: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    instance_id: Uuid,
    token: String,
    request: Request,
}
#[derive(Serialize, Deserialize)]
struct Reply {
    instance_id: Uuid,
    response: Response,
}
fn io_error(e: std::io::Error) -> ApiError {
    ApiError::new(match e.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => "timeout",
        std::io::ErrorKind::NotFound
        | std::io::ErrorKind::ConnectionRefused
        | std::io::ErrorKind::ConnectionReset
        | std::io::ErrorKind::BrokenPipe => "not_running",
        _ => "transport_error",
    })
}
fn read_frame(stream: &mut UnixStream, max: usize) -> Result<Vec<u8>, ApiError> {
    let deadline = Instant::now() + DEADLINE;
    let mut bytes = vec![];
    let mut buffer = [0u8; 4096];
    loop {
        stream
            .set_read_timeout(Some(
                deadline
                    .checked_duration_since(Instant::now())
                    .ok_or_else(|| ApiError::new("timeout"))?,
            ))
            .map_err(io_error)?;
        let n = stream.read(&mut buffer).map_err(io_error)?;
        if n == 0 {
            return Err(ApiError::new("not_running"));
        }
        if let Some(end) = buffer[..n].iter().position(|b| *b == b'\n') {
            bytes.extend_from_slice(&buffer[..end]);
            if bytes.len() > max {
                return Err(ApiError::new("message_too_large"));
            }
            return Ok(bytes);
        }
        bytes.extend_from_slice(&buffer[..n]);
        if bytes.len() > max {
            return Err(ApiError::new("message_too_large"));
        }
    }
}
fn write_frame(stream: &mut UnixStream, bytes: &[u8]) -> Result<(), ApiError> {
    let deadline = Instant::now() + DEADLINE;
    let mut data = bytes.to_vec();
    data.push(b'\n');
    let mut offset = 0;
    while offset < data.len() {
        stream
            .set_write_timeout(Some(
                deadline
                    .checked_duration_since(Instant::now())
                    .ok_or_else(|| ApiError::new("timeout"))?,
            ))
            .map_err(io_error)?;
        let n = stream.write(&data[offset..]).map_err(io_error)?;
        if n == 0 {
            return Err(ApiError::new("not_running"));
        }
        offset += n;
    }
    Ok(())
}
fn private_file(path: &Path, create: bool) -> Result<File, ApiError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(create)
        .create(create)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW);
    let file = options.open(path).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    // Local credentials are confined to the launching user's private files.
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(ApiError::new("insecure_endpoint"));
    }
    Ok(file)
}
fn load_descriptor(project: &Path) -> Result<Descriptor, ApiError> {
    let path = descriptor_path(project)?;
    let mut bytes = vec![];
    private_file(&path, false)?
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > 8192 {
        return Err(ApiError::new("invalid_endpoint"));
    }
    let d: Descriptor =
        serde_json::from_slice(&bytes).map_err(|_| ApiError::new("invalid_endpoint"))?;
    if d.api_version != API_VERSION {
        return Err(ApiError::new("unsupported_api_version"));
    }
    if d.token.len() != 64 {
        return Err(ApiError::new("invalid_endpoint"));
    }
    Ok(d)
}
fn socket_stream(d: &Descriptor) -> Result<UnixStream, ApiError> {
    let metadata = std::fs::symlink_metadata(&d.socket).map_err(io_error)?;
    if !metadata.file_type().is_socket()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(ApiError::new("insecure_endpoint"));
    }
    let socket = socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None)
        .map_err(io_error)?;
    socket
        .connect_timeout(
            &socket2::SockAddr::unix(&d.socket).map_err(io_error)?,
            DEADLINE,
        )
        .map_err(io_error)?;
    let fd: std::os::fd::OwnedFd = socket.into();
    Ok(UnixStream::from(fd))
}
fn exchange(d: &Descriptor, request: &Request) -> Result<Response, ApiError> {
    let encoded = serde_json::to_vec(&Envelope {
        instance_id: d.instance_id,
        token: d.token.clone(),
        request: request.clone(),
    })
    .map_err(|_| ApiError::new("invalid_arguments"))?;
    if encoded.len() > MAX_REQUEST {
        return Err(ApiError::new("request_too_large"));
    }
    let mut stream = socket_stream(d)?;
    write_frame(&mut stream, &encoded)?;
    let bytes = read_frame(&mut stream, MAX_RESPONSE + 256)?;
    let reply: Reply =
        serde_json::from_slice(&bytes).map_err(|_| ApiError::new("invalid_response"))?;
    if reply.instance_id != d.instance_id
        || reply.response.request_id != request.request_id
        || reply.response.api_version != API_VERSION
    {
        return Err(ApiError::new("invalid_response"));
    }
    Ok(reply.response)
}
impl Client {
    pub fn request(&self, request: &Request) -> Result<Response, ApiError> {
        exchange(&load_descriptor(&self.project)?, request)
    }
    pub fn diagnose(&self) -> Diagnostic {
        let mut result = Diagnostic {
            connected: false,
            checks: vec![],
            instance_id: None,
            state: None,
        };
        let mut check = |stage: &str, code: &str, ok: bool| {
            result.checks.push(Check {
                stage: stage.into(),
                code: code.into(),
                ok,
            })
        };
        let d = match load_descriptor(&self.project) {
            Ok(d) => {
                check("discovery", "ok", true);
                d
            }
            Err(e) => {
                check(
                    if e.code == "unsupported_api_version" {
                        "api_version"
                    } else {
                        "discovery"
                    },
                    &e.code,
                    false,
                );
                return result;
            }
        };
        result.instance_id = Some(d.instance_id);
        match exchange(&d, &Request::new(Operation::Diagnose)) {
            Ok(response) => {
                check("transport", "ok", true);
                match response.into_result() {
                    Ok(state) => {
                        check("authentication", "ok", true);
                        check("api_version", "ok", true);
                        check("shared_state", "ok", true);
                        result.state = Some(state);
                        result.connected = true;
                    }
                    Err(e) => {
                        if e.code == "unauthorized" {
                            check("authentication", &e.code, false);
                        } else {
                            check("authentication", "ok", true);
                            check("api_version", "ok", true);
                            check("shared_state", &e.code, false);
                        }
                    }
                }
            }
            Err(e) => check("transport", &e.code, false),
        }
        result
    }
}
pub struct Server {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    descriptor: PathBuf,
    instance_id: Uuid,
    _lock: File,
    _temp: tempfile::TempDir,
}
impl Server {
    pub fn start(
        project: &Path,
        handler: impl Fn(Request) -> Response + Send + Sync + 'static,
    ) -> Result<Self, ApiError> {
        let path = descriptor_path(project)?;
        let lock = private_file(&path.with_extension("lock"), true)?;
        lock.try_lock()
            .map_err(|_| ApiError::new("already_running"))?;
        if path
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(ApiError::new("insecure_endpoint"));
        }
        let temp = tempfile::Builder::new()
            .prefix("atom-control-")
            .tempdir_in("/tmp")
            .map_err(io_error)?;
        std::fs::set_permissions(temp.path(), Permissions::from_mode(0o700)).map_err(io_error)?;
        let socket = temp.path().join("control.sock");
        let listener = UnixListener::bind(&socket).map_err(io_error)?;
        std::fs::set_permissions(&socket, Permissions::from_mode(0o600)).map_err(io_error)?;
        listener.set_nonblocking(true).map_err(io_error)?;
        let d = Descriptor {
            api_version: API_VERSION,
            instance_id: Uuid::new_v4(),
            pid: std::process::id(),
            socket,
            token: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
        };
        let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(io_error)?;
        file.as_file()
            .set_permissions(Permissions::from_mode(0o600))
            .map_err(io_error)?;
        file.write_all(&serde_json::to_vec(&d).map_err(|_| ApiError::new("internal_error"))?)
            .map_err(io_error)?;
        file.as_file().sync_all().map_err(io_error)?;
        file.persist(&path).map_err(|e| io_error(e.error))?;
        let instance_id = d.instance_id;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let handler = Arc::new(handler);
        let active = Arc::new(AtomicUsize::new(0));
        let thread = std::thread::spawn(move || {
            let mut workers: Vec<JoinHandle<()>> = vec![];
            while !stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if active.fetch_add(1, Ordering::AcqRel) >= 8 {
                            active.fetch_sub(1, Ordering::AcqRel);
                            continue;
                        }
                        let active = active.clone();
                        let handler = handler.clone();
                        let token = d.token.clone();
                        workers.retain(|t| !t.is_finished());
                        workers.push(std::thread::spawn(move || {
                            let result = (|| {
                                let bytes = read_frame(&mut stream, MAX_REQUEST)?;
                                let raw: Value = serde_json::from_slice(&bytes)
                                    .map_err(|_| ApiError::new("invalid_request"))?;
                                let request_id = raw["request"]["request_id"]
                                    .as_str()
                                    .and_then(|s| Uuid::parse_str(s).ok())
                                    .unwrap_or(Uuid::nil());
                                let response = if raw["instance_id"] != instance_id.to_string()
                                    || raw["token"] != token
                                {
                                    Response::error(request_id, "unauthorized")
                                } else {
                                    match serde_json::from_value::<Envelope>(raw) {
                                        Ok(envelope) => handler(envelope.request),
                                        Err(_) => Response::error(request_id, "invalid_request"),
                                    }
                                };
                                let bytes = serde_json::to_vec(&Reply {
                                    instance_id,
                                    response,
                                })
                                .map_err(|_| ApiError::new("internal_error"))?;
                                if bytes.len() > MAX_RESPONSE + 256 {
                                    return Err(ApiError::new("response_too_large"));
                                }
                                write_frame(&mut stream, &bytes)
                            })();
                            let _: Result<(), ApiError> = result;
                            active.fetch_sub(1, Ordering::AcqRel);
                        }));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
            for worker in workers {
                let _ = worker.join();
            }
        });
        Ok(Self {
            stop,
            thread: Some(thread),
            descriptor: path,
            instance_id,
            _lock: lock,
            _temp: temp,
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if let Ok(mut file) = private_file(&self.descriptor, false) {
            let mut bytes = vec![];
            let _ = (&mut file).take(8193).read_to_end(&mut bytes);
            if serde_json::from_slice::<Descriptor>(&bytes)
                .is_ok_and(|d| d.instance_id == self.instance_id)
            {
                let _ = std::fs::remove_file(&self.descriptor);
            }
        }
    }
}
