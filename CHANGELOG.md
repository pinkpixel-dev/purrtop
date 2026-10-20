# Changelog

## 0.1.0 - Unreleased

### 🐱 App shell
- Main window with a title bar, File / View / Help menus, a page sidebar and a status bar
- Help → About dialog with the version and a link to pinkpixel.dev
- purrtop logo in the title bar and About dialog

### 📊 Summary
- Dashboard that fills the window: LED columns for CPU, clock, temperature and GPU, a CPU history chart (utilization, kernel time, temperature), the busiest processes, a memory chart, and cards for system pressure, network, disk and GPUs
- Everything is sampled from `/proc` and `/sys` once a second, with two minutes of history
- The top processes list moves under the CPU chart in narrow windows
- GPUs are read only from sysfs, so a sleeping laptop GPU stays asleep

### 🎨 Themes
- Eight built-in themes: Neon, Rainbow, Cotton Candy, Sunset, Matcha, Midnight, Phosphor and Graphite
- Switch themes from View → Theme or the status bar picker; your pick is saved for next launch
