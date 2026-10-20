use std::borrow::Cow;

use gpui_kit::assets::{Assets as ComponentAssets, icon_assets};
use gpui_kit::{AssetSource, Result, SharedString};

// Lucide icons purrtop uses beyond Kit's default set.
icon_assets!(ExtraIcons, [ListTree, Activity, Cpu, MemoryStick, HardDrive, Usb, Wifi, EthernetPort, Gpu, Thermometer, Battery, BatteryCharging, Rocket, Users, Cog, Plug]);

/// The purrtop logo, shown in the title bar and the About dialog.
pub const LOGO: &str = "images/logo.png";

/// Kit's default icons, purrtop's extras and the logo.
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == LOGO {
            return Ok(Some(Cow::Borrowed(include_bytes!("../logo.png"))));
        }
        // Check the extras first: the default source errors on unknown paths.
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        ComponentAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = ComponentAssets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
