//! Segmented bars: a vertical LED column and a horizontal meter. Lit segments
//! use the full color, the rest a faint version of it.

use gpui_kit::*;

/// How visible the unlit segments are.
const UNLIT_OPACITY: f32 = 0.13;
const SEGMENT: f32 = 4.;
const GAP: f32 = 2.;

/// A vertical column of segments, lit from the bottom up to `fraction` (0 to 1).
pub fn led_column(fraction: f32, color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let count = segment_count(f32::from(bounds.size.height));
            let lit = lit_segments(fraction, count);
            let step = px(SEGMENT + GAP);
            for ix in 0..count {
                let y = bounds.bottom() - step * (ix + 1) as f32 + px(GAP);
                let rect = Bounds::new(point(bounds.left(), y), size(bounds.size.width, px(SEGMENT)));
                window.paint_quad(fill(rect, if ix < lit { color } else { color.opacity(UNLIT_OPACITY) }));
            }
        },
    )
    .size_full()
}

/// A horizontal row of segments, lit from the left up to `fraction` (0 to 1).
pub fn meter(fraction: f32, color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let count = segment_count(f32::from(bounds.size.width));
            let lit = lit_segments(fraction, count);
            let step = px(SEGMENT + GAP);
            for ix in 0..count {
                let x = bounds.left() + step * ix as f32;
                let rect = Bounds::new(point(x, bounds.top()), size(px(SEGMENT), bounds.size.height));
                window.paint_quad(fill(rect, if ix < lit { color } else { color.opacity(UNLIT_OPACITY) }));
            }
        },
    )
    .size_full()
}

/// How many segments fit along `length` pixels.
fn segment_count(length: f32) -> usize {
    ((length + GAP) / (SEGMENT + GAP)).floor().max(0.) as usize
}

/// Rounds up, so any reading above zero lights at least one segment.
fn lit_segments(fraction: f32, count: usize) -> usize {
    ((fraction.clamp(0., 1.) * count as f32).ceil() as usize).min(count)
}

#[cfg(test)]
mod tests {
    // Not `super::*`: gpui_kit's glob exports a `test` item that shadows `#[test]`.
    use super::{lit_segments, segment_count};

    #[test]
    fn counts_and_lights_segments() {
        assert_eq!(segment_count(58.), 10);
        assert_eq!(segment_count(3.), 0);
        assert_eq!(lit_segments(0., 10), 0);
        assert_eq!(lit_segments(0.01, 10), 1);
        assert_eq!(lit_segments(0.5, 10), 5);
        assert_eq!(lit_segments(2., 10), 10);
    }
}
