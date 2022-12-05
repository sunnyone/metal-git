use std::mem;
use self::glib_sys::gpointer;
use self::gobject_sys::GTypeInstance;
use self::glib_sys::GType;
use std::ptr;
use std::os::raw::c_void;
use gobject_utils::PrivateAccessor;
use std::rc::Rc;

use glib::object::Downcast;
use glib::translate::*;

use railway::RailwayStation;

#[repr(C)]
pub struct StationWrapperC(c_void);

#[repr(C)]
pub struct StationWrapperClass {
    pub parent_class: gobject_sys::GObjectClass,
}

pub struct StationWrapperPrivate {
    pub station: Option<Rc<RailwayStation>>,
}

// finalize: Option<unsafe extern "C" fn(*mut GObject)>,
unsafe extern "C" fn finalize(instance: *mut gobject_sys::GObject) {
    let mut accessor = get_private_accessor(instance);
    let _ = accessor.get(); // drop
}

// pub type GClassInitFunc = Option<unsafe extern "C" fn(gpointer, gpointer)>;
unsafe extern "C" fn class_init(g_class: gpointer, _class_data: gpointer) {
    // i32 is dummy
    gobject_sys::g_type_class_add_private(g_class, mem::size_of::<Box<i32>>());

    let klass = g_class as *mut gobject_sys::GInitiallyUnownedClass;
    (*klass).finalize = Some(finalize);
}

// pub type GInstanceInitFunc = Option<unsafe extern "C" fn(*mut GTypeInstance, gpointer)>;
unsafe extern "C" fn init(instance: *mut GTypeInstance, _g_class: gpointer) {
    let priv_ = Box::new(StationWrapperPrivate { station: None });

    let mut accessor = PrivateAccessor::<StationWrapperPrivate>::from_instance(instance,
                                                                               get_type());
    accessor.set(priv_);
}

fn get_private_accessor(obj: *mut gobject_sys::GObject) -> PrivateAccessor<StationWrapperPrivate> {
    unsafe { PrivateAccessor::<StationWrapperPrivate>::from_object(obj, get_type()) }
}

static mut TYPE: GType = 0;
pub fn get_type() -> glib_sys::GType {
    unsafe {
        if TYPE == 0 {
            let gtype = ::gobject_utils::register_static_type::<StationWrapperClass>(
                    "StationWrapper", gobject_sys::g_object_get_type(),
                    Some(class_init), Some(init));
            TYPE = gtype;
        }
        TYPE
    }
}

glib_wrapper! {
     pub struct StationWrapper(Object<StationWrapperC>);
     
     match fn {
         get_type => || get_type(),
     }
}

impl StationWrapper {
    pub fn new() -> StationWrapper {
        unsafe {
            let ptr = gobject_sys::g_object_new(get_type(), ptr::null());
            StationWrapper::from_glib_full(ptr as *mut _).downcast_unchecked()
        }
    }

    pub fn get_station(&self) -> Rc<RailwayStation> {
        unsafe {
            let accessor = get_private_accessor(self.to_glib_none().0);
            accessor.borrow().station.as_ref().unwrap().clone()
        }
    }

    pub fn set_station(&mut self, station: RailwayStation) {
        let mut accessor = get_private_accessor(self.to_glib_none().0);
        unsafe {
            accessor.borrow_mut().station = Some(Rc::new(station));
        }
    }
}
