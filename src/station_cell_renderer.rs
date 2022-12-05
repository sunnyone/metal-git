use std::mem;
use std::ptr;
use std::os::raw::c_void;
use std::ffi::CString;

use glib::translate::*;
use station_wrapper;
use station_wrapper::StationWrapper;
use station_renderer;
use std::rc::Rc;

use railway;

mod imp {
    use std::rc::Rc;
    use glib::IsA;
    use glib::subclass::object::ObjectImpl;
    use gtk::{CellEditable, CellRenderer, CellRendererState, Rectangle, SizeRequestMode, Widget};
    use gtk::cairo::Context;
    use gtk::gdk::Event;
    use gtk::subclass::cell_renderer::CellRendererImplExt;
    use gtk::subclass::prelude::CellRendererImpl;
    use railway;

    pub struct StationCellRendererImpl {
        pub num: i32,
        pub station: Option<Rc<railway::RailwayStation>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StationCellRendererImpl {
        const NAME: &'static str = "StationWrapper";
        type Type = super::StationCellRenderer;
        type ParentType = glib::Object;
        type Interfaces = ();
    }

    // Trait shared by all GObjects
    impl ObjectImpl for StationWrapperImpl {}

    impl CellRendererImpl for StationCellRendererImpl {
        fn render<P: IsA<Widget>>(&self, cr: &Context, widget: &P, background_area: &Rectangle, cell_area: &Rectangle, flags: CellRendererState) {
            //    println!("Render: {},{},{},{}",
            //             (*cell_area).x,
            //             (*cell_area).y,
            //             (*background_area).x,
            //             (*background_area).y);

            let bg_rect = to_gtk_rect(background_area);
            let cell_rect = to_gtk_rect(cell_area);

            let context = self::cairo::Context::from_glib_none(cr);

            let (new_bg_rect, new_cell_rect) = match private.station {
                Some(ref station) => station_renderer::render(&station, &context, &bg_rect, &cell_rect),
                None => (bg_rect, cell_rect),
            };

            let new_background_area = to_gdk_rect_box(&new_bg_rect);
            let new_cell_area = to_gdk_rect_box(&new_cell_rect);

            let cell_class = keep_parent_class as *mut gtk_sys::GtkCellRendererClass;
            (*cell_class).render.unwrap()(cell,
                                          cr,
                                          widget,
                                          &*new_background_area,
                                          &*new_cell_area,
                                          flags);
        }
    }

    fn to_gtk_rect(rect: &gtk::gdk::GdkRectangle) -> gtk::Rectangle {
        gtk::Rectangle::new(rect.x, rect.y, rect.width, rect.height)
    }


}
// TODO: is finalize correct?
// finalize: Option<unsafe extern "C" fn(*mut GObject)>,
unsafe extern "C" fn finalize(instance: *mut gobject_sys::GObject) {
    let mut unknown = PrivateAccessor::<StationCellRendererPrivate>::from_instance(instance as *mut GTypeInstance, get_type());
    // let sample = unknown.borrow();
    // println!("Fianlize Sample: {}", sample.num);
    let _sample = unknown.get(); // drop
}

// pub get_preferred_width: Option<unsafe extern "C" fn(*mut GtkCellRenderer, *mut GtkWidget, *mut c_int, *mut c_int)>,
unsafe extern "C" fn get_preferred_width(cell: *mut gtk_sys::GtkCellRenderer,
                                         widget: *mut gtk_sys::GtkWidget,
                                         minimum_size: *mut i32,
                                         natural_size: *mut i32) {
    let cell_class = keep_parent_class as *mut gtk_sys::GtkCellRendererClass;
    (*cell_class).get_preferred_width.unwrap()(cell, widget, minimum_size, natural_size);
}

fn to_gdk_rect_box(rect: &gtk::Rectangle) -> Box<gdk_sys::GdkRectangle> {
    Box::new(gdk_sys::GdkRectangle {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    })
}

// pub render: Option<unsafe extern "C" fn(*mut GtkCellRenderer, *mut cairo::cairo_t, *mut GtkWidget, *const gdk::GdkRectangle, *const gdk::GdkRectangle, GtkCellRendererState)>,
unsafe extern "C" fn render(cell: *mut gtk_sys::GtkCellRenderer,
                            cr: *mut cairo_sys::cairo_t,
                            widget: *mut gtk_sys::GtkWidget,
                            background_area: *const gdk_sys::GdkRectangle,
                            cell_area: *const gdk_sys::GdkRectangle,
                            flags: gtk_sys::GtkCellRendererState) {
    let accessor =
        PrivateAccessor::<StationCellRendererPrivate>::from_instance(cell as *mut GTypeInstance,
                                                                     get_type());
    let private = accessor.borrow();

    //    println!("Render: {},{},{},{}",
    //             (*cell_area).x,
    //             (*cell_area).y,
    //             (*background_area).x,
    //             (*background_area).y);

    let bg_rect = to_gtk_rect(background_area);
    let cell_rect = to_gtk_rect(cell_area);

    let context = self::cairo::Context::from_glib_none(cr);

    let (new_bg_rect, new_cell_rect) = match private.station {
        Some(ref station) => station_renderer::render(&station, &context, &bg_rect, &cell_rect),
        None => (bg_rect, cell_rect),
    };

    let new_background_area = to_gdk_rect_box(&new_bg_rect);
    let new_cell_area = to_gdk_rect_box(&new_cell_rect);

    let cell_class = keep_parent_class as *mut gtk_sys::GtkCellRendererClass;
    (*cell_class).render.unwrap()(cell,
                                  cr,
                                  widget,
                                  &*new_background_area,
                                  &*new_cell_area,
                                  flags);
}

const PROP_NUM: u32 = 1;
const PROP_STATION: u32 = 2;

// pub set_property: Option<unsafe extern "C" fn(*mut GObject, c_uint, *mut GValue, *mut GParamSpec)>,
unsafe extern "C" fn set_property(gobject: *mut gobject_sys::GObject,
                                  property_id: u32,
                                  value: *mut gobject_sys::GValue,
                                  _pspec: *mut gobject_sys::GParamSpec) {

    let mut accessor =
        PrivateAccessor::<StationCellRendererPrivate>::from_instance(gobject as *mut GTypeInstance,
                                                                     get_type());

    match property_id {
        PROP_NUM => {
            let val = gobject_sys::g_value_get_int(value);
            accessor.borrow_mut().num = val;
        }
        PROP_STATION => {
            let obj_ptr =
                gobject_sys::g_value_get_object(value) as *mut station_wrapper::StationWrapperC;
            let station_wrapper = StationWrapper::from_glib_none(obj_ptr);
            let station = station_wrapper.get_station();
            accessor.borrow_mut().station = Some(station);
        }
        _ => {}
    }
}

// pub get_property: Option<unsafe extern "C" fn(*mut GObject, c_uint, *mut GValue, *mut GParamSpec)>,
unsafe extern "C" fn get_property(_gobject: *mut gobject_sys::GObject,
                                  property_id: u32,
                                  _value: *mut gobject_sys::GValue,
                                  _pspec: *mut gobject_sys::GParamSpec) {
    match property_id {
        PROP_NUM => {
            // println!("Get");
        }
        _ => {}
    }
}

static mut keep_parent_class: *mut gtk_sys::GtkCellRendererTextClass = 0 as *mut _;

// pub type GClassInitFunc = Option<unsafe extern "C" fn(gpointer, gpointer)>;
#[allow(dead_code)]
unsafe extern "C" fn class_init(g_class: gpointer, _class_data: gpointer) {
    keep_parent_class =
        gobject_sys::g_type_class_peek_parent(g_class) as *mut gtk_sys::GtkCellRendererTextClass;

    // i32 is dummy
    gobject_sys::g_type_class_add_private(g_class, mem::size_of::<Box<i32>>());

    let klass = g_class as *mut gobject_sys::GInitiallyUnownedClass;
    (*klass).finalize = Some(finalize);
    (*klass).get_property = Some(get_property);
    (*klass).set_property = Some(set_property);

    let cell = g_class as *mut gtk_sys::GtkCellRendererClass;
    (*cell).get_preferred_width = Some(get_preferred_width);
    (*cell).render = Some(render);

    let _myclass = g_class as *mut StationCellRendererClass;

    let prop_name = CString::new("num").unwrap();
    gobject_sys::g_object_class_install_property(g_class as *mut gobject_sys::GObjectClass,
                                        PROP_NUM,
                                        gobject_sys::g_param_spec_int(
                                            prop_name.as_ptr(),
                                            prop_name.as_ptr(),
                                            prop_name.as_ptr(),
                                            0, 65535, 0, gobject_sys::G_PARAM_READWRITE));

    let prop_name = CString::new("station").unwrap();
    gobject_sys::g_object_class_install_property(g_class as *mut gobject_sys::GObjectClass,
                                        PROP_STATION,
                                        gobject_sys::g_param_spec_object(
                                            prop_name.as_ptr(),
                                            prop_name.as_ptr(),
                                            prop_name.as_ptr(),
                                            station_wrapper::get_type(),
                                            gobject_sys::G_PARAM_READWRITE));
}

// pub type GInstanceInitFunc = Option<unsafe extern "C" fn(*mut GTypeInstance, gpointer)>;
unsafe extern "C" fn init(instance: *mut GTypeInstance, _g_class: gpointer) {
    let sample = Box::new(StationCellRendererPrivate {
        num: 500,
        station: None,
    });

    let mut unknown = PrivateAccessor::<StationCellRendererPrivate>::from_instance(instance,
                                                                                   get_type());
    unknown.set(sample);
}

static mut TYPE: GType = 0;
pub fn get_type() -> glib_sys::GType {
    unsafe {
        if TYPE == 0 {
            let gtype = ::gobject_utils::register_static_type::<StationCellRendererClass>(
                    "StationCellRenderer", gtk_sys::gtk_cell_renderer_text_get_type(),
                    Some(class_init), Some(init));
            TYPE = gtype;
        }
        TYPE
    }
}

glib::wrapper! {
     pub struct StationCellRenderer(ObjectSubclass<imp::StationCellRendererImpl>) @extends gtk::CellRenderer;
}

impl StationCellRenderer {
    pub fn new() -> StationCellRenderer {
        unsafe {
            let ptr = gobject_sys::g_object_new(get_type(), ptr::null());
            StationCellRenderer::from_glib_none(ptr as *mut _).downcast_unchecked()
        }
    }
}
