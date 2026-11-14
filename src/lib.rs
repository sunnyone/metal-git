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
mod status_bar;
pub mod diff;
mod diff_view;
mod branch_context_menu;
mod push_dialog;

mod repository_ext;
mod static_resource;

pub mod app_settings;
pub mod background_task;
pub mod fetch_operation;
pub mod pull_operation;
pub mod push_operation;
pub mod switch_branch_operation;
pub mod long_running_operation;
pub mod railway;
pub mod remote_operation;
pub mod work_tree_status;

pub fn run_app() {
    static_resource::init();

    let app = gtk::Application::builder()
        .application_id("org.sunnyone.MetalGit")
        .build();

    app.connect_activate(|app| {
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::IconTheme::for_display(&display).add_resource_path("/org/sunnyone/MetalGit/icons");
        }

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
