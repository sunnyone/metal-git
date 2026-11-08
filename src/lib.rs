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
mod startup_log;

pub mod railway;

pub fn run_app() {
    startup_log::init();
    let activation_wait = startup_log::Span::new("app.until_activate");
    let app = startup_log::measure("app.create", || {
        gtk::Application::builder()
            .application_id("org.sunnyone.MetalGit")
            .build()
    });

    let activation_wait = std::cell::RefCell::new(Some(activation_wait));
    app.connect_activate(move |app| {
        drop(activation_wait.borrow_mut().take());
        let _span = startup_log::Span::new("app.activate");
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
