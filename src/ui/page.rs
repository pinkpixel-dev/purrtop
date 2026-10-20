use gpui_kit::assets::IconName as CatalogIcon;

/// The pages in the sidebar, in order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Summary,
    Processes,
}

impl Page {
    pub const ALL: [Page; 2] = [Page::Summary, Page::Processes];

    pub fn label(self) -> &'static str {
        match self {
            Page::Summary => "Summary",
            Page::Processes => "Processes",
        }
    }

    pub fn icon(self) -> CatalogIcon {
        match self {
            Page::Summary => CatalogIcon::LayoutDashboard,
            Page::Processes => CatalogIcon::ListTree,
        }
    }
}
