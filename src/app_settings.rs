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

/// Only all branches and the current branch are kept. A scope of one
/// reference or prefix leaves the file as it is, so the next launch starts
/// from the last of those two.
pub fn save_history_scope(path: &Path, scope: &HistoryScope) -> io::Result<()> {
    let value = match scope {
        HistoryScope::AllBranches => ALL_BRANCHES_VALUE,
        HistoryScope::CurrentBranch => CURRENT_BRANCH_VALUE,
        HistoryScope::Ref(_) | HistoryScope::Prefix(_) => return Ok(()),
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
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

const VISIBLE_VALUE: &str = "visible";
const HIDDEN_VALUE: &str = "hidden";

pub fn branch_tree_visible_path() -> PathBuf {
    gtk::glib::user_config_dir()
        .join("metal-git")
        .join("branch-tree-visible")
}

/// Whether the history window shows the branch tree. A missing or unreadable
/// file means the default, showing it.
pub fn load_branch_tree_visible(path: &Path) -> bool {
    !matches!(
        fs::read_to_string(path).as_deref().map(str::trim),
        Ok(HIDDEN_VALUE)
    )
}

pub fn save_branch_tree_visible(path: &Path, visible: bool) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, if visible { VISIBLE_VALUE } else { HIDDEN_VALUE })
}
