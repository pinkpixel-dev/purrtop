//! purrtop's own chart pieces, drawn on GPUI canvases. Kit's charts draw one
//! series with axes and an appear animation, which doesn't fit live readings
//! that update every second.

mod line;
mod palette;
mod segments;

pub use line::{Grid, Series, line_chart};
pub use palette::Palette;
pub use segments::{led_column, meter};
