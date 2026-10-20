<p align="center">
  <img src="logo.png" alt="purrtop logo" width="250">
</p>

# purrtop

Keep your system purring.

purrtop is a colorful system monitor and task manager for Linux. It's a desktop app built with Rust and [GPUI Kit](https://gpui-kit.com), and it reads everything straight from `/proc`.

I wanted something closer to Windows Task Manager (or System Informer) on Linux, but prettier, with themes that actually have some color in them.

> purrtop is early. Right now it has the app shell, a live Summary dashboard, and eight themes. Processes, Performance, Services, Disk Space and the rest are on the way.

## What it does right now

- A live Summary dashboard, updated every second:
  - LED columns for CPU, clock speed, CPU temperature and GPU
  - A CPU chart with utilization, kernel time and temperature over the last two minutes
  - The busiest processes right now
  - A memory chart, plus cards for system pressure, network, disk and GPUs
- GPUs are read in a way that never wakes a sleeping laptop GPU. The tradeoff is that NVIDIA cards only show whether they're awake, not usage
- Status bar with the current readings and a quick theme picker
- Eight built-in themes: Neon, Rainbow, Cotton Candy, Sunset, Matcha, Midnight, Phosphor and Graphite
- File / View / Help menu bar, with themes under View → Theme
- Remembers your theme between launches

## Requirements

- Linux (purrtop reads `/proc` directly, so it won't run anywhere else)
- A working Vulkan driver, since GPUI draws with Vulkan
- Rust 1.92 or newer (same as the GPUI Kit apps it shares a stack with)

## Build and run

```bash
git clone https://github.com/pinkpixel-dev/purrtop.git
cd purrtop
cargo run --release
```

If the window doesn't open, run `vulkaninfo --summary` to check your Vulkan setup. On Arch, the driver packages are `vulkan-radeon`, `vulkan-intel` or `nvidia-utils`.

## Keyboard

| Keys | Action |
|---|---|
| `Ctrl+Q` | Quit |
| `Alt+Down` / `Alt+Up` | Next / previous page |

## Settings

purrtop saves your settings to `~/.config/purrtop/settings.json` (or `$XDG_CONFIG_HOME/purrtop/settings.json`). Right now that's only the theme.

## License

Apache 2.0. See [LICENSE](LICENSE).

Made with 💖 by [Pink Pixel](https://pinkpixel.dev)
