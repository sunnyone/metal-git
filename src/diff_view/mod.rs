//! The diff view: a summary line above the files of a diff.

mod diff_text_view;
mod gutter;
mod text;
mod unified;

use gtk::prelude::*;

use crate::diff_model::{FileDiff, LineKind};
use unified::UnifiedView;

pub struct DiffViewer {
    root: gtk::Box,
    summary: gtk::Label,
    unified: UnifiedView,
}

impl DiffViewer {
    pub fn new() -> DiffViewer {
        let summary = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["dim-label"])
            .margin_start(6)
            .margin_end(6)
            .margin_top(4)
            .margin_bottom(4)
            .build();

        let unified = UnifiedView::new();

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .build();
        root.append(&summary);
        root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        root.append(&unified.widget());

        DiffViewer {
            root,
            summary,
            unified,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn show_files(&self, files: &[FileDiff]) {
        self.summary.set_text(&summary_text(files));
        self.unified.show(files);
    }
}

fn summary_text(files: &[FileDiff]) -> String {
    let added: usize = files.iter().map(|f| f.count_lines(LineKind::Added)).sum();
    let removed: usize = files.iter().map(|f| f.count_lines(LineKind::Removed)).sum();
    let noun = if files.len() == 1 { "file" } else { "files" };
    format!("{} {}  +{} −{}", files.len(), noun, added, removed)
}
