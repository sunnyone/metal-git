//! The diff view: a summary line and a choice of layout above the files of a diff.

mod diff_text_view;
mod gutter;
mod split;
mod text;
mod unified;

use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::app_settings::{self, DiffLayout};
use crate::diff::{self, DiffSettings, FileDiff, FileStatus, LineKind, OldContentReader, Reveal};
use split::SplitView;
use text::{RevealFileGap, ToggleViewed};
use unified::UnifiedView;

const LAYOUTS: [DiffLayout; 2] = [DiffLayout::Unified, DiffLayout::Split];

fn layout_label(layout: DiffLayout) -> &'static str {
    match layout {
        DiffLayout::Unified => "Unified",
        DiffLayout::Split => "Split",
    }
}

/// The color of the badge of a status, in the colors many Git clients use:
/// green for added, red for deleted, amber for modified, and blue for renamed
/// or copied. The letter on it is white.
pub fn status_color(status: FileStatus) -> &'static str {
    match status {
        FileStatus::Added => "#2ea043",
        FileStatus::Deleted => "#da3633",
        FileStatus::Modified => "#bf8700",
        FileStatus::Renamed | FileStatus::Copied => "#3878e6",
        FileStatus::Other => "#7f7f7f",
    }
}

/// Pango markup of the badge of a status: its letter, white and bold on its
/// color, one monospace letter wide so that what follows lines up.
pub fn status_badge_markup(status: FileStatus) -> String {
    format!(
        "<span font_family=\"monospace\" weight=\"bold\" foreground=\"white\" background=\"{}\"> {} </span>",
        status_color(status),
        status.letter()
    )
}

/// The files a viewer shows and which of them are open, shared by its layouts
/// so that a file opened in one stays open in the other.
#[derive(Default)]
struct Content {
    files: Vec<FileDiff>,
    open: Vec<bool>,
    viewed: Vec<bool>,
}

type SharedContent = Rc<RefCell<Content>>;

/// Takes the diff to show, as the settings say.
pub type DiffSource = Rc<dyn Fn(DiffSettings) -> Result<Vec<FileDiff>, git2::Error>>;

pub struct DiffViewer {
    root: gtk::Box,
    summary: gtk::Label,
    whitespace_check: gtk::CheckButton,
    layout_dropdown: gtk::DropDown,
    stack: gtk::Stack,
    unified: Rc<UnifiedView>,
    split: Rc<SplitView>,
    /// Kept to show again in the other layout
    content: SharedContent,
    /// Reads the old versions of files, which hold the unchanged lines that
    /// the diff leaves out
    read_old_content: OldContentReader,
    viewed_changed: RefCell<Option<ViewedChanged>>,
    /// Asked again when the settings change
    source: RefCell<Option<DiffSource>>,
}

/// Called with which of the files shown are viewed, whenever that changes.
pub type ViewedChanged = Box<dyn Fn(&[bool])>;

impl DiffViewer {
    pub fn new(read_old_content: OldContentReader) -> Rc<DiffViewer> {
        let summary = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["dim-label"])
            .build();

        let whitespace_check = gtk::CheckButton::builder()
            .label("Hide whitespace")
            .tooltip_text("Leave out changes of whitespace")
            .active(app_settings::load_hide_whitespace(
                &app_settings::whitespace_path(),
            ))
            .build();

        let labels: Vec<&str> = LAYOUTS.iter().map(|l| layout_label(*l)).collect();
        let layout_dropdown = gtk::DropDown::from_strings(&labels);
        layout_dropdown.set_tooltip_text(Some("Diff layout"));

        let toolbar = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .margin_start(6)
            .margin_end(6)
            .margin_top(4)
            .margin_bottom(4)
            .build();
        toolbar.append(&summary);
        toolbar.append(&whitespace_check);
        toolbar.append(&layout_dropdown);

        let content = SharedContent::default();
        let unified = UnifiedView::new(content.clone());
        let split = SplitView::new(content.clone());
        let stack = gtk::Stack::builder().hexpand(true).vexpand(true).build();
        stack.add_named(&unified.widget(), Some(layout_label(DiffLayout::Unified)));
        stack.add_named(&split.widget(), Some(layout_label(DiffLayout::Split)));

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        root.append(&toolbar);
        root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        root.append(&stack);

        let layout = app_settings::load_diff_layout(&app_settings::diff_layout_path());
        layout_dropdown.set_selected(LAYOUTS.iter().position(|l| *l == layout).unwrap() as u32);
        stack.set_visible_child_name(layout_label(layout));

        let viewer = Rc::new(DiffViewer {
            root,
            summary,
            whitespace_check,
            layout_dropdown,
            stack,
            unified,
            split,
            content,
            read_old_content,
            viewed_changed: RefCell::new(None),
            source: RefCell::new(None),
        });

        let w = Rc::downgrade(&viewer);
        viewer.whitespace_check.connect_toggled(move |_| {
            if let Some(viewer) = w.upgrade() {
                viewer.whitespace_toggled();
            }
        });

        let w = Rc::downgrade(&viewer);
        let reveal: RevealFileGap = Rc::new(move |file_index, before_hunk, how| {
            if let Some(viewer) = w.upgrade() {
                dialog_when_error!(
                    "Failed to show the unchanged lines: {}",
                    viewer.reveal(file_index, before_hunk, how)
                );
            }
        });
        viewer.unified.connect_reveal(reveal.clone());
        viewer.split.connect_reveal(reveal);

        let w = Rc::downgrade(&viewer);
        let toggle_viewed: ToggleViewed = Rc::new(move |file_index| {
            if let Some(viewer) = w.upgrade() {
                viewer.toggle_viewed(file_index);
            }
        });
        viewer.unified.connect_toggle_viewed(toggle_viewed.clone());
        viewer.split.connect_toggle_viewed(toggle_viewed);

        let w = Rc::downgrade(&viewer);
        viewer.layout_dropdown.connect_selected_notify(move |_| {
            if let Some(viewer) = w.upgrade() {
                viewer.layout_changed();
            }
        });

        viewer
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    /// Sets what is told which of the files shown are viewed, whenever that
    /// changes.
    pub fn connect_viewed_changed(&self, viewed_changed: impl Fn(&[bool]) + 'static) {
        self.viewed_changed.replace(Some(Box::new(viewed_changed)));
    }

    /// Shows the diff a source takes, and takes it again when the settings
    /// change.
    pub fn show_diff(&self, source: DiffSource) -> Result<(), git2::Error> {
        self.source.replace(Some(source));
        self.refresh()
    }

    /// Shows files that do not depend on the settings, such as a file that is
    /// not tracked yet.
    pub fn show_fixed_files(&self, files: Vec<FileDiff>) {
        self.source.replace(None);
        self.show_files(files);
    }

    fn settings(&self) -> DiffSettings {
        DiffSettings {
            ignore_whitespace: self.whitespace_check.is_active(),
        }
    }

    /// Takes the diff again from the source.
    fn refresh(&self) -> Result<(), git2::Error> {
        let Some(source) = self.source.borrow().clone() else {
            return Ok(());
        };
        let settings = self.settings();
        let mut files = source(settings)?;
        for file in &mut files {
            file.whitespace_ignored = settings.ignore_whitespace;
        }
        self.show_files(files);
        Ok(())
    }

    fn whitespace_toggled(&self) {
        let hide = self.whitespace_check.is_active();
        if let Err(err) = app_settings::save_hide_whitespace(&app_settings::whitespace_path(), hide)
        {
            eprintln!("Failed to save whether whitespace is hidden: {}", err);
        }
        dialog_when_error!("Failed to show the diff: {}", self.refresh());
    }

    /// Shows files, those of a large diff and those viewed before closed at
    /// first. See `diff::initially_open`.
    fn show_files(&self, files: Vec<FileDiff>) {
        let viewed_keys = app_settings::load_viewed_changes(&app_settings::viewed_changes_path());
        let viewed: Vec<bool> = files
            .iter()
            .map(|file| viewed_keys.contains(&file.change_key()))
            .collect();
        let open = diff::initially_open(&files)
            .into_iter()
            .zip(&viewed)
            .map(|(open, &viewed)| open && !viewed)
            .collect();
        self.content.replace(Content {
            files,
            open,
            viewed,
        });
        self.update_summary();
        self.render();
        self.notify_viewed();
    }

    /// Marks a file as viewed and closes it, or unmarks it and opens it.
    fn toggle_viewed(&self, file_index: usize) {
        let (key, viewed) = {
            let mut content = self.content.borrow_mut();
            let Some(&viewed) = content.viewed.get(file_index) else {
                return;
            };
            content.viewed[file_index] = !viewed;
            (content.files[file_index].change_key(), !viewed)
        };
        if let Err(err) =
            app_settings::save_change_viewed(&app_settings::viewed_changes_path(), &key, viewed)
        {
            eprintln!("Failed to save the viewed files: {}", err);
        }
        match self.layout() {
            DiffLayout::Unified => self.unified.set_file_viewed(file_index, viewed),
            DiffLayout::Split => self.split.set_file_viewed(file_index, viewed),
        }
        self.update_summary();
        self.notify_viewed();
    }

    fn update_summary(&self) {
        let content = self.content.borrow();
        self.summary
            .set_text(&summary_text(&content.files, &content.viewed));
    }

    fn notify_viewed(&self) {
        if let Some(viewed_changed) = self.viewed_changed.borrow().as_ref() {
            viewed_changed(&self.content.borrow().viewed);
        }
    }

    /// Opens the file at an index of the files shown, and scrolls to it.
    pub fn scroll_to_file(&self, file_index: usize) {
        match self.layout() {
            DiffLayout::Unified => self.unified.scroll_to_file(file_index),
            DiffLayout::Split => self.split.scroll_to_file(file_index),
        }
    }

    /// Reveals unchanged lines of a gap of a file, reading its old version
    /// the first time, and draws the file again. The other layout draws the
    /// revealed lines when it is shown.
    fn reveal(
        &self,
        file_index: usize,
        before_hunk: usize,
        how: Reveal,
    ) -> Result<(), git2::Error> {
        {
            let mut content = self.content.borrow_mut();
            let Some(file) = content.files.get_mut(file_index) else {
                return Ok(());
            };
            if file.old_content.is_none() {
                file.old_content = Some((self.read_old_content)(file)?);
            }
            if !file.reveal(before_hunk, how) {
                return Ok(());
            }
        }
        match self.layout() {
            DiffLayout::Unified => self.unified.reload_file(file_index),
            DiffLayout::Split => self.split.reload_file(file_index),
        }
        Ok(())
    }

    fn layout(&self) -> DiffLayout {
        LAYOUTS[self.layout_dropdown.selected() as usize]
    }

    fn layout_changed(&self) {
        if let Err(err) =
            app_settings::save_diff_layout(&app_settings::diff_layout_path(), self.layout())
        {
            eprintln!("Failed to save the diff layout: {}", err);
        }
        self.render();
    }

    fn render(&self) {
        let layout = self.layout();
        match layout {
            DiffLayout::Unified => self.unified.show(),
            DiffLayout::Split => self.split.show(),
        }
        self.stack.set_visible_child_name(layout_label(layout));
    }
}

fn summary_text(files: &[FileDiff], viewed: &[bool]) -> String {
    let added: usize = files.iter().map(|f| f.count_lines(LineKind::Added)).sum();
    let removed: usize = files.iter().map(|f| f.count_lines(LineKind::Removed)).sum();
    let noun = if files.len() == 1 { "file" } else { "files" };
    let viewed_count = viewed.iter().filter(|&&v| v).count();
    format!(
        "{} {}  +{} −{}    {}/{} viewed",
        files.len(),
        noun,
        added,
        removed,
        viewed_count,
        files.len()
    )
}
