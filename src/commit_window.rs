use gtk::gdk;
use gtk::glib::Propagation;
use gtk::prelude::*;

use std::cell::RefCell;
use std::collections::HashSet;
use std::error;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::rc::Rc;
use std::str;

use git2::build::CheckoutBuilder;
use git2::{Error, StatusOptions};

use crate::diff_text_view_util;
use crate::diff_text_view_util::create_diff_text_buffer;
use crate::gtk_utils;
use crate::repository_ext::RepositoryExt;
use crate::repository_manager::RepositoryManager;

pub struct CommitWindow {
    window: gtk::Window,
    refresh_button: gtk::Button,
    revert_button: gtk::Button,
    stage_button: gtk::Button,
    unstage_button: gtk::Button,
    amend_checkbutton: gtk::CheckButton,
    commit_button: gtk::Button,
    work_tree_files_list_box: gtk::ListBox,
    staged_files_list_box: gtk::ListBox,
    diff_text_view: gtk::TextView,
    message_text_view: gtk::TextView,
    repository_manager: Rc<RepositoryManager>,
    commited: RefCell<Box<dyn Fn()>>,
    work_tree_files: RefCell<Vec<String>>,
    staged_files: RefCell<Vec<String>>,
}

enum TreeType {
    WorkDir,
    Index,
}

impl CommitWindow {
    pub fn new(app: &gtk::Application, repository_manager: Rc<RepositoryManager>) -> Rc<CommitWindow> {
        let window = gtk::Window::builder()
            .application(app)
            .title("Commit Window")
            .default_width(1000)
            .default_height(700)
            .build();

        let root = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .wide_handle(true)
            .hexpand(true)
            .vexpand(true)
            .build();
        root.set_position(300);
        root.set_resize_start_child(false);
        root.set_shrink_start_child(false);
        window.set_child(Some(&root));

        let left = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .hexpand(true)
            .vexpand(true)
            .build();
        root.set_start_child(Some(&left));

        let top_buttons = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        left.append(&top_buttons);

        let refresh_button = gtk::Button::with_label("Refresh");
        top_buttons.append(&refresh_button);

        let top_buttons_spacer = gtk::Box::builder().hexpand(true).build();
        top_buttons.append(&top_buttons_spacer);

        let revert_button = gtk::Button::with_label("Revert");
        top_buttons.append(&revert_button);

        let work_tree_files_list_box = gtk::ListBox::new();
        work_tree_files_list_box.set_activate_on_single_click(false);
        work_tree_files_list_box.set_selection_mode(gtk::SelectionMode::Multiple);
        let work_tree_frame =
            build_list_frame("Working Directory", &work_tree_files_list_box);
        work_tree_frame.set_hexpand(true);
        work_tree_frame.set_vexpand(true);
        left.append(&work_tree_frame);

        let center_buttons = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        left.append(&center_buttons);

        let unstage_button = gtk::Button::with_label("^ Unstage");
        center_buttons.append(&unstage_button);

        let center_buttons_spacer = gtk::Box::builder().hexpand(true).build();
        center_buttons.append(&center_buttons_spacer);

        let stage_button = gtk::Button::with_label("v Stage");
        center_buttons.append(&stage_button);

        let staged_files_list_box = gtk::ListBox::new();
        staged_files_list_box.set_activate_on_single_click(false);
        staged_files_list_box.set_selection_mode(gtk::SelectionMode::Multiple);
        let staged_frame = build_list_frame("Staging Area", &staged_files_list_box);
        staged_frame.set_hexpand(true);
        staged_frame.set_vexpand(true);
        left.append(&staged_frame);

        let right = gtk::Paned::builder()
            .orientation(gtk::Orientation::Vertical)
            .wide_handle(true)
            .hexpand(true)
            .vexpand(true)
            .build();
        right.set_position(400);
        right.set_resize_start_child(false);
        right.set_shrink_start_child(true);
        root.set_end_child(Some(&right));

        let diff_text_buffer = create_diff_text_buffer();
        let diff_text_view = gtk::TextView::builder()
            .editable(false)
            .buffer(&diff_text_buffer)
            .monospace(true)
            .build();
        let diff_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&diff_text_view)
            .build();
        right.set_start_child(Some(&diff_scrolled));

        let bottom = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .hexpand(true)
            .vexpand(true)
            .build();
        right.set_end_child(Some(&bottom));

        let message_text_view = gtk::TextView::new();
        let message_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&message_text_view)
            .build();
        bottom.append(&message_scrolled);

        let action_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        bottom.append(&action_row);

        let action_row_spacer = gtk::Box::builder().hexpand(true).build();
        action_row.append(&action_row_spacer);

        let amend_checkbutton = gtk::CheckButton::with_label("amend");
        action_row.append(&amend_checkbutton);

        let commit_button = gtk::Button::with_label("Commit");
        action_row.append(&commit_button);

        let commit_window = Rc::new(CommitWindow {
            window,
            refresh_button,
            revert_button,
            stage_button,
            unstage_button,
            amend_checkbutton,
            commit_button,
            work_tree_files_list_box,
            staged_files_list_box,
            diff_text_view,
            message_text_view,
            repository_manager,
            commited: RefCell::new(Box::new(|| {})),
            work_tree_files: RefCell::new(Vec::new()),
            staged_files: RefCell::new(Vec::new()),
        });

        commit_window.setup_callbacks();
        commit_window
    }

    fn setup_callbacks(self: &Rc<Self>) {
        let w = Rc::downgrade(self);
        self.window.connect_close_request(move |_| {
            w.upgrade().unwrap().hide();
            Propagation::Stop
        });

        let w = Rc::downgrade(self);
        self.refresh_button.connect_clicked(move |_| {
            w.upgrade().unwrap().refresh();
        });

        let w = Rc::downgrade(self);
        self.work_tree_files_list_box
            .connect_selected_rows_changed(move |list_box| {
                let Some(commit_window) = w.upgrade() else {
                    return;
                };

                let file = commit_window.get_single_selected_file(list_box, TreeType::WorkDir);
                if let Some(file) = file {
                    dialog_when_error!(
                        "Failed to diff: {:?}",
                        commit_window.work_tree_files_selected(&file)
                    );
                }
            });

        let w = Rc::downgrade(self);
        self.staged_files_list_box
            .connect_selected_rows_changed(move |list_box| {
                let Some(commit_window) = w.upgrade() else {
                    return;
                };

                let file = commit_window.get_single_selected_file(list_box, TreeType::Index);
                if let Some(file) = file {
                    dialog_when_error!(
                        "Failed to diff: {:?}",
                        commit_window.index_files_selected(&file)
                    );
                }
            });

        let w = Rc::downgrade(self);
        self.work_tree_files_list_box.connect_row_activated(move |_, row| {
            let Some(commit_window) = w.upgrade() else {
                return;
            };

            let file = commit_window.get_file_from_row(row, TreeType::WorkDir);
            if let Some(file) = file {
                dialog_when_error!("Failed to stage: {:?}", commit_window.stage_files(vec![file]));
            }
        });

        let w = Rc::downgrade(self);
        self.staged_files_list_box.connect_row_activated(move |_, row| {
            let Some(commit_window) = w.upgrade() else {
                return;
            };

            let file = commit_window.get_file_from_row(row, TreeType::Index);
            if let Some(file) = file {
                dialog_when_error!(
                    "Failed to unstage: {:?}",
                    commit_window.unstage_files(vec![file])
                );
            }
        });

        let w = Rc::downgrade(self);
        self.revert_button.connect_clicked(move |_| {
            dialog_when_error!(
                "Failed to revert: {:?}",
                w.upgrade().unwrap().revert_button_clicked()
            );
        });

        let w = Rc::downgrade(self);
        self.stage_button.connect_clicked(move |_| {
            dialog_when_error!(
                "Failed to stage: {:?}",
                w.upgrade().unwrap().stage_button_clicked()
            );
        });

        let w = Rc::downgrade(self);
        self.unstage_button.connect_clicked(move |_| {
            dialog_when_error!(
                "Failed to unstage: {:?}",
                w.upgrade().unwrap().unstage_button_clicked()
            );
        });

        let controller = gtk::EventControllerKey::new();
        let w = Rc::downgrade(self);
        controller.connect_key_pressed(move |_, key, _, state| {
            if state.intersects(gdk::ModifierType::CONTROL_MASK)
                && (key == gdk::Key::Return || key == gdk::Key::KP_Enter)
            {
                dialog_when_error!(
                    "Failed to commit: {:?}",
                    w.upgrade().unwrap().commit_or_amend()
                );
                return Propagation::Stop;
            }

            Propagation::Proceed
        });
        self.message_text_view.add_controller(controller);

        let w = Rc::downgrade(self);
        self.amend_checkbutton.connect_toggled(move |_| {
            dialog_when_error!(
                "Failed to toggle amend: {:?}",
                w.upgrade().unwrap().amend_checkbutton_clicked()
            );
        });

        let w = Rc::downgrade(self);
        self.commit_button.connect_clicked(move |_| {
            dialog_when_error!(
                "Failed to commit: {:?}",
                w.upgrade().unwrap().commit_or_amend()
            );
        });
    }

    fn files_for_tree_type(&self, tree_type: TreeType) -> &RefCell<Vec<String>> {
        match tree_type {
            TreeType::WorkDir => &self.work_tree_files,
            TreeType::Index => &self.staged_files,
        }
    }

    fn get_selected_files(&self, list_box: &gtk::ListBox, tree_type: TreeType) -> Vec<String> {
        let files = self.files_for_tree_type(tree_type).borrow();
        list_box
            .selected_rows()
            .iter()
            .filter_map(|row| {
                let index = row.index();
                if index < 0 {
                    None
                } else {
                    files.get(index as usize).cloned()
                }
            })
            .collect()
    }

    fn get_single_selected_file(
        &self,
        list_box: &gtk::ListBox,
        tree_type: TreeType,
    ) -> Option<String> {
        let mut files = self.get_selected_files(list_box, tree_type);
        if files.len() == 1 {
            Some(files.pop().unwrap())
        } else {
            None
        }
    }

    fn restore_selection(
        &self,
        list_box: &gtk::ListBox,
        tree_type: TreeType,
        selected_files: &[String],
    ) {
        let wanted = selected_files.iter().collect::<HashSet<_>>();
        let files = self.files_for_tree_type(tree_type).borrow();

        for (index, file) in files.iter().enumerate() {
            if !wanted.contains(file) {
                continue;
            }

            if let Some(row) = list_box.row_at_index(index as i32) {
                list_box.select_row(Some(&row));
            }
        }
    }

    fn get_file_from_row(&self, row: &gtk::ListBoxRow, tree_type: TreeType) -> Option<String> {
        let files = self.files_for_tree_type(tree_type).borrow();
        let index = row.index();
        if index < 0 {
            None
        } else {
            files.get(index as usize).cloned()
        }
    }

    fn revert_button_clicked(&self) -> Result<(), Error> {
        let files = self.get_selected_files(&self.work_tree_files_list_box, TreeType::WorkDir);
        if files.is_empty() {
            return Ok(());
        }

        self.revert(files)
    }

    fn revert(&self, files: Vec<String>) -> Result<(), Error> {
        if files.is_empty() {
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
            if let Err(error) = fs::remove_file(&path) {
                println!("Failed to remove {}: {}", path.to_string_lossy(), error);
            }
        }

        self.refresh();
        Ok(())
    }

    fn stage_button_clicked(&self) -> Result<(), Error> {
        let files = self.get_selected_files(&self.work_tree_files_list_box, TreeType::WorkDir);
        self.stage_files(files)?;
        Ok(())
    }

    fn stage_files(&self, files: Vec<String>) -> Result<(), Error> {
        if files.is_empty() {
            return Ok(());
        }

        let repo = self.repository_manager.open()?;
        let mut index = repo.index()?;

        for file in files {
            let file = Path::new(&file);
            let path_repo = repo.get_full_path(file).unwrap();
            if fs::metadata(path_repo).is_err() {
                index.remove_path(file)?;
            } else {
                index.add_path(file)?;
            }
        }

        index.write()?;
        self.refresh();
        Ok(())
    }

    fn unstage_button_clicked(&self) -> Result<(), Error> {
        let files = self.get_selected_files(&self.staged_files_list_box, TreeType::Index);
        self.unstage_files(files)?;
        Ok(())
    }

    fn unstage_files(&self, files: Vec<String>) -> Result<(), Error> {
        if files.is_empty() {
            return Ok(());
        }

        let repo = self.repository_manager.open()?;

        let head_ref = repo.head()?;
        let head_object = head_ref.peel(git2::ObjectType::Commit)?;
        repo.reset_default(Some(&head_object), files)?;

        self.refresh();
        Ok(())
    }

    fn amend_checkbutton_clicked(&self) -> Result<(), Error> {
        let to_amend = self.amend_checkbutton.is_active();
        let commit_message = self.get_commit_message();
        if !to_amend || !commit_message.is_empty() {
            return Ok(());
        }

        let repo = self.repository_manager.open()?;

        let head_ref = repo.head()?;
        let head_object = head_ref.peel(git2::ObjectType::Commit)?;
        let head_commit = head_object.as_commit().unwrap();

        if let Some(message) = head_commit.message() {
            self.set_commit_message(message);
        }

        Ok(())
    }

    fn get_commit_message(&self) -> String {
        let buffer = self.message_text_view.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string()
    }

    fn set_commit_message(&self, message: &str) {
        self.message_text_view.buffer().set_text(message);
    }

    fn commit(&self, to_amend: bool) -> Result<(), Error> {
        let message = self.get_commit_message();

        let repo = self.repository_manager.open()?;
        let signature = repo.signature()?;

        let head_ref = repo.head()?;
        let head_object = head_ref.peel(git2::ObjectType::Commit)?;
        let head_commit = head_object.as_commit().unwrap();

        let mut index = repo.index()?;
        let tree_oid = index.write_tree()?;
        let tree_object = repo.find_object(tree_oid, Some(git2::ObjectType::Tree))?;
        let tree = tree_object.as_tree().unwrap();

        if !to_amend {
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                &message,
                &tree,
                &[head_commit],
            )?;
        } else {
            head_commit.amend(
                Some("HEAD"),
                Some(&signature),
                Some(&signature),
                None,
                Some(&message),
                Some(&tree),
            )?;
        }

        self.refresh();
        self.set_commit_message("");
        self.commited.borrow()();

        Ok(())
    }

    fn commit_or_amend(&self) -> Result<(), Error> {
        self.commit(self.amend_checkbutton.is_active())
    }

    pub fn show(&self) {
        self.window.present();
        self.refresh();
    }

    pub fn hide(&self) {
        self.window.hide();
    }

    pub fn refresh(&self) {
        let work_selected =
            self.get_selected_files(&self.work_tree_files_list_box, TreeType::WorkDir);
        let staged_selected = self.get_selected_files(&self.staged_files_list_box, TreeType::Index);

        clear_list_box(&self.work_tree_files_list_box);
        clear_list_box(&self.staged_files_list_box);
        self.work_tree_files.borrow_mut().clear();
        self.staged_files.borrow_mut().clear();

        match collect_changed_status_items(&self.repository_manager) {
            Err(_) => crate::gtk_utils::message_box_error("Error!"),
            Ok(list) => {
                for item in list {
                    match item.tree_type {
                        TreeType::WorkDir => {
                            self.work_tree_files.borrow_mut().push(item.path.clone());
                            self.work_tree_files_list_box
                                .append(&build_file_row(&item.path));
                        }
                        TreeType::Index => {
                            self.staged_files.borrow_mut().push(item.path.clone());
                            self.staged_files_list_box.append(&build_file_row(&item.path));
                        }
                    }
                }
            }
        }

        self.restore_selection(
            &self.work_tree_files_list_box,
            TreeType::WorkDir,
            &work_selected,
        );
        self.restore_selection(
            &self.staged_files_list_box,
            TreeType::Index,
            &staged_selected,
        );
    }

    pub fn work_tree_files_selected(&self, filename: &str) -> Result<(), Error> {
        let repo = self.repository_manager.open()?;

        let path = Path::new(filename);
        let status = repo.status_file(path)?;
        if status == git2::Status::WT_NEW {
            self.show_new_file(path);
        } else {
            let mut diff_opts = git2::DiffOptions::new();
            diff_opts.pathspec(filename);
            let diff = repo.diff_index_to_workdir(None, Some(&mut diff_opts))?;
            self.show_diff(&diff);
        }

        Ok(())
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
        let buffer = self.diff_text_view.buffer();
        diff_text_view_util::print_diff_to_text_view(diff, &buffer);
    }

    pub fn set_diff_all_add_text(&self, text: &str) {
        let buffer = self.diff_text_view.buffer();
        buffer.set_text("");

        let mut iter = buffer.start_iter();
        gtk_utils::text_buffer_insert_with_tag_by_name(&buffer, &mut iter, text, "add");
    }

    fn show_new_file(&self, path_in_repository: &Path) {
        match self.read_contents(path_in_repository) {
            Ok(s) => self.set_diff_all_add_text(&s),
            Err(err) => {
                let msg = format!("This file is not browsable: {}", err);
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
    where
        F: Fn() + 'static,
    {
        *self.commited.borrow_mut() = Box::new(callback);
    }
}

struct StatusItem {
    path: String,
    tree_type: TreeType,
}

fn collect_changed_status_items(
    repository_manager: &RepositoryManager,
) -> Result<Vec<StatusItem>, Error> {
    let repo = repository_manager.open()?;
    if repo.is_bare() {
        return Err(Error::from_str("cannot report status on bare repository"));
    }

    let mut opts = StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);

    let statuses = repo.statuses(Some(&mut opts))?;
    let mut status_items: Vec<StatusItem> = Vec::new();

    for status in statuses.iter() {
        let Some(path) = status.path() else {
            return Err(Error::from_str("Invalid file path exist"));
        };

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

fn build_list_frame(title: &str, list_box: &gtk::ListBox) -> gtk::Frame {
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .min_content_height(180)
        .child(list_box)
        .build();

    gtk::Frame::builder().label(title).child(&scrolled).build()
}

fn build_file_row(path: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(path)
        .xalign(0.0)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build()
}

fn clear_list_box(list_box: &gtk::ListBox) {
    while let Some(row) = list_box.row_at_index(0) {
        list_box.remove(&row);
    }
}
