extern crate gio_sys;
extern crate glib_sys;
extern crate libc;

use std;

pub fn init() {
    let res_bytes = include_bytes!("resources/resources.gresource");
    unsafe {
        // gbytes and resource will not be freed
        let gbytes = glib_sys::g_bytes_new(res_bytes.as_ptr() as *const libc::c_void,
                                           res_bytes.len());
        let resource = gio_sys::g_resource_new_from_data(gbytes, std::ptr::null_mut());
        gio_sys::g_resources_register(resource);
    }
}
