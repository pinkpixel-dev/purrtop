//! The Run new task dialog: type a command and start it, detached from purrtop.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::*;

use crate::system::launch;

const DIALOG_WIDTH: f32 = 460.;

pub fn open(window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("Program and arguments, like firefox --private-window"));
    let view = cx.new(|_| RunView { input: input.clone(), error: None });
    window.open_dialog(cx, move |dialog, _, _| {
        let (on_ok, on_run) = (view.clone(), view.clone());
        dialog
            .w(px(DIALOG_WIDTH))
            .title("Run new task")
            // Enter runs it. Returning false keeps the dialog open to fix a typo.
            .on_ok(move |_, window, cx| on_ok.update(cx, |view, cx| view.run(window, cx)))
            .child(view.clone())
            .footer(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap_2()
                    .child(Button::new("run-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                    .child(Button::new("run-ok").primary().label("Run").on_click(move |_, window, cx| {
                        if on_run.update(cx, |view, cx| view.run(window, cx)) {
                            window.close_dialog(cx);
                        }
                    })),
            )
    });
    input.update(cx, |input, cx| input.focus(window, cx));
}

struct RunView {
    input: Entity<InputState>,
    error: Option<String>,
}

impl RunView {
    /// Starts the command. Returns true when the dialog can close.
    fn run(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let command = self.input.read(cx).value().to_string();
        match launch::run(&command) {
            Ok(()) => {
                window.push_notification(Notification::success(format!("Started {}", command.trim())), cx);
                true
            }
            Err(err) => {
                self.error = Some(err);
                cx.notify();
                false
            }
        }
    }
}

impl Render for RunView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child(Input::new(&self.input).aria_label("Command"))
            .children(self.error.clone().map(|error| div().text_xs().text_color(cx.theme().danger).child(error)))
    }
}
