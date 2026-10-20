//! The "Set affinity" dialog: tick the CPUs a process may run on.

use std::collections::BTreeSet;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::*;

const DIALOG_WIDTH: f32 = 520.;
const CELL_WIDTH: f32 = 72.;

/// Opens the dialog. `apply` gets the chosen CPUs when you press Apply.
pub fn open(name: String, cpu_count: usize, current: Vec<usize>, apply: impl Fn(Vec<usize>, &mut Window, &mut App) + 'static, window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }
    let view = cx.new(|_| AffinityView { cpu_count, chosen: current.into_iter().filter(|&cpu| cpu < cpu_count).collect() });
    let apply = Rc::new(apply);
    window.open_dialog(cx, move |dialog, _, cx| {
        let (apply, chosen_view) = (apply.clone(), view.clone());
        let nothing_chosen = view.read(cx).chosen.is_empty();
        dialog.w(px(DIALOG_WIDTH)).title(format!("CPUs {name} may run on")).child(view.clone()).footer(
            h_flex()
                .w_full()
                .justify_end()
                .gap_2()
                .child(Button::new("affinity-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                .child(Button::new("affinity-apply").primary().label("Apply").disabled(nothing_chosen).on_click(move |_, window, cx| {
                    let cpus: Vec<usize> = chosen_view.read(cx).chosen.iter().copied().collect();
                    window.close_dialog(cx);
                    apply(cpus, window, cx);
                })),
        )
    });
}

struct AffinityView {
    cpu_count: usize,
    chosen: BTreeSet<usize>,
}

impl Render for AffinityView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let all = self.chosen.len() == self.cpu_count;
        let boxes = (0..self.cpu_count).map(|cpu| {
            div().w(px(CELL_WIDTH)).child(
                Checkbox::new(("cpu", cpu)).label(format!("CPU {cpu}")).checked(self.chosen.contains(&cpu)).on_click(cx.listener(
                    move |view, checked: &bool, window, cx| {
                        if *checked {
                            view.chosen.insert(cpu);
                        } else {
                            view.chosen.remove(&cpu);
                        }
                        // Re-render the dialog too, so Apply turns off with nothing ticked.
                        window.refresh();
                        cx.notify();
                    },
                )),
            )
        });
        v_flex()
            .gap_3()
            .child(
                Checkbox::new("cpu-all").label("All CPUs").checked(all).on_click(cx.listener(move |view, checked: &bool, window, cx| {
                    view.chosen = if *checked { (0..view.cpu_count).collect() } else { BTreeSet::new() };
                    window.refresh();
                    cx.notify();
                })),
            )
            .child(h_flex().flex_wrap().gap_y_2().children(boxes))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Applies to every thread of the process."))
    }
}
