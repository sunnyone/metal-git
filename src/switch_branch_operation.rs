use crate::background_task::CancelToken;
use crate::branch::{create_tracking_branch_and_switch, switch_local_branch, BranchCandidate, BranchKind};
use crate::long_running_operation::{describe_common, LongRunningEvent, LongRunningOperation, LongRunningStatus};
use git2::{Error, Repository};
use std::path::{Path, PathBuf};

pub struct SwitchBranchOperation {
    pub git_dir: PathBuf,
    pub candidate: BranchCandidate,
}

impl LongRunningOperation for SwitchBranchOperation {
    const NAME: &'static str = "Switch branch";
    type Progress = String;
    type Output = String;
    type Error = String;

    fn run(self, _cancel: &CancelToken, report: &dyn Fn(String)) -> Result<String, String> {
        report(self.candidate.name.clone());
        switch_branch(&self.git_dir, &self.candidate).map_err(|err| err.message().to_string())
    }

    fn describe(event: &LongRunningEvent<String, String, String>) -> LongRunningStatus {
        match event {
            // Stopping a checkout halfway would leave a partly updated work tree
            LongRunningEvent::Started => LongRunningStatus::Started {
                message: "Switching branch...".to_string(),
                cancellable: false,
            },
            LongRunningEvent::Progress(name) => LongRunningStatus::Progress {
                message: format!("Switching to {}...", name),
                fraction: None,
            },
            LongRunningEvent::Succeeded(branch_name) => LongRunningStatus::Succeeded {
                message: format!("Switched to {}", branch_name),
                reload_history: true,
            },
            other => describe_common(Self::NAME, other),
        }
    }
}

/// Returns the name of the branch that HEAD points to afterwards.
fn switch_branch(git_dir: &Path, candidate: &BranchCandidate) -> Result<String, Error> {
    let repo = Repository::open(git_dir)?;
    match candidate.kind {
        BranchKind::Local => switch_local_branch(&repo, &candidate.name)?,
        BranchKind::Remote => create_tracking_branch_and_switch(&repo, &candidate.name)?,
    }
    let head = repo.head()?;
    let branch_name = head
        .shorthand()
        .map_err(|_| Error::from_str("Branch name is not valid UTF-8"))?;
    Ok(branch_name.to_string())
}
