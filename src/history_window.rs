use crate::commit_diff_panel::CommitDiffPanel;
use crate::gtk_utils;
use crate::railway;
use crate::repository_manager::RepositoryManager;
use crate::station_renderer;
use crate::window_manager::WindowManager;
use git2::Error;
use gtk::gdk;
use gtk::gio;
use gtk::glib::BoxedAnyObject;
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
    history_list_view: gtk::ListView,
    // Items are BoxedAnyObject(usize) row indexes into `graph`.
    history_store: gio::ListStore,
    history_selection: gtk::SingleSelection,
    history_rows: Rc<RefCell<Vec<Rc<HistoryRow>>>>,
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

        let history_window = Rc::new(HistoryWindow {
            window,
            window_manager,
            repository_manager,
            commit_diff_panel,
            commit_button,
            refresh_button,
            history_list_view,
            history_store,
            history_selection,
            history_rows: Rc::new(RefCell::new(Vec::new())),
            commit_textview,
            graph: Rc::new(RefCell::new(Rc::new(railway::RailwayGraph::default()))),
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
        let factory = gtk::SignalListItemFactory::new();

        let rows = self.history_rows.clone();
        factory.connect_setup(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let row = HistoryRow::new();
            list_item.set_child(Some(&row.content));
            rows.borrow_mut().push(row);
        });

        let rows = self.history_rows.clone();
        let graph = self.graph.clone();
        factory.connect_bind(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let station = list_item
                .item()
                .and_downcast::<BoxedAnyObject>()
                .and_then(|index| graph.borrow().station(*index.borrow::<usize>()));
            if let Some(row) = find_history_row(&rows, list_item) {
                row.bind(station);
            }
        });

        let rows = self.history_rows.clone();
        factory.connect_unbind(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            if let Some(row) = find_history_row(&rows, list_item) {
                row.bind(None);
            }
        });

        let rows = self.history_rows.clone();
        factory.connect_teardown(move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            if let Some(child) = list_item.child() {
                rows.borrow_mut().retain(|row| row.content.upcast_ref::<gtk::Widget>() != &child);
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

fn find_history_row(
    rows: &Rc<RefCell<Vec<Rc<HistoryRow>>>>,
    list_item: &gtk::ListItem,
) -> Option<Rc<HistoryRow>> {
    let child = list_item.child()?;
    rows.borrow()
        .iter()
        .find(|row| row.content.upcast_ref::<gtk::Widget>() == &child)
        .cloned()
}

/// Widgets of one history row. ListView reuses them for whichever station is
/// scrolled into view, so the station is replaced on every bind.
struct HistoryRow {
    content: gtk::Box,
    graph: gtk::DrawingArea,
    subject: gtk::Label,
    author: gtk::Label,
    time: gtk::Label,
    station: Rc<RefCell<Option<railway::RailwayStation>>>,
}

impl HistoryRow {
    fn new() -> Rc<HistoryRow> {
        let station: Rc<RefCell<Option<railway::RailwayStation>>> = Rc::new(RefCell::new(None));

        let content_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .margin_start(4)
            .margin_end(4)
            .build();

        let graph = gtk::DrawingArea::builder()
            .content_height(station_renderer::desired_height())
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Center)
            .build();
        let station_for_draw = station.clone();
        graph.set_draw_func(move |_, cr, width, height| {
            if let Some(station) = station_for_draw.borrow().as_ref() {
                let _ = station_renderer::render(station, cr, width, height);
            }
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
        content.append(&subject);

        let author = gtk::Label::builder()
            .xalign(0.0)
            .width_chars(16)
            .max_width_chars(24)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        author.add_css_class("dim-label");
        content.append(&author);

        let time = gtk::Label::builder()
            .xalign(1.0)
            .width_chars(19)
            .build();
        time.add_css_class("dim-label");
        content.append(&time);

        Rc::new(HistoryRow {
            content: content_row,
            graph,
            subject,
            author,
            time,
            station,
        })
    }

    fn bind(&self, station: Option<railway::RailwayStation>) {
        match station.as_ref() {
            Some(station) => {
                self.graph
                    .set_content_width(station_renderer::desired_width_for_lanes(station.tracks.len()));
                self.subject
                    .set_markup(&create_subject_markup(&station.ref_names, &station.subject));
                self.author.set_label(&station.author_name);
                self.time.set_label(&station.time);
            }
            None => {
                self.subject.set_label("");
                self.author.set_label("");
                self.time.set_label("");
            }
        }
        self.station.replace(station);
        self.graph.queue_draw();
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
