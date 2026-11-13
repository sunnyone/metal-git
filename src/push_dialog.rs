use crate::push_operation::{self, commit_count_text, ConfirmMode};
use crate::remote_operation::{self, PushOptions, PushPreview, PushRequest, PushTarget};
use crate::repository_manager::RepositoryManager;
use git2::Error;
use gtk::gdk;
use gtk::glib::Propagation;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

// The commit list shows this many rows before it scrolls
const VISIBLE_COMMIT_ROWS: i32 = 5;

/// Asks where and how to push, then hands the confirmed request to `on_confirm`.
pub struct PushDialog {
    window: gtk::Window,
    repository_manager: Rc<RepositoryManager>,
    local_names: Vec<String>,
    remote_names: Vec<String>,
    // The remote the selected local branch normally pushes to
    default_remote: RefCell<String>,
    local_dropdown: gtk::DropDown,
    remote_dropdown: gtk::DropDown,
    remote_branch_entry: gtk::Entry,
    url_label: gtk::Label,
    info_label: gtk::Label,
    warning_box: gtk::Box,
    warning_label: gtk::Label,
    upstream_check: gtk::CheckButton,
    force_check: gtk::CheckButton,
    commits_title: gtk::Label,
    commit_list: gtk::ListBox,
    commit_scrolled: gtk::ScrolledWindow,
    commits_more_label: gtk::Label,
    cancel_button: gtk::Button,
    push_button: gtk::Button,
    preview: RefCell<Option<PushPreview>>,
    last_target: RefCell<Option<PushTarget>>,
    // Set while the dialog changes its own widgets, so their signals are ignored
    updating: Cell<bool>,
    on_confirm: Box<dyn Fn(PushRequest)>,
}

impl PushDialog {
    pub fn present(
        parent: &impl IsA<gtk::Window>,
        repository_manager: Rc<RepositoryManager>,
        on_confirm: impl Fn(PushRequest) + 'static,
    ) -> Result<(), Error> {
        let repo = repository_manager.open()?;
        let local_names = remote_operation::local_branch_names(&repo)?;
        let remote_names = remote_operation::remote_names(&repo)?;
        if remote_names.is_empty() {
            return Err(Error::from_str("No remotes are configured"));
        }
        let initial_branch = remote_operation::current_branch_name(&repo)
            .or_else(|| local_names.first().cloned())
            .ok_or_else(|| Error::from_str("There are no local branches to push"))?;
        drop(repo);

        install_css();

        let window = gtk::Window::builder()
            .transient_for(parent)
            .modal(true)
            .title("Push")
            .default_width(560)
            .resizable(false)
            .build();

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(16)
            .margin_end(16)
            .build();
        window.set_child(Some(&root));

        // Push to: local -> remote / remote branch
        root.append(&section_label("Push to"));
        let grid = gtk::Grid::builder()
            .row_spacing(6)
            .column_spacing(8)
            .margin_start(12)
            .build();
        let local_refs: Vec<&str> = local_names.iter().map(String::as_str).collect();
        let local_dropdown = gtk::DropDown::from_strings(&local_refs);
        local_dropdown.set_hexpand(true);
        let remote_refs: Vec<&str> = remote_names.iter().map(String::as_str).collect();
        let remote_dropdown = gtk::DropDown::from_strings(&remote_refs);
        let remote_branch_entry = gtk::Entry::builder()
            .hexpand(true)
            .activates_default(true)
            .build();
        let url_label = gtk::Label::builder()
            .xalign(0.0)
            .selectable(true)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .css_classes(["dim-label", "monospace"])
            .build();
        let info_label = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        grid.attach(&caption_label("Local branch"), 0, 0, 1, 1);
        grid.attach(&caption_label("Remote"), 2, 0, 1, 1);
        grid.attach(&caption_label("Remote branch"), 4, 0, 1, 1);
        grid.attach(&local_dropdown, 0, 1, 1, 1);
        grid.attach(&gtk::Label::new(Some("\u{2192}")), 1, 1, 1, 1);
        grid.attach(&remote_dropdown, 2, 1, 1, 1);
        grid.attach(&gtk::Label::new(Some("/")), 3, 1, 1, 1);
        grid.attach(&remote_branch_entry, 4, 1, 1, 1);
        grid.attach(&url_label, 2, 2, 3, 1);
        grid.attach(&info_label, 0, 3, 5, 1);
        root.append(&grid);

        let warning_label = gtk::Label::builder().xalign(0.0).wrap(true).build();
        let warning_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .margin_start(12)
            .css_classes(["push-warning"])
            .visible(false)
            .build();
        warning_box.append(&warning_label);
        root.append(&warning_box);

        // Options
        root.append(&section_label("Options"));
        let options_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .margin_start(12)
            .build();
        let upstream_check = gtk::CheckButton::new();
        let force_check = gtk::CheckButton::with_label("Force push (with lease)");
        options_box.append(&upstream_check);
        options_box.append(&force_check);
        root.append(&options_box);

        // Commits
        let commits_title = section_label("");
        root.append(&commits_title);
        let commit_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build();
        let commit_scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .margin_start(12)
            .css_classes(["push-commit-list"])
            .child(&commit_list)
            .build();
        root.append(&commit_scrolled);
        let commits_more_label = gtk::Label::builder()
            .xalign(0.0)
            .margin_start(12)
            .css_classes(["dim-label"])
            .visible(false)
            .build();
        root.append(&commits_more_label);

        let buttons = gtk::Box::builder()
            .spacing(8)
            .halign(gtk::Align::End)
            .margin_top(8)
            .build();
        let cancel_button = gtk::Button::with_label("Cancel");
        let push_button = gtk::Button::with_label("Push");
        buttons.append(&cancel_button);
        buttons.append(&push_button);
        root.append(&buttons);

        let dialog = Rc::new(PushDialog {
            window,
            repository_manager,
            local_names,
            remote_names,
            default_remote: RefCell::new(String::new()),
            local_dropdown,
            remote_dropdown,
            remote_branch_entry,
            url_label,
            info_label,
            warning_box,
            warning_label,
            upstream_check,
            force_check,
            commits_title,
            commit_list,
            commit_scrolled,
            commits_more_label,
            cancel_button,
            push_button,
            preview: RefCell::new(None),
            last_target: RefCell::new(None),
            updating: Cell::new(false),
            on_confirm: Box::new(on_confirm),
        });

        dialog.select_local_branch(&initial_branch);
        dialog.setup_callbacks();
        dialog.window.present();
        if dialog.window.default_widget().is_some() {
            dialog.push_button.grab_focus();
        } else {
            dialog.cancel_button.grab_focus();
        }
        Ok(())
    }

    fn setup_callbacks(self: &Rc<Self>) {
        // The window owns the dialog until it closes
        let owner = RefCell::new(Some(self.clone()));
        self.window.connect_close_request(move |_| {
            owner.borrow_mut().take();
            Propagation::Proceed
        });

        let keys = gtk::EventControllerKey::new();
        let window = self.window.downgrade();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gdk::Key::Escape {
                if let Some(window) = window.upgrade() {
                    window.close();
                }
                return Propagation::Stop;
            }
            Propagation::Proceed
        });
        self.window.add_controller(keys);

        let d = Rc::downgrade(self);
        self.local_dropdown.connect_selected_notify(move |dropdown| {
            with_dialog(&d, |d| {
                if let Some(name) = d.local_names.get(dropdown.selected() as usize) {
                    let name = name.clone();
                    d.select_local_branch(&name);
                }
            });
        });

        let d = Rc::downgrade(self);
        self.remote_dropdown.connect_selected_notify(move |_| with_dialog(&d, |d| d.update()));
        let d = Rc::downgrade(self);
        self.remote_branch_entry.connect_changed(move |_| with_dialog(&d, |d| d.update()));
        let d = Rc::downgrade(self);
        self.force_check.connect_toggled(move |_| with_dialog(&d, |d| d.update()));

        let window = self.window.downgrade();
        self.cancel_button.connect_clicked(move |_| {
            if let Some(window) = window.upgrade() {
                close_later(&window);
            }
        });

        let d = Rc::downgrade(self);
        self.push_button.connect_clicked(move |_| {
            if let Some(d) = d.upgrade() {
                d.push_clicked();
            }
        });
    }

    /// Selects a local branch and fills in where it is pushed by default.
    fn select_local_branch(&self, local_branch: &str) {
        let target = self
            .repository_manager
            .open()
            .and_then(|repo| remote_operation::default_push_target(&repo, local_branch));

        self.updating.set(true);
        if let Some(index) = self.local_names.iter().position(|name| name == local_branch) {
            self.local_dropdown.set_selected(index as u32);
        }
        if let Ok(target) = &target {
            if let Some(index) = self.remote_names.iter().position(|name| *name == target.remote_name) {
                self.remote_dropdown.set_selected(index as u32);
            }
            self.remote_branch_entry.set_text(&target.remote_branch);
            self.default_remote.replace(target.remote_name.clone());
        }
        self.updating.set(false);
        self.update();
    }

    fn current_target(&self) -> Option<PushTarget> {
        Some(PushTarget {
            local_branch: self.local_names.get(self.local_dropdown.selected() as usize)?.clone(),
            remote_name: self.remote_names.get(self.remote_dropdown.selected() as usize)?.clone(),
            remote_branch: self.remote_branch_entry.text().trim().to_string(),
        })
    }

    fn current_options(&self) -> PushOptions {
        PushOptions {
            set_upstream: self.upstream_check.is_active() && self.upstream_check.is_sensitive(),
            force_with_lease: self.force_check.is_active(),
        }
    }

    /// Recomputes everything shown from the selected branches and options.
    fn update(&self) {
        let Some(target) = self.current_target() else { return };

        if !remote_operation::is_valid_branch_name(&target.remote_branch) {
            self.preview.replace(None);
            let message = if target.remote_branch.is_empty() {
                "Enter the remote branch name.".to_string()
            } else {
                format!("{} is not a valid branch name.", target.remote_branch)
            };
            self.show_problem(&message);
            return;
        }

        let preview = self
            .repository_manager
            .open()
            .and_then(|repo| remote_operation::preview_push(&repo, &target));
        let preview = match preview {
            Ok(preview) => preview,
            Err(error) => {
                self.preview.replace(None);
                self.show_problem(error.message());
                return;
            }
        };

        let destination = target.remote_ref_display();
        let target_changed = self.last_target.borrow().as_ref() != Some(&target);
        self.last_target.replace(Some(target.clone()));

        self.window.set_title(Some("Push"));
        self.url_label.set_text(&preview.display_url);
        self.url_label.set_tooltip_text(Some(&preview.display_url));
        self.info_label.set_text(&describe_info(&target, &preview));
        self.update_upstream_check(&destination, &preview, target_changed);

        let options = self.current_options();
        let default_remote = self.default_remote.borrow().clone();
        let warnings = push_operation::push_warnings(&target, &preview, &options, &default_remote);
        self.show_warnings(&warnings);
        self.show_commits(&target, &preview);

        let mut mode = push_operation::confirm_mode(&target, &options, &default_remote);
        if mode == ConfirmMode::Routine && !warnings.is_empty() {
            mode = ConfirmMode::Careful;
        }
        self.update_push_button(mode);
        self.preview.replace(Some(preview));
    }

    fn update_upstream_check(&self, destination: &str, preview: &PushPreview, target_changed: bool) {
        if preview.upstream.as_deref() == Some(destination) {
            self.upstream_check.set_label(Some(&format!("Set as upstream (already {})", destination)));
            self.upstream_check.set_active(true);
            self.upstream_check.set_sensitive(false);
            return;
        }

        self.upstream_check.set_label(Some(&format!("Set {} as upstream", destination)));
        self.upstream_check.set_sensitive(true);
        // Pushing somewhere else should not move an existing upstream unless asked
        if target_changed {
            self.upstream_check.set_active(preview.upstream.is_none());
        }
    }

    fn show_warnings(&self, warnings: &[String]) {
        let text = warnings
            .iter()
            .map(|warning| format!("\u{26a0} {}", warning))
            .collect::<Vec<_>>()
            .join("\n");
        self.warning_label.set_text(&text);
        self.warning_box.remove_css_class("push-error");
        self.warning_box.set_visible(!warnings.is_empty());
    }

    /// Shows why the push cannot start, and disables it.
    fn show_problem(&self, message: &str) {
        self.warning_label.set_text(message);
        self.warning_box.add_css_class("push-error");
        self.warning_box.set_visible(true);
        self.push_button.set_sensitive(false);
        self.window.set_default_widget(None::<&gtk::Widget>);
    }

    fn show_commits(&self, target: &PushTarget, preview: &PushPreview) {
        let outgoing = &preview.outgoing;
        let count = if outgoing.count_is_capped {
            format!("{}+", format_thousands(outgoing.count))
        } else {
            format_thousands(outgoing.count)
        };
        self.commits_title.set_text(&if preview.creates_branch() {
            format!("Commits not on {} ({})", target.remote_name, count)
        } else {
            format!("Commits to push ({})", count)
        });

        while let Some(row) = self.commit_list.row_at_index(0) {
            self.commit_list.remove(&row);
        }
        if outgoing.commits.is_empty() {
            let label = gtk::Label::builder()
                .label("Nothing to push")
                .xalign(0.0)
                .margin_top(4)
                .margin_bottom(4)
                .margin_start(8)
                .css_classes(["dim-label"])
                .build();
            self.commit_list.append(&label);
        }
        for commit in &outgoing.commits {
            let row = gtk::Box::builder()
                .spacing(12)
                .margin_top(4)
                .margin_bottom(4)
                .margin_start(8)
                .margin_end(8)
                .build();
            row.append(&gtk::Label::builder().label(&commit.short_id).css_classes(["dim-label", "monospace"]).build());
            row.append(
                &gtk::Label::builder()
                    .label(&commit.summary)
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .build(),
            );
            self.commit_list.append(&row);
        }

        // Fit whole rows, measured from a real row so the theme's padding is included.
        // A short list does not scroll, so the scrollbar does not add to its height.
        let rows = outgoing.commits.len().max(1) as i32;
        if rows > VISIBLE_COMMIT_ROWS {
            if let Some(row) = self.commit_list.row_at_index(0) {
                let (_, row_height, _, _) = row.measure(gtk::Orientation::Vertical, -1);
                self.commit_scrolled.set_max_content_height(row_height * VISIBLE_COMMIT_ROWS);
            }
            self.commit_scrolled.set_vscrollbar_policy(gtk::PolicyType::Automatic);
        } else {
            self.commit_scrolled.set_max_content_height(-1);
            self.commit_scrolled.set_vscrollbar_policy(gtk::PolicyType::Never);
        }

        let hidden = outgoing.count - outgoing.commits.len();
        self.commits_more_label.set_visible(hidden > 0);
        if outgoing.count_is_capped {
            self.commits_more_label.set_text("\u{2026} more commits are not shown");
        } else if hidden > 0 {
            self.commits_more_label.set_text(&format!(
                "\u{2026} {} more {} not shown",
                format_thousands(hidden),
                if hidden == 1 { "commit is" } else { "commits are" }
            ));
        }
    }

    fn update_push_button(&self, mode: ConfirmMode) {
        let label = match mode {
            ConfirmMode::Force => "Force Push\u{2026}",
            _ => "Push",
        };
        self.push_button.set_label(label);
        self.push_button.set_sensitive(true);
        if mode == ConfirmMode::Force {
            self.push_button.remove_css_class("suggested-action");
            self.push_button.add_css_class("destructive-action");
        } else {
            self.push_button.remove_css_class("destructive-action");
            self.push_button.add_css_class("suggested-action");
        }

        // Enter pushes only when nothing about this push is unusual
        if mode == ConfirmMode::Routine {
            self.window.set_default_widget(Some(&self.push_button));
        } else {
            self.window.set_default_widget(None::<&gtk::Widget>);
        }
    }

    fn push_clicked(self: &Rc<Self>) {
        let (Some(target), Some(preview)) = (self.current_target(), self.preview.borrow().clone()) else {
            return;
        };
        let request = PushRequest {
            target,
            options: self.current_options(),
            expected_remote_oid: preview.remote_oid,
        };

        if request.options.force_with_lease {
            self.confirm_force_push(request, &preview);
        } else {
            self.confirm(request);
        }
    }

    fn confirm_force_push(self: &Rc<Self>, request: PushRequest, preview: &PushPreview) {
        let destination = request.target.remote_ref_display();
        let mut detail = format!(
            "{} will be overwritten with your local {}.\nThe push is refused if {} has changed since your last fetch.",
            destination, request.target.local_branch, destination
        );
        if preview.remote_only_commits > 0 {
            detail.push_str(&format!(
                "\n\n{} on {} will be removed from the remote branch.",
                commit_count_text(preview.remote_only_commits),
                destination
            ));
        }

        let confirmation = gtk::MessageDialog::builder()
            .transient_for(&self.window)
            .modal(true)
            .message_type(gtk::MessageType::Warning)
            .buttons(gtk::ButtonsType::None)
            .text(format!("Force push {} to {}?", request.target.local_branch, destination))
            .secondary_text(detail)
            .build();
        confirmation.add_button("Cancel", gtk::ResponseType::Cancel);
        confirmation
            .add_button("Force Push", gtk::ResponseType::Accept)
            .add_css_class("destructive-action");
        confirmation.set_default_response(gtk::ResponseType::Cancel);

        let d = Rc::downgrade(self);
        confirmation.connect_response(move |confirmation, response| {
            confirmation.close();
            if response == gtk::ResponseType::Accept {
                if let Some(d) = d.upgrade() {
                    d.confirm(request.clone());
                }
            }
        });
        confirmation.present();
    }

    fn confirm(&self, request: PushRequest) {
        (self.on_confirm)(request);
        close_later(&self.window);
    }
}

/// Closes the window once the click that asked for it has finished,
/// so the button is not destroyed while it is still pressed.
fn close_later(window: &gtk::Window) {
    let window = window.downgrade();
    gtk::glib::idle_add_local_once(move || {
        if let Some(window) = window.upgrade() {
            window.close();
        }
    });
}

fn with_dialog(dialog: &Weak<PushDialog>, f: impl FnOnce(&PushDialog)) {
    if let Some(dialog) = dialog.upgrade() {
        if !dialog.updating.get() {
            f(&dialog);
        }
    }
}

fn describe_info(target: &PushTarget, preview: &PushPreview) -> String {
    let destination = target.remote_ref_display();
    if preview.creates_branch() {
        return format!("{} does not exist yet. A new remote branch will be created.", destination);
    }
    let pending = if preview.outgoing.count == 0 {
        format!("{} is up to date", destination)
    } else if preview.outgoing.count_is_capped {
        format!("{}+ commits to push", format_thousands(preview.outgoing.count))
    } else {
        format!("{} to push", commit_count_text(preview.outgoing.count))
    };
    match &preview.upstream {
        Some(upstream) => format!("Upstream: {}  \u{00b7}  {}", upstream, pending),
        None => format!("No upstream  \u{00b7}  {}", pending),
    }
}

fn format_thousands(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn section_label(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .css_classes(["heading"])
        .build()
}

fn caption_label(text: &str) -> gtk::Label {
    gtk::Label::builder().label(text).xalign(0.0).build()
}

fn install_css() {
    thread_local! {
        static INSTALLED: Cell<bool> = const { Cell::new(false) };
    }
    if INSTALLED.with(|installed| installed.replace(true)) {
        return;
    }

    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "
        .push-warning {
            padding: 8px;
            border-radius: 6px;
            border: 1px solid #e5a50a;
            background-color: alpha(#e5a50a, 0.15);
        }

        .push-warning.push-error {
            border-color: #e01b24;
            background-color: alpha(#e01b24, 0.12);
        }

        .push-commit-list {
            border: 1px solid alpha(currentColor, 0.25);
            border-radius: 4px;
        }

        label.heading {
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
