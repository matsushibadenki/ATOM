use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::Duration,
};
pub(super) struct Worker {
    child: Child,
    input: ChildStdin,
    replies: Receiver<std::io::Result<Vec<u8>>>,
    _temp: tempfile::TempDir,
}
impl Worker {
    pub fn start(manifest: &Manifest) -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let model = temp.path().join("model.xml");
        let config = temp.path().join("config.json");
        std::fs::write(&model, mujoco::export(&manifest.project)?)?;
        std::fs::write(&config, serde_json::to_vec(manifest)?)?;
        let mut child = Command::new(
            std::env::var_os("ATOM_MUJOCO_PYTHON").unwrap_or_else(|| "python3".into()),
        )
        .args(["-I", "-c", include_str!("worker.py")])
        .arg(model)
        .arg(config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| Error::PhysicsUnavailable)?;
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut line = Vec::new();
                let result = (&mut reader)
                    .take(512 * 1024 + 1)
                    .read_until(b'\n', &mut line);
                let result = match result {
                    Ok(n) if n > 0 && n <= 512 * 1024 && line.last() == Some(&b'\n') => Ok(line),
                    _ => Err(std::io::Error::other("worker response")),
                };
                let end = result.is_err();
                if tx.send(result).is_err() || end {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            replies: rx,
            _temp: temp,
        })
    }
    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
    pub fn request(&mut self, request: serde_json::Value) -> Result<Observation> {
        let result = (|| {
            serde_json::to_writer(&mut self.input, &request)?;
            self.input.write_all(b"\n")?;
            self.input.flush()?;
            let bytes = self
                .replies
                .recv_timeout(Duration::from_secs(if request["op"] == "step" {
                    3
                } else {
                    15
                }))
                .map_err(|_| Error::PhysicsFailed)??;
            Ok(serde_json::from_slice(&bytes)?)
        })();
        if result.is_err() {
            let _ = self.child.kill();
            let status = self.child.wait();
            if status.is_ok_and(|s| s.code() == Some(42)) {
                return Err(Error::PhysicsUnavailable);
            }
        }
        result
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[cfg(test)]
impl Worker {
    pub fn running_for_test(&mut self) -> bool {
        self.child.try_wait().unwrap().is_none()
    }
    pub fn terminate_for_test(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
