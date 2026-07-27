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

fn row_for_commit(history_window: &HistoryWindow, commit_id: git2::Oid) -> gtk::ListBoxRow {
    let index = history_window
        .stations
        .borrow()
        .iter()
        .position(|station| station.oid == commit_id)
        .unwrap();
    history_window
        .history_list_box
        .row_at_index(index as i32)
        .unwrap()
}

fn open_context_menu(row: &gtk::ListBoxRow) -> (gtk::Box, gtk::PopoverMenu) {
    let content = row.child().unwrap().downcast::<gtk::Box>().unwrap();
    let controllers = content.observe_controllers();
    let gesture = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::GestureClick>().ok())
        .find(|gesture| gesture.button() == gdk::BUTTON_SECONDARY)
        .unwrap();

    gesture.emit_by_name::<()>("pressed", &[&1_i32, &0_f64, &0_f64]);
    while gtk::glib::MainContext::default().iteration(false) {}

    let popover = content
        .last_child()
        .unwrap()
        .downcast::<gtk::PopoverMenu>()
        .unwrap();
    (content, popover)
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

fn menu_button(popover: &gtk::PopoverMenu, label: &str) -> gtk::MenuButton {
    let mut pending = vec![popover.clone().upcast::<gtk::Widget>()];
    while let Some(widget) = pending.pop() {
        if let Ok(menu_button) = widget.clone().downcast::<gtk::MenuButton>() {
            if menu_button.label().as_deref() == Some(label) {
                return menu_button;
            }
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

#[test]
fn should_switch_from_the_right_clicked_history_row_and_refresh_the_window() {
    gtk::init().unwrap();
    let test_repo = HistoryWindowTestRepo::new();
    let app = gtk::Application::builder()
        .application_id("org.sunnyone.MetalGit.HistoryWindowTest")
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let history_window = HistoryWindow::new(
        &app,
        Weak::<WindowManager>::new(),
        Rc::clone(&test_repo.repository_manager),
    );
    history_window.refresh();
    history_window.window.present();
    while gtk::glib::MainContext::default().iteration(false) {}
    let initial_title = history_window.window.title();

    let root_row = row_for_commit(&history_window, test_repo.root);
    let (_root_content, root_popover) = open_context_menu(&root_row);
    let empty_menu = root_menu(&root_popover);
    assert_eq!(
        Some("Switch branch".to_owned()),
        empty_menu
            .item_attribute_value(0, LABEL_ATTRIBUTE_NAME, None)
            .and_then(|value| value.get::<String>())
    );
    let switch_branch_button = menu_button(&root_popover, "Switch branch");
    assert!(root_popover.is_visible());
    assert!(!switch_branch_button.is_sensitive());
    assert_eq!(0, switch_branch_button.menu_model().unwrap().n_items());

    let tip_row = row_for_commit(&history_window, test_repo.tip);
    let (tip_content, tip_popover) = open_context_menu(&tip_row);
    let tip_menu = root_menu(&tip_popover);
    menu_label(&tip_popover, "origin/group_feature");
    let collision_action = action_for_label(&tip_menu, "origin/master");
    tip_content
        .activate_action(&collision_action, None)
        .unwrap();
    assert_eq!(initial_title, history_window.window.title());
    assert!(tip_row.parent().is_some());

    let previous_row = row_for_commit(&history_window, test_repo.middle);

    let (content, popover) = open_context_menu(&previous_row);
    let menu = root_menu(&popover);
    menu_label(&popover, "feature_test");
    let action = action_for_label(&menu, "feature_test");
    content.activate_action(&action, None).unwrap();

    assert_eq!(
        Some("[feature_test]".to_owned()),
        history_window
            .window
            .title()
            .map(|title| title.split_once(' ').unwrap().0.to_owned())
    );
    assert!(previous_row.parent().is_none());
    assert!(history_window
        .stations
        .borrow()
        .iter()
        .all(|station| station.oid != test_repo.tip));

    let current_row = row_for_commit(&history_window, test_repo.middle);
    let (content, popover) = open_context_menu(&current_row);
    let menu = root_menu(&popover);
    let current_action = action_for_label(&menu, "feature_test");
    content.activate_action(&current_action, None).unwrap();
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
