use crate::branch::{head_label, recent_branches, BranchCandidate, BranchKind};
use crate::repository_manager::RepositoryManager;
use git2::Error;
use gtk::prelude::*;
use std::rc::Rc;

const RECENT_BRANCH_LIMIT: usize = 5;
// Keeps a long branch name from widening the toolbar
const LABEL_MAX_WIDTH_CHARS: i32 = 32;

/// The button beside the sidebar toggle that names what HEAD is on. Its menu
/// switches to one of the branches HEAD was on recently.
pub struct BranchMenu {
    button: gtk::Button,
    label: gtk::Label,
    popover: gtk::Popover,
    content: gtk::Box,
    repository_manager: Rc<RepositoryManager>,
    is_busy: Box<dyn Fn() -> bool>,
    on_switch: Rc<dyn Fn(BranchCandidate)>,
}

impl BranchMenu {
    pub fn new(
        repository_manager: Rc<RepositoryManager>,
        is_busy: impl Fn() -> bool + 'static,
        on_switch: impl Fn(BranchCandidate) + 'static,
    ) -> Rc<BranchMenu> {
        let label = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .max_width_chars(LABEL_MAX_WIDTH_CHARS)
            .build();
        let button_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        button_content.append(&label);
        button_content.append(&gtk::Image::from_icon_name("pan-down-symbolic"));
        let button = gtk::Button::builder().child(&button_content).build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();
        let popover = gtk::Popover::builder()
            .child(&content)
            .position(gtk::PositionType::Bottom)
            .build();
        popover.set_parent(&button);

        let menu = Rc::new(BranchMenu {
            button,
            label,
            popover,
            content,
            repository_manager,
            is_busy: Box::new(is_busy),
            on_switch: Rc::new(on_switch),
        });

        let weak = Rc::downgrade(&menu);
        menu.button.connect_clicked(move |_| {
            let Some(menu) = weak.upgrade() else { return };
            if let Err(error) = menu.open() {
                crate::gtk_utils::message_box_error(&format!("Failed to list recent branches: {}", error.message()));
            }
        });

        menu
    }

    pub fn widget(&self) -> gtk::Button {
        self.button.clone()
    }

    /// Names what HEAD is on now. Called whenever the window refreshes.
    pub fn update(&self) {
        let text = self
            .repository_manager
            .open()
            .and_then(|repo| head_label(&repo))
            .unwrap_or_else(|_| "HEAD".to_owned());
        self.label.set_text(&text);
        // The label may be cut short
        self.button.set_tooltip_text(Some(&text));
    }

    /// The rows are made again on each opening, as the reflog changes with every checkout.
    fn open(self: &Rc<Self>) -> Result<(), Error> {
        let repo = self.repository_manager.open()?;
        let current = head_label(&repo)?;
        let recent = recent_branches(&repo, RECENT_BRANCH_LIMIT)?;

        while let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }

        let current_row = gtk::Label::builder()
            .label(format!("\u{2714} {}", current))
            .xalign(0.0)
            .css_classes(["branch-menu-current"])
            .build();
        self.content.append(&current_row);

        if !recent.is_empty() {
            let header = gtk::Label::builder()
                .label("Recent")
                .xalign(0.0)
                .css_classes(["branch-menu-header"])
                .build();
            self.content.append(&header);
        }
        let busy = (self.is_busy)();
        for name in recent {
            self.content.append(&self.recent_row(name, busy));
        }

        self.popover.popup();
        Ok(())
    }

    fn recent_row(self: &Rc<Self>, name: String, busy: bool) -> gtk::Button {
        let row_label = gtk::Label::builder().label(&name).xalign(0.0).build();
        let row = gtk::Button::builder()
            .child(&row_label)
            .css_classes(["flat", "branch-menu-row"])
            // Switching waits until the running operation ends, like the other buttons
            .sensitive(!busy)
            .build();
        let weak = Rc::downgrade(self);
        row.connect_clicked(move |_| {
            let Some(menu) = weak.upgrade() else { return };
            menu.popover.popdown();
            (menu.on_switch)(BranchCandidate {
                name: name.clone(),
                kind: BranchKind::Local,
                is_current: false,
            });
        });
        row
    }
}
