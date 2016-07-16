extern crate gtk;
extern crate git2;

use std::rc::{Rc, Weak};
use gtk::prelude::*;
use window_manager::WindowManager;
use repository_manager::RepositoryManager;
use git2::Error;
use railway;
use station_wrapper::StationWrapper;
use gtk_utils;

use glib::object;

pub struct HistoryWindow {
    window: gtk::Window,

    window_manager: Weak<WindowManager>,
    repository_manager: Rc<RepositoryManager>,

    commit_button: gtk::Button,
    refresh_button: gtk::Button,

    history_treeview: gtk::TreeView,
    commit_textview: gtk::TextView,

    history_list_store: gtk::ListStore
}

const COLUMN_STATION: u32 = 0;
const COLUMN_SUBJECT: u32 = 1;
const COLUMN_AUTHOR_NAME: u32 = 2;
const COLUMN_TIME: u32 = 3;

impl HistoryWindow {
    pub fn new(window_manager: Weak<WindowManager>,
               repository_manager: Rc<RepositoryManager>)
               -> Rc<HistoryWindow> {
        let builder = gtk::Builder::new_from_resource("/org/sunnyone/MetalGit/history_window.ui");

        let history_window = HistoryWindow {
            window_manager: window_manager,
            repository_manager: repository_manager,

            window: builder.get_object("history_window").unwrap(),
            commit_button: builder.get_object("commit_button").unwrap(),
            refresh_button: builder.get_object("refresh_button").unwrap(),
            history_treeview: builder.get_object("history_treeview").unwrap(),

            commit_textview: builder.get_object("commit_textview").unwrap(),
            
            history_list_store: gtk::ListStore::new(&[
                object::Object::static_type(),
                String::static_type(),
                String::static_type(),
                String::static_type()]),
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

        let w = Rc::downgrade(&history_window);
        history_window.history_treeview.get_selection().connect_changed(move |selection| {
            dialog_when_error!("Failed to get commit info: {:?}",
                               w.upgrade().unwrap().history_tree_selected(&selection));
        });
        
        history_window
    }

    fn setup_history_tree(treeview: &gtk::TreeView, store: &gtk::ListStore) {
        treeview.set_model(Some(store));

        let subject_renderer = ::station_cell_renderer::StationCellRenderer::new();
        let col = gtk::TreeViewColumn::new();
        col.set_title("Subject");
        col.pack_start(&subject_renderer, false);
        col.add_attribute(&subject_renderer, "station", COLUMN_STATION as i32);
        col.add_attribute(&subject_renderer, "markup", COLUMN_SUBJECT as i32);
        treeview.append_column(&col);
        
        let renderer = gtk::CellRendererText::new();
        let col = gtk::TreeViewColumn::new();
        col.set_title("Author");
        col.pack_start(&renderer, false);
        col.add_attribute(&renderer, "text", COLUMN_AUTHOR_NAME as i32);
        treeview.append_column(&col);
        
        let renderer = gtk::CellRendererText::new();
        let col = gtk::TreeViewColumn::new();
        col.set_title("Time");
        col.pack_start(&renderer, false);
        col.add_attribute(&renderer, "text", COLUMN_TIME as i32);
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
        let repo = try!(self.repository_manager.open());

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

        let stations = try!(railway::collect_tree(&self.repository_manager));
        for station in stations {
            let subject = Self::create_subject_markup(&station);
            let author_name = station.author_name.clone();
            let time = station.time.clone();
            
            let mut station_wrapper = StationWrapper::new();
            station_wrapper.set_station(station);
            
            self.history_list_store
                .insert_with_values(None,
                                    &[COLUMN_STATION,
                                      COLUMN_SUBJECT,
                                      COLUMN_AUTHOR_NAME,
                                      COLUMN_TIME],
                                    &[&station_wrapper,
                                      &subject,
                                      &author_name,
                                      &time]);
        }

        Ok(())
    }

    fn create_subject_markup(station: &railway::RailwayStation) -> String {
        let mut markup = String::new();

        for ref_name in &station.ref_names {
            let tag = format!("<span foreground=\"#a00000\"><b>[{}]</b></span>",
                              &gtk_utils::escape_markup_text(&ref_name));
            markup.push_str(&tag);
        }

        markup.push(' ');
        markup.push_str(&gtk_utils::escape_markup_text(&station.subject));

        markup
    }

    pub fn refresh(&self) {
        dialog_when_error!("Failed to load repository: {:?}", self.load_title());
        dialog_when_error!("Failed to load history: {:?}", self.load_history());
    }

    fn get_selected_station_wrapper(&self, selection: &gtk::TreeSelection) -> Option<StationWrapper> {
        let (tree_paths, model) = selection.get_selected_rows();
        let station_wrapper = tree_paths.iter()
                  .flat_map(|tree_path|
                      self.history_list_store.get_iter(tree_path)
                  ).map(|iter| {
                      let value = self.history_list_store.get_value(&iter, COLUMN_STATION as i32);
                      let obj = value.get::<object::Object>().unwrap();
                      let station_wrapper = obj.downcast::<StationWrapper>().unwrap();
                      station_wrapper }).next();
                  
        station_wrapper
    }

    fn history_tree_selected(&self, selection: &gtk::TreeSelection) -> Result<(), Error> {
        let buffer = self.commit_textview.get_buffer().unwrap();

        let station_wrapper = self.get_selected_station_wrapper(selection);
        if let Some(station) = station_wrapper.map(|x| x.get_station()) {
            let mut text = String::new();
            text.push_str(&format!("Author: {} ({})\n", station.author_name, station.author_email));
            text.push_str(&format!("Date: {}\n", station.time));
            text.push_str(&format!("Commit Hash: {}\n\n", station.oid));
            
            text.push_str(&station.message);
            
            buffer.set_text(&text);
        }
        
        Ok(())    
    }
    
    fn commit_button_clicked(&self) {
        self.window_manager.upgrade().unwrap().show_commit_window();
    }

    fn refresh_button_clicked(&self) {
        self.refresh();
    }
}
