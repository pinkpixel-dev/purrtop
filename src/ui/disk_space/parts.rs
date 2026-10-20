//! Small pieces the three Disk Space tables share.

use std::path::Path;

use gpui_kit::component::table::Column;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::*;

use crate::system::file_types::Kind;

/// What the page's keyboard handling needs from each table.
pub trait Rows {
    fn row_count(&self) -> usize;
    fn selected_row(&self) -> Option<usize>;
    fn select_row(&mut self, row: usize);
}

/// `(key, title, width, right aligned)` per column, left to right.
pub type ColumnSpec = (&'static str, &'static str, f32, bool);

pub fn columns(spec: &[ColumnSpec]) -> Vec<Column> {
    spec.iter()
        .map(|&(key, title, width, right)| {
            let column = Column::new(key, title).width(width).sortable();
            if right { column.text_right() } else { column }
        })
        .collect()
}

/// The path with your home folder shortened to `~`.
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && text.starts_with(&home) && text[home.len()..].chars().next().is_none_or(|c| c == '/') => format!("~{}", &text[home.len()..]),
        _ => text.into_owned(),
    }
}

/// The last part of a path, or the whole thing for `/`.
pub fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.to_string_lossy().into_owned(), |name| name.to_string_lossy().into_owned())
}

/// The folder a path sits in, shortened like [`display_path`].
pub fn location(path: &Path) -> String {
    path.parent().map(display_path).unwrap_or_default()
}

/// Each kind's color, the same in every table and the bubble view.
pub fn kind_color(kind: Kind, cx: &App) -> Hsla {
    let theme = cx.theme();
    match kind {
        Kind::Video => theme.red,
        Kind::Image => theme.magenta,
        Kind::Audio => theme.yellow,
        Kind::Archive => theme.blue,
        Kind::DiskImage => theme.cyan,
        Kind::Document => theme.green,
        Kind::Code => theme.magenta_light,
        Kind::Binary => theme.blue_light,
        Kind::Other => theme.muted_foreground,
    }
}

/// A dot in the kind's color, then the text.
pub fn with_dot(kind: Kind, text: String, cx: &App) -> AnyElement {
    h_flex()
        .min_w_0()
        .gap_2()
        .items_center()
        .child(div().size_2().flex_shrink_0().rounded_full().bg(kind_color(kind, cx)))
        .child(div().min_w_0().truncate().child(text))
        .into_any_element()
}

/// A thin bar showing `fraction` of the whole scan, with the percentage.
pub fn share_bar(fraction: f64, color: Hsla, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let fraction = fraction.clamp(0., 1.);
    h_flex()
        .w_full()
        .gap_2()
        .items_center()
        .child(div().flex_1().h_1p5().rounded_full().bg(theme.muted).child(div().h_full().rounded_full().bg(color).w(relative(fraction as f32))))
        .child(div().w(px(44.)).flex_shrink_0().text_right().text_xs().text_color(theme.muted_foreground).child(percent(fraction)))
        .into_any_element()
}

/// `12.3%`, or `<0.1%` for slivers that would round to nothing.
pub fn percent(fraction: f64) -> String {
    if fraction > 0. && fraction < 0.001 { "<0.1%".into() } else { format!("{:.1}%", fraction * 100.) }
}

pub fn fraction(part: u64, whole: u64) -> f64 {
    if whole == 0 { 0. } else { part as f64 / whole as f64 }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{file_name, fraction, location, percent};

    #[test]
    fn names_and_locations() {
        assert_eq!(file_name(&PathBuf::from("/a/b/c.txt")), "c.txt");
        assert_eq!(file_name(&PathBuf::from("/")), "/");
        assert_eq!(location(&PathBuf::from("/var/log/x")), "/var/log");
    }

    #[test]
    fn percentages() {
        assert_eq!(percent(0.1234), "12.3%");
        assert_eq!(percent(0.0004), "<0.1%");
        assert_eq!(percent(0.), "0.0%");
        assert_eq!(fraction(1, 0), 0.);
        assert_eq!(fraction(1, 4), 0.25);
    }
}
