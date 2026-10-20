"""Generates purrtop's bundled GPUI Kit themes from one palette table.

Usage, from the repo root: python3 tools/gen_themes.py themes
"""
import json
import sys
from pathlib import Path

# bg: window, surface: sidebar/title/status, raised: popovers, hover, active: selected rows
# fg / fg2 / muted: text levels, primary + primary_fg: the accent
# red..cyan: base colors, danger: destructive actions, charts: series order
PALETTES = {
    "neon": dict(
        name="Neon", bg="#17181a", surface="#1f2023", raised="#222326", hover="#26272b",
        active="#2c2d31", border="#2e3034", fg="#ededed", fg2="#bdbdbd", muted="#8d8d8d",
        primary="#f92571", primary_hover="#ff5894", primary_active="#e0185f", primary_fg="#1d0510",
        red="#f92571", red_l="#ff5894", green="#81b313", green_l="#b5e354", blue="#3390ff",
        blue_l="#6aaeff", yellow="#f1d300", yellow_l="#fff269", magenta="#9d6efe",
        magenta_l="#bb9afe", cyan="#00d8f5", cyan_l="#80f4f4", danger="#ff4757",
        charts=["#00d8f5", "#f92571", "#81b313", "#f1d300", "#9d6efe"], selection="#5a2440",
    ),
    "rainbow": dict(
        name="Rainbow", bg="#18171c", surface="#1f1e24", raised="#232228", hover="#29282f",
        active="#302f37", border="#33323a", fg="#efedf3", fg2="#c2bfca", muted="#8e8a98",
        primary="#a77bff", primary_hover="#bf9cff", primary_active="#9163f0", primary_fg="#140a26",
        red="#ff5f6d", red_l="#ff8a94", green="#5ee08a", green_l="#8ff0b0", blue="#4da6ff",
        blue_l="#80c0ff", yellow="#ffd84d", yellow_l="#ffe680", magenta="#a77bff",
        magenta_l="#c4a6ff", cyan="#4de0e0", cyan_l="#85eded", danger="#ff5f6d",
        charts=["#4da6ff", "#5ee08a", "#ffd84d", "#ffa04d", "#ff5f6d"], selection="#3d2f5c",
    ),
    "cotton-candy": dict(
        name="Cotton Candy", bg="#1c1a20", surface="#232027", raised="#27242c", hover="#2d2a33",
        active="#34303b", border="#38343f", fg="#f3eef5", fg2="#cbc3d0", muted="#958d9c",
        primary="#ffa3cf", primary_hover="#ffbfdd", primary_active="#f58cbf", primary_fg="#2a0d1c",
        red="#ff8fa3", red_l="#ffb3c1", green="#a6e9c3", green_l="#c4f2d7", blue="#9cc7ff",
        blue_l="#bfdbff", yellow="#ffe3a3", yellow_l="#fff0c9", magenta="#c9a7ff",
        magenta_l="#dcc6ff", cyan="#8fe3e8", cyan_l="#b5eef1", danger="#ff7a8f",
        charts=["#ffa3cf", "#8fe3e8", "#c9a7ff", "#a6e9c3", "#ffe3a3"], selection="#4d3448",
    ),
    "sunset": dict(
        name="Sunset", bg="#1c1716", surface="#231d1c", raised="#28211f", hover="#2e2624",
        active="#362c29", border="#3a302d", fg="#f5eeeb", fg2="#d0c3be", muted="#9c8c86",
        primary="#ff7a59", primary_hover="#ff9a80", primary_active="#f06542", primary_fg="#2a0f07",
        red="#ff5468", red_l="#ff8291", green="#b5d46a", green_l="#cde396", blue="#7aa2ff",
        blue_l="#a3bfff", yellow="#ffc145", yellow_l="#ffd47d", magenta="#e05297",
        magenta_l="#ea7fb2", cyan="#5fd0c6", cyan_l="#8edfd8", danger="#ff5468",
        charts=["#ff7a59", "#ffc145", "#e05297", "#5fd0c6", "#b5d46a"], selection="#5a3026",
    ),
    "matcha": dict(
        name="Matcha", bg="#171a17", surface="#1d211d", raised="#212521", hover="#262b26",
        active="#2d332d", border="#313831", fg="#eef2e9", fg2="#c5cdbd", muted="#8c9685",
        primary="#8fc58a", primary_hover="#aad6a6", primary_active="#7ab574", primary_fg="#0d1a0c",
        red="#e08b7a", red_l="#eaab9e", green="#a3d47a", green_l="#bfe2a0", blue="#8fb6c9",
        blue_l="#b0cddb", yellow="#e6d29a", yellow_l="#efe1ba", magenta="#c7a8d6",
        magenta_l="#d9c3e3", cyan="#7fc8b5", cyan_l="#a5d9cb", danger="#e07a6a",
        charts=["#8fc58a", "#e6d29a", "#7fc8b5", "#c7a8d6", "#8fb6c9"], selection="#34482f",
    ),
    "graphite": dict(
        name="Graphite", bg="#161718", surface="#1c1d1f", raised="#202123", hover="#252628",
        active="#2b2c2f", border="#2f3033", fg="#ededee", fg2="#bfc0c3", muted="#8a8c90",
        primary="#e8e8ea", primary_hover="#ffffff", primary_active="#cfcfd2", primary_fg="#161718",
        red="#f2555a", red_l="#f6858a", green="#4fc985", green_l="#80dba8", blue="#4d9fff",
        blue_l="#80bbff", yellow="#f5a524", yellow_l="#f8c066", magenta="#a78bfa",
        magenta_l="#c3b0fc", cyan="#4cc7d9", cyan_l="#80d8e5", danger="#f2555a",
        charts=["#e8e8ea", "#4d9fff", "#4fc985", "#f5a524", "#a78bfa"], selection="#3a3b40",
    ),
    "midnight": dict(
        name="Midnight", bg="#1a1b26", surface="#16161e", raised="#1f2030", hover="#222436",
        active="#292e42", border="#292e42", fg="#c0caf5", fg2="#a9b1d6", muted="#636a91",
        primary="#7aa2f7", primary_hover="#9ab8ff", primary_active="#6690ea", primary_fg="#0f1322",
        red="#f7768e", red_l="#ff9aac", green="#9ece6a", green_l="#b9e08f", blue="#7aa2f7",
        blue_l="#9ab8ff", yellow="#e0af68", yellow_l="#ebc690", magenta="#bb9af7",
        magenta_l="#d0b8fa", cyan="#7dcfff", cyan_l="#a6defe", danger="#f7768e",
        charts=["#7aa2f7", "#bb9af7", "#7dcfff", "#9ece6a", "#e0af68"], selection="#33467c",
    ),
    "phosphor": dict(
        name="Phosphor", bg="#0d110d", surface="#0a0e0a", raised="#111811", hover="#142014",
        active="#1a2b1a", border="#1f301f", fg="#7dff9b", fg2="#55d974", muted="#3a8a4c",
        primary="#33ff66", primary_hover="#70ff94", primary_active="#22e052", primary_fg="#031a08",
        red="#ff5f3a", red_l="#ff8a6e", green="#33ff66", green_l="#70ff94", blue="#4dd2ff",
        blue_l="#80e0ff", yellow="#ffb000", yellow_l="#ffc94d", magenta="#d98cff",
        magenta_l="#e6b3ff", cyan="#3fffd0", cyan_l="#80ffe2", danger="#ff5f3a",
        charts=["#33ff66", "#ffb000", "#9dff5c", "#3fffd0", "#ff7a3a"], selection="#1f4d2a",
    ),
}


def colors(p):
    a = lambda hex_, alpha: hex_ + alpha
    return {
        "accent.background": p["active"],
        "accent.foreground": p["fg"],
        "accordion.background": p["bg"],
        "background": p["bg"],
        "border": p["border"],
        "group_box.background": p["surface"],
        "group_box.foreground": p["fg"],
        "caret": p["primary"],
        **{f"chart.{i + 1}": c for i, c in enumerate(p["charts"])},
        "chart.bullish": p["green"],
        "chart.bearish": p["red"],
        "chart.grid": a(p["border"], "99"),
        "danger.background": p["danger"],
        "danger.foreground": p["danger"],
        "description_list_label.background": p["surface"],
        "description_list_label.foreground": p["fg2"],
        "drag_border": p["primary"],
        "drop_target.background": a(p["primary"], "1f"),
        "foreground": p["fg"],
        "info.background": p["cyan"],
        "info.foreground": p["cyan_l"],
        "input.border": p["border"],
        "link.foreground": p["primary"],
        "link.active.foreground": p["primary_active"],
        "link.hover.foreground": p["primary_hover"],
        "list.background": p["bg"],
        "list.active.background": a(p["primary"], "26"),
        "list.active.border": p["primary"],
        "list.even.background": a(p["surface"], "66"),
        "list.head.background": p["surface"],
        "list.hover.background": p["hover"],
        "muted.background": p["hover"],
        "muted.foreground": p["muted"],
        "popover.background": p["raised"],
        "popover.foreground": p["fg"],
        "primary.background": p["primary"],
        "primary.active.background": p["primary_active"],
        "primary.foreground": p["primary_fg"],
        "primary.hover.background": p["primary_hover"],
        "progress_bar.background": p["primary"],
        "ring": p["primary"],
        "scrollbar.background": a(p["bg"], "00"),
        "scrollbar.thumb.background": a(p["muted"], "66"),
        "scrollbar.thumb.hover.background": a(p["muted"], "aa"),
        "secondary.background": p["hover"],
        "secondary.active.background": p["raised"],
        "secondary.foreground": p["fg"],
        "secondary.hover.background": p["active"],
        "selection.background": p["selection"],
        "sidebar.background": p["surface"],
        "sidebar.accent.background": a(p["primary"], "26"),
        "sidebar.accent.foreground": p["fg"],
        "sidebar.border": p["border"],
        "sidebar.foreground": p["fg2"],
        "sidebar.primary.background": p["primary"],
        "sidebar.primary.foreground": p["primary_fg"],
        "skeleton.background": p["hover"],
        "slider.bar.background": p["primary"],
        "slider.thumb.background": p["fg"],
        "success.background": p["green"],
        "success.foreground": p["green"],
        "switch.background": p["border"],
        "tab.background": "#00000000",
        "tab.active.background": p["bg"],
        "tab.active.foreground": p["fg"],
        "tab_bar.background": p["surface"],
        "tab_bar.segmented.background": p["surface"],
        "tab.foreground": p["muted"],
        "table.background": p["bg"],
        "table.head.foreground": p["muted"],
        "table.row.border": a(p["border"], "b3"),
        "title_bar.background": p["surface"],
        "title_bar.border": p["border"],
        "status_bar.background": p["surface"],
        "status_bar.border": p["border"],
        "warning.background": p["yellow"],
        "warning.foreground": p["yellow"],
        "overlay": "#00000066",
        "window.border": p["border"],
        "base.red": p["red"],
        "base.red.light": p["red_l"],
        "base.green": p["green"],
        "base.green.light": p["green_l"],
        "base.blue": p["blue"],
        "base.blue.light": p["blue_l"],
        "base.yellow": p["yellow"],
        "base.yellow.light": p["yellow_l"],
        "base.magenta": p["magenta"],
        "base.magenta.light": p["magenta_l"],
        "base.cyan": p["cyan"],
        "base.cyan.light": p["cyan_l"],
    }


def main():
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    for slug, p in PALETTES.items():
        theme = {
            "$schema": "https://github.com/longbridge/gpui-kit/raw/refs/heads/main/.theme-schema.json",
            "name": p["name"],
            "author": "Pink Pixel",
            "url": "https://pinkpixel.dev",
            "themes": [
                {
                    "name": p["name"],
                    "mode": "dark",
                    "radius": 6,
                    "radius.lg": 8,
                    "colors": colors(p),
                }
            ],
        }
        (out / f"{slug}.json").write_text(json.dumps(theme, indent=2) + "\n")
        print(f"wrote {slug}.json")


if __name__ == "__main__":
    main()
