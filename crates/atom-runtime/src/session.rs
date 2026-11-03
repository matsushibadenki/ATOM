//! Shared runtime/candidate state for every window; no authoritative scene mutations.
use super::*;
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub config: Config,
    pub revision: u64,
    pub busy: bool,
    pub last_result: Option<&'static str>,
    pub capabilities: Option<Capabilities>,
    pub completion: Option<Completion>,
    pub error: Option<Error>,
    pub plan: Option<planning::Report>,
}
pub struct Session {
    snapshot: Snapshot,
    next_job: u64,
    active_job: Option<u64>,
}
impl Session {
    pub fn new(config: Config) -> Result<Self> {
        Ok(Self {
            snapshot: Snapshot {
                config: config.normalized()?,
                revision: 0,
                busy: false,
                last_result: None,
                capabilities: None,
                completion: None,
                error: None,
                plan: None,
            },
            next_job: 0,
            active_job: None,
        })
    }
    pub fn settings_warning(&mut self) {
        self.snapshot.error = Some(Error::new("runtime_invalid_config"));
    }
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.clone()
    }
    fn check(&self, revision: u64) -> Result<()> {
        if revision != self.snapshot.revision {
            return Err(Error::new("runtime_conflict"));
        }
        if self.snapshot.busy {
            return Err(Error::new("runtime_busy"));
        }
        Ok(())
    }
    pub fn configure(&mut self, revision: u64, config: Config) -> Result<()> {
        self.check(revision)?;
        self.snapshot.config = config.normalized()?;
        self.snapshot.last_result = None;
        self.snapshot.plan = None;
        self.snapshot.capabilities = None;
        self.snapshot.completion = None;
        self.snapshot.error = None;
        self.snapshot.revision += 1;
        Ok(())
    }
    pub fn begin(&mut self, revision: u64) -> Result<(u64, Config)> {
        self.check(revision)?;
        self.next_job += 1;
        let id = self.next_job;
        self.active_job = Some(id);
        self.snapshot.busy = true;
        self.snapshot.last_result = None;
        self.snapshot.error = None;
        self.snapshot.completion = None;
        // An older successful probe must not look like the result of this operation.
        self.snapshot.capabilities = None;
        self.snapshot.revision += 1;
        Ok((id, self.snapshot.config.clone()))
    }
    pub fn begin_planning(&mut self, revision: u64) -> Result<(u64, Config)> {
        let caps = self.snapshot.capabilities.clone();
        let result = self.begin(revision)?;
        self.snapshot.capabilities = caps;
        self.snapshot.plan = None;
        Ok(result)
    }
    pub fn finish_plan(&mut self, job: u64, result: Result<planning::Report>) -> bool {
        self.finish(job, |s| {
            s.last_result = Some("plan");
            match result {
                Ok(p) => s.plan = Some(p),
                Err(e) => s.error = Some(e),
            }
        })
    }
    pub fn finish_probe(&mut self, job: u64, result: Result<Capabilities>) -> bool {
        self.finish(job, |s| {
            s.last_result = Some("probe");
            match result {
                Ok(c) => s.capabilities = Some(c),
                Err(e) => s.error = Some(e),
            }
        })
    }
    pub fn finish_smoke(&mut self, job: u64, result: Result<Completion>) -> bool {
        self.finish(job, |s| {
            s.last_result = Some("smoke");
            match result {
                Ok(c) => s.completion = Some(c),
                Err(e) => s.error = Some(e),
            }
        })
    }
    fn finish(&mut self, job: u64, update: impl FnOnce(&mut Snapshot)) -> bool {
        if self.active_job != Some(job) {
            return false;
        }
        update(&mut self.snapshot);
        self.snapshot.busy = false;
        self.snapshot.revision += 1;
        self.active_job = None;
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_session_rejects_competing_windows_and_late_replies() {
        let c = Config {
            profile: Profile::VllmApplePreview,
            origin: "http://127.0.0.1:8000".into(),
        };
        let mut s = Session::new(c.clone()).unwrap();
        let (job, _) = s.begin(0).unwrap();
        assert!(s.begin(0).is_err());
        assert!(s.begin(1).is_err());
        assert!(s.configure(1, c.clone()).is_err());
        assert!(!s.finish_probe(job + 1, Err(Error::new("runtime_unavailable"))));
        assert!(s.snapshot().busy);
        assert!(s.finish_probe(job, Err(Error::new("runtime_unavailable"))));
        assert!(!s.finish_probe(job, Err(Error::new("runtime_unavailable"))));
        assert_eq!(s.snapshot().revision, 2);
        s.configure(2, c).unwrap();
        assert!(s.snapshot().error.is_none());
        assert_eq!(s.snapshot().revision, 3);
    }
}
