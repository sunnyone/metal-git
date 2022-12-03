use std::cell::Ref;
use std::rc::Rc;

use gtk::glib::prelude::*;
use gtk::glib::subclass::prelude::*;

use railway::RailwayStation;

mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct StationWrapperImpl {
        pub station: RefCell<Option<RailwayStation>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StationWrapperImpl {
        const NAME: &'static str = "StationWrapper";
        type Type = super::StationWrapper;
        type ParentType = gtk::glib::Object;
        type Interfaces = ();
    }

    // Trait shared by all GObjects
    impl ObjectImpl for StationWrapperImpl {}
}

gtk::glib::wrapper! {
    pub struct StationWrapper(ObjectSubclass<imp::StationWrapperImpl>);
}

impl StationWrapper {
    pub fn new() -> Self {
        let obj: Self = gtk::glib::Object::new(&[]);
        obj
    }

    pub fn get_impl(&self) -> &imp::StationWrapperImpl {
        imp::StationWrapperImpl::from_instance(self)
    }

    pub fn get_station(&self) -> Ref<Option<RailwayStation>> {
        let priv_ = imp::StationWrapperImpl::from_instance(self);
        priv_.station.borrow()
    }

    pub fn set_station(&mut self, station: RailwayStation) {
        let priv_ = imp::StationWrapperImpl::from_instance(self);
        priv_.station.replace(Some(station));
    }
}
