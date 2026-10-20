//! The CPU detail: overall meter, a chart per logical processor, and stats.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::devices;
use super::parts::{facts, header, stats};
use crate::system::cpu_info::CoreKind;
use crate::system::history::History;
use crate::system::monitor::Monitor;
use crate::system::{Snapshot, format};
use crate::ui::charts::{Grid, Palette, Series, line_chart};
use crate::ui::summary::panel::legend;

/// Vertical space taken by everything but the core grid: title and status
/// bars, page heading, the header, legend, stats, facts, padding and gaps.
const RESERVED_HEIGHT: f32 = 450.;
/// Horizontal space taken by everything but the grid and the device list:
/// sidebar, page and panel padding, the gap between list and panel.
const RESERVED_WIDTH: f32 = 290.;
const MIN_GRID_HEIGHT: f32 = 240.;
const MIN_CELL_HEIGHT: f32 = 56.;
const CELL_GAP: f32 = 4.;
/// The cell shape the grid aims for, width over height.
const CELL_ASPECT: f32 = 1.;

pub fn render(monitor: &Monitor, snapshot: &Snapshot, palette: &Palette, viewport: Size<Pixels>, compact: bool, cx: &App) -> Div {
    let info = &snapshot.cpu_info;
    let hybrid = info.hybrid_split();
    let count = monitor.cores.len();

    let width = (f32::from(viewport.width) - RESERVED_WIDTH - devices::width(compact)).max(1.);
    let wanted_height = (f32::from(viewport.height) - RESERVED_HEIGHT).max(MIN_GRID_HEIGHT);
    let columns = grid_columns(count, width, wanted_height);
    let rows = count.div_ceil(columns.max(1));
    let height = wanted_height.max(rows as f32 * (MIN_CELL_HEIGHT + CELL_GAP));

    let kind = |ix: usize| info.kinds.get(ix).copied().unwrap_or_default();
    let cells: Vec<AnyElement> = (0..count)
        .map(|ix| {
            let label = match (hybrid, kind(ix)) {
                (Some(_), CoreKind::Performance) => format!("CPU {ix} · P"),
                (Some(_), CoreKind::Efficiency) => format!("CPU {ix} · E"),
                _ => format!("CPU {ix}"),
            };
            let color = if kind(ix) == CoreKind::Efficiency { palette.efficiency } else { palette.cpu };
            let kernel = monitor.core_kernel.get(ix);
            core_cell(label, &monitor.cores[ix], kernel, color, palette, cx)
        })
        .collect();
    let mut cells = cells.into_iter();
    let grid = v_flex().h(px(height)).flex_shrink_0().gap(px(CELL_GAP)).children((0..rows).map(|_| {
        let row: Vec<AnyElement> = cells.by_ref().take(columns).collect();
        // Pad the last row so its cells keep the same width as the rest.
        let padding = columns - row.len();
        h_flex()
            .flex_1()
            .min_h_0()
            .gap(px(CELL_GAP))
            .children(row)
            .children((0..padding).map(|_| div().flex_1().min_w_0()))
    }));

    let mut subtitle = info.model.clone();
    if subtitle.is_empty() {
        subtitle = format!("{count} logical processors");
    }
    let threads: u64 = snapshot.processes.iter().map(|process| u64::from(process.threads)).sum();
    let [load1, load5, load15] = snapshot.load;

    v_flex()
        .gap_3()
        .child(header("CPU", subtitle, snapshot.cpu, format::percent(snapshot.cpu), palette.cpu, cx))
        .child(
            h_flex()
                .flex_wrap()
                .gap_x_4()
                .gap_y_1()
                .child(legend("Utilization", palette.cpu, cx))
                .child(legend("Kernel", palette.kernel, cx))
                .when(hybrid.is_some(), |this| {
                    this.child(legend("Performance cores", palette.cpu, cx)).child(legend("Efficiency cores", palette.efficiency, cx))
                }),
        )
        .child(grid)
        .child(stats(
            vec![
                ("Utilization", format::percent(snapshot.cpu)),
                ("Speed", snapshot.clock.map_or("n/a".into(), |clock| format::frequency(clock.current_mhz))),
                ("Temperature", snapshot.temperature.map_or("n/a".into(), format::celsius)),
                ("Processes", snapshot.processes.len().to_string()),
                ("Threads", threads.to_string()),
                ("Up time", format::uptime(snapshot.uptime)),
                ("Load", format!("{load1:.2} · {load5:.2} · {load15:.2}")),
            ],
            cx,
        ))
        .child(facts(
            vec![
                ("Base speed", info.base_mhz.map_or("n/a".into(), format::frequency)),
                ("Max speed", snapshot.clock.map_or("n/a".into(), |clock| format::frequency(clock.max_mhz))),
                ("Sockets", info.sockets.to_string()),
                ("Physical cores", info.physical_cores.to_string()),
                ("Logical processors", count.to_string()),
                (
                    "Core types",
                    hybrid.map_or("Uniform".into(), |(performance, efficiency)| {
                        format!("{performance} P-core and {efficiency} E-core threads")
                    }),
                ),
                ("Virtualization", info.virtualization.map_or("Not available".into(), str::to_string)),
                ("Virtual machine", if info.virtual_machine { "Yes" } else { "No" }.into()),
                ("Frequency driver", info.driver.clone().unwrap_or_else(|| "n/a".into())),
                ("Governor", info.governor.clone().unwrap_or_else(|| "n/a".into())),
            ],
            cx,
        ))
}

fn core_cell(label: String, usage: &History, kernel: Option<&History>, color: Hsla, palette: &Palette, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let mut series = Vec::new();
    if let Some(kernel) = kernel {
        series.push(Series::new(kernel, 1., palette.kernel));
    }
    series.push(Series::new(usage, 1., color).area());
    div()
        .relative()
        .flex_1()
        .min_w_0()
        .h_full()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .overflow_hidden()
        .child(line_chart(series, Some(Grid { rows: 4, columns: 6, color: palette.grid })))
        .child(div().absolute().top_0p5().left_1p5().text_xs().text_color(theme.muted_foreground).child(label))
        .into_any_element()
}

/// How many columns the grid uses so cells come out closest to
/// [`CELL_ASPECT`] in a `width` by `height` area, with few empty slots
/// left in the last row.
fn grid_columns(count: usize, width: f32, height: f32) -> usize {
    if count == 0 || width <= 0. || height <= 0. {
        return 1;
    }
    (1..=count)
        .min_by(|&a, &b| aspect_error(count, a, width, height).total_cmp(&aspect_error(count, b, width, height)))
        .unwrap_or(1)
}

fn aspect_error(count: usize, columns: usize, width: f32, height: f32) -> f32 {
    let rows = count.div_ceil(columns);
    let aspect = (width / columns as f32) / (height / rows as f32);
    let empty = (rows * columns - count) as f32 / (rows * columns) as f32;
    (aspect / CELL_ASPECT).ln().abs() + empty
}

#[cfg(test)]
mod tests {
    use super::grid_columns;

    #[test]
    fn picks_a_grid_that_fits_the_area() {
        // 32 threads in a wide area: 8 across, 4 down.
        assert_eq!(grid_columns(32, 1100., 560.), 8);
        // The same CPU in a tall, narrow area goes 4 across.
        assert_eq!(grid_columns(32, 500., 1000.), 4);
        assert_eq!(grid_columns(4, 1000., 300.), 4);
        assert_eq!(grid_columns(1, 800., 400.), 1);
        assert_eq!(grid_columns(0, 800., 400.), 1);
    }
}
