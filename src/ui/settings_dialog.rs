//! The Settings dialog (File → Settings…, `Ctrl+,`): the theme and the
//! opt-in NVIDIA details. Every change applies and saves right away.

use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::*;

use super::SwitchTheme;
use crate::settings::Settings;
use crate::system::gpu::Vendor;
use crate::system::monitor::Monitor;
use crate::theme::Themes;

const DIALOG_WIDTH: f32 = 440.;
const LABEL_WIDTH: f32 = 128.;

pub fn open(monitor: Entity<Monitor>, window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }
    let view = cx.new(|cx| {
        let has_nvidia = monitor.read(cx).latest().is_some_and(|snapshot| snapshot.gpus.iter().any(|gpu| gpu.vendor == Vendor::Nvidia));
        // The theme can change from the dropdown, so its label follows along.
        let _theme = cx.observe_global::<Themes>(|_, cx| cx.notify());
        SettingsView { monitor, nvidia: Settings::load().nvidia, has_nvidia, _theme }
    });
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .w(px(DIALOG_WIDTH))
            .title("Settings")
            .child(view.clone())
            .footer(h_flex().w_full().justify_end().child(Button::new("settings-done").primary().label("Done").on_click(|_, window, cx| window.close_dialog(cx))))
    });
}

struct SettingsView {
    monitor: Entity<Monitor>,
    nvidia: bool,
    /// Without one the switch would do nothing, so it's turned off.
    has_nvidia: bool,
    _theme: Subscription,
}

impl SettingsView {
    fn set_nvidia(&mut self, nvidia: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.nvidia = nvidia;
        self.monitor.read(cx).set_nvidia(nvidia);
        let mut settings = Settings::load();
        settings.nvidia = nvidia;
        if let Err(err) = settings.save() {
            window.push_notification(Notification::error(format!("Couldn't save the setting: {err}")), cx);
        }
        cx.notify();
    }
}

fn row(label: &'static str, control: impl IntoElement, cx: &App) -> Div {
    h_flex().gap_3().items_center().child(div().w(px(LABEL_WIDTH)).flex_shrink_0().text_sm().text_color(cx.theme().muted_foreground).child(label)).child(control)
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_picker = Button::new("settings-theme")
            .outline()
            .small()
            .icon(Icon::new(CatalogIcon::Palette))
            .label(Themes::global(cx).current_name())
            .dropdown_menu(|mut menu, _, cx| {
                let themes = Themes::global(cx);
                let current = themes.current();
                for (ix, name) in themes.names().enumerate() {
                    menu = menu.menu_with_check(name.clone(), ix == current, Box::new(SwitchTheme(ix)));
                }
                menu
            });
        let nvidia = Switch::new("settings-nvidia")
            .checked(self.nvidia)
            .disabled(!self.has_nvidia)
            .tooltip(if self.nvidia { "Stop reading NVIDIA GPUs through NVML" } else { "Read NVIDIA GPUs through NVML" })
            .on_click(cx.listener(|view, checked: &bool, window, cx| view.set_nvidia(*checked, window, cx)));
        let note = if self.has_nvidia { "Usage, memory, power and clocks, read only while the GPU is awake." } else { "No NVIDIA GPU found." };
        v_flex().gap_4().child(row("Theme", theme_picker, cx)).child(
            v_flex()
                .gap_1()
                .child(row("NVIDIA details", nvidia, cx))
                .child(div().pl(px(LABEL_WIDTH + 12.)).text_xs().text_color(cx.theme().muted_foreground).child(note)),
        )
    }
}
