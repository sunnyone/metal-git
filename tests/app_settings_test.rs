extern crate metal_git;
extern crate tempdir;

use metal_git::app_settings::{
    load_branch_tree_visible, load_diff_layout, load_hide_whitespace, load_history_scope,
    save_branch_tree_visible, save_diff_layout, save_hide_whitespace, save_history_scope,
    DiffLayout,
};
use metal_git::railway::HistoryScope;
use std::fs;
use tempdir::TempDir;

#[test]
pub fn missing_file_shows_all_branches() {
    let temp_dir = TempDir::new("app-settings").unwrap();

    assert_eq!(HistoryScope::AllBranches, load_history_scope(&temp_dir.path().join("missing")));
}

#[test]
pub fn unknown_value_shows_all_branches() {
    let temp_dir = TempDir::new("app-settings").unwrap();
    let path = temp_dir.path().join("history-scope");
    fs::write(&path, "something else").unwrap();

    assert_eq!(HistoryScope::AllBranches, load_history_scope(&path));
}

#[test]
pub fn saved_scope_is_loaded_back_and_creates_directories() {
    let temp_dir = TempDir::new("app-settings").unwrap();
    let path = temp_dir.path().join("metal-git").join("history-scope");

    save_history_scope(&path, &HistoryScope::CurrentBranch).unwrap();
    assert_eq!(HistoryScope::CurrentBranch, load_history_scope(&path));

    save_history_scope(&path, &HistoryScope::AllBranches).unwrap();
    assert_eq!(HistoryScope::AllBranches, load_history_scope(&path));
}

#[test]
pub fn scope_of_one_reference_keeps_the_saved_scope() {
    let temp_dir = TempDir::new("app-settings").unwrap();
    let path = temp_dir.path().join("metal-git").join("history-scope");

    save_history_scope(&path, &HistoryScope::CurrentBranch).unwrap();
    save_history_scope(&path, &HistoryScope::Ref("refs/heads/feature".to_owned())).unwrap();
    save_history_scope(&path, &HistoryScope::Prefix("refs/tags/".to_owned())).unwrap();
    assert_eq!(HistoryScope::CurrentBranch, load_history_scope(&path));
}

#[test]
pub fn scope_of_one_reference_writes_nothing_when_nothing_was_saved() {
    let temp_dir = TempDir::new("app-settings").unwrap();
    let path = temp_dir.path().join("metal-git").join("history-scope");

    save_history_scope(&path, &HistoryScope::Ref("refs/heads/feature".to_owned())).unwrap();
    assert!(!path.exists());
}

#[test]
pub fn branch_tree_is_visible_unless_saved_as_hidden() {
    let temp_dir = TempDir::new("app-settings").unwrap();
    let path = temp_dir.path().join("metal-git").join("branch-tree-visible");

    assert!(load_branch_tree_visible(&path));
    save_branch_tree_visible(&path, false).unwrap();
    assert!(!load_branch_tree_visible(&path));
    save_branch_tree_visible(&path, true).unwrap();
    assert!(load_branch_tree_visible(&path));
}

#[test]
pub fn missing_file_shows_the_unified_diff() {
    let temp_dir = TempDir::new("app-settings").unwrap();

    assert_eq!(DiffLayout::Unified, load_diff_layout(&temp_dir.path().join("missing")));
}

#[test]
pub fn saved_diff_layout_is_loaded_back() {
    let temp_dir = TempDir::new("app-settings").unwrap();
    let path = temp_dir.path().join("metal-git").join("diff-layout");

    save_diff_layout(&path, DiffLayout::Split).unwrap();
    assert_eq!(DiffLayout::Split, load_diff_layout(&path));

    save_diff_layout(&path, DiffLayout::Unified).unwrap();
    assert_eq!(DiffLayout::Unified, load_diff_layout(&path));
}

#[test]
pub fn whitespace_is_shown_unless_saved_as_hidden() {
    let temp_dir = TempDir::new("app-settings").unwrap();
    let path = temp_dir.path().join("metal-git").join("diff-whitespace");

    assert!(!load_hide_whitespace(&path));
    save_hide_whitespace(&path, true).unwrap();
    assert!(load_hide_whitespace(&path));
    save_hide_whitespace(&path, false).unwrap();
    assert!(!load_hide_whitespace(&path));
}
