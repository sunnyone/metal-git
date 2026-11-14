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
use crate::diff_model::{self, FileDiff, LineKind};
use split::SplitView;
use unified::UnifiedView;

const LAYOUTS: [DiffLayout; 2] = [DiffLayout::Unified, DiffLayout::Split];

fn layout_label(layout: DiffLayout) -> &'static str {
    match layout {
        DiffLayout::Unified => "Unified",
        DiffLayout::Split => "Split",
    }
}

/// The files a viewer shows and which of them are open, shared by its layouts
/// so that a file opened in one stays open in the other.
#[derive(Default)]
struct Content {
    files: Vec<FileDiff>,
    open: Vec<bool>,
}

type SharedContent = Rc<RefCell<Content>>;

pub struct DiffViewer {
    root: gtk::Box,
    summary: gtk::Label,
    layout_dropdown: gtk::DropDown,
    stack: gtk::Stack,
    unified: Rc<UnifiedView>,
    split: Rc<SplitView>,
    /// Kept to show again in the other layout
    content: SharedContent,
}

impl DiffViewer {
    pub fn new() -> Rc<DiffViewer> {
        let summary = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["dim-label"])
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
            layout_dropdown,
            stack,
            unified,
            split,
            content,
        });

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

    /// Shows files, those of a large diff closed at first. See
    /// `diff_model::initially_open`.
    pub fn show_files(&self, files: Vec<FileDiff>) {
        self.summary.set_text(&summary_text(&files));
        let open = diff_model::initially_open(&files);
        self.content.replace(Content { files, open });
        self.render();
    }

    /// Opens the file at an index of the files shown, and scrolls to it.
    pub fn scroll_to_file(&self, file_index: usize) {
        match self.layout() {
            DiffLayout::Unified => self.unified.scroll_to_file(file_index),
            DiffLayout::Split => self.split.scroll_to_file(file_index),
        }
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

fn summary_text(files: &[FileDiff]) -> String {
    let added: usize = files.iter().map(|f| f.count_lines(LineKind::Added)).sum();
    let removed: usize = files.iter().map(|f| f.count_lines(LineKind::Removed)).sum();
    let noun = if files.len() == 1 { "file" } else { "files" };
    format!("{} {}  +{} −{}", files.len(), noun, added, removed)
}
