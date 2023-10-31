use git2::{DiffOptions, Error, Oid};
use glib::{Cast, StaticType};
use gtk::prelude::GtkListStoreExt;
use gtk::prelude::GtkListStoreExtManual;
use gtk::prelude::TreeViewColumnExt;
use gtk::traits::{
    ContainerExt, PanedExt, TextViewExt, TreeModelExt, TreeSelectionExt, TreeViewExt,
};
use gtk::Orientation;
use std::cell::RefCell;
use std::rc::Rc;

use crate::commit_diff_util;
use crate::commit_diff_util::ListCommitDiffResult;
use crate::diff_text_view_util;
use crate::diff_text_view_util::create_diff_text_buffer;
use crate::repository_manager::RepositoryManager;

pub struct CommitDiffPanel {
    paned: gtk::Paned,

    diff_list_store: gtk::ListStore,
    diff_tree_view: gtk::TreeView,

    commit_text_view: gtk::TextView,

    repository_manager: Rc<RepositoryManager>,
    current_list_result: RefCell<Option<Rc<ListCommitDiffResult>>>,
}

const COLUMN_FILENAME: u32 = 0;
const COLUMN_INDEX: u32 = 1;

impl CommitDiffPanel {
    pub fn new(repository_manager: Rc<RepositoryManager>) -> Rc<CommitDiffPanel> {
        let paned = gtk::Paned::new(Orientation::Horizontal);

        let diff_list_store = gtk::ListStore::new(&[
            String::static_type(), // COLUMN_FILENAME
            u32::static_type(),    // COLUMN_INDEX
        ]);

        let diff_tree_view = gtk::TreeView::new();
        diff_tree_view.set_model(Some(&diff_list_store));

        let renderer = gtk::CellRendererText::new();
        let col = gtk::TreeViewColumn::new();
        col.set_title("Filename");
        col.pack_start(&renderer, false);
        col.add_attribute(&renderer, "text", COLUMN_FILENAME as i32);
        diff_tree_view.append_column(&col);

        let scrolled = gtk::ScrolledWindow::builder().build();
        scrolled.add(&diff_tree_view);
        paned.pack1(&scrolled, true, false);

        let scrolled = gtk::ScrolledWindow::builder().build();

        let diff_text_buffer = create_diff_text_buffer();
        let commit_text_view = gtk::TextView::builder()
            .editable(false)
            .buffer(&diff_text_buffer)
            .monospace(true)
            .build();
        scrolled.add(&commit_text_view);

        paned.pack2(&scrolled, true, false);

        let commit_diff_panel = Rc::new(CommitDiffPanel {
            paned,
            diff_list_store,
            diff_tree_view,
            commit_text_view,
            repository_manager,
            current_list_result: RefCell::new(None),
        });

        commit_diff_panel.setup_tree_view();

        commit_diff_panel
    }

    pub fn container(&self) -> gtk::Container {
        self.paned.clone().upcast::<gtk::Container>()
    }

    pub fn update_commit(&self, oid: Oid) -> Result<(), Error> {
        let result =
            commit_diff_util::list_commit_diff_files(self.repository_manager.as_ref(), oid)?;

        self.diff_list_store.clear();

        for (i, x) in result.files.iter().enumerate() {
            let index: u32 = i as u32;
            self.diff_list_store.insert_with_values(
                None,
                &[
                    (COLUMN_FILENAME, &x.format_file_move()),
                    (COLUMN_INDEX, &index),
                ],
            );
        }

        self.current_list_result.replace(Some(Rc::new(result)));

        Ok(())
    }

    fn setup_tree_view(self: &Rc<Self>) {
        let selection = self.diff_tree_view.selection();
        let w = Rc::downgrade(self);
        selection.connect_changed(move |x| {
            if let Some((model, iter)) = x.selected() {
                let index = model
                    .value(&iter, COLUMN_INDEX as i32)
                    .get::<u32>()
                    .expect("Incorrect column type");
                w.upgrade()
                    .unwrap()
                    .file_selected(index)
                    .expect("Failed to select a file");
            }
        });
    }

    fn file_selected(self: &Rc<Self>, file_index: u32) -> Result<(), Error> {
        if let Some(list_result) = self.current_list_result.borrow().as_ref() {
            let repo = self.repository_manager.open()?;

            let entry = &list_result.files[file_index as usize];

            let current_commit = repo.find_commit(list_result.current_oid)?;
            let parent_commit = repo.find_commit(list_result.parent_oid)?;

            let parent_tree = parent_commit.tree()?;
            let current_tree = current_commit.tree()?;

            let new_file_path = entry.new_file_path.as_ref().unwrap();
            let mut opts = DiffOptions::new();
            opts.pathspec(new_file_path);

            let diff =
                repo.diff_tree_to_tree(Some(&parent_tree), Some(&current_tree), Some(&mut opts))?;

            if let Some(buffer) = self.commit_text_view.buffer() {
                diff_text_view_util::print_diff_to_text_view(&diff, &buffer);
            }
        }

        Ok(())
    }
}
