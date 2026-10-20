use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;

/// Which theme color each reading uses, so CPU is the same color on every page.
#[derive(Clone, Copy)]
pub struct Palette {
    pub cpu: Hsla,
    /// Efficiency cores on hybrid CPUs. Performance cores use `cpu`.
    pub efficiency: Hsla,
    pub kernel: Hsla,
    pub clock: Hsla,
    pub temperature: Hsla,
    pub gpu: Hsla,
    pub memory: Hsla,
    pub swap: Hsla,
    pub received: Hsla,
    pub sent: Hsla,
    pub read: Hsla,
    pub written: Hsla,
    pub battery: Hsla,
    pub power: Hsla,
    /// Colors handed out in turn when a chart has many lines (thermal sensors).
    pub cycle: [Hsla; 6],
    pub grid: Hsla,
}

impl Palette {
    pub fn new(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            cpu: theme.green,
            efficiency: theme.cyan,
            kernel: theme.blue,
            clock: theme.red,
            temperature: theme.yellow,
            gpu: theme.cyan,
            memory: theme.magenta,
            swap: theme.blue,
            received: theme.cyan,
            sent: theme.red,
            read: theme.blue,
            written: theme.yellow,
            battery: theme.success,
            power: theme.red,
            cycle: [theme.yellow, theme.red, theme.cyan, theme.magenta, theme.blue, theme.green],
            grid: theme.chart_grid,
        }
    }

    /// The `ix`th color from [`Self::cycle`], fainter on the second time round
    /// so repeats can still be told apart.
    pub fn cycled(&self, ix: usize) -> Hsla {
        let color = self.cycle[ix % self.cycle.len()];
        if ix < self.cycle.len() { color } else { color.opacity(0.55) }
    }
}
