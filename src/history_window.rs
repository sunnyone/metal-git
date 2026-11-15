use crate::app_settings;
use crate::background_task::TaskRunner;
use crate::branch::{BranchCandidate, BranchKind};
use crate::branch_context_menu::BranchContextMenu;
use crate::branch_tree::{self, BranchTreeKind, BranchTreeNode};
use crate::commit_diff_panel::CommitDiffPanel;
use crate::fetch_operation::FetchOperation;
use crate::gtk_utils;
use crate::long_running_operation::{self, LongRunningStatus, LongRunningStatusSender};
use crate::pull_operation::PullOperation;
use crate::push_dialog::PushDialog;
use crate::push_operation::PushOperation;
use crate::remote_operation::{self, PullButtonState, PushButtonState, PushRequest};
use crate::railway::{self, HistoryScope};
use crate::repository_manager::RepositoryManager;
use crate::station_renderer;
use crate::switch_branch_operation::SwitchBranchOperation;
use crate::status_bar::StatusBar;
use crate::window_manager::WindowManager;
use crate::work_tree_status;
use git2::Error;
use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::glib::BoxedAnyObject;
use gtk::glib::Propagation;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::time::Duration;

// How long a closing window waits for a running task to clean up
const TASK_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

const TOGGLE_BRANCH_TREE_ACTION: &str = "toggle-branch-tree";
const BRANCH_TREE_WIDTH: i32 = 220;

pub struct HistoryWindow {
    window: gtk::ApplicationWindow,
    window_manager: Weak<WindowManager>,
    repository_manager: Rc<RepositoryManager>,
    commit_diff_panel: Rc<CommitDiffPanel>,
    status_bar: Rc<StatusBar>,
    task_runner: Rc<TaskRunner>,
    status_sender: LongRunningStatusSender,
    fetch_button: gtk::Button,
    pull_button: gtk_utils::BadgeButton,
    // False when the pull button could not do anything, like on a branch without an upstream
    pull_available: Cell<bool>,
    push_button: gtk_utils::BadgeButton,
    // False when there is nothing the push button could do, like on a detached HEAD
    push_available: Cell<bool>,
    commit_button: gtk_utils::BadgeButton,
    refresh_button: gtk::Button,
    history_scope: RefCell<HistoryScope>,
    branch_tree_paned: gtk::Paned,
    branch_tree_scrolled: gtk::ScrolledWindow,
    // Where the divider goes back to when the hidden tree is shown again
    branch_tree_position: Cell<i32>,
    // Items are BoxedAnyObject(Rc<BranchTreeNode>), the rows at the top level
    branch_tree_store: gio::ListStore,
    branch_tree_model: gtk::TreeListModel,
    branch_tree_selection: gtk::SingleSelection,
    branch_tree_view: gtk::ListView,
    // True while the tree is rebuilt, so that its selection does not change the scope
    updating_branch_tree: Cell<bool>,
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

        let branch_tree_visible = app_settings::load_branch_tree_visible(&app_settings::branch_tree_visible_path());
        let toggle_branch_tree_action =
            gio::SimpleAction::new_stateful(TOGGLE_BRANCH_TREE_ACTION, None, &branch_tree_visible.to_variant());
        window.add_action(&toggle_branch_tree_action);
        app.set_accels_for_action(&format!("win.{}", TOGGLE_BRANCH_TREE_ACTION), &["F9"]);

        // Icon only, so that it takes as little of the toolbar as a sidebar toggle usually does
        let branch_tree_toggle = gtk::ToggleButton::builder()
            .icon_name("metal-git-sidebar-symbolic")
            .tooltip_text("Branch tree (F9)")
            .action_name(format!("win.{}", TOGGLE_BRANCH_TREE_ACTION))
            .build();
        button_row.append(&branch_tree_toggle);

        let button_row_spacer = gtk::Box::builder().hexpand(true).build();
        button_row.append(&button_row_spacer);

        let fetch_button = gtk_utils::icon_button("metal-git-fetch-symbolic", "Fetch");
        button_row.append(&fetch_button);

        // The badge shows how many commits there are to pull as of the last fetch, like `↓3`
        let pull_button = gtk_utils::BadgeButton::new("metal-git-pull-symbolic", "Pull");
        button_row.append(pull_button.button());

        // The badge shows how many commits there are to push, like `↑3`
        // (`↑+` when the branch has no upstream yet)
        let push_button = gtk_utils::BadgeButton::new("metal-git-push-symbolic", "Push");
        button_row.append(push_button.button());

        // The badge shows how many files have changes to commit
        let commit_button = gtk_utils::BadgeButton::new("metal-git-commit-symbolic", "Commit");
        button_row.append(commit_button.button());

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

        let branch_tree_store = gio::ListStore::new::<BoxedAnyObject>();
        let branch_tree_model = gtk::TreeListModel::new(branch_tree_store.clone(), false, false, |item| {
            let node = item.downcast_ref::<BoxedAnyObject>()?.borrow::<Rc<BranchTreeNode>>().clone();
            if node.children.is_empty() {
                return None;
            }
            let children = gio::ListStore::new::<BoxedAnyObject>();
            for child in &node.children {
                children.append(&BoxedAnyObject::new(Rc::new(child.clone())));
            }
            Some(children.upcast())
        });
        let branch_tree_selection = gtk::SingleSelection::new(Some(branch_tree_model.clone()));
        branch_tree_selection.set_autoselect(false);
        let branch_tree_view =
            gtk::ListView::new(Some(branch_tree_selection.clone()), None::<gtk::ListItemFactory>);
        branch_tree_view.add_css_class("branch-tree");
        let branch_tree_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .min_content_width(120)
            .child(&branch_tree_view)
            .build();
        branch_tree_scrolled.set_visible(branch_tree_visible);

        // The tree cannot be dragged closed: it is hidden with the toggle button or F9
        let branch_tree_paned = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .wide_handle(true)
            .build();
        branch_tree_paned.set_start_child(Some(&branch_tree_scrolled));
        branch_tree_paned.set_end_child(Some(&main_paned));
        branch_tree_paned.set_position(BRANCH_TREE_WIDTH);
        branch_tree_paned.set_resize_start_child(false);
        branch_tree_paned.set_shrink_start_child(false);
        branch_tree_paned.set_resize_end_child(true);
        branch_tree_paned.set_shrink_end_child(false);
        root.append(&branch_tree_paned);

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
                pull_available: Cell::new(true),
                push_button,
                push_available: Cell::new(true),
                commit_button,
                refresh_button,
                history_scope: RefCell::new(app_settings::load_history_scope(&app_settings::history_scope_path())),
                branch_tree_paned,
                branch_tree_scrolled,
                branch_tree_position: Cell::new(BRANCH_TREE_WIDTH),
                branch_tree_store,
                branch_tree_model,
                branch_tree_selection,
                branch_tree_view,
                updating_branch_tree: Cell::new(false),
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

        history_window.setup_branch_tree();

        let w = Rc::downgrade(&history_window);
        toggle_branch_tree_action.connect_change_state(move |action, state| {
            let Some(visible) = state.and_then(|state| state.get::<bool>()) else { return };
            action.set_state(&visible.to_variant());
            if let Some(w) = w.upgrade() {
                w.set_branch_tree_visible(visible);
            }
        });

        // Children like the history list get the key first, so this only sees an unused Escape
        let key_controller = gtk::EventControllerKey::new();
        let w = Rc::downgrade(&history_window);
        key_controller.connect_key_pressed(move |_, key, _, _| {
            if key != gdk::Key::Escape {
                return Propagation::Proceed;
            }
            if let Some(w) = w.upgrade() {
                w.show_all_branches();
            }
            Propagation::Stop
        });
        history_window.window.add_controller(key_controller);

        let w = Rc::downgrade(&history_window);
        history_window.fetch_button.connect_clicked(move |_| {
            w.upgrade().unwrap().fetch_button_clicked();
        });

        let w = Rc::downgrade(&history_window);
        history_window.pull_button.button().connect_clicked(move |_| {
            w.upgrade().unwrap().pull_button_clicked();
        });

        let w = Rc::downgrade(&history_window);
        history_window.push_button.button().connect_clicked(move |_| {
            w.upgrade().unwrap().push_button_clicked();
        });

        let w = Rc::downgrade(&history_window);
        history_window.commit_button.button().connect_clicked(move |_| {
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

    fn setup_branch_tree(self: &Rc<Self>) {
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::builder().xalign(0.0).build();
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let expander = gtk::TreeExpander::new();
            expander.set_child(Some(&label));
            list_item.set_child(Some(&expander));
        });
        factory.connect_bind(|_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            let Some(expander) = list_item.child().and_downcast::<gtk::TreeExpander>() else { return };
            let row = list_item.item().and_downcast::<gtk::TreeListRow>();
            expander.set_list_row(row.as_ref());
            let Some(node) = row.as_ref().and_then(branch_tree_node) else { return };
            if let Some(label) = expander.child().and_downcast::<gtk::Label>() {
                label.set_label(&node.label);
                label.set_css_classes(if node.kind == BranchTreeKind::Header { &["branch-tree-header"] } else { &[] });
            }
            // A line between the current branch and the references below it
            if node.kind == BranchTreeKind::CurrentBranch {
                expander.add_css_class("branch-tree-last-scope");
            } else {
                expander.remove_css_class("branch-tree-last-scope");
            }
        });
        self.branch_tree_view.set_factory(Some(&factory));

        let w = Rc::downgrade(self);
        self.branch_tree_selection.connect_selected_notify(move |selection| {
            let Some(w) = w.upgrade() else { return };
            if w.updating_branch_tree.get() {
                return;
            }
            if let Some(node) = selection.selected_item().and_downcast::<gtk::TreeListRow>().as_ref().and_then(branch_tree_node) {
                w.set_history_scope(node.scope.clone());
            }
        });

        // A double click, or Enter, checks out a branch
        let w = Rc::downgrade(self);
        self.branch_tree_view.connect_activate(move |_, position| {
            let Some(w) = w.upgrade() else { return };
            let Some(node) = w.branch_tree_model.row(position).as_ref().and_then(branch_tree_node) else { return };
            w.check_out_branch_tree_node(&node);
        });
    }

    /// Rebuilds the tree from the references, keeping the folders that were
    /// collapsed, and selects the row of the history scope again. A scope whose
    /// reference is gone goes back to all branches.
    fn load_branch_tree(&self) -> Result<(), Error> {
        let repo = self.repository_manager.open()?;
        let nodes = branch_tree::collect_branch_tree(&repo)?;

        let collapsed = (0..self.branch_tree_model.n_items())
            .filter_map(|position| self.branch_tree_model.row(position))
            .filter(|row| row.is_expandable() && !row.is_expanded())
            .filter_map(|row| branch_tree_node(&row).map(|node| node.scope.clone()))
            .collect::<HashSet<_>>();

        if !branch_tree::contains_scope(&nodes, &self.history_scope.borrow()) {
            self.history_scope.replace(HistoryScope::AllBranches);
        }

        self.updating_branch_tree.set(true);
        let items = nodes
            .into_iter()
            .map(|node| BoxedAnyObject::new(Rc::new(node)))
            .collect::<Vec<_>>();
        self.branch_tree_store.splice(0, self.branch_tree_store.n_items(), &items);

        // Rows of an expanded folder come right after it, so they are visited next
        let scope = self.history_scope.borrow().clone();
        let mut selected = gtk::INVALID_LIST_POSITION;
        let mut position = 0;
        while let Some(row) = self.branch_tree_model.row(position) {
            if let Some(node) = branch_tree_node(&row) {
                if node.scope == scope {
                    selected = position;
                }
                let holds_scope = scope_is_inside(&node.scope, &scope);
                if row.is_expandable() && (holds_scope || !collapsed.contains(&node.scope)) {
                    row.set_expanded(true);
                }
            }
            position += 1;
        }
        self.branch_tree_selection.set_selected(selected);
        self.updating_branch_tree.set(false);

        Ok(())
    }

    fn set_history_scope(&self, scope: HistoryScope) {
        if *self.history_scope.borrow() == scope {
            return;
        }
        if let Err(err) = app_settings::save_history_scope(&app_settings::history_scope_path(), &scope) {
            eprintln!("Failed to save the history scope: {}", err);
        }
        self.history_scope.replace(scope);
        self.reload_history();
    }

    /// Escape goes back to all branches, whether the tree is shown or not.
    fn show_all_branches(&self) {
        let position = (0..self.branch_tree_model.n_items()).find(|position| {
            self.branch_tree_model
                .row(*position)
                .as_ref()
                .and_then(branch_tree_node)
                .is_some_and(|node| node.kind == BranchTreeKind::AllBranches)
        });
        if let Some(position) = position {
            // Selecting the row sets the scope
            self.branch_tree_selection.set_selected(position);
        }
        self.set_history_scope(HistoryScope::AllBranches);
    }

    fn set_branch_tree_visible(&self, visible: bool) {
        if visible == self.branch_tree_scrolled.is_visible() {
            return;
        }
        if visible {
            self.branch_tree_scrolled.set_visible(true);
            self.branch_tree_paned.set_position(self.branch_tree_position.get());
        } else {
            self.branch_tree_position.set(self.branch_tree_paned.position());
            self.branch_tree_scrolled.set_visible(false);
        }
        if let Err(err) = app_settings::save_branch_tree_visible(&app_settings::branch_tree_visible_path(), visible) {
            eprintln!("Failed to save whether the branch tree is shown: {}", err);
        }
    }

    fn check_out_branch_tree_node(&self, node: &BranchTreeNode) {
        let kind = match node.kind {
            BranchTreeKind::LocalBranch if !node.is_head => BranchKind::Local,
            BranchTreeKind::RemoteBranch => BranchKind::Remote,
            _ => return,
        };
        let Some(name) = node.branch_name() else { return };
        self.switch_branch(BranchCandidate {
            name: name.to_owned(),
            kind,
            is_current: false,
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

        let graph = railway::collect_tree(&self.repository_manager, self.history_scope.borrow().clone())?;
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
        dialog_when_error!("Failed to load branches: {:?}", self.load_branch_tree());
        self.update_pull_button();
        self.update_push_button();
        self.update_commit_button();
        self.reload_history();
    }

    fn reload_history(&self) {
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

    fn push_button_clicked(self: &Rc<Self>) {
        if self.task_runner.is_running() {
            return;
        }
        let w = Rc::downgrade(self);
        let result = PushDialog::present(&self.window, self.repository_manager.clone(), move |request| {
            if let Some(w) = w.upgrade() {
                w.start_push(request);
            }
        });
        if let Err(err) = result {
            gtk_utils::message_box_error(&format!("Cannot push: {}", err.message()));
        }
    }

    fn start_push(&self, request: PushRequest) {
        if let Some(git_dir) = self.git_dir() {
            long_running_operation::start_operation(
                &self.task_runner,
                PushOperation { git_dir, request },
                &self.status_sender,
            );
        }
    }

    /// Shows how many commits the current branch can pull, or why it cannot pull.
    fn update_pull_button(&self) {
        let state = match self.repository_manager.open() {
            Ok(repo) => remote_operation::pull_button_state(&repo),
            Err(_) => PullButtonState::NoBranch,
        };

        let (badge, tooltip, available) = match &state {
            PullButtonState::NoBranch => (None, "HEAD is not on a branch".to_string(), false),
            PullButtonState::NoUpstream { branch } => {
                (None, format!("{} has no upstream to pull from", branch), false)
            }
            PullButtonState::Behind { branch, upstream, count } => (
                Some(format!("\u{2193}{}", count)),
                format!(
                    "Pull {} from {} into {} (as of the last fetch)",
                    crate::push_operation::commit_count_text(*count),
                    upstream,
                    branch
                ),
                true,
            ),
            // Still available: the pull fetches first, and the upstream may have been reset
            PullButtonState::Diverged { branch, upstream, behind, ahead } => (
                Some(format!("\u{2193}{}\u{2191}{}", behind, ahead)),
                format!(
                    "{} and {} have diverged ({} behind, {} ahead), so they cannot be fast-forwarded",
                    branch, upstream, behind, ahead
                ),
                true,
            ),
            PullButtonState::UpToDate { branch, upstream } => (
                None,
                format!("{} is up to date with {} as of the last fetch", branch, upstream),
                true,
            ),
        };

        self.pull_button.set_badge(badge.as_deref());
        self.pull_button.button().set_tooltip_text(Some(&tooltip));
        self.pull_available.set(available);
        self.pull_button.button().set_sensitive(available && !self.task_runner.is_running());
    }

    /// Shows how many files have changes to commit.
    fn update_commit_button(&self) {
        let counts = self
            .repository_manager
            .open()
            .and_then(|repo| work_tree_status::count_changes(&repo));

        let (badge, tooltip) = match counts {
            Ok(counts) if counts.changed_files > 0 => (
                Some(counts.changed_files.to_string()),
                format!(
                    "Commit: {} ({} staged)",
                    changed_files_text(counts.changed_files),
                    counts.staged_files
                ),
            ),
            Ok(_) => (None, "Commit: no changes".to_string()),
            Err(_) => (None, "Commit".to_string()),
        };

        self.commit_button.set_badge(badge.as_deref());
        self.commit_button.button().set_tooltip_text(Some(&tooltip));
    }

    /// Shows how many commits the current branch has to push, or why it cannot push.
    fn update_push_button(&self) {
        let state = match self.repository_manager.open() {
            Ok(repo) => remote_operation::push_button_state(&repo),
            Err(_) => PushButtonState::NoBranch,
        };

        let (badge, tooltip, available) = match &state {
            PushButtonState::NoBranch => (None, "HEAD is not on a branch".to_string(), false),
            PushButtonState::NoRemote => (None, "No remotes are configured".to_string(), false),
            PushButtonState::NoUpstream { branch } => (
                Some("\u{2191}+".to_string()),
                format!("{} has no upstream. Push it to a new remote branch", branch),
                true,
            ),
            PushButtonState::Ahead { branch, upstream, count } => (
                Some(format!("\u{2191}{}", count)),
                format!(
                    "Push {} from {} to {}",
                    crate::push_operation::commit_count_text(*count),
                    branch,
                    upstream
                ),
                true,
            ),
            PushButtonState::UpToDate { branch, upstream } => {
                (None, format!("{} is up to date with {}", branch, upstream), true)
            }
        };

        self.push_button.set_badge(badge.as_deref());
        self.push_button.button().set_tooltip_text(Some(&tooltip));
        self.push_available.set(available);
        self.push_button.button().set_sensitive(available && !self.task_runner.is_running());
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
        self.pull_button.button().set_sensitive(!status.is_running() && self.pull_available.get());
        self.push_button.button().set_sensitive(!status.is_running() && self.push_available.get());
        self.commit_button.button().set_sensitive(!status.is_running());
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

        .button-badge {
            background-color: @theme_selected_bg_color;
            color: @theme_selected_fg_color;
            border-radius: 9px;
            padding: 0 6px;
            font-size: smaller;
            font-weight: bold;
        }

        button:disabled .button-badge {
            background-color: alpha(currentColor, 0.3);
        }

        .branch-tree row {
            min-height: 28px;
            padding-top: 0;
            padding-bottom: 0;
        }

        .branch-tree-header {
            font-weight: bold;
            opacity: 0.7;
        }

        .branch-tree-last-scope {
            border-bottom: 1px solid alpha(currentColor, 0.18);
            padding-bottom: 4px;
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

fn changed_files_text(count: usize) -> String {
    if count == 1 {
        "1 changed file".to_string()
    } else {
        format!("{} changed files", count)
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
                    .set_markup(&create_subject_markup(&station.ref_names, station.head_ref_name.as_deref(), &station.subject));
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

fn branch_tree_node(row: &gtk::TreeListRow) -> Option<Rc<BranchTreeNode>> {
    row.item()
        .and_downcast::<BoxedAnyObject>()
        .map(|item| item.borrow::<Rc<BranchTreeNode>>().clone())
}

/// Whether the folder of `outer` holds the reference or folder of `inner`, so
/// that it is expanded to show the selected row.
fn scope_is_inside(outer: &HistoryScope, inner: &HistoryScope) -> bool {
    let HistoryScope::Prefix(prefix) = outer else { return false };
    match inner {
        HistoryScope::Ref(name) => name.starts_with(prefix.as_str()),
        HistoryScope::Prefix(inner_prefix) => inner_prefix != prefix && inner_prefix.starts_with(prefix.as_str()),
        _ => false,
    }
}

fn create_subject_markup(ref_names: &[String], head_ref_name: Option<&str>, subject: &str) -> String {
    let mut markup = String::new();

    for ref_name in ref_names {
        let attributes = if head_ref_name == Some(ref_name.as_str()) {
            "foreground=\"#ffffff\" background=\"#a00000\""
        } else {
            "foreground=\"#a00000\""
        };
        let tag = format!(
            "<span {}><b>[{}]</b></span>",
            attributes,
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
