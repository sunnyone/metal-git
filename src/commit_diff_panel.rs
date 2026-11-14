use git2::{Error, Oid};
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::commit_diff_util;
use crate::commit_diff_util::{ListCommitDiffFileEntry, ListCommitDiffResult};
use crate::diff_model;
use crate::diff_view::{self, DiffViewer};
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
            self.diff_list_box.append(&file_row(entry));
        }

        self.current_list_result.replace(Some(Rc::new(result)));
        self.show_diff()?;
        self.diff_list_box.unselect_all();

        Ok(())
    }

    /// Choosing a file in the list scrolls the diff to it, rather than showing
    /// only that file, so that the whole commit stays in view.
    fn setup_list_box(self: &Rc<Self>) {
        let w = Rc::downgrade(self);
        self.diff_list_box.connect_selected_rows_changed(move |list_box| {
            if let (Some(panel), Some(row)) = (w.upgrade(), list_box.selected_row()) {
                panel.file_chosen(row.index());
            }
        });
        // Clicking the row that is selected already scrolls to it again
        let w = Rc::downgrade(self);
        self.diff_list_box.connect_row_activated(move |_, row| {
            if let Some(panel) = w.upgrade() {
                panel.file_chosen(row.index());
            }
        });
    }

    fn file_chosen(&self, row_index: i32) {
        // The list and the diff come from the same diff of the trees, so they
        // have the files in the same order
        if let Ok(file_index) = usize::try_from(row_index) {
            self.diff_viewer.scroll_to_file(file_index);
        }
    }

    /// Shows the diff of every file of the commit.
    fn show_diff(&self) -> Result<(), Error> {
        let Some(list_result) = self.current_list_result.borrow().clone() else {
            return Ok(());
        };

        let repo = self.repository_manager.open()?;
        let parent_tree = repo.find_commit(list_result.parent_oid)?.tree()?;
        let current_tree = repo.find_commit(list_result.current_oid)?.tree()?;
        let diff = diff_model::diff_trees(&repo, Some(&parent_tree), Some(&current_tree))?;
        self.diff_viewer.show_files(diff_model::files_from_diff(&diff)?);
        Ok(())
    }
}

/// A row of the list: the badge of the status of a file, as on its head line
/// in the diff, then its path.
fn file_row(entry: &ListCommitDiffFileEntry) -> gtk::Box {
    let badge = gtk::Label::builder()
        .use_markup(true)
        .label(diff_view::status_badge_markup(entry.status))
        .build();
    let path = gtk::Label::builder()
        .label(entry.format_file_move())
        .xalign(0.0)
        .build();
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    row.append(&badge);
    row.append(&path);
    row
}

fn clear_list_box(list_box: &gtk::ListBox) {
    while let Some(row) = list_box.row_at_index(0) {
        list_box.remove(&row);
    }
}
