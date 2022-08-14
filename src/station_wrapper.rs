extern crate glib;
extern crate gtk;
extern crate cairo;

use std::rc::Rc;
use glib::Cast;

use glib::subclass::object::ObjectImpl;
use glib::subclass::types::ObjectSubclass;
use glib::translate::*;

use railway::RailwayStation;

#[derive(Default)]
pub struct StationWrapper {
    pub station: Option<Rc<RailwayStation>>,
}

#[glib::object_subclass]
impl ObjectSubclass for StationWrapper {
    const NAME: &'static str = "StationWrapper";
    type Type = super::StationWrapper;
    type ParentType = glib::Object;
}

// Trait shared by all GObjects
impl ObjectImpl for StationWrapper {
}

glib::wrapper! {
    pub struct SimpleObject(ObjectSubclass<SimpleObject>);
}

impl StationWrapper {
    pub fn new() -> Self {
        glib::Object::new(
            Self::static_type())
            .expect("Failed to create StationWrapper")
            .downcast()
            .expect("Wrong type")
    }

    pub fn get_station(&self) -> Rc<RailwayStation> {
        self.station.as_ref().unwrap().clone()
    }

    pub fn set_station(&mut self, station: RailwayStation) {
        self.station = Some(Rc::new(station));
    }
}
