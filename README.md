<p align="center">
  <img src="logo.png" alt="purrtop logo" width="250">
</p>

# purrtop

Keep your system purring.

purrtop is a colorful system monitor and task manager for Linux. It's a desktop app built with Rust and [GPUI Kit](https://gpui-kit.com), and it reads everything straight from `/proc`.

I wanted something closer to Windows Task Manager (or System Informer) on Linux, but prettier, with themes that actually have some color in them.

> purrtop is early. Right now it has the app shell, a live Summary dashboard, the Processes page, and eight themes. Performance, Services, Disk Space and the rest are on the way.

## What it does right now

- A live Summary dashboard, updated every second:
  - LED columns for CPU, clock speed, CPU temperature and GPU
  - A CPU chart with utilization, kernel time and temperature over the last two minutes
  - The busiest processes right now
  - A memory chart, plus cards for system pressure, network, disk and GPUs
- GPUs are read in a way that never wakes a sleeping laptop GPU. The tradeoff is that NVIDIA cards only show whether they're awake, not usage
- A Processes page that works a lot like Windows Task Manager's Details tab:
  - Applications and Background groups with child processes folded under their parents, or one flat list
  - Sortable, resizable columns for CPU, memory, disk I/O, threads, open files, nice and the full command
  - Filter by name, user or PID
  - New processes flash green, and ones that exit stay on screen in red for a moment
  - A right-click menu to terminate, kill, suspend or resume a process or a whole tree, send signals, change the nice value or CPU affinity, take a stack sample, open the file location, and more
  - A details dialog with the process's status, limits, environment and open files
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

On the Processes page:

| Keys | Action |
|---|---|
| `Up` / `Down`, `Page Up` / `Page Down`, `Home` / `End` | Move the selection |
| `Right` / `Left` | Open or close a process's children (Left on a child jumps to its parent) |
| `Enter` | Details for the selected process |
| `Delete` | Terminate (SIGTERM) |
| `Shift+Delete` | Force terminate (SIGKILL), after asking |
| `Ctrl+F` | Filter |

## Admin actions

Ending, renicing or changing the CPU affinity of your own processes just works. For other users' processes (or a nice value below 0), purrtop runs the same command through `pkexec`, so your desktop's polkit agent asks for your password first.

"Create process sample" uses `eu-stack` from elfutils, and it always goes through `pkexec`, since reading another process's stacks needs ptrace rights. The sample is saved to `~/.cache/purrtop/samples/` and opened in your text editor.

## Settings

purrtop saves your settings to `~/.config/purrtop/settings.json` (or `$XDG_CONFIG_HOME/purrtop/settings.json`). Right now that's only the theme.

## License

Apache 2.0. See [LICENSE](LICENSE).

Made with 💖 by [Pink Pixel](https://pinkpixel.dev)
