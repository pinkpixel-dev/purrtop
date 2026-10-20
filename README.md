<p align="center">
  <img src="logo.png" alt="purrtop logo" width="250">
</p>

# purrtop

Keep your system purring.

purrtop is a colorful system monitor and task manager for Linux. It's a desktop app built with Rust and [GPUI Kit](https://gpui-kit.com), and it reads everything straight from `/proc`.

I wanted something closer to Windows Task Manager (or System Informer) on Linux, but prettier, with themes that actually have some color in them.

> purrtop is early. Right now it has the app shell, a live Summary dashboard, a Performance page for CPU, memory, battery, thermals, disks, network and GPUs, the Processes page, a System Info page with Hardware, Software, Network and Diagnostics tabs, a Startup apps page, a Users page, and eight themes. Services, Disk Space and the rest are on the way.

## What it does right now

- A live Summary dashboard, updated every second:
  - LED columns for CPU, clock speed, CPU temperature and GPU
  - A CPU chart with utilization, kernel time and temperature over the last two minutes
  - The busiest processes right now
  - A memory chart, plus cards for system pressure, network, disk and GPUs
- A Performance page with a device list on the left and the selected device on the right:
  - CPU: a chart for every logical processor (P-cores and E-cores are labeled on hybrid Intel chips), plus speed, temperature, process and thread counts, load and the CPU's specs
  - Memory: use and swap over time, a bar showing what RAM is holding, and commit charge
  - Each disk: active time, read and write speed, response time, and how full each mounted volume is
  - Each network interface (Wi-Fi or Ethernet): receive and send speed, totals since boot, link speed, MAC and IP addresses
  - Battery: charge and power draw over time, time left, health and cycle count
  - Thermals: every temperature sensor on one chart (CPU, NVMe drives, memory modules, Wi-Fi and network cards), with each one's limit and peak
  - Each GPU: usage, clock, temperature, power and video memory, whichever the driver shares. NVIDIA cards show their state and PCI details only
- GPUs are read in a way that never wakes a sleeping laptop GPU. The tradeoff is that NVIDIA cards only show whether they're awake, not usage
- A Processes page that works a lot like Windows Task Manager's Details tab:
  - Applications and Background groups with child processes folded under their parents, or one flat list
  - Sortable, resizable columns for CPU, memory, disk I/O, threads, open files, nice and the full command
  - Filter by name, user or PID
  - New processes flash green, and ones that exit stay on screen in red for a moment
  - A right-click menu to terminate, kill, suspend or resume a process or a whole tree, send signals, change the nice value or CPU affinity, take a stack sample, open the file location, and more
  - A details dialog with the process's status, limits, environment and open files
- A System Info page:
  - Hardware: your machine's maker and model, board, firmware, boot mode and Secure Boot, plus the processor, memory, GPUs, disks and batteries
  - Software: distro, kernel, desktop and display server, shell, locale, time zone, and how many packages you have installed (pacman, dpkg and Flatpak)
  - Network: default gateways, DNS servers, search domains, and every interface (physical ones first, then bridges, Docker, VPN and other virtual ones) with its addresses, MAC, MTU and driver. It refreshes every 5 seconds while you're looking at it
  - Connected Wi-Fi also shows the network name, Wi-Fi generation, band, signal and link rates. This part needs `iw` installed (`sudo pacman -S iw` on Arch) and is skipped without it
  - Diagnostics: failed systemd units, error messages since boot (grouped, so one noisy app doesn't bury everything else), crashes from the last 7 days, and anything killed for running out of memory. It reads the journal when you open the tab, and the Refresh button reads it again
  - Serial numbers and RAM slot details need root, so they're not shown
- A Startup apps page for everything in your autostart folders (`~/.config/autostart` and `/etc/xdg/autostart`):
  - Flip the switch on a row to turn it on or off for your next login. Changes only apply when you log in again, because that's when your desktop reads these files
  - Turning off a system entry writes your own copy with `Hidden=true` instead of touching `/etc`. If an entry is a link to an app launcher, purrtop swaps the link for a real file, so the launcher and your app menu stay as they were
  - Status shows whether each one is running, failed or finished. This comes from systemd's autostart units, which COSMIC, KDE and GNOME all use
  - Start or stop an entry right now from the right-click menu, open its file location, or copy its command
  - Entries for other desktops (KDE-only stuff on COSMIC, for example) are folded into a "Not started here" group
- A Users page that shows who's signed in and what every account is running:
  - CPU, memory and process count per account, with signed-in people first and system accounts (root, `systemd-*` and friends) folded into their own group
  - Status (Active, Idle or Locked) and session type for anyone signed in, read from systemd-logind
  - Open a user to see their processes, busiest first
  - Right-click to lock a user's sessions or sign them out. Sign out asks first, since everything in the session closes (purrtop too, if it's your own)
  - Memory is each process's resident memory added up, so libraries shared between processes count more than once. It runs a bit high compared to the Memory page
- File → Run new task (`Ctrl+N`) to start any command, like Windows Task Manager's Run dialog. It keeps running after you close purrtop
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
| `Ctrl+N` | Run new task |
| `Alt+Down` / `Alt+Up` | Next / previous page |

On the Performance page, `Up` and `Down` move through the device list.

On the System Info page, `Ctrl+Tab` and `Ctrl+Shift+Tab` switch tabs.

On the Startup apps page, `Up`, `Down`, `Home` and `End` move the selection, `Space` turns the selected entry on or off, and `F5` reads the folders again.

On the Users page, `Up`, `Down`, `Home` and `End` move the selection, and `Right` / `Left` open or close a user's processes (Left on a process jumps back to its user).

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
