use super::*;
use git2::{BranchType, Commit, Repository, Signature};
use std::cell::Cell;
use tempdir::TempDir;

#[path = "branch_context_menu_position_tests.rs"]
mod position_tests;

const SUBMENU_LINK_NAME: &str = "submenu";
const CUSTOM_ATTRIBUTE_NAME: &str = "custom";

struct MenuTestRepo {
    _temp_dir: TempDir,
    repository_manager: Rc<RepositoryManager>,
    parent: Oid,
    tip: Oid,
}

impl MenuTestRepo {
    fn new() -> Self {
        let temp_dir = TempDir::new("branch-context-menu").unwrap();
        let repo = Repository::init(temp_dir.path()).unwrap();
        let parent = commit(&repo, "parent", &[]);
        let parent_commit = repo.find_commit(parent).unwrap();
        let tip = commit(&repo, "tip", &[&parent_commit]);
        repo.set_head("refs/heads/master").unwrap();

        let repository_manager = RepositoryManager::new();
        repository_manager.set_work_dir_path(temp_dir.path().to_str().unwrap());
        Self {
            _temp_dir: temp_dir,
            repository_manager,
            parent,
            tip,
        }
    }

    fn create_local_branch(&self, name: &str, target: Oid) {
        let repo = self.repository_manager.open().unwrap();
        let commit = repo.find_commit(target).unwrap();
        repo.branch(name, &commit, false).unwrap();
    }

    fn create_remote_branch(&self, name: &str, target: Oid) {
        let repo = self.repository_manager.open().unwrap();
        let (remote_name, _) = name.split_once('/').unwrap();
        repo.remote(remote_name, "https://example.invalid/repository.git")
            .unwrap();
        repo.reference(
            &format!("refs/remotes/{name}"),
            target,
            false,
            "test remote-tracking branch",
        )
        .unwrap();
    }

    fn head_name(&self) -> String {
        self.repository_manager
            .open()
            .unwrap()
            .head()
            .unwrap()
            .shorthand()
            .unwrap()
            .to_owned()
    }

    fn upstream_name(&self, branch_name: &str) -> String {
        self.repository_manager
            .open()
            .unwrap()
            .find_branch(branch_name, BranchType::Local)
            .unwrap()
            .upstream()
            .unwrap()
            .name()
            .unwrap()
            .unwrap()
            .to_owned()
    }
}

fn commit(repo: &Repository, message: &str, parents: &[&Commit<'_>]) -> Oid {
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

fn candidate(name: &str, kind: BranchKind, is_current: bool) -> BranchCandidate {
    BranchCandidate {
        name: name.to_owned(),
        kind,
        is_current,
    }
}

fn action_group(candidates: Vec<BranchCandidate>) -> gio::SimpleActionGroup {
    create_action_group(
        candidates,
        RepositoryManager::new(),
        Rc::new(|| {}),
        Rc::new(|_| {}),
    )
}

fn submenu(menu: &gio::Menu) -> gio::MenuModel {
    menu.item_link(0, SUBMENU_LINK_NAME).unwrap()
}

fn action_for_label(menu: &gio::Menu, group: &gio::SimpleActionGroup, label: &str) -> gio::Action {
    let submenu = submenu(menu);
    let model_label = label.replace('_', "__");
    let index = (0..submenu.n_items())
        .find(|index| {
            submenu
                .item_attribute_value(*index, "label", None)
                .and_then(|value| value.get::<String>())
                .as_deref()
                == Some(model_label.as_str())
        })
        .unwrap();
    group
        .lookup_action(&format!("{SWITCH_ACTION_PREFIX}{index}"))
        .unwrap()
}

#[test]
fn should_use_a_custom_only_item_for_the_disabled_empty_submenu() {
    let menu = create_menu_model(&[]);
    let group = action_group(Vec::new());

    let custom_widget = menu
        .item_attribute_value(0, CUSTOM_ATTRIBUTE_NAME, None)
        .and_then(|value| value.get::<String>())
        .unwrap();

    assert!(menu.item_link(0, SUBMENU_LINK_NAME).is_none());
    assert_eq!(EMPTY_SUBMENU_WIDGET_ID, custom_widget);
    assert!(group.list_actions().is_empty());
}

#[test]
fn should_disable_only_the_current_branch_action() {
    let candidates = vec![
        candidate("master", BranchKind::Local, true),
        candidate("feature", BranchKind::Local, false),
    ];
    let menu = create_menu_model(&candidates);
    let group = action_group(candidates);

    assert!(!action_for_label(&menu, &group, "master").is_enabled());
    assert!(action_for_label(&menu, &group, "feature").is_enabled());
}

#[test]
fn should_add_every_candidate_to_the_switch_branch_submenu() {
    let candidates = vec![
        candidate("feature", BranchKind::Local, false),
        candidate("origin/release", BranchKind::Remote, false),
    ];

    let menu = create_menu_model(&candidates);

    assert_eq!(2, submenu(&menu).n_items());
}

#[test]
fn should_escape_mnemonics_in_local_and_remote_candidate_labels() {
    let candidates = vec![
        candidate("feature_test", BranchKind::Local, false),
        candidate("origin/group_feature", BranchKind::Remote, false),
    ];
    let menu = create_menu_model(&candidates);
    let submenu = submenu(&menu);

    assert_eq!(
        Some("feature__test".to_owned()),
        submenu
            .item_attribute_value(0, "label", None)
            .and_then(|value| value.get::<String>())
    );
    assert_eq!(
        Some("origin/group__feature".to_owned()),
        submenu
            .item_attribute_value(1, "label", None)
            .and_then(|value| value.get::<String>())
    );
}

#[test]
fn should_distinguish_same_named_local_and_remote_candidates_and_preserve_actions() {
    let test_repo = MenuTestRepo::new();
    test_repo.create_local_branch("origin/feature", test_repo.tip);
    test_repo.create_remote_branch("origin/feature", test_repo.tip);
    let (menu, group) = create_menu_for_commit(
        test_repo.tip,
        Rc::clone(&test_repo.repository_manager),
        Rc::new(|| {}),
        Rc::new(|error| panic!("unexpected switch error: {error}")),
    )
    .unwrap();

    action_for_label(&menu, &group, "origin/feature (local)").activate(None);
    assert_eq!("origin/feature", test_repo.head_name());

    action_for_label(&menu, &group, "origin/feature (remote)").activate(None);
    assert_eq!("feature", test_repo.head_name());
    assert_eq!("origin/feature", test_repo.upstream_name("feature"));
}

#[test]
fn should_use_the_right_clicked_commit_to_build_candidates() {
    let test_repo = MenuTestRepo::new();
    test_repo.create_local_branch("older", test_repo.parent);

    let (menu, _) = create_menu_for_commit(
        test_repo.parent,
        Rc::clone(&test_repo.repository_manager),
        Rc::new(|| {}),
        Rc::new(|_| {}),
    )
    .unwrap();
    let submenu = submenu(&menu);

    assert_eq!(1, submenu.n_items());
    assert_eq!(
        Some("older".to_owned()),
        submenu
            .item_attribute_value(0, "label", None)
            .and_then(|value| value.get::<String>())
    );
}

#[test]
fn should_switch_local_branch_and_refresh_once_when_action_is_activated() {
    let test_repo = MenuTestRepo::new();
    test_repo.create_local_branch("feature_test", test_repo.tip);
    let refresh_count = Rc::new(Cell::new(0));
    let refresh_count_for_action = Rc::clone(&refresh_count);
    let (menu, group) = create_menu_for_commit(
        test_repo.tip,
        Rc::clone(&test_repo.repository_manager),
        Rc::new(move || refresh_count_for_action.set(refresh_count_for_action.get() + 1)),
        Rc::new(|error| panic!("unexpected switch error: {error}")),
    )
    .unwrap();

    action_for_label(&menu, &group, "feature_test").activate(None);

    assert_eq!("feature_test", test_repo.head_name());
    assert_eq!(1, refresh_count.get());
}

#[test]
fn should_create_tracking_branch_and_refresh_once_when_remote_action_is_activated() {
    let test_repo = MenuTestRepo::new();
    test_repo.create_remote_branch("origin/group_feature", test_repo.tip);
    let refresh_count = Rc::new(Cell::new(0));
    let refresh_count_for_action = Rc::clone(&refresh_count);
    let (menu, group) = create_menu_for_commit(
        test_repo.tip,
        Rc::clone(&test_repo.repository_manager),
        Rc::new(move || refresh_count_for_action.set(refresh_count_for_action.get() + 1)),
        Rc::new(|error| panic!("unexpected switch error: {error}")),
    )
    .unwrap();

    action_for_label(&menu, &group, "origin/group_feature").activate(None);

    assert_eq!("group_feature", test_repo.head_name());
    assert_eq!(
        "origin/group_feature",
        test_repo.upstream_name("group_feature")
    );
    assert_eq!(1, refresh_count.get());
}

#[test]
fn should_not_refresh_when_activated_action_fails() {
    let refresh_count = Rc::new(Cell::new(0));
    let error_count = Rc::new(Cell::new(0));
    let refresh_count_for_action = Rc::clone(&refresh_count);
    let error_count_for_action = Rc::clone(&error_count);
    let candidates = vec![candidate("missing", BranchKind::Local, false)];
    let group = create_action_group(
        candidates,
        RepositoryManager::new(),
        Rc::new(move || refresh_count_for_action.set(refresh_count_for_action.get() + 1)),
        Rc::new(move |_| error_count_for_action.set(error_count_for_action.get() + 1)),
    );

    group
        .lookup_action(&format!("{SWITCH_ACTION_PREFIX}0"))
        .unwrap()
        .activate(None);

    assert_eq!(0, refresh_count.get());
    assert_eq!(1, error_count.get());
}

#[test]
fn should_propagate_the_git_error_without_refreshing_failed_action() {
    let refresh_count = Rc::new(Cell::new(0));
    let received_error = Rc::new(Cell::new(None));
    let refresh_count_for_action = Rc::clone(&refresh_count);
    let received_error_for_action = Rc::clone(&received_error);
    let group = create_action_group(
        vec![candidate("missing", BranchKind::Local, false)],
        RepositoryManager::new(),
        Rc::new(move || refresh_count_for_action.set(refresh_count_for_action.get() + 1)),
        Rc::new(move |error| received_error_for_action.set(Some(error.code()))),
    );

    group
        .lookup_action(&format!("{SWITCH_ACTION_PREFIX}0"))
        .unwrap()
        .activate(None);

    assert_eq!(0, refresh_count.get());
    assert_eq!(Some(git2::ErrorCode::NotFound), received_error.get());
}
