use gpui_kit::assets::IconName as CatalogIcon;

/// The pages in the sidebar, in order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Summary,
}

impl Page {
    pub const ALL: [Page; 1] = [Page::Summary];

    pub fn label(self) -> &'static str {
        match self {
            Page::Summary => "Summary",
        }
    }

    pub fn icon(self) -> CatalogIcon {
        match self {
            Page::Summary => CatalogIcon::LayoutDashboard,
        }
    }
}
