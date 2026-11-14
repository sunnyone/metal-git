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
use crate::diff_model::{FileDiff, LineKind};
use split::SplitView;
use unified::UnifiedView;

const LAYOUTS: [DiffLayout; 2] = [DiffLayout::Unified, DiffLayout::Split];

fn layout_label(layout: DiffLayout) -> &'static str {
    match layout {
        DiffLayout::Unified => "Unified",
        DiffLayout::Split => "Split",
    }
}

pub struct DiffViewer {
    root: gtk::Box,
    summary: gtk::Label,
    layout_dropdown: gtk::DropDown,
    stack: gtk::Stack,
    unified: UnifiedView,
    split: SplitView,
    /// Kept to show again in the other layout
    files: RefCell<Vec<FileDiff>>,
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

        let unified = UnifiedView::new();
        let split = SplitView::new();
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
            files: RefCell::new(Vec::new()),
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

    pub fn show_files(&self, files: &[FileDiff]) {
        self.files.replace(files.to_vec());
        self.summary.set_text(&summary_text(files));
        self.render();
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
        let files = self.files.borrow();
        match layout {
            DiffLayout::Unified => self.unified.show(&files),
            DiffLayout::Split => self.split.show(&files),
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
