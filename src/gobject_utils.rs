extern crate glib;
extern crate glib_sys;
extern crate gobject_sys;

use std::mem;
use self::gobject_sys::{GTypeInstance, GTypeQuery, GClassInitFunc, GInstanceInitFunc, GObject};
use self::glib_sys::GType;
use std::ptr;
use std::marker::PhantomData;
use std::ffi::CString;

fn gtype_get_instance_size(gtype: GType) -> Option<u32> {
    // instance struct is defined as opaque
    let mut query = Box::new(GTypeQuery {
        type_name: ptr::null(),
        type_: 0,
        instance_size: 0,
        class_size: 0,
    });
    unsafe {
        gobject_sys::g_type_query(gtype, &mut *query);
    }
    if query.type_ == 0 {
        None
    } else {
        Some(query.instance_size as u32)
    }
}

pub unsafe fn register_static_type<TClass>(name: &str,
                                           parent_type: GType,
                                           class_init_func: GClassInitFunc,
                                           instance_init_func: GInstanceInitFunc)
                                           -> GType {
    let name = CString::new(name).unwrap();

    let parent_size = gtype_get_instance_size(parent_type).unwrap();
    let gtype = gobject_sys::g_type_register_static_simple(parent_type,
                                                           name.as_ptr(),
                                                           mem::size_of::<TClass>() as u32,
                                                           class_init_func,
                                                           parent_size,
                                                           instance_init_func,
                                                           gobject_sys::GTypeFlags::empty());
    gtype
}

pub struct PrivateAccessor<T> {
    private_ptr: *mut *mut T, // pading_ptr indicates a box(box indicates a object)
    marker: PhantomData<T>,
}

impl<T> PrivateAccessor<T> {
    pub unsafe fn from_instance(ptr: *mut GTypeInstance, gtype: GType) -> PrivateAccessor<T> {
        let private_ptr = gobject_sys::g_type_instance_get_private(ptr, gtype) as *mut *mut T;
        PrivateAccessor {
            private_ptr: private_ptr,
            marker: PhantomData,
        }
    }

    pub unsafe fn from_object(ptr: *mut GObject, gtype: GType) -> PrivateAccessor<T> {
        Self::from_instance(ptr as *mut GTypeInstance, gtype)
    }

    pub unsafe fn set(&mut self, t: Box<T>) {
        ptr::write(self.private_ptr, Box::into_raw(t));
    }

    pub unsafe fn borrow(&self) -> &T {
        &*ptr::read(self.private_ptr)
    }

    pub unsafe fn borrow_mut(&mut self) -> &mut T {
        &mut *ptr::read(self.private_ptr)
    }

    pub unsafe fn get(&mut self) -> Box<T> {
        Box::from_raw(*self.private_ptr)
    }
}
