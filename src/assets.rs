use std::borrow::Cow;

use gpui_kit::assets::Assets as ComponentAssets;
use gpui_kit::{AssetSource, Result, SharedString};

/// The purrtop logo, shown in the title bar and the About dialog.
pub const LOGO: &str = "images/logo.png";

/// Kit's default icons plus the purrtop logo.
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == LOGO {
            return Ok(Some(Cow::Borrowed(include_bytes!("../logo.png"))));
        }
        ComponentAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        ComponentAssets.list(path)
    }
}
