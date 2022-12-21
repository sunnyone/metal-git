use git2::{Error, Oid};
use glib::{Cast, StaticType};
use gtk::traits::{PanedExt, TreeViewExt};
use gtk::Orientation;
use std::rc::Rc;
use gtk::prelude::TreeViewColumnExt;
use gtk::prelude::GtkListStoreExt;
use gtk::prelude::GtkListStoreExtManual;

use crate::commit_diff_util;
use crate::repository_manager::RepositoryManager;

pub struct CommitDiffPanel {
    paned: gtk::Paned,

    diff_list_store: gtk::ListStore,
    diff_tree_view: gtk::TreeView,

    commit_text_view: gtk::TextView,

    repository_manager: Rc<RepositoryManager>,
}

const COLUMN_FILENAME: u32 = 0;
const COLUMN_OLD_FILENAME: u32 = 1;
const COLUMN_NEW_FILENAME: u32 = 2;

impl CommitDiffPanel {
    pub fn new(repository_manager: Rc<RepositoryManager>) -> Rc<CommitDiffPanel> {
        let paned = gtk::Paned::new(Orientation::Horizontal);

        let diff_list_store = gtk::ListStore::new(&[
            String::static_type(), // filename
            String::static_type(), // old_filename
            String::static_type(), // new_filename
        ]);

        let diff_tree_view = gtk::TreeView::new();
        diff_tree_view.set_model(Some(&diff_list_store));

        let renderer = gtk::CellRendererText::new();
        let col = gtk::TreeViewColumn::new();
        col.set_title("Filename");
        col.pack_start(&renderer, false);
        col.add_attribute(&renderer, "text", COLUMN_FILENAME as i32);
        diff_tree_view.append_column(&col);

        paned.pack1(&diff_tree_view, true, false);

        let commit_text_view = gtk::TextView::new();
        paned.pack2(&commit_text_view, true, false);

        Rc::new(CommitDiffPanel {
            paned,
            diff_list_store,
            diff_tree_view,
            commit_text_view,
            repository_manager,
        })
    }

    pub fn container(&self) -> gtk::Container {
        self.paned.clone().upcast::<gtk::Container>()
    }

    pub fn update_commit(&self, oid: Oid) -> Result<(), Error> {
        let result = commit_diff_util::list_commit_diff_files(self.repository_manager.as_ref(), oid)?;

        self.diff_list_store.clear();

        for x in result.files {
            self.diff_list_store.insert_with_values(
                None,
                &[
                    (COLUMN_FILENAME, &x.format_file_move()),
                    (COLUMN_OLD_FILENAME, &x.old_file_path),
                    (COLUMN_NEW_FILENAME, &x.new_file_path),
                ],
            );
        }

        Ok(())
    }
}
