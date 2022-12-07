use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib::subclass::prelude::*;

use crate::railway::RailwayStation;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct StationWrapperImpl {
        pub station: RefCell<Option<Rc<RailwayStation>>>,
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
        gtk::glib::Object::new(&[])
    }

    pub fn get_impl(&self) -> &imp::StationWrapperImpl {
        imp::StationWrapperImpl::from_instance(self)
    }

    pub fn get_station(&self) -> Option<Rc<RailwayStation>> {
        let priv_ = imp::StationWrapperImpl::from_instance(self);
        let station_ref = priv_.station.borrow();
        match station_ref.as_ref() {
            Some(x) => Some(Rc::clone(x)),
            None => None,
        }
    }

    pub fn set_station(&mut self, station: RailwayStation) {
        let priv_ = imp::StationWrapperImpl::from_instance(self);
        priv_.station.replace(Some(Rc::new(station)));
    }
}

impl Default for StationWrapper {
    fn default() -> Self {
        Self::new()
    }
}
