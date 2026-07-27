use crate::branch::{
    create_tracking_branch_and_switch, find_branch_candidates, switch_local_branch,
    BranchCandidate, BranchKind,
};
use crate::repository_manager::RepositoryManager;
use git2::{Error, Oid};
use gtk::gdk;
use gtk::gio;
use gtk::prelude::*;
use std::rc::Rc;

const SWITCH_BRANCH_LABEL: &str = "Switch branch";
const ACTION_GROUP_NAME: &str = "branch";
const SWITCH_ACTION_PREFIX: &str = "switch-";
const CUSTOM_ATTRIBUTE_NAME: &str = "custom";
const EMPTY_SUBMENU_WIDGET_ID: &str = "empty-switch-branch";

pub fn install(
    widget: &gtk::Box,
    commit_id: Oid,
    repository_manager: Rc<RepositoryManager>,
    on_switched: impl Fn() + 'static,
) {
    let gesture = gtk::GestureClick::new();
    gesture.set_button(gdk::BUTTON_SECONDARY);

    let menu_widget = widget.clone();
    let on_switched: Rc<dyn Fn()> = Rc::new(on_switched);
    gesture.connect_pressed(move |_, _, x, y| {
        let result = show_menu(
            &menu_widget,
            x,
            y,
            commit_id,
            Rc::clone(&repository_manager),
            Rc::clone(&on_switched),
        );
        if let Err(error) = result {
            crate::gtk_utils::message_box_error(&format!(
                "Failed to load branch candidates: {error:?}"
            ));
        }
    });

    widget.add_controller(gesture);
}

fn show_menu(
    widget: &gtk::Box,
    x: f64,
    y: f64,
    commit_id: Oid,
    repository_manager: Rc<RepositoryManager>,
    on_switched: Rc<dyn Fn()>,
) -> Result<(), Error> {
    let (menu_model, action_group) = create_menu_for_commit(
        commit_id,
        repository_manager,
        on_switched,
        Rc::new(report_error),
    )?;
    widget.insert_action_group(ACTION_GROUP_NAME, Some(&action_group));

    let popover = gtk::PopoverMenu::from_model(Some(&menu_model));
    install_empty_submenu_widget(&popover, &menu_model)?;
    popover.set_parent(widget);
    popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    popover.connect_closed(|popover| popover.unparent());
    popover.popup();
    Ok(())
}

fn create_menu_for_commit(
    commit_id: Oid,
    repository_manager: Rc<RepositoryManager>,
    on_switched: Rc<dyn Fn()>,
    on_error: Rc<dyn Fn(&Error)>,
) -> Result<(gio::Menu, gio::SimpleActionGroup), Error> {
    let repo = repository_manager.open()?;
    let candidates = find_branch_candidates(&repo, commit_id)?;
    let menu = create_menu_model(&candidates);
    let actions = create_action_group(candidates, repository_manager, on_switched, on_error);
    Ok((menu, actions))
}

fn create_menu_model(candidates: &[BranchCandidate]) -> gio::Menu {
    let root = gio::Menu::new();
    if candidates.is_empty() {
        let item = gio::MenuItem::new(Some(SWITCH_BRANCH_LABEL), None);
        item.set_attribute_value(
            CUSTOM_ATTRIBUTE_NAME,
            Some(&EMPTY_SUBMENU_WIDGET_ID.to_variant()),
        );
        root.append_item(&item);
        return root;
    }

    let submenu = gio::Menu::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let action = format!("{ACTION_GROUP_NAME}.{SWITCH_ACTION_PREFIX}{index}");
        let label = candidate_menu_label(candidate, candidates);
        submenu.append(Some(&label), Some(&action));
    }

    let item = gio::MenuItem::new_submenu(Some(SWITCH_BRANCH_LABEL), &submenu);
    root.append_item(&item);
    root
}

fn install_empty_submenu_widget(popover: &gtk::PopoverMenu, menu: &gio::Menu) -> Result<(), Error> {
    let custom_widget = menu
        .item_attribute_value(0, CUSTOM_ATTRIBUTE_NAME, None)
        .and_then(|value| value.get::<String>());
    if custom_widget.as_deref() != Some(EMPTY_SUBMENU_WIDGET_ID) {
        return Ok(());
    }

    let empty_menu = gio::Menu::new();
    let button = gtk::MenuButton::builder()
        .label(SWITCH_BRANCH_LABEL)
        .menu_model(&empty_menu)
        .has_frame(false)
        .sensitive(false)
        .build();
    if !popover.add_child(&button, EMPTY_SUBMENU_WIDGET_ID) {
        return Err(Error::from_str(
            "Failed to install disabled Switch branch menu item",
        ));
    }
    Ok(())
}

fn candidate_menu_label(candidate: &BranchCandidate, candidates: &[BranchCandidate]) -> String {
    let has_duplicate_name = candidates
        .iter()
        .filter(|other| other.name == candidate.name)
        .nth(1)
        .is_some();
    let label = if has_duplicate_name {
        let kind = match candidate.kind {
            BranchKind::Local => "local",
            BranchKind::Remote => "remote",
        };
        format!("{} ({kind})", candidate.name)
    } else {
        candidate.name.clone()
    };
    label.replace('_', "__")
}

fn create_action_group(
    candidates: Vec<BranchCandidate>,
    repository_manager: Rc<RepositoryManager>,
    on_switched: Rc<dyn Fn()>,
    on_error: Rc<dyn Fn(&Error)>,
) -> gio::SimpleActionGroup {
    let group = gio::SimpleActionGroup::new();

    for (index, candidate) in candidates.into_iter().enumerate() {
        let action = gio::SimpleAction::new(&format!("{SWITCH_ACTION_PREFIX}{index}"), None);
        action.set_enabled(!candidate.is_current);

        let repository_manager = Rc::clone(&repository_manager);
        let on_switched = Rc::clone(&on_switched);
        let on_error = Rc::clone(&on_error);
        action.connect_activate(
            move |_, _| match switch_branch(&repository_manager, &candidate) {
                Ok(()) => on_switched(),
                Err(error) => on_error(&error),
            },
        );
        group.add_action(&action);
    }

    group
}

fn switch_branch(
    repository_manager: &RepositoryManager,
    candidate: &BranchCandidate,
) -> Result<(), Error> {
    let repo = repository_manager.open()?;
    match candidate.kind {
        BranchKind::Local => switch_local_branch(&repo, &candidate.name),
        BranchKind::Remote => create_tracking_branch_and_switch(&repo, &candidate.name),
    }
}

fn report_error(error: &Error) {
    crate::gtk_utils::message_box_error(&format!("Failed to switch branch: {error:?}"));
}

#[cfg(test)]
#[path = "branch_context_menu_tests.rs"]
mod tests;
