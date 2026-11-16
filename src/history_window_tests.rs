use super::*;
use git2::{Commit, Repository, Signature};
use gtk::gio;
use std::rc::Weak;
use tempdir::TempDir;

const ACTION_ATTRIBUTE_NAME: &str = "action";
const LABEL_ATTRIBUTE_NAME: &str = "label";
const SUBMENU_LINK_NAME: &str = "submenu";

struct HistoryWindowTestRepo {
    _temp_dir: TempDir,
    repository_manager: Rc<RepositoryManager>,
    root: git2::Oid,
    middle: git2::Oid,
    tip: git2::Oid,
}

impl HistoryWindowTestRepo {
    fn new() -> Self {
        let temp_dir = TempDir::new("history-window").unwrap();
        let repo = Repository::init(temp_dir.path()).unwrap();
        let root = commit(&repo, "root", &[]);
        let root_commit = repo.find_commit(root).unwrap();
        let middle = commit(&repo, "middle", &[&root_commit]);
        let middle_commit = repo.find_commit(middle).unwrap();
        let tip = commit(&repo, "tip", &[&middle_commit]);
        repo.branch("feature_test", &middle_commit, false).unwrap();
        repo.remote("origin", "https://example.invalid/repository.git")
            .unwrap();
        repo.reference(
            "refs/remotes/origin/master",
            tip,
            false,
            "test remote-tracking branch",
        )
        .unwrap();
        repo.reference(
            "refs/remotes/origin/group_feature",
            tip,
            false,
            "test remote-tracking branch",
        )
        .unwrap();
        repo.set_head("refs/heads/master").unwrap();

        let repository_manager = RepositoryManager::new();
        repository_manager.set_work_dir_path(temp_dir.path().to_str().unwrap());
        Self {
            _temp_dir: temp_dir,
            repository_manager,
            root,
            middle,
            tip,
        }
    }
}

fn commit(repo: &Repository, message: &str, parents: &[&Commit<'_>]) -> git2::Oid {
    let signature = Signature::now("test", "test@example.com").unwrap();
    let tree_oid = repo.treebuilder(None).unwrap().write().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    repo.commit(
        Some("refs/heads/master"),
        &signature,
        &signature,
        message,
        &tree,
        parents,
    )
    .unwrap()
}

fn row_for_commit(history_window: &HistoryWindow, commit_id: git2::Oid) -> HistoryRow {
    while gtk::glib::MainContext::default().iteration(false) {}

    let mut pending = vec![history_window
        .history_list_view
        .clone()
        .upcast::<gtk::Widget>()];
    while let Some(widget) = pending.pop() {
        if let Ok(row) = widget.clone().downcast::<HistoryRow>() {
            if row.commit_id() == Some(commit_id) {
                return row;
            }
        }

        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    panic!("history row not found: {commit_id}");
}

fn wait_for_task(history_window: &HistoryWindow) {
    let context = gtk::glib::MainContext::default();
    while history_window.task_runner.is_running() {
        context.iteration(true);
    }
    while context.iteration(false) {}
}

fn graph_contains(history_window: &HistoryWindow, commit_id: git2::Oid) -> bool {
    let graph = history_window.graph.borrow().clone();
    (0..graph.len()).any(|index| graph.commit(index).unwrap().oid == commit_id)
}

fn open_context_menu(
    history_window: &HistoryWindow,
    row: &HistoryRow,
) -> (gtk::Widget, gtk::PopoverMenu) {
    let controllers = row.observe_controllers();
    let gesture = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::GestureClick>().ok())
        .find(|gesture| gesture.button() == gdk::BUTTON_SECONDARY)
        .unwrap();

    gesture.emit_by_name::<()>("pressed", &[&1_i32, &0_f64, &0_f64]);
    while gtk::glib::MainContext::default().iteration(false) {}

    let popover = history_window.branch_context_menu.popover().clone();
    (popover.parent().unwrap(), popover)
}

fn menu_label(popover: &gtk::PopoverMenu, label: &str) -> gtk::Label {
    let mut pending = vec![popover.clone().upcast::<gtk::Widget>()];
    while let Some(widget) = pending.pop() {
        if let Ok(menu_label) = widget.clone().downcast::<gtk::Label>() {
            if menu_label.text() == label {
                return menu_label;
            }
        }

        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    panic!("menu label not found: {label}");
}

fn model_button(popover: &gtk::PopoverMenu, label: &str) -> gtk::Widget {
    let mut pending = vec![popover.clone().upcast::<gtk::Widget>()];
    while let Some(widget) = pending.pop() {
        if widget.type_().name() == "GtkModelButton"
            && widget.property::<Option<String>>("text").as_deref() == Some(label)
        {
            return widget;
        }

        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    panic!("menu button not found: {label}");
}

fn root_menu(popover: &gtk::PopoverMenu) -> gio::MenuModel {
    popover.menu_model().unwrap()
}

fn submenu(menu: &gio::MenuModel) -> gio::MenuModel {
    menu.item_link(0, SUBMENU_LINK_NAME).unwrap()
}

fn action_for_label(menu: &gio::MenuModel, label: &str) -> String {
    let submenu = submenu(menu);
    let model_label = label.replace('_', "__");
    let index = (0..submenu.n_items())
        .find(|index| {
            submenu
                .item_attribute_value(*index, LABEL_ATTRIBUTE_NAME, None)
                .and_then(|value| value.get::<String>())
                .as_deref()
                == Some(model_label.as_str())
        })
        .unwrap();
    submenu
        .item_attribute_value(index, ACTION_ATTRIBUTE_NAME, None)
        .and_then(|value| value.get::<String>())
        .unwrap()
}

fn open_history_window(app: &gtk::Application, test_repo: &HistoryWindowTestRepo) -> Rc<HistoryWindow> {
    let history_window = HistoryWindow::new(
        app,
        Weak::<WindowManager>::new(),
        Rc::clone(&test_repo.repository_manager),
    );
    history_window.refresh();
    history_window.window.present();
    while gtk::glib::MainContext::default().iteration(false) {}
    history_window
}

/// The labels of the visible tree rows, indented by two spaces per level.
fn branch_tree_labels(history_window: &HistoryWindow) -> Vec<String> {
    let model = &history_window.branch_tree_model;
    (0..model.n_items())
        .filter_map(|position| model.row(position))
        .map(|row| {
            let node = branch_tree_node(&row).unwrap();
            format!("{}{}", "  ".repeat(row.depth() as usize), node.label)
        })
        .collect()
}

fn branch_tree_position(history_window: &HistoryWindow, label: &str) -> u32 {
    let labels = branch_tree_labels(history_window);
    labels
        .iter()
        .position(|row| row.trim_start() == label)
        .unwrap_or_else(|| panic!("branch tree row not found: {label} in {labels:?}")) as u32
}

fn select_branch_tree_row(history_window: &HistoryWindow, label: &str) {
    let position = branch_tree_position(history_window, label);
    history_window.branch_tree_selection.set_selected(position);
}

fn selected_branch_tree_label(history_window: &HistoryWindow) -> String {
    let row = history_window
        .branch_tree_selection
        .selected_item()
        .and_downcast::<gtk::TreeListRow>()
        .unwrap();
    branch_tree_node(&row).unwrap().label.clone()
}

// GTK may only be used from the thread that initialized it, so the scenarios
// that open a window run one after another in this single test.
#[test]
fn history_window_scenarios() {
    // The settings are saved under the user's config directory
    let config_dir = TempDir::new("history-window-config").unwrap();
    std::env::set_var("XDG_CONFIG_HOME", config_dir.path());
    gtk::init().unwrap();
    let app = gtk::Application::builder()
        .application_id("org.sunnyone.MetalGit.HistoryWindowTest")
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();

    should_switch_from_the_right_clicked_history_row_and_refresh_the_window(&app);
    branch_tree_should_list_the_references_in_folders(&app);
    branch_tree_selection_should_choose_what_the_history_shows(&app);
    branch_tree_should_show_nothing_when_the_reference_is_gone(&app);
    branch_tree_double_click_should_check_out_a_remote_branch(&app);
    sidebar_should_be_hidden_and_shown_by_the_toggle_action(&app);
    branch_tree_filter_should_keep_the_history_and_the_collapsed_folders(&app);
    branch_filter_should_be_reached_from_the_keyboard(&app);
}

fn should_switch_from_the_right_clicked_history_row_and_refresh_the_window(app: &gtk::Application) {
    app_settings::save_history_scope(&app_settings::history_scope_path(), &HistoryScope::AllBranches).unwrap();
    let test_repo = HistoryWindowTestRepo::new();
    let history_window = open_history_window(app, &test_repo);
    let initial_title = history_window.window.title();

    let root_row = row_for_commit(&history_window, test_repo.root);
    let (_root_content, root_popover) = open_context_menu(&history_window, &root_row);
    let empty_menu = root_menu(&root_popover);
    assert_eq!(
        Some("Switch branch".to_owned()),
        empty_menu
            .item_attribute_value(0, LABEL_ATTRIBUTE_NAME, None)
            .and_then(|value| value.get::<String>())
    );
    assert!(root_popover.is_visible());
    assert!(empty_menu.item_link(0, SUBMENU_LINK_NAME).is_none());
    assert!(!model_button(&root_popover, "Switch branch").is_sensitive());

    let tip_row = row_for_commit(&history_window, test_repo.tip);
    let (tip_content, tip_popover) = open_context_menu(&history_window, &tip_row);
    let tip_menu = root_menu(&tip_popover);
    menu_label(&tip_popover, "origin/group_feature");
    let collision_action = action_for_label(&tip_menu, "origin/master");
    tip_content
        .activate_action(&collision_action, None)
        .unwrap();
    wait_for_task(&history_window);
    assert_eq!(initial_title, history_window.window.title());
    assert!(tip_row.parent().is_some());

    history_window.on_status(LongRunningStatus::Started {
        message: "Testing...".to_owned(),
        cancellable: false,
    });
    assert!(!history_window.commit_button.button().is_sensitive());
    history_window.on_status(LongRunningStatus::Cancelled {
        message: "Cancelled".to_owned(),
    });
    assert!(history_window.commit_button.button().is_sensitive());

    let previous_row = row_for_commit(&history_window, test_repo.middle);

    // Another task is running, so the menu must not offer a switch.
    assert!(history_window.task_runner.start(
        |token| {
            while !token.is_cancelled() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Ok::<(), ()>(())
        },
        |_| {},
    ));
    let (_content, busy_popover) = open_context_menu(&history_window, &previous_row);
    assert!(!model_button(&busy_popover, "Switch branch").is_sensitive());
    busy_popover.popdown();
    history_window.task_runner.cancel();
    wait_for_task(&history_window);

    let (_content, popover) = open_context_menu(&history_window, &previous_row);
    // A real click: the popover closes before GTK activates the action.
    model_button(&popover, "feature_test").emit_by_name::<()>("clicked", &[]);
    wait_for_task(&history_window);

    assert_eq!(
        Some("[feature_test]".to_owned()),
        history_window
            .window
            .title()
            .map(|title| title.split_once(' ').unwrap().0.to_owned())
    );
    // All branches stay visible, so the previous branch can be reached again.
    assert_eq!(HistoryScope::AllBranches, *history_window.history_scope.borrow());
    assert_eq!("All Branches", selected_branch_tree_label(&history_window));
    assert!(graph_contains(&history_window, test_repo.tip));

    select_branch_tree_row(&history_window, "Current Branch");
    assert!(!graph_contains(&history_window, test_repo.tip));
    assert_eq!(
        HistoryScope::CurrentBranch,
        app_settings::load_history_scope(&app_settings::history_scope_path())
    );

    let current_row = row_for_commit(&history_window, test_repo.middle);
    let (content, popover) = open_context_menu(&history_window, &current_row);
    let menu = root_menu(&popover);
    let current_action = action_for_label(&menu, "feature_test");
    content.activate_action(&current_action, None).unwrap();
    wait_for_task(&history_window);
    assert_eq!(
        "feature_test",
        test_repo
            .repository_manager
            .open()
            .unwrap()
            .head()
            .unwrap()
            .shorthand()
            .unwrap()
    );
    assert!(!menu_label(&popover, "feature_test").is_sensitive());
}

#[test]
fn should_mark_only_the_current_branch_label() {
    let ref_names = vec!["master".to_owned(), "origin/master".to_owned()];

    let markup = create_subject_markup(&ref_names, Some("master"), "subject <1>");

    assert_eq!(
        "<span foreground=\"#ffffff\" background=\"#a00000\"><b>[master]</b></span>\
         <span foreground=\"#a00000\"><b>[origin/master]</b></span> subject &lt;1&gt;",
        markup
    );
}

fn branch_tree_should_list_the_references_in_folders(app: &gtk::Application) {
    let test_repo = HistoryWindowTestRepo::new();
    {
        let repo = test_repo.repository_manager.open().unwrap();
        let middle = repo.find_commit(test_repo.middle).unwrap();
        repo.branch("user/me/fix", &middle, false).unwrap();
        repo.reference_symbolic("refs/remotes/origin/HEAD", "refs/remotes/origin/master", false, "test")
            .unwrap();
    }
    let history_window = open_history_window(app, &test_repo);

    assert_eq!(
        vec![
            "All Branches",
            "Current Branch",
            "Local",
            "  feature_test",
            "  master",
            "  user/me/",
            "    fix",
            "Remote",
            "  origin/",
            "    group_feature",
            "    master",
        ],
        branch_tree_labels(&history_window)
    );
}

fn branch_tree_selection_should_choose_what_the_history_shows(app: &gtk::Application) {
    app_settings::save_history_scope(&app_settings::history_scope_path(), &HistoryScope::AllBranches).unwrap();
    let test_repo = HistoryWindowTestRepo::new();
    let history_window = open_history_window(app, &test_repo);

    select_branch_tree_row(&history_window, "feature_test");
    assert!(!graph_contains(&history_window, test_repo.tip));
    assert_eq!(test_repo.middle, history_window.graph.borrow().commit(0).unwrap().oid);
    // Only all branches and the current branch are kept for the next launch
    assert_eq!(
        HistoryScope::AllBranches,
        app_settings::load_history_scope(&app_settings::history_scope_path())
    );

    // A refresh keeps the selected row and what it shows
    history_window.refresh();
    assert_eq!("feature_test", selected_branch_tree_label(&history_window));
    assert!(!graph_contains(&history_window, test_repo.tip));

    select_branch_tree_row(&history_window, "origin/");
    assert!(graph_contains(&history_window, test_repo.tip));

    history_window.show_all_branches();
    assert_eq!("All Branches", selected_branch_tree_label(&history_window));
    assert_eq!(HistoryScope::AllBranches, *history_window.history_scope.borrow());
}

fn branch_tree_should_show_nothing_when_the_reference_is_gone(app: &gtk::Application) {
    let test_repo = HistoryWindowTestRepo::new();
    let history_window = open_history_window(app, &test_repo);
    select_branch_tree_row(&history_window, "feature_test");

    test_repo
        .repository_manager
        .open()
        .unwrap()
        .find_branch("feature_test", git2::BranchType::Local)
        .unwrap()
        .delete()
        .unwrap();
    history_window.refresh();

    assert_eq!(
        HistoryScope::Ref("refs/heads/feature_test".to_owned()),
        *history_window.history_scope.borrow()
    );
    assert!(history_window.branch_tree_selection.selected_item().is_none());
    assert_eq!(0, history_window.graph.borrow().len());
}

fn branch_tree_double_click_should_check_out_a_remote_branch(app: &gtk::Application) {
    let test_repo = HistoryWindowTestRepo::new();
    let history_window = open_history_window(app, &test_repo);

    let position = branch_tree_position(&history_window, "group_feature");
    history_window
        .branch_tree_view
        .emit_by_name::<()>("activate", &[&position]);
    wait_for_task(&history_window);

    let repo = test_repo.repository_manager.open().unwrap();
    assert_eq!("group_feature", repo.head().unwrap().shorthand().unwrap());
    // The local branch made for it shows up in the refreshed tree
    assert!(branch_tree_labels(&history_window).contains(&"  group_feature".to_owned()));
}

fn sidebar_should_be_hidden_and_shown_by_the_toggle_action(app: &gtk::Application) {
    let test_repo = HistoryWindowTestRepo::new();
    let history_window = open_history_window(app, &test_repo);
    let path = app_settings::sidebar_visible_path();
    assert!(history_window.sidebar.is_visible());

    WidgetExt::activate_action(&history_window.window, "win.toggle-sidebar", None).unwrap();
    assert!(!history_window.sidebar.is_visible());
    assert!(!app_settings::load_sidebar_visible(&path));

    // The history keeps what the hidden tree chose
    history_window.branch_tree_selection.set_selected(branch_tree_position(&history_window, "feature_test"));
    assert!(!graph_contains(&history_window, test_repo.tip));

    WidgetExt::activate_action(&history_window.window, "win.toggle-sidebar", None).unwrap();
    assert!(history_window.sidebar.is_visible());
    assert!(app_settings::load_sidebar_visible(&path));
    let accels = app.accels_for_action("win.toggle-sidebar");
    assert_eq!(vec!["F9"], accels.iter().map(|accel| accel.as_str()).collect::<Vec<_>>());

    // The next window starts hidden when it was hidden last
    WidgetExt::activate_action(&history_window.window, "win.toggle-sidebar", None).unwrap();
    let next_window = open_history_window(app, &test_repo);
    assert!(!next_window.sidebar.is_visible());
    WidgetExt::activate_action(&history_window.window, "win.toggle-sidebar", None).unwrap();
}

fn branch_tree_filter_should_keep_the_history_and_the_collapsed_folders(app: &gtk::Application) {
    let test_repo = HistoryWindowTestRepo::new();
    let history_window = open_history_window(app, &test_repo);
    select_branch_tree_row(&history_window, "feature_test");
    let history_length = history_window.graph.borrow().len();
    let origin = branch_tree_position(&history_window, "origin/");
    history_window.branch_tree_model.row(origin).unwrap().set_expanded(false);

    history_window.branch_filter_entry.set_text("GROUP");

    // Only the matching rows show, with their folders open
    assert_eq!(vec!["Remote", "  origin/", "    group_feature"], branch_tree_labels(&history_window));
    // The history stays on the branch whose row is filtered out
    assert_eq!(
        HistoryScope::Ref("refs/heads/feature_test".to_owned()),
        *history_window.history_scope.borrow()
    );
    assert!(history_window.branch_tree_selection.selected_item().is_none());
    assert_eq!(history_length, history_window.graph.borrow().len());

    // A refresh filters the references read again
    history_window.refresh();
    assert_eq!(vec!["Remote", "  origin/", "    group_feature"], branch_tree_labels(&history_window));

    history_window.branch_filter_entry.set_text("nothing-like-this");
    assert!(branch_tree_labels(&history_window).is_empty());
    assert!(history_window.branch_tree_empty_label.is_visible());
    assert!(!history_window.branch_tree_scrolled.is_visible());

    history_window.branch_filter_entry.set_text("");

    // The folder collapsed before the filter is collapsed again
    let labels = branch_tree_labels(&history_window);
    assert!(labels.contains(&"  origin/".to_owned()));
    assert!(!labels.contains(&"    group_feature".to_owned()));
    assert!(!history_window.branch_tree_empty_label.is_visible());
    assert_eq!("feature_test", selected_branch_tree_label(&history_window));
}

fn focus_is_in(history_window: &HistoryWindow, widget: &impl IsA<gtk::Widget>) -> bool {
    GtkWindowExt::focus(&history_window.window).is_some_and(|focus| focus.is_ancestor(widget) || &focus == widget.upcast_ref())
}

fn branch_filter_should_be_reached_from_the_keyboard(app: &gtk::Application) {
    let test_repo = HistoryWindowTestRepo::new();
    let history_window = open_history_window(app, &test_repo);
    let entry = history_window.branch_filter_entry.clone();
    assert!(history_window.sidebar.is_visible());
    history_window.history_list_view.grab_focus();

    // Showing the sidebar with F9 focuses the filter, and hiding it gives the
    // focus back to the history
    WidgetExt::activate_action(&history_window.window, "win.toggle-sidebar", None).unwrap();
    assert!(focus_is_in(&history_window, &history_window.history_list_view));
    WidgetExt::activate_action(&history_window.window, "win.toggle-sidebar", None).unwrap();
    assert!(focus_is_in(&history_window, &entry));
    WidgetExt::activate_action(&history_window.window, "win.toggle-sidebar", None).unwrap();
    assert!(focus_is_in(&history_window, &history_window.history_list_view));

    // Ctrl+B shows the sidebar too, and selects the text to replace it
    entry.set_text("group");
    WidgetExt::activate_action(&history_window.window, "win.find-branch", None).unwrap();
    assert!(history_window.sidebar.is_visible());
    assert!(focus_is_in(&history_window, &entry));
    assert_eq!(Some((0, 5)), entry.selection_bounds());
    let accels = app.accels_for_action("win.find-branch");
    assert_eq!(vec!["<Control>b"], accels.iter().map(|accel| accel.as_str()).collect::<Vec<_>>());

    // Enter shows the history of the first branch left
    entry.emit_activate();
    assert_eq!(
        HistoryScope::Ref("refs/remotes/origin/group_feature".to_owned()),
        *history_window.history_scope.borrow()
    );
    assert_eq!("group_feature", selected_branch_tree_label(&history_window));

    entry.set_text("");
    WidgetExt::activate_action(&history_window.window, "win.toggle-sidebar", None).unwrap();
    WidgetExt::activate_action(&history_window.window, "win.toggle-sidebar", None).unwrap();
}
