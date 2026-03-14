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

use gtk::glib::Propagation;

use crate::repository_manager::RepositoryManager;
use crate::gtk_utils;
use crate::repository_ext::RepositoryExt;
use crate::diff_text_view_util;
use crate::diff_text_view_util::create_diff_text_buffer;

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

    commited: RefCell<Box<dyn Fn() -> ()>>,
}

const FILENAME_COLUMN: u32 = 0;

enum TreeType {
    WorkDir,
    Index,
}

impl CommitWindow {
    pub fn new(repository_manager: Rc<RepositoryManager>) -> Rc<CommitWindow> {
        let builder = gtk::Builder::from_resource("/org/sunnyone/MetalGit/commit_window.ui");

        let commit_window = CommitWindow {
            repository_manager: repository_manager,

            window: builder.object("commit_window").unwrap(),

            refresh_button: builder.object("refresh_button").unwrap(),
            revert_button: builder.object("revert_button").unwrap(),

            stage_button: builder.object("stage_button").unwrap(),
            unstage_button: builder.object("unstage_button").unwrap(),

            amend_checkbutton: builder.object("amend_checkbutton").unwrap(),
            commit_button: builder.object("commit_button").unwrap(),

            work_tree_files_list_store: builder.object("work_tree_files_list_store").unwrap(),
            work_tree_files_tree_view: builder.object("work_tree_files_tree_view").unwrap(),

            staged_files_list_store: builder.object("staged_files_list_store").unwrap(),
            staged_files_tree_view: builder.object("staged_files_tree_view").unwrap(),

            diff_text_view: builder.object("diff_text_view").unwrap(),
            message_text_view: builder.object("message_text_view").unwrap(),

            commited: RefCell::new(Box::new(|| {})),
        };

        let commit_window = Rc::new(commit_window);
        commit_window.diff_text_view.set_monospace(true);

        let diff_text_buffer = create_diff_text_buffer();
        commit_window.diff_text_view.set_buffer(Some(&diff_text_buffer));

        let w = Rc::downgrade(&commit_window);
        commit_window.window.connect_delete_event(move |_, _| {
            w.upgrade().unwrap().hide();
            Propagation::Stop
        });

        let w = Rc::downgrade(&commit_window);
        commit_window.refresh_button.connect_clicked(move |_| {
            w.upgrade().unwrap().refresh();
        });

        let w = Rc::downgrade(&commit_window);
        commit_window.work_tree_files_tree_view.selection().connect_changed(move |selection| {
            let file = Self::get_selection_selected_file_single(selection);

            if let Some(file) = file {
                dialog_when_error!("Failed to diff: {:?}",
                                   w.upgrade().unwrap().work_tree_files_selected(&file));
            }
        });

        let w = Rc::downgrade(&commit_window);
        commit_window.staged_files_tree_view.selection().connect_changed(move |selection| {
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
            if key.state().intersects(gtk::gdk::ModifierType::CONTROL_MASK) {
                // TODO: works?
                // TODO: "KP_Enter" is nessessary?
                if key.keyval().name().map(|n| n == "Return").unwrap_or(false) {
                    dialog_when_error!("Failed to commit: {:?}",
                                       w.upgrade().unwrap().commit_or_amend());
                    return Propagation::Stop;
                }
            }
            Propagation::Proceed
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
        let (tree_paths, model) = selection.selected_rows();
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

        if let Some(tree_iter) = list_store.iter_first() {
            loop {
                let value = list_store.value(&tree_iter, FILENAME_COLUMN as i32);
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

    fn get_file_from_tree_path<T: gtk::traits::TreeModelExt>(list_store: &T,
                                                             tree_path: &gtk::TreePath)
                                                             -> Option<String> {
        list_store.iter(tree_path)
            .and_then(|iter| {
                let value = list_store.value(&iter, FILENAME_COLUMN as i32);
                let s = value.get::<String>();
                s.ok()
            })
    }

    fn revert_button_clicked(&self) -> Result<(), Error> {
        let selection = self.work_tree_files_tree_view.selection();
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

        let repo = self.repository_manager.open()?;

        let mut builder = CheckoutBuilder::new();
        let mut to_checkout = false;
        builder.force();

        let mut remove_file_paths = Vec::new();
        for file in &files {
            let file_path = Path::new(file);
            let status = repo.status_file(file_path)?;
            if status == git2::Status::WT_NEW {
                remove_file_paths.push(file_path);
            } else {
                to_checkout = true;
                builder.path(file_path);
            }
        }

        if to_checkout {
            repo.checkout_head(Some(&mut builder))?;
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
        let selection = self.work_tree_files_tree_view.selection();
        let files = Self::get_selection_selected_files(&selection);

        self.stage_files(files)?;

        Ok(())
    }

    fn stage_files(&self, files: Vec<String>) -> Result<(), Error> {
        if files.len() == 0 {
            return Ok(());
        }

        let repo = self.repository_manager.open()?;
        let mut index = repo.index()?;

        for file in files {
            let file = Path::new(&file);
            let path_repo = repo.get_full_path(file).unwrap();
            if !fs::metadata(path_repo).is_ok() {
                index.remove_path(&file)?;
            } else {
                index.add_path(&file)?;
            }
        }

        index.write()?;

        // TODO: partial update
        self.refresh();

        Ok(())
    }

    fn unstage_button_clicked(&self) -> Result<(), Error> {
        let selection = self.staged_files_tree_view.selection();
        let files = Self::get_selection_selected_files(&selection);

        self.unstage_files(files)?;

        Ok(())
    }

    fn unstage_files(&self, files: Vec<String>) -> Result<(), Error> {
        if files.len() == 0 {
            return Ok(());
        }

        let repo = self.repository_manager.open()?;

        let head_ref = repo.head()?;
        let head_object = head_ref.peel(git2::ObjectType::Commit)?;

        repo.reset_default(Some(&head_object), files)?;

        // TODO: partial update
        self.refresh();

        Ok(())
    }

    fn amend_checkbutton_clicked(&self) -> Result<(), Error> {
        let to_amend = self.amend_checkbutton.is_active();
        let commit_message = self.get_commit_message();
        if !to_amend || commit_message.len() > 0 {
            return Ok(());
        }

        let repo = self.repository_manager.open()?;

        let head_ref = repo.head()?;
        let head_object = head_ref.peel(git2::ObjectType::Commit)?;
        let head_commit = head_object.as_commit().unwrap();
        let last_commit_message = head_commit.message();

        if let Some(message) = last_commit_message {
            self.set_commit_message(&message);
        }

        Ok(())
    }

    fn get_commit_message(&self) -> String {
        let buffer = self.message_text_view.buffer().unwrap();
        let message = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false)
            .unwrap();
        message.to_string()
    }

    fn set_commit_message(&self, message: &str) {
        self.message_text_view.buffer().unwrap().set_text(message);
    }

    fn commit(&self, to_amend: bool) -> Result<(), Error> {
        let message = self.get_commit_message();

        let repo = self.repository_manager.open()?;
        let signature = repo.signature()?;

        // TODO: initial repository does not have a commit
        let head_ref = repo.head()?;
        let head_object = head_ref.peel(git2::ObjectType::Commit)?;
        let head_commit = head_object.as_commit().unwrap();

        let mut index = repo.index()?;
        let tree_oid = index.write_tree()?;
        // TODO: use find_tree
        let tree_object = repo.find_object(tree_oid, Some(git2::ObjectType::Tree))?;
        let tree = tree_object.as_tree().unwrap();

        if !to_amend {
            repo.commit(Some("HEAD"),
                        &signature,
                        &signature,
                        &message,
                        &tree,
                        &[head_commit])?;
        } else {
            head_commit.amend(Some("HEAD"),
                              Some(&signature),
                              Some(&signature),
                              None,
                              Some(&message),
                              Some(&tree))?;
        }

        // self.hide();
        self.refresh();
        self.set_commit_message("");

        self.commited.borrow()();

        Ok(())
    }

    fn commit_or_amend(&self) -> Result<(), Error> {
        let to_amend = self.amend_checkbutton.is_active();

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
        let work_tree_selection = self.work_tree_files_tree_view.selection();
        let staged_selection = self.staged_files_tree_view.selection();

        // back selected files up
        let work_selected = Self::get_selection_selected_files(&work_tree_selection);
        let staged_selected = Self::get_selection_selected_files(&staged_selection);

        work_tree_selection.unselect_all();
        staged_selection.unselect_all();

        self.work_tree_files_list_store.clear();
        self.staged_files_list_store.clear();

        match collect_changed_status_items(&self.repository_manager) {
            Err(_) => crate::gtk_utils::message_box_error("Error!"),
            Ok(list) => {
                for item in list {
                    let list_store = match item.tree_type {
                        TreeType::WorkDir => &self.work_tree_files_list_store,
                        TreeType::Index => &self.staged_files_list_store,
                    };

                    let _ = list_store.insert_with_values(None, &[(FILENAME_COLUMN, &item.path)]);
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
        let repo = self.repository_manager.open()?;

        let path = Path::new(filename);
        let status = repo.status_file(path)?;
        if status == git2::Status::WT_NEW {
            self.show_new_file(path);
            Ok(())
        } else {
            let mut diff_opts = git2::DiffOptions::new();
            diff_opts.pathspec(filename);
            let diff = repo.diff_index_to_workdir(None, Some(&mut diff_opts))?;

            self.show_diff(&diff);

            Ok(())
        }
    }

    pub fn index_files_selected(&self, filename: &str) -> Result<(), Error> {
        let repo = self.repository_manager.open()?;

        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.pathspec(filename);

        let head_ref = repo.head()?;
        let head_tree_object = head_ref.peel(git2::ObjectType::Tree)?;
        let head_tree = head_tree_object.as_tree().unwrap();

        let diff = repo.diff_tree_to_index(Some(&head_tree), None, Some(&mut diff_opts))?;

        self.show_diff(&diff);

        Ok(())
    }

    fn show_diff(&self, diff: &git2::Diff) {
        let buffer = self.diff_text_view.buffer().unwrap();

        diff_text_view_util::print_diff_to_text_view(diff, &buffer);
    }

    pub fn set_diff_all_add_text(&self, text: &str) {
        let buffer = self.diff_text_view.buffer().unwrap();
        buffer.set_text("");

        let mut iter = buffer.start_iter();
        gtk_utils::text_buffer_insert_with_tag_by_name(&buffer, &mut iter, text, "add");
    }

    fn show_new_file(&self, path_in_repository: &Path) {
        match self.read_contents(&path_in_repository) {
            Ok(s) => {
                self.set_diff_all_add_text(&s);
            }
            Err(err) => {
                let msg = format!("This file is not browsable: {}", err.to_string());
                self.set_diff_all_add_text(&msg);
            }
        }
    }

    fn read_contents(&self, path_in_repository: &Path) -> Result<String, Box<dyn error::Error>> {
        let repo = self.repository_manager.open()?;

        let path = repo.get_full_path(path_in_repository).unwrap();

        let mut s = String::new();
        let mut f = File::open(path)?;
        f.read_to_string(&mut s)?;
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
    let repo = repository_manager.open()?;
    if repo.is_bare() {
        return Err(Error::from_str("cannot report status on bare repository"));
    }

    let mut opts = StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);

    let statuses = repo.statuses(Some(&mut opts))?;
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
