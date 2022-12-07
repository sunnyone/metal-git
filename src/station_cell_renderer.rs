use gtk::glib::prelude::*;
use gtk::glib::subclass::prelude::*;
use gtk::subclass::prelude::*;
use std::cell::RefCell;
use glib;
use glib::once_cell::sync::Lazy;

const PROP_NUM: usize = 1;
const PROP_STATION: usize = 2;

mod imp {
    use glib::IsA;
    use gtk::Widget;
    use crate::station_wrapper::StationWrapper;
    use crate::station_renderer;
    use super::*;

    #[derive(Default)]
    pub struct StationCellRendererImpl {
        pub num: RefCell<i32>,
        pub station_wrapper: RefCell<StationWrapper>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StationCellRendererImpl {
        const NAME: &'static str = "StationCellRenderer";
        type Type = super::StationCellRenderer;
        type ParentType = gtk::CellRendererText;
        type Interfaces = ();
    }

    impl ObjectImpl for StationCellRendererImpl {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecInt::new(
                        "num",
                        "Num",
                        "Num",
                        0,
                        65535,
                        0,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecObject::new(
                        "station",
                        "Station",
                        "Station",
                        StationWrapper::static_type(),
                        glib::ParamFlags::READWRITE,
                    ),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, _pspec: &glib::ParamSpec) {
            match _id {
                PROP_NUM => {
                    let name = value
                        .get()
                        .expect("type conformity checked by `Object::set_property`");
                    self.num.replace(name);
                }
                PROP_STATION => {
                    let station_wrapper = value
                        .get()
                        .expect("type conformity checked by `Object::set_property`");
                    self.station_wrapper.replace(station_wrapper);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, _pspec: &glib::ParamSpec) -> glib::Value {
            match _id {
                PROP_NUM => self.num.borrow().to_value(),
                PROP_STATION => self.station_wrapper.borrow().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl CellRendererImpl for StationCellRendererImpl {
        fn render<P: IsA<Widget>>(
            &self,
            cr: &gtk::cairo::Context,
            widget: &P,
            background_area: &gtk::Rectangle,
            cell_area: &gtk::Rectangle,
            flags: gtk::CellRendererState,
        ) {
            let rendered = self.station_wrapper.borrow().get_station().map(|x| {
                station_renderer::render(&x, &cr, &background_area, &cell_area).ok()
            }).flatten();

            match rendered {
                Some((new_bg_rect, new_cell_rect)) => self.parent_render(cr, widget,  &new_bg_rect, &new_cell_rect, flags),
                None => self.parent_render(cr, widget, background_area, cell_area, flags)
            }
        }
    }

    impl CellRendererTextImpl for StationCellRendererImpl {

    }
}

glib::wrapper! {
     pub struct StationCellRenderer(ObjectSubclass<imp::StationCellRendererImpl>) @extends gtk::CellRenderer;
}

impl StationCellRenderer {
    pub fn new() -> Self {
        gtk::glib::Object::new(&[])
    }
}