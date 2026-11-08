use crate::background_task::TaskRunner;
use crate::branch::BranchCandidate;
use crate::branch_context_menu::BranchContextMenu;
use crate::commit_diff_panel::CommitDiffPanel;
use crate::fetch_operation::FetchOperation;
use crate::gtk_utils;
use crate::long_running_operation::{self, LongRunningStatus, LongRunningStatusSender};
use crate::pull_operation::PullOperation;
use crate::railway;
use crate::repository_manager::RepositoryManager;
use crate::station_renderer;
use crate::switch_branch_operation::SwitchBranchOperation;
use crate::status_bar::StatusBar;
use crate::window_manager::WindowManager;
use git2::Error;
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::glib::BoxedAnyObject;
use gtk::glib::Propagation;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::time::Duration;

// How long a closing window waits for a running task to clean up
const TASK_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

pub struct HistoryWindow {
    window: gtk::ApplicationWindow,
    window_manager: Weak<WindowManager>,
    repository_manager: Rc<RepositoryManager>,
    commit_diff_panel: Rc<CommitDiffPanel>,
    status_bar: Rc<StatusBar>,
    task_runner: Rc<TaskRunner>,
    status_sender: LongRunningStatusSender,
    fetch_button: gtk::Button,
    pull_button: gtk::Button,
    commit_button: gtk::Button,
    refresh_button: gtk::Button,
    history_list_view: gtk::ListView,
    // Items are BoxedAnyObject(usize) row indexes into `graph`.
    history_store: gio::ListStore,
    history_selection: gtk::SingleSelection,
    branch_context_menu: Rc<BranchContextMenu>,
    commit_textview: gtk::TextView,
    graph: Rc<RefCell<Rc<railway::RailwayGraph>>>,
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

        let fetch_button = gtk_utils::icon_button("metal-git-fetch-symbolic", "Fetch");
        button_row.append(&fetch_button);

        let pull_button = gtk_utils::icon_button("metal-git-pull-symbolic", "Pull");
        button_row.append(&pull_button);

        let commit_button = gtk_utils::icon_button("metal-git-commit-symbolic", "Commit");
        button_row.append(&commit_button);

        let refresh_button = gtk_utils::icon_button("metal-git-refresh-symbolic", "Refresh");
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

        // ListView only creates widgets for the visible rows, so startup time
        // does not grow with the number of commits.
        let history_store = gio::ListStore::new::<BoxedAnyObject>();
        let history_selection = gtk::SingleSelection::new(Some(history_store.clone()));
        history_selection.set_autoselect(false);
        let history_list_view =
            gtk::ListView::new(Some(history_selection.clone()), None::<gtk::ListItemFactory>);
        history_list_view.add_css_class("history-list");

        let history_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&history_list_view)
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

        let status_bar = StatusBar::new();
        root.append(&status_bar.widget());

        let (status_sender, mut status_receiver) = long_running_operation::status_channel();

        let task_runner = TaskRunner::new();
        let history_window = Rc::new_cyclic(|weak: &Weak<HistoryWindow>| {
            let weak = weak.clone();
            let busy_task_runner = task_runner.clone();
            let branch_context_menu = BranchContextMenu::new(
                &history_panel,
                repository_manager.clone(),
                move || busy_task_runner.is_running(),
                move |candidate| {
                    if let Some(history_window) = weak.upgrade() {
                        history_window.switch_branch(candidate);
                    }
                },
            );

            HistoryWindow {
                window,
                window_manager,
                repository_manager,
                commit_diff_panel,
                status_bar,
                task_runner,
                status_sender,
                fetch_button,
                pull_button,
                commit_button,
                refresh_button,
                history_list_view,
                history_store,
                history_selection,
                branch_context_menu,
                commit_textview,
                graph: Rc::new(RefCell::new(Rc::new(railway::RailwayGraph::default()))),
                current_index: Rc::new(RefCell::new(None)),
            }
        });

        let task_runner = history_window.task_runner.clone();
        history_window.status_bar.connect_cancel(move || task_runner.cancel());

        let w = Rc::downgrade(&history_window);
        glib::spawn_future_local(async move {
            while let Some(status) = status_receiver.next().await {
                let Some(w) = w.upgrade() else { return };
                w.on_status(status);
            }
        });

        install_history_list_css();
        history_window.setup_history_list();

        let w = Rc::downgrade(&history_window);
        history_window.fetch_button.connect_clicked(move |_| {
            w.upgrade().unwrap().fetch_button_clicked();
        });

        let w = Rc::downgrade(&history_window);
        history_window.pull_button.connect_clicked(move |_| {
            w.upgrade().unwrap().pull_button_clicked();
        });

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
        let factory = gtk::SignalListItemFactory::new();

        let branch_context_menu = Rc::downgrade(&self.branch_context_menu);
        factory.connect_setup(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let row = HistoryRow::new();
            if let Some(branch_context_menu) = branch_context_menu.upgrade() {
                let row_weak = row.downgrade();
                branch_context_menu.attach(&row, move || {
                    row_weak.upgrade().and_then(|row| row.commit_id())
                });
            }
            list_item.set_child(Some(&row));
        });

        let graph = self.graph.clone();
        factory.connect_bind(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let station = list_item
                .item()
                .and_downcast::<BoxedAnyObject>()
                .and_then(|index| graph.borrow().station(*index.borrow::<usize>()));
            if let Some(row) = list_item.child().and_downcast::<HistoryRow>() {
                row.bind(station);
            }
        });

        factory.connect_unbind(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            if let Some(row) = list_item.child().and_downcast::<HistoryRow>() {
                row.bind(None);
            }
        });

        self.history_list_view.set_factory(Some(&factory));

        let graph = self.graph.clone();
        let repository_manager = self.repository_manager.clone();
        let commit_textview = self.commit_textview.clone();
        let commit_diff_panel = self.commit_diff_panel.clone();
        let current_index = self.current_index.clone();
        self.history_selection.connect_selected_notify(move |selection| {
            update_from_selection(
                selection,
                &graph,
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
        let callback = Rc::new(callback);
        let task_runner = self.task_runner.clone();
        self.window.connect_close_request(move |window| {
            if !task_runner.is_running() {
                callback();
                return Propagation::Proceed;
            }

            window.set_visible(false);
            let callback = callback.clone();
            let task_runner = task_runner.clone();
            glib::spawn_future_local(async move {
                task_runner.shutdown(TASK_SHUTDOWN_TIMEOUT).await;
                callback();
            });
            Propagation::Stop
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
            if let Ok(head_shorthand) = reference.shorthand() {
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
        self.history_store.remove_all();

        let graph = railway::collect_tree(&self.repository_manager)?;
        self.graph.replace(Rc::new(graph));
        self.current_index.replace(None);

        let items = (0..self.graph.borrow().len())
            .map(BoxedAnyObject::new)
            .collect::<Vec<_>>();
        self.history_store.splice(0, 0, &items);

        if self.history_store.n_items() > 0 {
            self.history_selection.set_selected(0);
        } else {
            self.commit_textview.buffer().set_text("");
        }

        Ok(())
    }

    pub fn refresh(&self) {
        dialog_when_error!("Failed to load repository: {:?}", self.load_title());
        dialog_when_error!("Failed to load history: {:?}", self.load_history());
        self.status_bar.set_message(&format!("{} commits", self.graph.borrow().len()));
    }

    fn git_dir(&self) -> Option<PathBuf> {
        match self.repository_manager.open() {
            Ok(repo) => Some(repo.path().to_path_buf()),
            Err(err) => {
                gtk_utils::message_box_error(&format!("Failed to open repository: {}", err.message()));
                None
            }
        }
    }

    fn fetch_button_clicked(&self) {
        if let Some(git_dir) = self.git_dir() {
            long_running_operation::start_operation(
                &self.task_runner,
                FetchOperation { git_dir },
                &self.status_sender,
            );
        }
    }

    fn pull_button_clicked(&self) {
        if let Some(git_dir) = self.git_dir() {
            long_running_operation::start_operation(
                &self.task_runner,
                PullOperation { git_dir },
                &self.status_sender,
            );
        }
    }

    fn switch_branch(&self, candidate: BranchCandidate) {
        if let Some(git_dir) = self.git_dir() {
            long_running_operation::start_operation(
                &self.task_runner,
                SwitchBranchOperation { git_dir, candidate },
                &self.status_sender,
            );
        }
    }

    fn on_status(&self, status: LongRunningStatus) {
        self.fetch_button.set_sensitive(!status.is_running());
        self.pull_button.set_sensitive(!status.is_running());
        self.commit_button.set_sensitive(!status.is_running());
        // refresh shows its own message, so it must come before the status
        if status.reload_history() {
            self.refresh();
        }
        if let LongRunningStatus::Failed { message } = &status {
            gtk_utils::message_box_error(message);
        }
        self.status_bar.apply(&status);
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
    station: &railway::RailwayCommit,
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
    graph: &Rc<RefCell<Rc<railway::RailwayGraph>>>,
    repository_manager: &Rc<RepositoryManager>,
    commit_textview: &gtk::TextView,
    commit_diff_panel: &Rc<CommitDiffPanel>,
    index: usize,
) -> Result<(), git2::Error> {
    let Some(station) = graph.borrow().commit(index).cloned() else {
        return Ok(());
    };

    update_selected_commit_widgets(
        repository_manager,
        commit_textview,
        commit_diff_panel,
        &station,
    )
}

fn update_from_selection(
    selection: &gtk::SingleSelection,
    graph: &Rc<RefCell<Rc<railway::RailwayGraph>>>,
    repository_manager: &Rc<RepositoryManager>,
    commit_textview: &gtk::TextView,
    commit_diff_panel: &Rc<CommitDiffPanel>,
    current_index: &Rc<RefCell<Option<usize>>>,
) {
    let selected = selection.selected();
    if selected == gtk::INVALID_LIST_POSITION {
        return;
    }

    let index = selected as usize;
    if current_index.borrow().as_ref() == Some(&index) {
        return;
    }
    current_index.replace(Some(index));
    let _ = update_selected_commit_by_index(
        graph,
        repository_manager,
        commit_textview,
        commit_diff_panel,
        index,
    );
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

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct HistoryRow {
        pub graph: gtk::DrawingArea,
        pub subject: gtk::Label,
        pub author: gtk::Label,
        pub time: gtk::Label,
        pub station: Rc<RefCell<Option<railway::RailwayStation>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for HistoryRow {
        const NAME: &'static str = "MetalGitHistoryRow";
        type Type = super::HistoryRow;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for HistoryRow {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();
            obj.set_orientation(gtk::Orientation::Horizontal);
            obj.set_spacing(8);
            obj.set_margin_start(4);
            obj.set_margin_end(4);

            self.graph.set_content_height(station_renderer::desired_height());
            self.graph.set_halign(gtk::Align::Start);
            self.graph.set_valign(gtk::Align::Center);
            let station_for_draw = self.station.clone();
            self.graph.set_draw_func(move |_, cr, width, height| {
                if let Some(station) = station_for_draw.borrow().as_ref() {
                    let _ = station_renderer::render(station, cr, width, height);
                }
            });
            obj.append(&self.graph);

            let content = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(12)
                .hexpand(true)
                .valign(gtk::Align::Center)
                .build();
            obj.append(&content);

            self.subject.set_xalign(0.0);
            self.subject.set_hexpand(true);
            self.subject.set_ellipsize(gtk::pango::EllipsizeMode::End);
            content.append(&self.subject);

            self.author.set_xalign(0.0);
            self.author.set_width_chars(16);
            self.author.set_max_width_chars(24);
            self.author.set_ellipsize(gtk::pango::EllipsizeMode::End);
            self.author.add_css_class("dim-label");
            content.append(&self.author);

            self.time.set_xalign(1.0);
            self.time.set_width_chars(19);
            self.time.add_css_class("dim-label");
            content.append(&self.time);
        }
    }

    impl WidgetImpl for HistoryRow {}
    impl BoxImpl for HistoryRow {}
}

glib::wrapper! {
    /// One history row. ListView reuses it for whichever station is scrolled
    /// into view, so the station is replaced on every bind.
    pub struct HistoryRow(ObjectSubclass<imp::HistoryRow>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl HistoryRow {
    fn new() -> HistoryRow {
        glib::Object::new()
    }

    fn commit_id(&self) -> Option<git2::Oid> {
        self.imp().station.borrow().as_ref().map(|station| station.oid)
    }

    fn bind(&self, station: Option<railway::RailwayStation>) {
        let imp = self.imp();
        match station.as_ref() {
            Some(station) => {
                imp.graph
                    .set_content_width(station_renderer::desired_width_for_lanes(station.tracks.len()));
                imp.subject
                    .set_markup(&create_subject_markup(&station.ref_names, &station.subject));
                imp.author.set_label(&station.author_name);
                imp.time.set_label(&station.time);
            }
            None => {
                imp.subject.set_label("");
                imp.author.set_label("");
                imp.time.set_label("");
            }
        }
        imp.station.replace(station);
        imp.graph.queue_draw();
    }
}

fn create_subject_markup(ref_names: &[String], subject: &str) -> String {
    let mut markup = String::new();

    for ref_name in ref_names {
        let tag = format!(
            "<span foreground=\"#a00000\"><b>[{}]</b></span>",
            gtk::glib::markup_escape_text(ref_name)
        );
        markup.push_str(&tag);
    }

    if !markup.is_empty() {
        markup.push(' ');
    }

    markup.push_str(&gtk::glib::markup_escape_text(subject));

    markup
}

#[cfg(test)]
#[path = "history_window_tests.rs"]
mod tests;
