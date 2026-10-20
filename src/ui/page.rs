use gpui_kit::assets::IconName as CatalogIcon;

/// The pages in the sidebar, in order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Summary,
    Performance,
    Processes,
}

impl Page {
    pub const ALL: [Page; 3] = [Page::Summary, Page::Performance, Page::Processes];

    pub fn label(self) -> &'static str {
        match self {
            Page::Summary => "Summary",
            Page::Performance => "Performance",
            Page::Processes => "Processes",
        }
    }

    pub fn icon(self) -> CatalogIcon {
        match self {
            Page::Summary => CatalogIcon::LayoutDashboard,
            Page::Performance => CatalogIcon::Activity,
            Page::Processes => CatalogIcon::ListTree,
        }
    }
}
