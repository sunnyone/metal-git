use super::diff_text_view::DiffTextView;
use super::*;
use gtk::{gdk, glib};
use std::cell::Cell;
use std::time::{Duration, Instant};

thread_local! {
    static ALLOCATION_WARNINGS: Cell<usize> = const { Cell::new(0) };
}

/// Counts the warnings GTK gives when it draws a widget whose allocation was
/// invalidated after the layout of the frame, and lets every message through.
/// GTK logs structured messages, which only a writer function sees.
fn count_allocation_warnings() {
    glib::log_set_writer_func(|level, fields| {
        let is_allocation_warning = fields.iter().any(|field| {
            field.key() == "MESSAGE"
                && field
                    .value_str()
                    .is_some_and(|message| message.contains("without a current allocation"))
        });
        if is_allocation_warning {
            ALLOCATION_WARNINGS.with(|count| count.set(count.get() + 1));
        }
        glib::log_writer_default(level, fields)
    });
}

fn allocation_warnings() -> usize {
    ALLOCATION_WARNINGS.with(|count| count.get())
}

/// Runs the main loop, drawing frames, for a while.
fn run_frames(duration: Duration) {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        let _wake = glib::timeout_add_local_once(Duration::from_millis(4), || {});
        context.iteration(true);
        while context.iteration(false) {}
    }
}

fn visible_text_views(viewer: &DiffViewer) -> Vec<DiffTextView> {
    let mut views = Vec::new();
    let mut pending = vec![viewer.stack.visible_child().unwrap()];
    while let Some(widget) = pending.pop() {
        if let Ok(view) = widget.clone().downcast::<DiffTextView>() {
            views.push(view);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    views
}

/// Clicks the head line of the first file, as a click on it does, which
/// opens or closes the file.
fn click_first_file_head(view: &DiffTextView) {
    let gesture = view
        .observe_controllers()
        .into_iter()
        .filter_map(|controller| controller.ok())
        .filter_map(|controller| controller.downcast::<gtk::GestureClick>().ok())
        // The view has a gesture of its own for any button
        .find(|gesture| gesture.button() == gdk::BUTTON_PRIMARY)
        .unwrap();
    // The middle of a character of the path on the head line
    let char_rect = view.iter_location(&view.buffer().iter_at_offset(6));
    let (x, y) = view.buffer_to_window_coords(
        gtk::TextWindowType::Widget,
        char_rect.x() + char_rect.width() / 2,
        char_rect.y() + char_rect.height() / 2,
    );
    let (x, y) = (f64::from(x), f64::from(y));
    gesture.emit_by_name::<()>("released", &[&1_i32, &x, &y]);
}

fn files() -> Vec<FileDiff> {
    (0..3)
        .map(|n| {
            let text: String = (0..200).map(|i| format!("file {n} line {i}\n")).collect();
            FileDiff::all_added(&format!("file{n}.txt"), &text)
        })
        .collect()
}

/// GTK handles input at the start of a frame, before its layout. A click that
/// folds lines there must have them laid out in the layout of that frame, not
/// while the frame is drawn, which changes the scroll bars and, in the split
/// layout, the other side, after they were allocated.
#[test]
fn should_lay_out_lines_folded_during_a_frame_before_drawing_them() {
    crate::gtk_test::run(should_lay_out_lines_folded_during_a_frame_before_drawing_them_on_gtk_thread);
}

fn should_lay_out_lines_folded_during_a_frame_before_drawing_them_on_gtk_thread() {
    // The diff layout is saved under the user's config directory
    let config_dir = tempdir::TempDir::new("diff-view-config").unwrap();
    std::env::set_var("XDG_CONFIG_HOME", config_dir.path());
    count_allocation_warnings();

    let viewer = DiffViewer::new();
    let window = gtk::Window::builder()
        .default_width(800)
        .default_height(400)
        .child(&viewer.widget())
        .build();
    window.present();

    for layout in 0..LAYOUTS.len() as u32 {
        viewer.layout_dropdown.set_selected(layout);
        viewer.show_files(files());
        run_frames(Duration::from_millis(200));

        let views = visible_text_views(&viewer);
        // Overlay scroll bars are drawn only for a while after the pointer
        // moves, and a warning comes only from those that are drawn
        for view in &views {
            view.parent()
                .and_downcast::<gtk::ScrolledWindow>()
                .unwrap()
                .set_overlay_scrolling(false);
        }
        run_frames(Duration::from_millis(100));
        let view = views.last().unwrap().clone();
        let clicks = Rc::new(Cell::new(0));
        let markers = Rc::new(RefCell::new(String::new()));
        let tick_clicks = clicks.clone();
        let tick_markers = markers.clone();
        // A tick callback runs in the frame, before its layout, as input does
        view.add_tick_callback(move |view, _| {
            click_first_file_head(view);
            tick_markers.borrow_mut().push(view.buffer().start_iter().char());
            tick_clicks.set(tick_clicks.get() + 1);
            if tick_clicks.get() < 10 {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while clicks.get() < 10 && Instant::now() < deadline {
            run_frames(Duration::from_millis(20));
        }
        run_frames(Duration::from_millis(100));
        assert_eq!(10, clicks.get(), "frames were not drawn");
        // Each click closed or opened the file
        assert_eq!("▸▾▸▾▸▾▸▾▸▾", *markers.borrow());
    }

    assert_eq!(0, allocation_warnings());
}
