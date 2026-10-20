//! Live line charts: several series on one plot, newest reading at the right edge.

use gpui_kit::*;

use crate::system::history::{HISTORY_LEN, History};

const STROKE: f32 = 1.5;
/// How strongly a series' area is tinted under its line.
const AREA_OPACITY: f32 = 0.14;

/// One line. Values run from 0 to `max`; anything above is drawn at the top.
#[derive(Clone)]
pub struct Series {
    values: Vec<f32>,
    max: f32,
    color: Hsla,
    area: bool,
}

impl Series {
    pub fn new(history: &History, max: f32, color: Hsla) -> Self {
        Self { values: history.values().collect(), max, color, area: false }
    }

    /// Tints the area under the line.
    pub fn area(mut self) -> Self {
        self.area = true;
        self
    }
}

/// Grid lines across the plot. `None` draws just the series, for sparklines.
#[derive(Clone, Copy)]
pub struct Grid {
    pub rows: usize,
    pub columns: usize,
    pub color: Hsla,
}

/// Draws `series` filling the element. Later series are drawn on top.
pub fn line_chart(series: Vec<Series>, grid: Option<Grid>) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                if let Some(grid) = grid {
                    paint_grid(bounds, grid, window);
                }
                for series in &series {
                    paint_series(bounds, series, window);
                }
            });
        },
    )
    .size_full()
}

fn paint_grid(bounds: Bounds<Pixels>, grid: Grid, window: &mut Window) {
    let line = px(1.);
    for row in 0..=grid.rows {
        let y = bounds.top() + (bounds.size.height - line) * (row as f32 / grid.rows.max(1) as f32);
        window.paint_quad(fill(Bounds::new(point(bounds.left(), y), size(bounds.size.width, line)), grid.color));
    }
    for column in 0..=grid.columns {
        let x = bounds.left() + (bounds.size.width - line) * (column as f32 / grid.columns.max(1) as f32);
        window.paint_quad(fill(Bounds::new(point(x, bounds.top()), size(line, bounds.size.height)), grid.color));
    }
}

fn paint_series(bounds: Bounds<Pixels>, series: &Series, window: &mut Window) {
    let points = plot_points(bounds, &series.values, series.max);
    if points.len() < 2 {
        return;
    }
    if series.area {
        let mut area = PathBuilder::fill();
        area.move_to(point(points[0].x, bounds.bottom()));
        for &p in &points {
            area.line_to(p);
        }
        area.line_to(point(points[points.len() - 1].x, bounds.bottom()));
        area.close();
        if let Ok(path) = area.build() {
            window.paint_path(path, series.color.opacity(AREA_OPACITY));
        }
    }
    let mut line = PathBuilder::stroke(px(STROKE));
    line.move_to(points[0]);
    for &p in &points[1..] {
        line.line_to(p);
    }
    if let Ok(path) = line.build() {
        window.paint_path(path, series.color);
    }
}

/// Places readings on the plot: one slot per sample across the full history
/// width, right-aligned so a short history grows in from the right.
fn plot_points(bounds: Bounds<Pixels>, values: &[f32], max: f32) -> Vec<Point<Pixels>> {
    let slots = HISTORY_LEN.max(2) - 1;
    let offset = HISTORY_LEN.saturating_sub(values.len());
    // Keep the stroke inside the plot at 0% and 100%.
    let inset = px(STROKE);
    let height = bounds.size.height - inset * 2.;
    values
        .iter()
        .enumerate()
        .map(|(ix, &value)| {
            let x = bounds.left() + bounds.size.width * ((offset + ix) as f32 / slots as f32);
            let level = if max > 0. { (value / max).clamp(0., 1.) } else { 0. };
            point(x, bounds.top() + inset + height * (1. - level))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::plot_points;
    use crate::system::history::HISTORY_LEN;
    use gpui_kit::{Bounds, point, px, size};

    #[test]
    fn newest_reading_sits_on_the_right_edge() {
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(119.), px(103.)));
        let points = plot_points(bounds, &[0., 0.5, 1.], 1.);
        assert_eq!(points.len(), 3);
        assert_eq!(points[2].x, px(119.));
        assert_eq!(points[1].x, px(118.));
        // 0 at the bottom, full at the top, both inset by the stroke width.
        assert_eq!(points[0].y, px(101.5));
        assert_eq!(points[2].y, px(1.5));
        assert_eq!(points[1].y, px(51.5));
        assert_eq!(HISTORY_LEN, 120);
    }

    #[test]
    fn readings_above_max_are_clamped() {
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(100.), px(100.)));
        let points = plot_points(bounds, &[5.], 1.);
        assert_eq!(points[0].y, px(1.5));
        assert!(plot_points(bounds, &[1.], 0.).iter().all(|p| p.y == px(98.5)));
    }
}
