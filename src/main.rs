extern crate gtk;
extern crate git2;
extern crate glib;

pub mod repository_manager;

mod static_resource;
#[macro_use]
mod gtk_utils;

mod station_renderer;
mod station_cell_renderer;
mod station_wrapper;

mod commit_window;
mod history_window;
mod window_manager;
mod commit_diff_panel;
mod commit_diff_util;
mod diff_text_view_util;

mod repository_ext;

pub mod railway;

use std::rc::Rc;

#[allow(dead_code)]
fn main() {
    gtk::init().unwrap();
    static_resource::init();

    let repository_manager = repository_manager::RepositoryManager::new();
    repository_manager.set_work_dir_path(".");

    let window_manager = Rc::new(window_manager::WindowManager::new(repository_manager));
    window_manager.start();

    // railway::collect_tree(".").unwrap();

    gtk::main();
}
