use crate::railway::HistoryScope;
use std::collections::BTreeSet;
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

/// How the diff view lays out the old and the new version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLayout {
    Unified,
    Split,
}

const UNIFIED_VALUE: &str = "unified";
const SPLIT_VALUE: &str = "split";

pub fn diff_layout_path() -> PathBuf {
    gtk::glib::user_config_dir().join("metal-git").join("diff-layout")
}

/// A missing or unreadable file means the default, the unified layout.
pub fn load_diff_layout(path: &Path) -> DiffLayout {
    match fs::read_to_string(path).as_deref().map(str::trim) {
        Ok(SPLIT_VALUE) => DiffLayout::Split,
        _ => DiffLayout::Unified,
    }
}

pub fn save_diff_layout(path: &Path, layout: DiffLayout) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let value = match layout {
        DiffLayout::Unified => UNIFIED_VALUE,
        DiffLayout::Split => SPLIT_VALUE,
    };
    fs::write(path, value)
}

/// Where the changes marked as viewed are kept, by `FileDiff::change_key`.
pub fn viewed_changes_path() -> PathBuf {
    gtk::glib::user_config_dir()
        .join("metal-git")
        .join("viewed-changes")
}

/// A missing or unreadable file means nothing has been viewed yet.
pub fn load_viewed_changes(path: &Path) -> BTreeSet<String> {
    fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Marks a change as viewed or not. The file is read again first, so that the
/// marks another window made in the meantime are kept.
pub fn save_change_viewed(path: &Path, key: &str, viewed: bool) -> io::Result<()> {
    let mut keys = load_viewed_changes(path);
    let changed = if viewed {
        keys.insert(key.to_string())
    } else {
        keys.remove(key)
    };
    if !changed {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text: String = keys.iter().map(|key| format!("{}\n", key)).collect();
    fs::write(path, text)
}

const HIDE_VALUE: &str = "hide";
const SHOW_VALUE: &str = "show";

pub fn whitespace_path() -> PathBuf {
    gtk::glib::user_config_dir()
        .join("metal-git")
        .join("diff-whitespace")
}

/// Whether diffs leave out changes of whitespace. A missing or unreadable
/// file means the default, showing them.
pub fn load_hide_whitespace(path: &Path) -> bool {
    matches!(
        fs::read_to_string(path).as_deref().map(str::trim),
        Ok(HIDE_VALUE)
    )
}

pub fn save_hide_whitespace(path: &Path, hide: bool) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, if hide { HIDE_VALUE } else { SHOW_VALUE })
}
