//! Turns the process list into table rows: grouped tree or flat list, sorted
//! and filtered. Pure, so it's easy to test.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::system::processes::{Kind, ProcessInfo};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Pid,
    Status,
    User,
    Cpu,
    Memory,
    DiskRead,
    DiskWrite,
    Threads,
    Virtual,
    Files,
    Nice,
    Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sort {
    pub key: SortKey,
    pub descending: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Self { key: SortKey::Name, descending: false }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewMode {
    /// Applications and Background groups, children under their parents.
    #[default]
    Tree,
    /// Every process in one sorted list.
    Flat,
}

/// Highlights for processes that just started or just exited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mark {
    #[default]
    None,
    New,
    Exited,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Group { kind: Kind, count: usize, collapsed: bool },
    Process { ix: usize, depth: usize, has_children: bool, expanded: bool, mark: Mark },
}

impl Row {
    pub fn process(&self) -> Option<usize> {
        match self {
            Row::Process { ix, .. } => Some(*ix),
            Row::Group { .. } => None,
        }
    }
}

pub struct RowOptions<'a> {
    pub filter: &'a str,
    pub sort: Sort,
    pub mode: ViewMode,
    pub expanded: &'a HashSet<u32>,
    pub collapsed_groups: &'a HashSet<Kind>,
    pub marks: &'a HashMap<u32, Mark>,
}

pub fn build_rows(processes: &[ProcessInfo], options: &RowOptions) -> Vec<Row> {
    let filter = options.filter.trim().to_lowercase();
    let mark = |pid: u32| options.marks.get(&pid).copied().unwrap_or_default();
    let leaf = |ix: usize| Row::Process { ix, depth: 0, has_children: false, expanded: false, mark: mark(processes[ix].pid) };

    let mut order: Vec<usize> = (0..processes.len()).filter(|&ix| filter.is_empty() || matches(&processes[ix], &filter)).collect();
    order.sort_by(|&a, &b| compare(&processes[a], &processes[b], options.sort));

    if options.mode == ViewMode::Flat {
        return order.into_iter().map(leaf).collect();
    }

    let mut rows = Vec::new();
    for kind in [Kind::Application, Kind::Background] {
        let members: Vec<usize> = order.iter().copied().filter(|&ix| processes[ix].kind == kind).collect();
        if members.is_empty() {
            continue;
        }
        let collapsed = options.collapsed_groups.contains(&kind);
        rows.push(Row::Group { kind, count: members.len(), collapsed });
        if collapsed {
            continue;
        }
        // A filtered tree would hide matches under collapsed parents, so
        // filtering lists the matches on their own.
        if !filter.is_empty() {
            rows.extend(members.into_iter().map(leaf));
            continue;
        }
        let in_group: HashSet<u32> = members.iter().map(|&ix| processes[ix].pid).collect();
        let mut children: HashMap<u32, Vec<usize>> = HashMap::new();
        let mut roots = Vec::new();
        // `members` is already sorted, so children lists come out sorted too.
        for &ix in &members {
            let process = &processes[ix];
            if process.ppid != process.pid && in_group.contains(&process.ppid) {
                children.entry(process.ppid).or_default().push(ix);
            } else {
                roots.push(ix);
            }
        }
        for root in roots {
            push_tree(processes, root, 0, &children, options, &mark, &mut rows);
        }
    }
    rows
}

fn push_tree(
    processes: &[ProcessInfo],
    ix: usize,
    depth: usize,
    children: &HashMap<u32, Vec<usize>>,
    options: &RowOptions,
    mark: &dyn Fn(u32) -> Mark,
    rows: &mut Vec<Row>,
) {
    let pid = processes[ix].pid;
    let kids = children.get(&pid);
    let expanded = options.expanded.contains(&pid);
    rows.push(Row::Process { ix, depth, has_children: kids.is_some(), expanded, mark: mark(pid) });
    // The depth cap guards against a parent loop in a malformed /proc read.
    if expanded && depth < 64 {
        for &child in kids.into_iter().flatten() {
            push_tree(processes, child, depth + 1, children, options, mark, rows);
        }
    }
}

/// Name or user contains the text, or the PID starts with it.
fn matches(process: &ProcessInfo, filter: &str) -> bool {
    process.name.to_lowercase().contains(filter)
        || process.user.to_lowercase().contains(filter)
        || process.pid.to_string().starts_with(filter)
}

fn compare(a: &ProcessInfo, b: &ProcessInfo, sort: Sort) -> Ordering {
    // Unreadable values (None) sort below every real one.
    let ordering = match sort.key {
        SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        SortKey::Pid => a.pid.cmp(&b.pid),
        SortKey::Status => a.state_label().cmp(b.state_label()),
        SortKey::User => a.user.cmp(&b.user),
        SortKey::Cpu => a.cpu.total_cmp(&b.cpu),
        SortKey::Memory => a.memory.cmp(&b.memory),
        SortKey::DiskRead => a.disk_read.unwrap_or(-1.).total_cmp(&b.disk_read.unwrap_or(-1.)),
        SortKey::DiskWrite => a.disk_write.unwrap_or(-1.).total_cmp(&b.disk_write.unwrap_or(-1.)),
        SortKey::Threads => a.threads.cmp(&b.threads),
        SortKey::Virtual => a.virtual_memory.cmp(&b.virtual_memory),
        SortKey::Files => a.open_files.cmp(&b.open_files),
        SortKey::Nice => a.nice.cmp(&b.nice),
        SortKey::Command => a.command.cmp(&b.command),
    };
    let ordering = if sort.descending { ordering.reverse() } else { ordering };
    // PID breaks ties, so equal rows don't swap places every second.
    ordering.then(a.pid.cmp(&b.pid))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub fn process(pid: u32, ppid: u32, name: &str, kind: Kind) -> ProcessInfo {
        ProcessInfo {
            pid,
            ppid,
            name: name.into(),
            command: name.into(),
            exe: None,
            user: "me".into(),
            uid: 1000,
            kind,
            cgroup: String::new(),
            state: 'S',
            cpu: 0.,
            user_time: 0.,
            system_time: 0.,
            memory: pid as u64,
            virtual_memory: 0,
            threads: 1,
            nice: 0,
            peak_memory: None,
            shared_memory: None,
            swap: None,
            disk_read: None,
            disk_write: None,
            open_files: None,
        }
    }

    fn sample() -> Vec<ProcessInfo> {
        vec![
            process(1, 0, "systemd", Kind::Background),
            process(100, 1, "chrome", Kind::Application),
            process(101, 100, "chrome-renderer", Kind::Application),
            process(102, 100, "chrome-gpu", Kind::Application),
            process(200, 1, "kitty", Kind::Application),
            process(300, 1, "pipewire", Kind::Background),
        ]
    }

    fn pids(processes: &[ProcessInfo], rows: &[Row]) -> Vec<u32> {
        rows.iter().filter_map(Row::process).map(|ix| processes[ix].pid).collect()
    }

    fn options<'a>(expanded: &'a HashSet<u32>, collapsed: &'a HashSet<Kind>, marks: &'a HashMap<u32, Mark>) -> RowOptions<'a> {
        RowOptions { filter: "", sort: Sort::default(), mode: ViewMode::Tree, expanded, collapsed_groups: collapsed, marks }
    }

    #[test]
    fn groups_apps_first_with_children_hidden_until_expanded() {
        let processes = sample();
        let (mut expanded, collapsed, marks) = (HashSet::new(), HashSet::new(), HashMap::new());
        let rows = build_rows(&processes, &options(&expanded, &collapsed, &marks));
        assert_eq!(rows[0], Row::Group { kind: Kind::Application, count: 4, collapsed: false });
        // pipewire's parent, systemd, is in the same group, so it starts hidden.
        assert_eq!(pids(&processes, &rows), [100, 200, 1]);
        assert!(matches!(rows[1], Row::Process { has_children: true, expanded: false, .. }));

        expanded.insert(100);
        let rows = build_rows(&processes, &options(&expanded, &collapsed, &marks));
        // Children sorted by name, one level deeper.
        assert_eq!(pids(&processes, &rows), [100, 102, 101, 200, 1]);
        assert!(matches!(rows[2], Row::Process { depth: 1, .. }));
    }

    #[test]
    fn collapsed_groups_hide_their_processes() {
        let processes = sample();
        let (expanded, mut collapsed, marks) = (HashSet::new(), HashSet::new(), HashMap::new());
        collapsed.insert(Kind::Application);
        let rows = build_rows(&processes, &options(&expanded, &collapsed, &marks));
        assert_eq!(rows[0], Row::Group { kind: Kind::Application, count: 4, collapsed: true });
        assert_eq!(pids(&processes, &rows), [1]);
    }

    #[test]
    fn filtering_lists_matches_including_hidden_children() {
        let processes = sample();
        let (expanded, collapsed, marks) = (HashSet::new(), HashSet::new(), HashMap::new());
        let mut opts = options(&expanded, &collapsed, &marks);
        opts.filter = "Render";
        assert_eq!(pids(&processes, &build_rows(&processes, &opts)), [101]);
        opts.filter = "30";
        assert_eq!(pids(&processes, &build_rows(&processes, &opts)), [300]);
    }

    #[test]
    fn flat_mode_sorts_everything_together() {
        let processes = sample();
        let (expanded, collapsed, mut marks) = (HashSet::new(), HashSet::new(), HashMap::new());
        marks.insert(200, Mark::New);
        let mut opts = options(&expanded, &collapsed, &marks);
        opts.mode = ViewMode::Flat;
        opts.sort = Sort { key: SortKey::Memory, descending: true };
        let rows = build_rows(&processes, &opts);
        assert_eq!(pids(&processes, &rows), [300, 200, 102, 101, 100, 1]);
        assert!(matches!(rows[1], Row::Process { mark: Mark::New, .. }));
    }
}
