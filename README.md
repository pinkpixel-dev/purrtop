<p align="center">
  <img src="logo.png" alt="purrtop logo" width="250">
</p>

<h1 align="center">purrtop</h1>

<p align="center">Keep your system purring.</p>

<p align="center">
  <a href="https://aur.archlinux.org/packages/purrtop"><img src="https://img.shields.io/aur/version/purrtop" alt="AUR version"></a>
  <a href="https://crates.io/crates/purrtop"><img src="https://img.shields.io/crates/v/purrtop" alt="crates.io version"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="License: Apache 2.0"></a>
</p>

<p align="center">
  <img src="demo-small.gif" alt="purrtop demo">
</p>

purrtop is a colorful system monitor and task manager for Linux. It's a desktop app built with Rust and [GPUI Kit](https://gpui-kit.com), and it reads everything straight from `/proc`.

I wanted something closer to Windows Task Manager (or System Informer) on Linux, but prettier, with themes that actually have some color in them.

## Contents

- [Features](#features)
- [Requirements](#requirements)
- [Install](#install)
- [Build and run](#build-and-run)
- [Keyboard](#keyboard)
- [Admin actions](#admin-actions)
- [Settings](#settings)
- [License](#license)

## Features

### Summary

A live dashboard, updated every second:

- LED columns for CPU, clock speed, CPU temperature and GPU
- A CPU chart with utilization, kernel time and temperature over the last two minutes
- The busiest processes right now
- A memory chart, plus cards for system pressure, network, disk and GPUs

### Performance

A device list on the left and the selected device on the right:

- **CPU:** a chart for every logical processor (P-cores and E-cores are labeled on hybrid Intel chips), plus speed, temperature, process and thread counts, load and the CPU's specs
- **Memory:** use and swap over time, a bar showing what RAM is holding, and commit charge
- **Disks:** active time, read and write speed, response time, and how full each mounted volume is
- **Network:** receive and send speed for each Wi-Fi or Ethernet interface, totals since boot, link speed, MAC and IP addresses
- **Battery:** charge and power draw over time, time left, health and cycle count
- **Thermals:** every temperature sensor on one chart (CPU, NVMe drives, memory modules, Wi-Fi and network cards), with each one's limit and peak
- **GPUs:** usage, clock, temperature, power and video memory, whichever the driver shares. NVIDIA cards show their state and PCI details unless you turn on NVIDIA details

GPUs are read in a way that never wakes a sleeping laptop GPU. NVIDIA's driver doesn't share usage through sysfs, so there's an optional NVIDIA details setting that reads it through NVML instead (see [Settings](#settings)).

### Processes

This works a lot like Windows Task Manager's Details tab:

- Applications and Background groups with child processes folded under their parents, or one flat list
- Sortable, resizable columns for CPU, memory, disk I/O, threads, open files, nice and the full command
- Filter by name, user or PID
- New processes flash green, and ones that exit stay on screen in red for a moment
- A right-click menu to terminate, kill, suspend or resume a process or a whole tree, send signals, change the nice value or CPU affinity, take a stack sample, open the file location, and more
- A details dialog with the process's status, limits, environment and open files

### System Info

- **Hardware:** your machine's maker and model, board, firmware, boot mode and Secure Boot, plus the processor, memory, GPUs, disks and batteries
- **Software:** distro, kernel, desktop and display server, shell, locale, time zone, and how many packages you have installed (pacman, dpkg and Flatpak)
- **Network:** default gateways, DNS servers, search domains, and every interface (physical ones first, then bridges, Docker, VPN and other virtual ones) with its addresses, MAC, MTU and driver. It refreshes every 5 seconds while you're looking at it
- **Wi-Fi:** if you're connected, you also get the network name, Wi-Fi generation, band, signal and link rates. This part needs `iw` installed (`sudo pacman -S iw` on Arch) and is skipped without it
- **Diagnostics:** failed systemd units, error messages since boot (grouped, so one noisy app doesn't bury everything else), crashes from the last 7 days, and anything killed for running out of memory. It reads the journal when you open the tab, and the Refresh button reads it again

Serial numbers and RAM slot details need root, so they're not shown.

### Startup apps

Everything in your autostart folders (`~/.config/autostart` and `/etc/xdg/autostart`):

- Flip the switch on a row to turn it on or off for your next login. Changes only apply when you log in again, because that's when your desktop reads these files
- Turning off a system entry writes your own copy with `Hidden=true` instead of touching `/etc`. If an entry is a link to an app launcher, purrtop swaps the link for a real file, so the launcher and your app menu stay as they were
- Status shows whether each one is running, failed or finished. This comes from systemd's autostart units, which COSMIC, KDE and GNOME all use
- Start or stop an entry right now from the right-click menu, open its file location, or copy its command
- Add an installed app to startup from the Add button, which has a search box. purrtop links the app's launcher into your autostart folder
- Remove one of your own entries from the right-click menu or with `Delete`. Links just get deleted, and real files go to the Trash in case you want them back. System entries can't be removed, but you can turn them off
- When some of your entries point at apps or files that are gone (an uninstalled AppImage, say), a Clean up button shows up and removes them in one go
- Entries for other desktops (KDE-only stuff on COSMIC, for example) are folded into a "Not started here" group

### Users

Who's signed in and what every account is running:

- CPU, memory and process count per account, with signed-in people first and system accounts (root, `systemd-*` and friends) folded into their own group
- Status (Active, Idle or Locked) and session type for anyone signed in, read from systemd-logind
- Open a user to see their processes, busiest first
- Right-click to lock a user's sessions or sign them out. Sign out asks first, since everything in the session closes (purrtop too, if it's your own)

Memory here is each process's resident memory added up, so libraries shared between processes count more than once. It runs a bit high compared to the Memory page.

### Services

systemd services, either the system ones or your own user services:

- State, startup setting, main PID and memory for every service, with failed ones at the top and stopped ones that won't start on their own folded away
- It stays live: purrtop listens to systemd over D-Bus and updates as soon as something starts, stops or fails
- Right-click to start, stop, restart, enable or disable a service, or jump to its main process on the Processes page
- System services ask for your password through polkit, and systemd's default policy remembers it for a few minutes
- The detail strip shows the unit file, when it last changed state, and the last few lines of its log

### Ports

What's listening:

- Every listening TCP port and bound UDP port, with the address, process, PID and user
- Right-click to end or kill the process holding a port (it asks first), or jump to it on the Processes page
- Ports held by root or system services show who owns them, but Linux hides which process has them unless you're root. Click "Find hidden owners" to look them up once through polkit
- The detail strip shows whether a port is reachable from this computer only or from every network, plus the full command line

### Disk Space

For finding what's eating your drive:

- Scan your home folder, any mounted drive, or any folder you pick. It counts space on disk the same way `du` does and stays on one drive
- A bubble view of the 300 largest files, sized by space and colored by type. Hover for the name and size, click to select
- See the 1,000 largest files, the 1,000 largest folders, and totals by extension, colored by file type
- Right-click to open the location, copy the path, or move it to the Trash (it asks first)
- Folders that need admin rights to read are skipped and counted, never prompted for

### Everything else

- File → Run new task (`Ctrl+N`) starts any command, like Windows Task Manager's Run dialog. It keeps running after you close purrtop
- File → Settings… (`Ctrl+,`) for the theme and NVIDIA details
- A status bar with the current readings and a quick theme picker
- Eight built-in themes: Neon, Rainbow, Cotton Candy, Sunset, Matcha, Midnight, Phosphor and Graphite. They're also under View → Theme
- A File / View / Help menu bar
- Your theme is remembered between launches

## Requirements

- Linux (purrtop reads `/proc` directly, so it won't run anywhere else)
- A working Vulkan driver, since GPUI draws with Vulkan
- Rust 1.92 or newer (same as the GPUI Kit apps it shares a stack with)
- `gio` (from GLib, already on most desktops) for Move to Trash on the Disk Space page

## Install

### Arch Linux

On Arch Linux and Arch-based distros, install it from the AUR:

```bash
yay -S purrtop
```

`paru -S purrtop` works too. The package installs the admin helper and polkit policy, so admin actions only ask for your password once every few minutes, and purrtop shows up in your app menu.

### Other distros

Install it with Cargo (check the [requirements](#requirements) first):

```bash
cargo install purrtop
```

That puts `purrtop` in `~/.cargo/bin`. It's the same app, with two differences:

- There's no app menu launcher
- Admin actions ask for your password every time, since the helper only kicks in when it's installed to `/usr/lib/purrtop` with its polkit policy

If you want to package purrtop for another distro, the launcher, icons and policy are in the crate's `data/` folder.

## Build and run

```bash
git clone https://github.com/pinkpixel-dev/purrtop.git
cd purrtop
cargo run --release
```

If the window doesn't open, run `vulkaninfo --summary` to check your Vulkan setup. On Arch, the driver packages are `vulkan-radeon`, `vulkan-intel` or `nvidia-utils`.

## Keyboard

### Everywhere

| Keys | Action |
|---|---|
| `Ctrl+Q` | Quit |
| `Ctrl+N` | Run new task |
| `Ctrl+,` | Settings |
| `Alt+Down` / `Alt+Up` | Next / previous page |

### Lists and tables

On the Processes, Startup apps, Users, Services, Ports and Disk Space pages, `Up`, `Down`, `Home` and `End` move the selection. Processes, Services, Ports and Disk Space also support `Page Up` and `Page Down`, and `Ctrl+F` jumps to the filter on those four pages.

### Page shortcuts

| Page | Keys | Action |
|---|---|---|
| Performance | `Up` / `Down` | Move through the device list |
| System Info | `Ctrl+Tab` / `Ctrl+Shift+Tab` | Switch tabs |
| Processes | `Right` / `Left` | Open or close a process's children (Left on a child jumps to its parent) |
| Processes | `Enter` | Details for the selected process |
| Processes | `Delete` | Terminate (SIGTERM) |
| Processes | `Shift+Delete` | Force terminate (SIGKILL), after asking |
| Startup apps | `Space` | Turn the selected entry on or off |
| Startup apps | `Delete` | Remove the selected entry |
| Startup apps | `F5` | Read the autostart folders again |
| Users | `Right` / `Left` | Open or close a user's processes (Left on a process jumps back to its user) |
| Ports | `Delete` | End the selected port's process, after asking |
| Ports | `Shift+Delete` | Kill the selected port's process, after asking |
| Disk Space | `F5` | Scan again |
| Disk Space | `Esc` | Stop a running scan |
| Disk Space | `Enter` | Open the selected item's location |
| Disk Space | `Delete` | Move the selected item to the Trash, after asking |
| Disk Space | `Ctrl+Tab` | Switch tabs |

## Admin actions

Ending, renicing or changing the CPU affinity of your own processes just works. For other users' processes (or a nice value below 0), purrtop runs the action as admin through `pkexec`, so your desktop's polkit agent asks for your password first.

When purrtop is installed as a package, admin actions go through its own small helper (`/usr/lib/purrtop/purrtop-helper`) and polkit policy, and polkit remembers your password for a few minutes. Ending a couple of root processes in a row only asks once. The helper only does purrtop's five admin jobs (signals, nice, CPU affinity, stack samples and the port owner lookup) and refuses PID 0 and 1. If you're running a `cargo run` build without the helper installed, everything still works, it just asks every time.

"Create process sample" uses `eu-stack` from elfutils, and it always goes through `pkexec`, since reading another process's stacks needs ptrace rights. The sample is saved to `~/.cache/purrtop/samples/` and opened in your text editor.

## Settings

Open File → Settings… (`Ctrl+,`) to change the theme or turn on NVIDIA details. Everything saves right away to `~/.config/purrtop/settings.json` (or `$XDG_CONFIG_HOME/purrtop/settings.json`).

NVIDIA details is off by default. When it's on, purrtop reads usage, temperature, power, clocks and video memory from NVML (`libnvidia-ml.so`, which comes with the NVIDIA driver), but only while the GPU is already awake. When nothing else is using the GPU, purrtop waits 30 seconds between readings, which is longer than it takes the GPU to fall asleep. On my laptop's RTX 4090, it went back to sleep just as fast as it does without purrtop. If something else keeps it awake (like a local AI model), you get live numbers every second.

## License

Apache 2.0. See [LICENSE](LICENSE).

Made with 💖 by [Pink Pixel](https://pinkpixel.dev)
