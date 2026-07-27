extern crate git2;
extern crate gtk;

use gtk::prelude::*;

pub mod repository_manager;
pub mod branch;

#[macro_use]
mod gtk_utils;

mod station_renderer;
mod commit_window;
mod history_window;
mod window_manager;
mod commit_diff_panel;
mod commit_diff_util;
mod diff_text_view_util;
mod branch_context_menu;

mod repository_ext;

pub mod railway;

pub fn run_app() {
    let app = gtk::Application::builder()
        .application_id("org.sunnyone.MetalGit")
        .build();

    app.connect_activate(|app| {
        let repository_manager = repository_manager::RepositoryManager::new();
        repository_manager.set_work_dir_path(".");

        let window_manager = window_manager::WindowManager::new(app, repository_manager);
        unsafe {
            app.set_data("metal-git-window-manager", window_manager.clone());
        }
        window_manager.start();
    });

    app.run();
}
