use super::*;
use gtk::graphene;

const WINDOW_WIDTH: i32 = 640;
const WINDOW_HEIGHT: i32 = 480;
const CONTENT_OFFSET_X: i32 = 48;
const CONTENT_OFFSET_Y: i32 = 36;
const CONTENT_WIDTH: i32 = WINDOW_WIDTH - CONTENT_OFFSET_X;
const CONTENT_HEIGHT: i32 = WINDOW_HEIGHT - CONTENT_OFFSET_Y;
const EDGE_INSET: f64 = 4.0;

struct PositionedMenu {
    window: gtk::Window,
    content: gtk::Box,
    popover: gtk::PopoverMenu,
}

impl PositionedMenu {
    fn open(click_x: f64, click_y: f64) -> Self {
        let test_repo = MenuTestRepo::new();
        let window = gtk::Window::builder()
            .default_width(WINDOW_WIDTH)
            .default_height(WINDOW_HEIGHT)
            .build();
        let fixed = gtk::Fixed::new();
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        content.set_size_request(CONTENT_WIDTH, CONTENT_HEIGHT);
        fixed.put(&content, CONTENT_OFFSET_X as f64, CONTENT_OFFSET_Y as f64);
        window.set_child(Some(&fixed));
        window.present();
        drain_main_context();

        install(
            &content,
            test_repo.tip,
            Rc::clone(&test_repo.repository_manager),
            || {},
        );
        emit_right_click(&content, click_x, click_y);
        drain_main_context();
        let popover = content
            .last_child()
            .unwrap()
            .downcast::<gtk::PopoverMenu>()
            .unwrap();

        Self {
            window,
            content,
            popover,
        }
    }

    fn click_in_window(&self, click_x: f64, click_y: f64) -> graphene::Point {
        self.content
            .compute_point(
                &self.window,
                &graphene::Point::new(click_x as f32, click_y as f32),
            )
            .unwrap()
    }

    fn popover_bounds(&self) -> graphene::Rect {
        popover_bounds(&self.popover)
    }
}

impl Drop for PositionedMenu {
    fn drop(&mut self) {
        self.popover.popdown();
        self.window.close();
        drain_main_context();
    }
}

fn drain_main_context() {
    while gtk::glib::MainContext::default().iteration(false) {}
}

fn emit_right_click(content: &gtk::Box, click_x: f64, click_y: f64) {
    let controllers = content.observe_controllers();
    let gesture = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::GestureClick>().ok())
        .find(|gesture| gesture.button() == gdk::BUTTON_SECONDARY)
        .unwrap();
    gesture.emit_by_name::<()>("pressed", &[&1_i32, &click_x, &click_y]);
}

fn popover_bounds(popover: &gtk::PopoverMenu) -> graphene::Rect {
    let surface = popover.native().unwrap().surface().unwrap();
    let popup = surface.downcast::<gdk::Popup>().unwrap();
    let (_, width, _, _) = popover.measure(gtk::Orientation::Horizontal, -1);
    let (_, height, _, _) = popover.measure(gtk::Orientation::Vertical, width);
    graphene::Rect::new(
        popup.position_x() as f32,
        popup.position_y() as f32,
        width as f32,
        height as f32,
    )
}

fn assert_inside_window(bounds: &graphene::Rect) {
    assert!(
        bounds.x() >= 0.0,
        "popover extends past left edge: {bounds:?}"
    );
    assert!(
        bounds.y() >= 0.0,
        "popover extends past top edge: {bounds:?}"
    );
    assert!(
        bounds.x() + bounds.width() <= WINDOW_WIDTH as f32,
        "popover extends past right edge: {bounds:?}"
    );
    assert!(
        bounds.y() + bounds.height() <= WINDOW_HEIGHT as f32,
        "popover extends past bottom edge: {bounds:?}"
    );
}

fn assert_would_overflow_right(click: &graphene::Point, bounds: &graphene::Rect) {
    assert!(
        click.x() + bounds.width() > WINDOW_WIDTH as f32,
        "unadjusted popover would not cross right edge: click={click:?}, bounds={bounds:?}"
    );
}

fn assert_would_overflow_bottom(click: &graphene::Point, bounds: &graphene::Rect) {
    assert!(
        click.y() + bounds.height() > WINDOW_HEIGHT as f32,
        "unadjusted popover would not cross bottom edge: click={click:?}, bounds={bounds:?}"
    );
}

#[gtk::test]
fn should_place_context_menu_to_bottom_right_of_regular_click() {
    let click_x = 180.0;
    let click_y = 140.0;
    let menu = PositionedMenu::open(click_x, click_y);

    let click = menu.click_in_window(click_x, click_y);
    let bounds = menu.popover_bounds();

    assert!(
        bounds.x() >= click.x(),
        "popover is not right of click: click={click:?}, bounds={bounds:?}"
    );
    assert!(
        bounds.y() >= click.y(),
        "popover is not below click: click={click:?}, bounds={bounds:?}"
    );
}

#[gtk::test]
fn should_keep_context_menu_inside_viewport_at_right_edge() {
    let click_x = CONTENT_WIDTH as f64 - EDGE_INSET;
    let click_y = CONTENT_HEIGHT as f64 / 2.0;
    let menu = PositionedMenu::open(click_x, click_y);

    let click = menu.click_in_window(click_x, click_y);
    let bounds = menu.popover_bounds();

    assert_would_overflow_right(&click, &bounds);
    assert_inside_window(&bounds);
}

#[gtk::test]
fn should_keep_context_menu_inside_viewport_at_bottom_edge() {
    let click_x = CONTENT_WIDTH as f64 / 2.0;
    let click_y = CONTENT_HEIGHT as f64 - EDGE_INSET;
    let menu = PositionedMenu::open(click_x, click_y);

    let click = menu.click_in_window(click_x, click_y);
    let bounds = menu.popover_bounds();

    assert_would_overflow_bottom(&click, &bounds);
    assert_inside_window(&bounds);
}

#[gtk::test]
fn should_keep_context_menu_inside_viewport_at_bottom_right_corner() {
    let click_x = CONTENT_WIDTH as f64 - EDGE_INSET;
    let click_y = CONTENT_HEIGHT as f64 - EDGE_INSET;
    let menu = PositionedMenu::open(click_x, click_y);

    let click = menu.click_in_window(click_x, click_y);
    let bounds = menu.popover_bounds();

    assert_would_overflow_right(&click, &bounds);
    assert_would_overflow_bottom(&click, &bounds);
    assert_inside_window(&bounds);
}

#[gtk::test]
fn should_use_parent_coordinates_when_parent_has_window_offset() {
    let click_x = 120.0;
    let click_y = 100.0;
    let menu = PositionedMenu::open(click_x, click_y);

    let click = menu.click_in_window(click_x, click_y);
    let bounds = menu.popover_bounds();

    assert!(click.x() >= CONTENT_OFFSET_X as f32);
    assert!(click.y() >= CONTENT_OFFSET_Y as f32);
    assert!(bounds.x() >= click.x());
    assert!(bounds.y() >= click.y());
}

#[gtk::test]
fn should_use_content_coordinates_after_scrolling() {
    let test_repo = MenuTestRepo::new();
    let window = gtk::Window::builder()
        .default_width(WINDOW_WIDTH)
        .default_height(WINDOW_HEIGHT)
        .build();
    let scrolled = gtk::ScrolledWindow::new();
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    content.set_size_request(WINDOW_WIDTH * 2, WINDOW_HEIGHT * 2);
    scrolled.set_child(Some(&content));
    window.set_child(Some(&scrolled));
    window.present();
    drain_main_context();
    let horizontal_scroll = 220.0;
    let vertical_scroll = 180.0;
    scrolled.hadjustment().set_value(horizontal_scroll);
    scrolled.vadjustment().set_value(vertical_scroll);
    drain_main_context();
    install(
        &content,
        test_repo.tip,
        Rc::clone(&test_repo.repository_manager),
        || {},
    );
    let click_x = horizontal_scroll + 140.0;
    let click_y = vertical_scroll + 120.0;

    emit_right_click(&content, click_x, click_y);
    drain_main_context();

    let popover = content
        .last_child()
        .unwrap()
        .downcast::<gtk::PopoverMenu>()
        .unwrap();
    let click = content
        .compute_point(
            &window,
            &graphene::Point::new(click_x as f32, click_y as f32),
        )
        .unwrap();
    let bounds = popover_bounds(&popover);
    assert!(scrolled.hadjustment().value() > 0.0);
    assert!(scrolled.vadjustment().value() > 0.0);
    assert!(bounds.x() >= click.x());
    assert!(bounds.y() >= click.y());
    popover.popdown();
    window.close();
    drain_main_context();
}
