//! A small "are you sure?" dialog for actions that can lose work.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex};
use gpui_kit::*;

const DIALOG_WIDTH: f32 = 420.;

/// Asks before running `on_confirm`. The confirm button is styled as dangerous.
pub fn confirm(
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    confirm_label: &'static str,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    if window.has_active_dialog(cx) {
        return;
    }
    let (title, message) = (title.into(), message.into());
    let on_confirm = std::rc::Rc::new(on_confirm);
    window.open_dialog(cx, move |dialog, _, cx| {
        let on_confirm = on_confirm.clone();
        dialog
            .w(px(DIALOG_WIDTH))
            .title(title.clone())
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child(message.clone()))
            .footer(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap_2()
                    .child(Button::new("confirm-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                    .child(Button::new("confirm-ok").danger().label(confirm_label).on_click(move |_, window, cx| {
                        window.close_dialog(cx);
                        on_confirm(window, cx);
                    })),
            )
    });
}
