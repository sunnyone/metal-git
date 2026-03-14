use crate::commit_diff_panel::CommitDiffPanel;
use crate::railway;
use crate::repository_manager::RepositoryManager;
use crate::station_renderer;
use crate::window_manager::WindowManager;
use git2::Error;
use gtk::gdk;
use gtk::glib::Propagation;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

pub struct HistoryWindow {
    window: gtk::ApplicationWindow,
    window_manager: Weak<WindowManager>,
    repository_manager: Rc<RepositoryManager>,
    commit_diff_panel: Rc<CommitDiffPanel>,
    commit_button: gtk::Button,
    refresh_button: gtk::Button,
    history_list_box: gtk::ListBox,
    commit_textview: gtk::TextView,
    stations: Rc<RefCell<Vec<railway::RailwayStation>>>,
    current_index: Rc<RefCell<Option<usize>>>,
}

impl HistoryWindow {
    pub fn new(
        app: &gtk::Application,
        window_manager: Weak<WindowManager>,
        repository_manager: Rc<RepositoryManager>,
    ) -> Rc<HistoryWindow> {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Metal Git")
            .default_width(900)
            .default_height(600)
            .build();

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .build();
        window.set_child(Some(&root));

        let button_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        root.append(&button_row);

        let button_row_spacer = gtk::Box::builder().hexpand(true).build();
        button_row.append(&button_row_spacer);

        let commit_button = gtk::Button::with_label("Commit");
        button_row.append(&commit_button);

        let refresh_button = gtk::Button::with_label("Refresh");
        button_row.append(&refresh_button);

        let main_paned = gtk::Paned::builder()
            .orientation(gtk::Orientation::Vertical)
            .wide_handle(true)
            .build();
        main_paned.set_position(100);
        main_paned.set_resize_start_child(false);
        main_paned.set_shrink_start_child(true);
        main_paned.set_resize_end_child(true);
        main_paned.set_shrink_end_child(true);
        root.append(&main_paned);

        let history_list_box = gtk::ListBox::new();
        history_list_box.set_selection_mode(gtk::SelectionMode::Single);
        history_list_box.set_activate_on_single_click(true);
        history_list_box.add_css_class("history-list");

        let history_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&history_list_box)
            .build();

        let history_header = build_history_header();
        let history_panel = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .hexpand(true)
            .vexpand(true)
            .build();
        history_panel.append(&history_header);
        history_panel.append(&history_scrolled);
        main_paned.set_start_child(Some(&history_panel));

        let commit_notebook = gtk::Notebook::new();
        commit_notebook.set_hexpand(true);
        commit_notebook.set_vexpand(true);
        main_paned.set_end_child(Some(&commit_notebook));

        let commit_textview = gtk::TextView::builder()
            .editable(false)
            .monospace(true)
            .build();
        let commit_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&commit_textview)
            .build();
        commit_notebook.append_page(&commit_scrolled, Some(&gtk::Label::new(Some("Commit"))));

        let commit_diff_panel = CommitDiffPanel::new(Rc::clone(&repository_manager));
        let diff_widget = commit_diff_panel.widget();
        commit_notebook.append_page(&diff_widget, Some(&gtk::Label::new(Some("Diff"))));

        let history_window = Rc::new(HistoryWindow {
            window,
            window_manager,
            repository_manager,
            commit_diff_panel,
            commit_button,
            refresh_button,
            history_list_box,
            commit_textview,
            stations: Rc::new(RefCell::new(Vec::new())),
            current_index: Rc::new(RefCell::new(None)),
        });

        install_history_list_css();
        history_window.setup_history_list();

        let w = Rc::downgrade(&history_window);
        history_window.commit_button.connect_clicked(move |_| {
            w.upgrade().unwrap().commit_button_clicked();
        });

        let w = Rc::downgrade(&history_window);
        history_window.refresh_button.connect_clicked(move |_| {
            w.upgrade().unwrap().refresh_button_clicked();
        });

        history_window
    }

    fn setup_history_list(self: &Rc<Self>) {
        let stations = self.stations.clone();
        let repository_manager = self.repository_manager.clone();
        let commit_textview = self.commit_textview.clone();
        let commit_diff_panel = self.commit_diff_panel.clone();
        let current_index = self.current_index.clone();

        self.history_list_box.connect_row_selected(move |list_box, row| {
            if row.is_none() {
                return;
            }

            update_from_selected_row(
                list_box,
                &stations,
                &repository_manager,
                &commit_textview,
                &commit_diff_panel,
                &current_index,
            );
        });

        let list_box = self.history_list_box.clone();
        let stations = self.stations.clone();
        let repository_manager = self.repository_manager.clone();
        let commit_textview = self.commit_textview.clone();
        let commit_diff_panel = self.commit_diff_panel.clone();
        let current_index = self.current_index.clone();
        self.history_list_box.connect_selected_rows_changed(move |_| {
            update_from_selected_row(
                &list_box,
                &stations,
                &repository_manager,
                &commit_textview,
                &commit_diff_panel,
                &current_index,
            );
        });
    }

    pub fn connect_closed<F>(&self, callback: F)
    where
        F: Fn() + 'static,
    {
        self.window.connect_close_request(move |_| {
            callback();
            Propagation::Proceed
        });
    }

    pub fn show(&self) {
        self.refresh();
        self.window.present();
    }

    fn load_title(&self) -> Result<(), Error> {
        let repo = self.repository_manager.open()?;

        let mut title = String::new();
        if let Ok(reference) = repo.head() {
            if let Some(head_shorthand) = reference.shorthand() {
                title.push('[');
                title.push_str(head_shorthand);
                title.push_str("] ");
            }
        }

        if let Some(path) = repo.workdir().and_then(|x| x.to_str()) {
            title.push('(');
            title.push_str(path);
            title.push_str(") - ");
        }

        title.push_str("Metal Git");
        self.window.set_title(Some(&title));

        Ok(())
    }

    fn load_history(&self) -> Result<(), Error> {
        clear_list_box(&self.history_list_box);

        let stations = railway::collect_tree(&self.repository_manager)?;
        self.stations.replace(stations);
        self.current_index.replace(None);

        for station in self.stations.borrow().iter() {
            let station = station.clone();
            let station_for_row = station.clone();
            let repository_manager = self.repository_manager.clone();
            let commit_textview = self.commit_textview.clone();
            let commit_diff_panel = self.commit_diff_panel.clone();
            self.history_list_box.append(&build_history_row(
                &self.history_list_box,
                &station_for_row,
                move || {
                    let _ = update_selected_commit_widgets(
                        &repository_manager,
                        &commit_textview,
                        &commit_diff_panel,
                        &station,
                    );
                },
            ));
        }

        if let Some(row) = self.history_list_box.row_at_index(0) {
            self.history_list_box.select_row(Some(&row));
        } else {
            self.commit_textview.buffer().set_text("");
        }

        Ok(())
    }

    pub fn refresh(&self) {
        dialog_when_error!("Failed to load repository: {:?}", self.load_title());
        dialog_when_error!("Failed to load history: {:?}", self.load_history());
    }

    fn commit_button_clicked(&self) {
        self.window_manager.upgrade().unwrap().show_commit_window();
    }

    fn refresh_button_clicked(&self) {
        self.refresh();
    }
}

fn update_selected_commit_widgets(
    repository_manager: &Rc<RepositoryManager>,
    commit_textview: &gtk::TextView,
    commit_diff_panel: &Rc<CommitDiffPanel>,
    station: &railway::RailwayStation,
) -> Result<(), git2::Error> {
    let repo = repository_manager.open()?;
    let commit = repo.find_commit(station.oid)?;

    let text = format!(
        "commit {}\nAuthor: {} <{}>\nDate: {}\n\n{}",
        station.oid,
        station.author_name,
        commit.author().email().unwrap_or(""),
        station.time,
        commit.message().unwrap_or("")
    );

    commit_textview.buffer().set_text(&text);
    commit_diff_panel.update_commit(station.oid)?;
    Ok(())
}

fn update_selected_commit_by_index(
    stations: &Rc<RefCell<Vec<railway::RailwayStation>>>,
    repository_manager: &Rc<RepositoryManager>,
    commit_textview: &gtk::TextView,
    commit_diff_panel: &Rc<CommitDiffPanel>,
    index: usize,
) -> Result<(), git2::Error> {
    let Some(station) = stations.borrow().get(index).cloned() else {
        return Ok(());
    };

    update_selected_commit_widgets(
        repository_manager,
        commit_textview,
        commit_diff_panel,
        &station,
    )
}

fn update_from_selected_row(
    list_box: &gtk::ListBox,
    stations: &Rc<RefCell<Vec<railway::RailwayStation>>>,
    repository_manager: &Rc<RepositoryManager>,
    commit_textview: &gtk::TextView,
    commit_diff_panel: &Rc<CommitDiffPanel>,
    current_index: &Rc<RefCell<Option<usize>>>,
) {
    let Some(row) = list_box.selected_row() else {
        return;
    };

    let index = row.index();
    if index >= 0 {
        let index = index as usize;
        if current_index.borrow().as_ref() == Some(&index) {
            return;
        }
        current_index.replace(Some(index));
        let _ = update_selected_commit_by_index(
            stations,
            repository_manager,
            commit_textview,
            commit_diff_panel,
            index,
        );
    }
}

fn install_history_list_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "
        .history-list {
            border-spacing: 0;
            padding: 0;
            margin: 0;
        }

        .history-list row {
            padding-top: 0;
            padding-bottom: 0;
            margin-top: 0;
            margin-bottom: 0;
            min-height: 0;
        }

        .history-list row:selected {
            border-radius: 0;
        }

        .history-header {
            padding: 6px 8px;
            border-bottom: 1px solid alpha(currentColor, 0.18);
            font-weight: bold;
        }
        ",
    );

    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn clear_list_box(list_box: &gtk::ListBox) {
    while let Some(row) = list_box.row_at_index(0) {
        list_box.remove(&row);
    }
}

fn build_history_header() -> gtk::Box {
    let header = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    header.add_css_class("history-header");

    let subject = gtk::Label::builder()
        .label("Subject")
        .xalign(0.0)
        .hexpand(true)
        .build();
    header.append(&subject);

    let author = gtk::Label::builder()
        .label("Author")
        .xalign(0.0)
        .width_chars(16)
        .max_width_chars(24)
        .build();
    header.append(&author);

    let time = gtk::Label::builder()
        .label("Time")
        .xalign(1.0)
        .width_chars(19)
        .build();
    header.append(&time);

    header
}

fn build_history_row(
    list_box: &gtk::ListBox,
    station: &railway::RailwayStation,
    on_pressed: impl Fn() + 'static,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(true);
    row.set_activatable(true);

    let content_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_start(4)
        .margin_end(4)
        .build();
    row.set_child(Some(&content_row));

    let graph = gtk::DrawingArea::builder()
        .content_width(station_renderer::desired_width(station))
        .content_height(station_renderer::desired_height())
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Center)
        .build();
    let station_for_draw = station.clone();
    graph.set_draw_func(move |_, cr, width, height| {
        let _ = station_renderer::render(&station_for_draw, cr, width, height);
    });
    content_row.append(&graph);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    content_row.append(&content);

    let subject = gtk::Label::builder()
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    subject.set_markup(&create_subject_markup(station));
    content.append(&subject);

    let meta = gtk::Label::builder()
        .label(&station.author_name)
        .xalign(0.0)
        .width_chars(16)
        .max_width_chars(24)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    meta.add_css_class("dim-label");
    content.append(&meta);

    let time = gtk::Label::builder()
        .label(&station.time)
        .xalign(1.0)
        .width_chars(19)
        .build();
    time.add_css_class("dim-label");
    content.append(&time);

    let gesture = gtk::GestureClick::new();
    let row_weak = row.downgrade();
    let list_box = list_box.clone();
    gesture.connect_pressed(move |_, _, _, _| {
        if let Some(row) = row_weak.upgrade() {
            list_box.select_row(Some(&row));
        }
        on_pressed();
    });
    content_row.add_controller(gesture);

    row
}

fn create_subject_markup(station: &railway::RailwayStation) -> String {
    let mut markup = String::new();

    for ref_name in &station.ref_names {
        let tag = format!(
            "<span foreground=\"#a00000\"><b>[{}]</b></span>",
            gtk::glib::markup_escape_text(ref_name)
        );
        markup.push_str(&tag);
    }

    if !markup.is_empty() {
        markup.push(' ');
    }

    markup.push_str(&gtk::glib::markup_escape_text(&station.subject));

    markup
}
