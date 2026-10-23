use super::*;
use std::path::Path;
impl AppCore {
    pub fn reach_command(&mut self, request: reach::Request, dir: &Path) -> Result<Snapshot> {
        self.check(request.expected_revision, request.project_id)?;
        if matches!(
            &request.command,
            reach::Command::Reset { .. } | reach::Command::Load { .. }
        ) && request.expected_sequence != self.sequence
        {
            return Err(Error::Conflict(self.revision));
        }
        if self.has_preview() && self.reach.is_none() {
            return Err(Error::EditBusy);
        }
        use reach::Command as C;
        match &request.command {
            C::Reset { .. } | C::Load { .. } => {}
            C::Step {
                id, expected_step, ..
            }
            | C::Run { id, expected_step } => {
                let p = self
                    .reach
                    .as_ref()
                    .filter(|p| p.episode().id == *id)
                    .ok_or(Error::EditBusy)?;
                if p.episode().frames.last().unwrap().index != *expected_step {
                    return Err(Error::Conflict(self.revision));
                }
            }
            C::Seek { id, .. }
            | C::Play { id }
            | C::Pause { id }
            | C::Close { id }
            | C::Save { id } => {
                if !self.reach.as_ref().is_some_and(|p| p.episode().id == *id) {
                    return Err(Error::EditBusy);
                }
            }
        }
        let physics_command = matches!(&request.command, C::Step { .. } | C::Run { .. });
        let result = (|| {
            match request.command {
                C::Reset { config } => {
                    let mut runner = reach::Runner::reset(&self.project, config)?;
                    runner.episode.manifest.source_revision = Some(self.revision);
                    self.reach = Some(reach::Preview::live(runner)?);
                }
                C::Load { name } => {
                    if !reach::valid_name(&name) {
                        return Err(Error::Invalid("Episode filename".into()));
                    }
                    self.reach = Some(reach::Preview::loaded(reach::Episode::load(
                        &dir.join(name),
                    )?)?);
                }
                C::Step { action, .. } => self.reach.as_mut().unwrap().step(1, action)?,
                C::Run { .. } => self.reach.as_mut().unwrap().step(500, None)?,
                C::Seek { index, .. } => {
                    let p = self.reach.as_mut().unwrap();
                    p.seek(index)?;
                    p.play_token = None;
                }
                C::Play { .. } => {
                    let p = self.reach.as_mut().unwrap();
                    if p.cursor + 1 >= p.episode().frames.len() {
                        p.seek(0)?;
                    }
                    if p.episode().frames.len() > 1 {
                        p.play_token = Some(self.sequence + 1);
                    }
                }
                C::Pause { .. } => self.reach.as_mut().unwrap().play_token = None,
                C::Close { .. } => self.reach = None,
                C::Save { .. } => self.reach.as_mut().unwrap().save(dir)?,
            }
            Ok(())
        })();
        // Even a failed physics batch may have valid earlier transitions to preserve.
        if result.is_err()
            && physics_command
            && let Some(p) = &mut self.reach
        {
            p.cursor = p.episode().frames.len() - 1;
            let _ = p.refresh();
        }
        self.sequence = self.sequence.saturating_add(1);
        result?;
        Ok(self.snapshot())
    }
    pub fn reach_play_token(&self) -> Option<(Uuid, u64)> {
        self.reach
            .as_ref()
            .and_then(|p| p.play_token.map(|t| (p.episode().id, t)))
    }
    pub fn reach_play_tick(&mut self, id: Uuid, token: u64) -> Result<Option<Snapshot>> {
        let Some(p) = self
            .reach
            .as_mut()
            .filter(|p| p.episode().id == id && p.play_token == Some(token))
        else {
            return Ok(None);
        };
        if p.cursor + 1 < p.episode().frames.len() {
            p.seek(p.cursor + 1)?;
        }
        if p.cursor + 1 >= p.episode().frames.len() {
            p.play_token = None;
        }
        self.sequence = self.sequence.saturating_add(1);
        Ok(Some(self.snapshot()))
    }
}
