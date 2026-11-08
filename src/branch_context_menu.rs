use crate::branch::{find_branch_candidates, BranchCandidate, BranchKind};
use crate::repository_manager::RepositoryManager;
use git2::{Error, Oid};
use gtk::gdk;
use gtk::gio;
use gtk::graphene;
use gtk::prelude::*;
use std::rc::Rc;

const SWITCH_BRANCH_LABEL: &str = "Switch branch";
const ACTION_GROUP_NAME: &str = "branch";
const SWITCH_ACTION_PREFIX: &str = "switch-";
const NO_CANDIDATES_ACTION: &str = "no-candidates";

/// The single popover shared by every history row. It lives on a widget that
/// outlives the rows, because ListView reuses rows and refreshing replaces them.
pub struct BranchContextMenu {
    parent: gtk::Box,
    popover: gtk::PopoverMenu,
    repository_manager: Rc<RepositoryManager>,
    is_busy: Box<dyn Fn() -> bool>,
    on_switch: Rc<dyn Fn(BranchCandidate)>,
}

impl BranchContextMenu {
    pub fn new(
        parent: &gtk::Box,
        repository_manager: Rc<RepositoryManager>,
        is_busy: impl Fn() -> bool + 'static,
        on_switch: impl Fn(BranchCandidate) + 'static,
    ) -> Rc<BranchContextMenu> {
        let popover = gtk::PopoverMenu::from_model(None::<&gio::MenuModel>);
        popover.set_parent(parent);
        Rc::new(BranchContextMenu {
            parent: parent.clone(),
            popover,
            repository_manager,
            is_busy: Box::new(is_busy),
            on_switch: Rc::new(on_switch),
        })
    }

    pub fn attach(
        self: &Rc<Self>,
        row: &impl IsA<gtk::Widget>,
        commit_id: impl Fn() -> Option<Oid> + 'static,
    ) {
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gdk::BUTTON_SECONDARY);

        let menu = Rc::downgrade(self);
        let row_weak = row.upcast_ref::<gtk::Widget>().downgrade();
        gesture.connect_pressed(move |_, _, x, y| {
            let (Some(menu), Some(row), Some(commit_id)) =
                (menu.upgrade(), row_weak.upgrade(), commit_id())
            else {
                return;
            };
            if let Err(error) = menu.open(&row, x, y, commit_id) {
                crate::gtk_utils::message_box_error(&format!(
                    "Failed to load branch candidates: {error:?}"
                ));
            }
        });

        row.add_controller(gesture);
    }

    fn open(&self, row: &gtk::Widget, x: f64, y: f64, commit_id: Oid) -> Result<(), Error> {
        let (menu_model, action_group) = create_menu_for_commit(
            commit_id,
            &self.repository_manager,
            (self.is_busy)(),
            Rc::clone(&self.on_switch),
        )?;
        self.parent
            .insert_action_group(ACTION_GROUP_NAME, Some(&action_group));
        self.popover.set_menu_model(Some(&menu_model));

        let point = row
            .compute_point(&self.parent, &graphene::Point::new(x as f32, y as f32))
            .ok_or_else(|| Error::from_str("Failed to locate the history row"))?;
        self.popover.set_pointing_to(Some(&gdk::Rectangle::new(
            point.x() as i32,
            point.y() as i32,
            1,
            1,
        )));
        self.popover.popup();
        Ok(())
    }

    #[cfg(test)]
    pub fn popover(&self) -> &gtk::PopoverMenu {
        &self.popover
    }
}

/// While another task is running the menu has no candidates, so nothing can be
/// picked that the task runner would refuse to start.
fn create_menu_for_commit(
    commit_id: Oid,
    repository_manager: &RepositoryManager,
    is_busy: bool,
    on_switch: Rc<dyn Fn(BranchCandidate)>,
) -> Result<(gio::Menu, gio::SimpleActionGroup), Error> {
    let candidates = if is_busy {
        Vec::new()
    } else {
        find_branch_candidates(&repository_manager.open()?, commit_id)?
    };
    let menu = create_menu_model(&candidates);
    let actions = create_action_group(candidates, on_switch);
    Ok((menu, actions))
}

fn create_menu_model(candidates: &[BranchCandidate]) -> gio::Menu {
    let root = gio::Menu::new();
    if candidates.is_empty() {
        let action = format!("{ACTION_GROUP_NAME}.{NO_CANDIDATES_ACTION}");
        root.append(Some(SWITCH_BRANCH_LABEL), Some(&action));
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
    on_switch: Rc<dyn Fn(BranchCandidate)>,
) -> gio::SimpleActionGroup {
    let group = gio::SimpleActionGroup::new();

    if candidates.is_empty() {
        let action = gio::SimpleAction::new(NO_CANDIDATES_ACTION, None);
        action.set_enabled(false);
        group.add_action(&action);
    }

    for (index, candidate) in candidates.into_iter().enumerate() {
        let action = gio::SimpleAction::new(&format!("{SWITCH_ACTION_PREFIX}{index}"), None);
        action.set_enabled(!candidate.is_current);

        let on_switch = Rc::clone(&on_switch);
        action.connect_activate(move |_, _| on_switch(candidate.clone()));
        group.add_action(&action);
    }

    group
}

#[cfg(test)]
#[path = "branch_context_menu_tests.rs"]
mod tests;
