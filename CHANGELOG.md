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

### 📈 Performance
- New Performance page with a device list (CPU and Memory for now) that shows each one's reading and a sparkline
- CPU view: a meter, a chart for every logical processor with kernel time, and P-core and E-core labels on hybrid Intel chips
- CPU stats and specs: speed, temperature, processes, threads, up time, load, base and max speed, sockets, cores, virtualization, frequency driver and governor
- Memory view: use and swap over time, a bar split into in use, cached and free, and commit charge
- Every disk gets its own entry: active time, read and write charts, response time, model and type, and each mounted volume with its free space
- Every physical network interface gets its own entry: receive and send chart, totals since boot, state, link speed, MAC, IPv4 and IPv6
- Battery entry: charge and power chart, state, time left, health, cycles, and full and design capacity
- Thermals entry: every temperature sensor with a readable name, one chart for all of them, and each sensor's limit and peak, flagged when it gets within 10 °C of its limit
- An entry for each GPU with the readings its driver shares: usage (amdgpu), clock (i915), temperature, power and video memory, plus driver, PCI slot and power state. A sleeping GPU is never read
- The device list scrolls to keep the keyboard selection in view
- `Up` and `Down` move through the device list, and the list drops its sparklines in narrow windows

### 🐾 Processes
- New Processes page with every process, grouped into Applications and Background with child processes under their parents, or as one flat list
- Columns for PID, status, user, CPU, memory, disk read and write, threads, virtual memory, open files, nice and the full command line; click a header to sort, drag edges to resize
- Filter by name, user or PID
- New processes flash green and exited ones stay on screen in red for a couple of seconds
- Detail strip under the table with CPU time, peak, shared and swap memory, cgroup and the executable path
- Right-click menu: details dialog, terminate, force terminate, end or force quit a whole tree, suspend, resume, send any common signal, nice presets, CPU affinity, a stack sample with `eu-stack`, open file location, search online, copy name, follow process, and memory as bytes or percent
- Actions on other users' processes ask for your password through polkit
- Keyboard: arrows to move, Left and Right to fold the tree, Enter for details, Delete to terminate, Shift+Delete to force terminate, Ctrl+F to filter

### 📊 Summary
- Pressure meters each take the color of their own level

### 🎨 Themes
- Eight built-in themes: Neon, Rainbow, Cotton Candy, Sunset, Matcha, Midnight, Phosphor and Graphite
- Switch themes from View → Theme or the status bar picker; your pick is saved for next launch
