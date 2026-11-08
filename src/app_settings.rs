use crate::railway::HistoryScope;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const ALL_BRANCHES_VALUE: &str = "all";
const CURRENT_BRANCH_VALUE: &str = "current";

pub fn history_scope_path() -> PathBuf {
    gtk::glib::user_config_dir().join("metal-git").join("history-scope")
}

/// A missing or unreadable file means the default, showing all branches.
pub fn load_history_scope(path: &Path) -> HistoryScope {
    match fs::read_to_string(path).as_deref().map(str::trim) {
        Ok(CURRENT_BRANCH_VALUE) => HistoryScope::CurrentBranch,
        _ => HistoryScope::AllBranches,
    }
}

pub fn save_history_scope(path: &Path, scope: HistoryScope) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let value = match scope {
        HistoryScope::AllBranches => ALL_BRANCHES_VALUE,
        HistoryScope::CurrentBranch => CURRENT_BRANCH_VALUE,
    };
    fs::write(path, value)
}
