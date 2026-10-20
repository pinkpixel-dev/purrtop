use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;

/// Which theme color each reading uses, so CPU is the same color on every page.
#[derive(Clone, Copy)]
pub struct Palette {
    pub cpu: Hsla,
    pub kernel: Hsla,
    pub clock: Hsla,
    pub temperature: Hsla,
    pub gpu: Hsla,
    pub memory: Hsla,
    pub received: Hsla,
    pub sent: Hsla,
    pub read: Hsla,
    pub written: Hsla,
    pub grid: Hsla,
}

impl Palette {
    pub fn new(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            cpu: theme.green,
            kernel: theme.blue,
            clock: theme.red,
            temperature: theme.yellow,
            gpu: theme.cyan,
            memory: theme.magenta,
            received: theme.cyan,
            sent: theme.red,
            read: theme.blue,
            written: theme.yellow,
            grid: theme.chart_grid,
        }
    }
}
