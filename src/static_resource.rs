extern crate gio;

pub fn init() {
    let res_bytes = include_bytes!("resources/resources.gresource");
    // gbytes and resource will not be freed
    let gbytes = glib::Bytes::from(res_bytes);
    let resource = gio::Resource::from_data(&gbytes).unwrap();
    gio::functions::resources_register(resource)
}
