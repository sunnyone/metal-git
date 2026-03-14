use std::f64::consts;

use crate::railway;
use gtk::cairo::{Context, Error, LineCap};

const TRACK_BOX_SIZE: i32 = 18;
const ROW_HEIGHT: i32 = 28;

fn commit_dot_radius(box_height: i32) -> i32 {
    (box_height as f64 / 5.0) as i32
}

fn calc_tracks_width(station: &railway::RailwayStation) -> i32 {
    let max_track_number = station
        .tracks
        .iter()
        .map(|x| x.track_number.as_usize())
        .max()
        .unwrap_or(0);
    (max_track_number + 1) as i32 * TRACK_BOX_SIZE
}

fn merge_line_offset(box_height: i32) -> i32 {
    box_height / 4
}

fn box_x(index: usize) -> i32 {
    index as i32 * TRACK_BOX_SIZE
}

pub fn desired_width(station: &railway::RailwayStation) -> i32 {
    calc_tracks_width(station) + 12
}

pub fn desired_height() -> i32 {
    ROW_HEIGHT
}

pub fn render(
    station: &railway::RailwayStation,
    context: &Context,
    width: i32,
    height: i32,
) -> Result<(), Error> {
    let box_width = TRACK_BOX_SIZE;
    let box_height = height.max(ROW_HEIGHT);
    let dot_radius = commit_dot_radius(box_height);
    let line_offset = merge_line_offset(box_height) as f64;
    let bottom_y = height as f64;

    context.set_source_rgb(0.1, 0.1, 0.8);
    context.set_line_cap(LineCap::Square);
    context.set_line_width(2.0);

    for track in &station.tracks {
        let track_box_x = box_x(track.track_number.as_usize()) as f64;
        let center_x = track_box_x + box_width as f64 / 2.0;
        let center_y = height as f64 / 2.0;

        if !track.from_tracks.is_empty() {
            context.move_to(center_x, line_offset);
            context.line_to(center_x, center_y);
            context.stroke()?;

            for num in &track.from_tracks {
                context.move_to(
                    (box_x(num.as_usize()) + box_width / 2) as f64 + 1.0,
                    0.0,
                );
                context.line_to(center_x, line_offset);
                context.stroke()?;
            }
        }

        if !track.to_tracks.borrow().is_empty() {
            context.move_to(center_x, center_y);
            context.line_to(center_x, bottom_y - line_offset);
            context.stroke()?;

            for num in track.to_tracks.borrow().iter() {
                context.move_to(center_x, bottom_y - line_offset);
                context.line_to(
                    (box_x(num.as_usize()) + box_width / 2) as f64 + 1.0,
                    bottom_y,
                );
                context.stroke()?;
            }
        }

        if track.is_active {
            context.arc(
                center_x,
                center_y - line_offset / 3.0,
                dot_radius as f64,
                0.0,
                2.0 * consts::PI,
            );
            context.fill()?;
        }
    }

    if width > calc_tracks_width(station) {
        context.new_path();
    }

    Ok(())
}
