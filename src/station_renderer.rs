// extern crate gtk;
// extern crate cairo;
// use std::f64::consts;
// use railway;
// use cairo::LineCap;
//
// fn commit_dot_box_size(cell_height: i32) -> (i32, i32) {
//     (cell_height, cell_height)
// }
//
// fn commit_dot_radius(_box_width: i32, box_height: i32) -> i32 {
//     (box_height as f64 / 3.5) as i32
// }
//
// fn calc_tracks_width(station: &railway::RailwayStation, box_width: i32) -> i32 {
//     let max_track_number = station.tracks.iter().map(|x| x.track_number.as_usize()).max();
//     (max_track_number.unwrap_or(0) + 1) as i32 * box_width
// }
//
// fn merge_line_offset(box_height: i32) -> i32 {
//     box_height / 4
// }
//
// fn box_x(start_x: i32, box_width: i32, index: usize) -> i32 {
//     start_x + index as i32 * box_width
// }
//
// pub fn render(station: &railway::RailwayStation,
//               context: &cairo::Context,
//               bg_area: &gtk::Rectangle,
//               cell_area: &gtk::Rectangle)
//               -> (gtk::Rectangle, gtk::Rectangle) {
//
//     let (box_width, box_height) = commit_dot_box_size(cell_area.height);
//     let dot_radius = commit_dot_radius(box_width, box_height);
//
//     // let c = 0.1 * ((private.num + 1) as f64);
//     let c = 0.1;
//     context.set_source_rgb(c, c, 0.8);
//     context.set_line_cap(LineCap::Square);
//
//     for track in &station.tracks {
//         let track_box_x = box_x(cell_area.x, box_width, track.track_number.as_usize()) as f64;
//         let track_box_y = cell_area.y as f64;
//
//         let center_x = track_box_x + box_width as f64 / 2.0;
//         let center_y = track_box_y + box_height as f64 / 2.0;
//
//         let merge_line_offset = merge_line_offset(box_height) as f64;
//
//         if !track.from_tracks.is_empty() {
//             let top_y = bg_area.y;
//
//             context.move_to(center_x, top_y as f64 + merge_line_offset);
//             context.line_to(center_x, center_y as f64);
//             context.stroke();
//
//             for num in &track.from_tracks {
//                 context.move_to((box_x(cell_area.x, box_width, num.as_usize()) +
//                                  box_width / 2) as f64 + 1.0,
//                                 top_y as f64);
//                 context.line_to(center_x, top_y as f64 + merge_line_offset);
//                 context.stroke();
//             }
//         }
//
//         if !track.to_tracks.borrow().is_empty() {
//             let bottom_y = bg_area.y + bg_area.height;
//
//             context.move_to(center_x, center_y as f64);
//             context.line_to(center_x, bottom_y as f64 - merge_line_offset);
//             context.stroke();
//
//             for num in track.to_tracks.borrow().iter() {
//                 context.move_to(center_x, bottom_y as f64 - merge_line_offset);
//                 context.line_to((box_x(cell_area.x, box_width, num.as_usize()) +
//                                  box_width / 2) as f64 + 1.0,
//                                 bottom_y as f64);
//                 context.stroke();
//             }
//         }
//
//         if track.is_active {
//             // the center of circle is a little up because a bottom line is shorter than a top line (a more clean way is required)
//             context.arc(center_x,
//                         center_y - merge_line_offset / 3.0,
//                         dot_radius as f64,
//                         0.0,
//                         2.0 * consts::PI);
//             context.fill();
//         }
//
//     }
//
//     let tracks_width = calc_tracks_width(station, box_width);
//     (gtk::Rectangle { x: bg_area.x + tracks_width, ..*bg_area },
//      gtk::Rectangle { x: cell_area.x + tracks_width, ..*cell_area })
// }
