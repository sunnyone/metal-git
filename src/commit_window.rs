use gtk::prelude::*;

use std::rc::Rc;
use std::cell::RefCell;
use std::str;
use std::path::Path;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::collections::HashSet;
use std::error;

use git2::{Error, StatusOptions};
use git2::build::CheckoutBuilder;

use repository_manager::RepositoryManager;
use gtk_utils;
use repository_ext::RepositoryExt;

pub struct CommitWindow {
    window: gtk::Window,

    refresh_button: gtk::Button,
    revert_button: gtk::Button,

    stage_button: gtk::Button,
    unstage_button: gtk::Button,

    amend_checkbutton: gtk::CheckButton,
    commit_button: gtk::Button,

    work_tree_files_list_store: gtk::ListStore,
    work_tree_files_tree_view: gtk::TreeView,

    staged_files_list_store: gtk::ListStore,
    staged_files_tree_view: gtk::TreeView,

    diff_text_view: gtk::TextView,
    message_text_view: gtk::TextView,

    repository_manager: Rc<RepositoryManager>,

    commited: RefCell<Box<Fn() -> ()>>,
}

const FILENAME_COLUMN: u32 = 0;

enum TreeType {
    WorkDir,
    Index,
}

impl CommitWindow {
    pub fn new(repository_manager: Rc<RepositoryManager>) -> Rc<CommitWindow> {
        let builder = gtk::Builder::new_from_resource("/org/sunnyone/MetalGit/commit_window.ui");

        let commit_window = CommitWindow {
            repository_manager: repository_manager,

            window: builder.get_object("commit_window").unwrap(),

            refresh_button: builder.get_object("refresh_button").unwrap(),
            revert_button: builder.get_object("revert_button").unwrap(),

            stage_button: builder.get_object("stage_button").unwrap(),
            unstage_button: builder.get_object("unstage_button").unwrap(),

            amend_checkbutton: builder.get_object("amend_checkbutton").unwrap(),
            commit_button: builder.get_object("commit_button").unwrap(),

            work_tree_files_list_store: builder.get_object("work_tree_files_list_store").unwrap(),
            work_tree_files_tree_view: builder.get_object("work_tree_files_tree_view").unwrap(),

            staged_files_list_store: builder.get_object("staged_files_list_store").unwrap(),
            staged_files_tree_view: builder.get_object("staged_files_tree_view").unwrap(),

            diff_text_view: builder.get_object("diff_text_view").unwrap(),
            message_text_view: builder.get_object("message_text_view").unwrap(),

            commited: RefCell::new(Box::new(|| {})),
        };

        let commit_window = Rc::new(commit_window);
        ::gtk_utils::modify_font_monospace(&commit_window.diff_text_view);

        let w = Rc::downgrade(&commit_window);
        commit_window.window.connect_delete_event(move |_, _| {
            w.upgrade().unwrap().hide();
            Inhibit(true)
        });

        let w = Rc::downgrade(&commit_window);
        commit_window.refresh_button.connect_clicked(move |_| {
            w.upgrade().unwrap().refresh();
        });

        let w = Rc::downgrade(&commit_window);
        commit_window.work_tree_files_tree_view.get_selection().connect_changed(move |selection| {
            let file = Self::get_selection_selected_file_single(selection);

            if let Some(file) = file {
                dialog_when_error!("Failed to diff: {:?}",
                                   w.upgrade().unwrap().work_tree_files_selected(&file));
            }
        });

        let w = Rc::downgrade(&commit_window);
        commit_window.staged_files_tree_view.get_selection().connect_changed(move |selection| {
            let file = Self::get_selection_selected_file_single(selection);

            if let Some(file) = file {
                dialog_when_error!("Failed to diff: {:?}",
                                   w.upgrade().unwrap().index_files_selected(&file));
            }
        });

        let w = Rc::downgrade(&commit_window);
        commit_window.work_tree_files_tree_view
                     .connect_row_activated(move |_tree_view, tree_path, _column| {
                         let w_ = w.upgrade().unwrap();
                         let file = Self::get_file_from_tree_path(&w_.work_tree_files_list_store,
                                                                  tree_path);

                         if let Some(file) = file {
                             dialog_when_error!("Failed to stage: {:?}",
                                                w_.stage_files(vec![file]));
                         }
                     });

        let w = Rc::downgrade(&commit_window);
        commit_window.staged_files_tree_view
                     .connect_row_activated(move |_tree_view, tree_path, _column| {
                         let w_ = w.upgrade().unwrap();
                         let file = Self::get_file_from_tree_path(&w_.staged_files_list_store,
                                                                  tree_path);

                         if let Some(file) = file {
                             dialog_when_error!("Failed to unstage: {:?}",
                                                w_.unstage_files(vec![file]));
                         }
                     });

        let w = Rc::downgrade(&commit_window);
        commit_window.revert_button.connect_clicked(move |_| {
            dialog_when_error!("Failed to revert: {:?}",
                               w.upgrade().unwrap().revert_button_clicked());
        });

        let w = Rc::downgrade(&commit_window);
        commit_window.stage_button.connect_clicked(move |_| {
            dialog_when_error!("Failed to stage: {:?}",
                               w.upgrade().unwrap().stage_button_clicked());
        });

        let w = Rc::downgrade(&commit_window);
        commit_window.unstage_button.connect_clicked(move |_| {
            dialog_when_error!("Failed to unstage: {:?}",
                               w.upgrade().unwrap().unstage_button_clicked());
        });


        let w = Rc::downgrade(&commit_window);
        commit_window.message_text_view.connect_key_press_event(move |_, key| {
            if key.get_state().intersects(gdk::CONTROL_MASK) {
                let keyval = key.get_keyval();
                // TODO: "KP_Enter" is nessessary?
                if gdk::keyval_name(keyval).map(|n| n == "Return").unwrap_or(false) {
                    dialog_when_error!("Failed to commit: {:?}",
                                       w.upgrade().unwrap().commit_or_amend());
                    return Inhibit(true);
                }
            }
            Inhibit(false)
        });

        let w = Rc::downgrade(&commit_window);
        commit_window.amend_checkbutton.connect_clicked(move |_| {
            dialog_when_error!("Failed to toggle amend: {:?}",
                               w.upgrade().unwrap().amend_checkbutton_clicked());
        });

        let w = Rc::downgrade(&commit_window);
        commit_window.commit_button.connect_clicked(move |_| {
            dialog_when_error!("Failed to commit: {:?}",
                               w.upgrade().unwrap().commit_or_amend());
        });

        commit_window
    }

    fn get_selection_selected_files(selection: &gtk::TreeSelection) -> Vec<String> {
        let (tree_paths, model) = selection.get_selected_rows();
        return tree_paths.iter()
                         .map(|path| Self::get_file_from_tree_path(&model, &path).unwrap())
                         .collect();
    }

    fn get_selection_selected_file_single(selection: &gtk::TreeSelection) -> Option<String> {
        let mut files = Self::get_selection_selected_files(selection);

        if files.len() == 1 {
            Some(files.pop().unwrap())
        } else {
            None
        }
    }

    fn set_selection_select_files(&self,
                                  selection: &gtk::TreeSelection,
                                  list_store: &gtk::ListStore,
                                  files: &Vec<String>) {
        let mut fileset = HashSet::new();
        for file in files {
            fileset.insert(file);
        }

        if let Some(tree_iter) = list_store.get_iter_first() {
            loop {
                let value = list_store.get_value(&tree_iter, FILENAME_COLUMN as i32);
                let filename = value.get::<String>().unwrap();
                if fileset.contains(&filename) {
                    selection.select_iter(&tree_iter);
                }

                if !list_store.iter_next(&tree_iter) {
                    break;
                }
            }
        }
    }

    fn get_file_from_tree_path<T: gtk::TreeModelExt>(list_store: &T,
                                                     tree_path: &gtk::TreePath)
                                                     -> Option<String> {
        list_store.get_iter(tree_path)
                  .and_then(|iter| {
                      let value = list_store.get_value(&iter, FILENAME_COLUMN as i32);
                      let s = value.get::<String>();
                      s
                  })
    }

    fn revert_button_clicked(&self) -> Result<(), Error> {
        let selection = self.work_tree_files_tree_view.get_selection();
        let files = Self::get_selection_selected_files(&selection);
        if files.len() == 0 {
            return Ok(());
        }

        self.revert(files)
    }

    fn revert(&self, files: Vec<String>) -> Result<(), Error> {
        if files.len() == 0 {
            return Ok(());
        }

        let repo = try!(self.repository_manager.open());

        let mut builder = CheckoutBuilder::new();
        let mut to_checkout = false;
        builder.force();

        let mut remove_file_paths = Vec::new();
        for file in &files {
            let file_path = Path::new(file);
            let status = try!(repo.status_file(file_path));
            if status == git2::STATUS_WT_NEW {
                remove_file_paths.push(file_path);
            } else {
                to_checkout = true;
                builder.path(file_path);
            }
        }

        if to_checkout {
            try!(repo.checkout_head(Some(&mut builder)));
        }

        for file_path in &remove_file_paths {
            let path = repo.get_full_path(file_path).unwrap();
            
            // TODO: convert error
            if let Err(error) = fs::remove_file(&path) {
                println!("Failed to remove {}: {}", path.to_string_lossy(), error);
            }
        }

        self.refresh();

        Ok(())
    }

    fn stage_button_clicked(&self) -> Result<(), Error> {
        let selection = self.work_tree_files_tree_view.get_selection();
        let files = Self::get_selection_selected_files(&selection);

        try!(self.stage_files(files));

        Ok(())
    }

    fn stage_files(&self, files: Vec<String>) -> Result<(), Error> {
        if files.len() == 0 {
            return Ok(());
        }

        let repo = try!(self.repository_manager.open());
        let mut index = try!(repo.index());

        for file in files {
            let file = Path::new(&file);
            let path_repo = repo.get_full_path(file).unwrap();
            if !fs::metadata(path_repo).is_ok() {
                // check exists
                try!(index.remove_path(&file));
            } else {
                try!(index.add_path(&file));
            }
        }

        try!(index.write());

        // TODO: partial update
        self.refresh();

        Ok(())
    }

    fn unstage_button_clicked(&self) -> Result<(), Error> {
        let selection = self.staged_files_tree_view.get_selection();
        let files = Self::get_selection_selected_files(&selection);

        try!(self.unstage_files(files));

        Ok(())
    }

    fn unstage_files(&self, files: Vec<String>) -> Result<(), Error> {
        if files.len() == 0 {
            return Ok(());
        }

        let repo = try!(self.repository_manager.open());

        let head_ref = try!(repo.head());
        let head_object = try!(head_ref.peel(git2::ObjectType::Commit));

        try!(repo.reset_default(Some(&head_object), files));

        // TODO: partial update
        self.refresh();

        Ok(())
    }

    fn amend_checkbutton_clicked(&self) -> Result<(), Error> {
        let to_amend = self.amend_checkbutton.get_active();
        let commit_message = self.get_commit_message();
        if !to_amend || commit_message.len() > 0 {
            return Ok(());
        }

        let repo = try!(self.repository_manager.open());

        let head_ref = try!(repo.head());
        let head_object = try!(head_ref.peel(git2::ObjectType::Commit));
        let head_commit = head_object.as_commit().unwrap();
        let last_commit_message = head_commit.message();

        if let Some(message) = last_commit_message {
            self.set_commit_message(&message);
        }

        Ok(())
    }

    fn get_commit_message(&self) -> String {
        let buffer = self.message_text_view.get_buffer().unwrap();
        let message = buffer.get_text(&buffer.get_start_iter(), &buffer.get_end_iter(), false)
                            .unwrap();
        message
    }

    fn set_commit_message(&self, message: &str) {
        self.message_text_view.get_buffer().unwrap().set_text(message);
    }

    fn commit(&self, to_amend: bool) -> Result<(), Error> {
        let message = self.get_commit_message();

        let repo = try!(self.repository_manager.open());
        let signature = try!(repo.signature());

        // TODO: initial repository does not have a commit
        let head_ref = try!(repo.head());
        let head_object = try!(head_ref.peel(git2::ObjectType::Commit));
        let head_commit = head_object.as_commit().unwrap();

        let mut index = try!(repo.index());
        let tree_oid = try!(index.write_tree());
        // TODO: use find_tree
        let tree_object = try!(repo.find_object(tree_oid, Some(git2::ObjectType::Tree)));
        let tree = tree_object.as_tree().unwrap();

        if !to_amend {
            try!(repo.commit(Some("HEAD"),
                             &signature,
                             &signature,
                             &message,
                             &tree,
                             &[head_commit]));
        } else {
            try!(head_commit.amend(Some("HEAD"),
                                   Some(&signature),
                                   Some(&signature),
                                   None,
                                   Some(&message),
                                   Some(&tree)));
        }

        // self.hide();
        self.refresh();
        self.set_commit_message("");

        self.commited.borrow()();

        Ok(())
    }

    fn commit_or_amend(&self) -> Result<(), Error> {
        let to_amend = self.amend_checkbutton.get_active();

        self.commit(to_amend)
    }

    pub fn show(&self) {
        self.window.show_all();
        self.refresh();
    }

    pub fn hide(&self) {
        self.window.hide();
    }

    pub fn refresh(&self) {
        let work_tree_selection = self.work_tree_files_tree_view.get_selection();
        let staged_selection = self.staged_files_tree_view.get_selection();

        // back selected files up
        let work_selected = Self::get_selection_selected_files(&work_tree_selection);
        let staged_selected = Self::get_selection_selected_files(&staged_selection);

        work_tree_selection.unselect_all();
        staged_selection.unselect_all();

        self.work_tree_files_list_store.clear();
        self.staged_files_list_store.clear();

        match collect_changed_status_items(&self.repository_manager) {
            Err(_) => ::gtk_utils::message_box_error("Error!"),
            Ok(list) => {
                for item in list {
                    let list_store = match item.tree_type {
                        TreeType::WorkDir => &self.work_tree_files_list_store,
                        TreeType::Index => &self.staged_files_list_store,
                    };

                    let _ = list_store.insert_with_values(None, &[FILENAME_COLUMN], &[&item.path]);
                }
            }
        }

        self.set_selection_select_files(&work_tree_selection,
                                        &self.work_tree_files_list_store,
                                        &work_selected);
        self.set_selection_select_files(&staged_selection,
                                        &self.staged_files_list_store,
                                        &staged_selected);
    }

    pub fn work_tree_files_selected(&self, filename: &str) -> Result<(), Error> {
        let repo = try!(self.repository_manager.open());

        let path = Path::new(filename);
        let status = try!(repo.status_file(path));
        if status == git2::STATUS_WT_NEW {
            self.show_new_file(path);
            Ok(())
        } else {
            let mut diff_opts = git2::DiffOptions::new();
            diff_opts.pathspec(filename);
            let diff = try!(repo.diff_index_to_workdir(None, Some(&mut diff_opts)));

            self.show_diff(&diff);

            Ok(())
        }
    }

    pub fn index_files_selected(&self, filename: &str) -> Result<(), Error> {
        let repo = try!(self.repository_manager.open());

        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.pathspec(filename);

        let head_ref = try!(repo.head());
        let head_tree_object = try!(head_ref.peel(git2::ObjectType::Tree));
        let head_tree = head_tree_object.as_tree().unwrap();

        let diff = try!(repo.diff_tree_to_index(Some(&head_tree), None, Some(&mut diff_opts)));

        self.show_diff(&diff);

        Ok(())
    }

    fn show_diff(&self, diff: &git2::Diff) {
        let buffer = self.diff_text_view.get_buffer().unwrap();
        buffer.set_text("");

        let mut iter = buffer.get_start_iter();
        let _ = diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let o = line.origin();
            let tag_name = match o {
                ' ' => "normal",
                '+' => "add",
                '-' => "delete",
                _ => "other",
            };

            let mut str = String::new();
            if o == '+' || o == '-' || o == ' ' {
                str.push(line.origin());
            }
            str.push_str(str::from_utf8(line.content()).unwrap());

            gtk_utils::text_buffer_insert_with_tag_by_name(&buffer, &mut iter, &str, tag_name);
            true
        });
    }

    pub fn set_diff_all_add_text(&self, text: &str) {
        let buffer = self.diff_text_view.get_buffer().unwrap();
        buffer.set_text("");

        let mut iter = buffer.get_start_iter();
        gtk_utils::text_buffer_insert_with_tag_by_name(&buffer, &mut iter, text, "add");
    }

    fn show_new_file(&self, path_in_repository: &Path) {
        match self.read_contents(&path_in_repository) {
            Ok(s) => {
                self.set_diff_all_add_text(&s);
            }
            Err(err) => {
                use std::error::Error;
                let msg = format!("This file is not browsable: {}", err.description());
                self.set_diff_all_add_text(&msg);
            }
        }
    }

    fn read_contents(&self, path_in_repository: &Path) -> Result<String, Box<error::Error>> {
        let repo = try!(self.repository_manager.open());

        let path = repo.get_full_path(path_in_repository).unwrap();
        
        let mut s = String::new();
        let mut f = try!(File::open(path));
        try!(f.read_to_string(&mut s));
        Ok(s)
    }

    pub fn connect_commited<F>(&self, callback: F)
        where F: Fn() -> () + 'static
    {
        *self.commited.borrow_mut() = Box::new(callback);
    }
}

struct StatusItem {
    path: String,
    tree_type: TreeType,
}

fn collect_changed_status_items(repository_manager: &RepositoryManager)
                                -> Result<Vec<StatusItem>, Error> {
    let repo = try!(repository_manager.open());
    if repo.is_bare() {
        return Err(Error::from_str("cannot report status on bare repository"));
    }

    let mut opts = StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);

    let statuses = try!(repo.statuses(Some(&mut opts)));
    let mut status_items: Vec<StatusItem> = Vec::new();

    for status in statuses.iter() {
        if status.path().is_none() {
            return Err(Error::from_str("Invalid file path exist"));
        }

        let path = status.path().unwrap();
        if status.index_to_workdir().is_some() {
            status_items.push(StatusItem {
                tree_type: TreeType::WorkDir,
                path: path.to_string(),
            });
        }
        if status.head_to_index().is_some() {
            status_items.push(StatusItem {
                tree_type: TreeType::Index,
                path: path.to_string(),
            });
        }
    }

    Ok(status_items)
}
