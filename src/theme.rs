//! purrtop's bundled themes, embedded so they work with no files on disk.

use std::rc::Rc;

use gpui_kit::component::{Theme, ThemeConfig, ThemeSet};
use gpui_kit::*;

pub const DEFAULT_THEME: &str = "Neon";

/// In the order the theme menus list them.
const BUNDLED: [&str; 8] = [
    include_str!("../themes/neon.json"),
    include_str!("../themes/rainbow.json"),
    include_str!("../themes/cotton-candy.json"),
    include_str!("../themes/sunset.json"),
    include_str!("../themes/matcha.json"),
    include_str!("../themes/midnight.json"),
    include_str!("../themes/phosphor.json"),
    include_str!("../themes/graphite.json"),
];

pub struct Themes {
    themes: Vec<Rc<ThemeConfig>>,
    current: usize,
}

impl Global for Themes {}

impl Themes {
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn names(&self) -> impl Iterator<Item = &SharedString> {
        self.themes.iter().map(|theme| &theme.name)
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn current_name(&self) -> SharedString {
        self.themes.get(self.current).map(|theme| theme.name.clone()).unwrap_or_default()
    }

    fn position(&self, name: &str) -> Option<usize> {
        self.themes.iter().position(|theme| theme.name == name)
    }
}

/// Loads the bundled themes and applies `saved`, or Neon when it's unknown.
pub fn init(saved: Option<&str>, cx: &mut App) {
    let themes: Vec<Rc<ThemeConfig>> = BUNDLED
        .iter()
        .filter_map(|source| match parse(source) {
            Ok(theme) => Some(theme),
            Err(err) => {
                eprintln!("purrtop: a bundled theme failed to parse: {err}");
                None
            }
        })
        .collect();
    cx.set_global(Themes { themes, current: 0 });
    let themes = Themes::global(cx);
    let ix = saved.and_then(|name| themes.position(name)).or_else(|| themes.position(DEFAULT_THEME)).unwrap_or(0);
    apply(ix, cx);
}

/// Switches to the theme at `ix` in menu order. Observers of [`Themes`] hear about it.
pub fn apply(ix: usize, cx: &mut App) {
    let Some(config) = Themes::global(cx).themes.get(ix).cloned() else {
        return;
    };
    Theme::update(cx, |theme| theme.apply_config(&config));
    cx.global_mut::<Themes>().current = ix;
    cx.refresh_windows();
}

fn parse(json: &str) -> serde_json::Result<Rc<ThemeConfig>> {
    let set: ThemeSet = serde_json::from_str(json)?;
    set.themes
        .into_iter()
        .next()
        .map(Rc::new)
        .ok_or_else(|| serde::de::Error::custom("the theme file has no themes"))
}

#[cfg(test)]
mod tests {
    // Not `super::*`: gpui_kit's glob exports a `test` item that shadows `#[test]`.
    use super::{BUNDLED, DEFAULT_THEME, parse};
    use gpui_kit::SharedString;

    #[test]
    fn every_bundled_theme_parses_with_a_unique_name() {
        let names: Vec<SharedString> = BUNDLED.iter().map(|source| parse(source).unwrap().name.clone()).collect();
        assert_eq!(names.len(), 8);
        assert_eq!(names[0], DEFAULT_THEME);
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
    }
}
