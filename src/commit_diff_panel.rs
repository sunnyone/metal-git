use std::rc::Rc;
use git2::Oid;
use glib::{Cast, StaticType};
use gtk::Orientation;
use gtk::traits::{ContainerExt, PanedExt, TreeViewExt};

pub struct CommitDiffPanel {
    paned: gtk::Paned,

    diff_list_store: gtk::ListStore,
    diff_tree_view: gtk::TreeView,

    commit_text_view: gtk::TextView,
}

impl CommitDiffPanel {
    pub fn new() -> Rc<CommitDiffPanel> {
        let paned = gtk::Paned::new(Orientation::Horizontal);

        let diff_list_store = gtk::ListStore::new(&[
            String::static_type()  // filename
        ]);

        let diff_tree_view = gtk::TreeView::new();
        diff_tree_view.set_model(Some(&diff_list_store));
        paned.pack1(&diff_tree_view, true, false);

        let commit_text_view = gtk::TextView::new();
        paned.pack2(&commit_text_view, true, false);

        Rc::new(CommitDiffPanel {
            paned,
            diff_list_store,
            diff_tree_view,
            commit_text_view
        })
    }

    pub fn container(&self) -> gtk::Container {
        self.paned.clone().upcast::<gtk::Container>()
    }

    pub fn update_commit(&self, oid: &Oid) {
        println!("Update: {}", oid);
    }
}