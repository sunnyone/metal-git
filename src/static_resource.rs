pub fn init() {
    gtk::gio::resources_register_include!("resources.gresource")
        .expect("failed to register the bundled resources");
}
