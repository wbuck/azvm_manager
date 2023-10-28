use tabled::settings::{style::{RawStyle, Style}, Color};

pub mod sub;
pub use sub::*;

pub mod rg;
pub use rg::*;

pub enum Output<'a, T> {
    Single(&'a T),
    Multiple(&'a [T])
}

pub(crate) fn get_style() -> RawStyle {
    let mut style = RawStyle::from(Style::modern());
    style
        .set_color_top(Color::FG_GREEN)
        .set_color_bottom(Color::FG_GREEN)
        .set_color_left(Color::FG_GREEN)
        .set_color_right(Color::FG_GREEN)
        .set_color_corner_top_left(Color::FG_GREEN)
        .set_color_corner_top_right(Color::FG_GREEN)
        .set_color_corner_bottom_left(Color::FG_GREEN)
        .set_color_corner_bottom_right(Color::FG_GREEN)
        .set_color_intersection_bottom(Color::FG_GREEN)
        .set_color_intersection_top(Color::FG_GREEN)
        .set_color_intersection_right(Color::FG_GREEN)
        .set_color_intersection_left(Color::FG_GREEN)
        .set_color_intersection(Color::FG_GREEN)
        .set_color_horizontal(Color::FG_GREEN)
        .set_color_vertical(Color::FG_GREEN);

    return style;
}