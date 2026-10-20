//! The Software tab: the operating system, the session and installed packages.

use gpui_kit::*;

use super::section;
use crate::system::software::SoftwareInfo;

pub fn render(info: &SoftwareInfo, cx: &App) -> Vec<AnyElement> {
    let mut sections = vec![
        section(
            "Operating system",
            vec![
                ("Name", info.os.clone()),
                ("Version", info.os_version.clone()),
                ("Kernel", info.kernel.clone()),
                ("Kernel build", info.kernel_build.clone()),
                ("Architecture", info.architecture.clone()),
                ("Init system", info.init.clone()),
            ],
            cx,
        ),
        section(
            "Session",
            vec![
                ("Computer name", info.hostname.clone()),
                ("Desktop", info.desktop.clone()),
                ("Display server", info.session.clone()),
                ("Shell", info.shell.clone()),
                ("Locale", info.locale.clone()),
                ("Time zone", info.time_zone.clone()),
            ],
            cx,
        ),
    ];
    if !info.packages.is_empty() {
        let items = info.packages.iter().map(|(manager, count)| (*manager, Some(count.to_string()))).collect();
        sections.push(section("Installed packages", items, cx));
    }
    sections
}
