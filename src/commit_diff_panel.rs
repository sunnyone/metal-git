use git2::{DiffOptions, Error, Oid};
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::commit_diff_util;
use crate::commit_diff_util::ListCommitDiffResult;
use crate::diff_model;
use crate::diff_view::DiffViewer;
use crate::repository_manager::RepositoryManager;

pub struct CommitDiffPanel {
    paned: gtk::Paned,
    diff_list_box: gtk::ListBox,
    diff_viewer: Rc<DiffViewer>,
    repository_manager: Rc<RepositoryManager>,
    current_list_result: RefCell<Option<Rc<ListCommitDiffResult>>>,
}

impl CommitDiffPanel {
    pub fn new(repository_manager: Rc<RepositoryManager>) -> Rc<CommitDiffPanel> {
        let paned = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .build();

        let diff_list_box = gtk::ListBox::new();
        diff_list_box.set_selection_mode(gtk::SelectionMode::Single);

        let list_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&diff_list_box)
            .build();
        paned.set_start_child(Some(&list_scrolled));

        let diff_viewer = DiffViewer::new();
        paned.set_end_child(Some(&diff_viewer.widget()));

        let commit_diff_panel = Rc::new(CommitDiffPanel {
            paned,
            diff_list_box,
            diff_viewer,
            repository_manager,
            current_list_result: RefCell::new(None),
        });

        commit_diff_panel.setup_list_box();
        commit_diff_panel
    }

    pub fn widget(&self) -> gtk::Widget {
        self.paned.clone().upcast::<gtk::Widget>()
    }

    pub fn update_commit(&self, oid: Oid) -> Result<(), Error> {
        let result =
            commit_diff_util::list_commit_diff_files(self.repository_manager.as_ref(), oid)?;

        clear_list_box(&self.diff_list_box);

        for entry in &result.files {
            let label = gtk::Label::builder()
                .label(entry.format_file_move())
                .xalign(0.0)
                .build();
            self.diff_list_box.append(&label);
        }

        self.current_list_result.replace(Some(Rc::new(result)));
        self.show_all_files_diff()?;
        self.diff_list_box.unselect_all();

        Ok(())
    }

    fn setup_list_box(self: &Rc<Self>) {
        let w = Rc::downgrade(self);
        self.diff_list_box.connect_selected_rows_changed(move |list_box| {
            let Some(panel) = w.upgrade() else {
                return;
            };

            let Some(row) = list_box.selected_row() else {
                return;
            };

            let index = row.index();
            if index >= 0 {
                panel
                    .file_selected(index as u32)
                    .expect("Failed to select a file");
            }
        });
    }

    fn file_selected(&self, file_index: u32) -> Result<(), Error> {
        let path = self
            .current_list_result
            .borrow()
            .as_ref()
            .and_then(|r| r.files[file_index as usize].new_file_path.clone());
        self.show_diff(path)
    }

    fn show_all_files_diff(&self) -> Result<(), Error> {
        self.show_diff(None)
    }

    /// Shows the diff of the commit, limited to one file when a path is given.
    fn show_diff(&self, path: Option<String>) -> Result<(), Error> {
        let Some(list_result) = self.current_list_result.borrow().clone() else {
            return Ok(());
        };

        let repo = self.repository_manager.open()?;
        let parent_tree = repo.find_commit(list_result.parent_oid)?.tree()?;
        let current_tree = repo.find_commit(list_result.current_oid)?.tree()?;

        let mut opts = DiffOptions::new();
        if let Some(path) = &path {
            opts.pathspec(path);
        }
        let diff =
            repo.diff_tree_to_tree(Some(&parent_tree), Some(&current_tree), Some(&mut opts))?;
        self.diff_viewer.show_files(&diff_model::files_from_diff(&diff)?);
        Ok(())
    }
}

fn clear_list_box(list_box: &gtk::ListBox) {
    while let Some(row) = list_box.row_at_index(0) {
        list_box.remove(&row);
    }
}
