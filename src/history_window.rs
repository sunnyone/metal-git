use std::rc::{Rc, Weak};
use crate::window_manager::WindowManager;
use crate::repository_manager::RepositoryManager;
use git2::Error;
use crate::railway;
use crate::station_wrapper::StationWrapper;
use crate::station_cell_renderer::StationCellRenderer;

use gtk::Inhibit;
use gtk::prelude::{BuilderExtManual, GtkListStoreExtManual};
use gtk::traits::{ButtonExt, GtkListStoreExt, TreeViewColumnExt, TreeViewExt, WidgetExt, GtkWindowExt};

pub struct HistoryWindow {
    window: gtk::Window,

    window_manager: Weak<WindowManager>,
    repository_manager: Rc<RepositoryManager>,

    commit_button: gtk::Button,
    refresh_button: gtk::Button,

    history_treeview: gtk::TreeView,

    history_list_store: gtk::ListStore,
}

const COLUMN_SUBJECT: u32 = 0;
const COLUMN_STATION: u32 = 1;

impl HistoryWindow {
    pub fn new(window_manager: Weak<WindowManager>,
               repository_manager: Rc<RepositoryManager>)
               -> Rc<HistoryWindow> {
        let builder = gtk::Builder::from_resource("/org/sunnyone/MetalGit/history_window.ui");

        let col_types = [gtk::glib::types::Type::STRING, gtk::glib::types::Type::OBJECT];

        let history_window = HistoryWindow {
            window_manager: window_manager,
            repository_manager: repository_manager,

            window: builder.object("history_window").unwrap(),
            commit_button: builder.object("commit_button").unwrap(),
            refresh_button: builder.object("refresh_button").unwrap(),
            history_treeview: builder.object("history_treeview").unwrap(),

            history_list_store: gtk::ListStore::new(&col_types),
        };

        Self::setup_history_tree(&history_window.history_treeview,
                                 &history_window.history_list_store);

        let history_window = Rc::new(history_window);

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

    fn setup_history_tree(treeview: &gtk::TreeView, store: &gtk::ListStore) {
        treeview.set_model(Some(store));

        let subject_renderer = StationCellRenderer::new();
        let col = gtk::TreeViewColumn::new();
        col.set_title("Subject");
        col.pack_start(&subject_renderer, false);
        col.add_attribute(&subject_renderer, "markup", COLUMN_SUBJECT as i32);
        col.add_attribute(&subject_renderer, "station", COLUMN_STATION as i32);
        treeview.append_column(&col);
    }

    pub fn connect_closed<F>(&self, callback: F)
        where F: Fn() -> () + 'static
    {
        self.window.connect_delete_event(move |_, _| {
            callback();
            Inhibit(false)
        });
    }

    pub fn show(&self) {
        self.refresh();
        self.window.show_all();
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

        self.window.set_title(&title);

        Ok(())
    }

    fn load_history(&self) -> Result<(), Error> {
        self.history_list_store.clear();

        let stations = railway::collect_tree(&self.repository_manager)?;
        for station in stations {
            let subject = Self::create_subject_markup(&station);
            let mut station_wrapper = StationWrapper::new();
            station_wrapper.set_station(station);

            self.history_list_store
                .insert_with_values(None,
                                    &[(COLUMN_SUBJECT, &subject),
                                        (COLUMN_STATION, &station_wrapper)]);
        }

        Ok(())
    }

    fn create_subject_markup(station: &railway::RailwayStation) -> String {
        let mut markup = String::new();

        for ref_name in &station.ref_names {
            let tag = format!("<span foreground=\"#a00000\"><b>[{}]</b></span>",
                gtk::glib::markup_escape_text(&ref_name));
            markup.push_str(&tag);
        }

        markup.push(' ');
        markup.push_str(&gtk::glib::markup_escape_text(&station.subject));

        markup
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
