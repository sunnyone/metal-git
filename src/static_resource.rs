pub fn init() {
    let res_bytes = include_bytes!("resources/resources.gresource");
    // gbytes and resource will not be freed
    let gbytes = gtk::glib::Bytes::from(res_bytes);
    let resource = gtk::gio::Resource::from_data(&gbytes).unwrap();
    gtk::gio::functions::resources_register(&resource)
}
