use gpui_kit::assets::IconName as CatalogIcon;

/// The pages in the sidebar, in order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Summary,
    Performance,
    Processes,
    SystemInfo,
    Startup,
}

impl Page {
    pub const ALL: [Page; 5] = [Page::Summary, Page::Performance, Page::Processes, Page::SystemInfo, Page::Startup];

    pub fn label(self) -> &'static str {
        match self {
            Page::Summary => "Summary",
            Page::Performance => "Performance",
            Page::Processes => "Processes",
            Page::SystemInfo => "System Info",
            Page::Startup => "Startup apps",
        }
    }

    pub fn icon(self) -> CatalogIcon {
        match self {
            Page::Summary => CatalogIcon::LayoutDashboard,
            Page::Performance => CatalogIcon::Activity,
            Page::Processes => CatalogIcon::ListTree,
            Page::SystemInfo => CatalogIcon::Info,
            Page::Startup => CatalogIcon::Rocket,
        }
    }
}
