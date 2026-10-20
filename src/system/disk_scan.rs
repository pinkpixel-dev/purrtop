//! Walks a folder and adds up what's using the space. Only the largest files
//! and folders are kept, so a scan of a whole drive stays small in memory.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use rayon::prelude::*;

use super::file_types::{self, Kind};

/// How many of the largest files and folders a scan keeps.
pub const KEEP: usize = 1000;
/// Enough threads to keep an SSD busy without taking over the machine.
const MAX_THREADS: usize = 8;
/// Deep trees like `node_modules` recurse a long way.
const STACK_SIZE: usize = 8 * 1024 * 1024;

/// Live counts while a scan runs, and the switch that stops it.
#[derive(Default)]
pub struct Progress {
    files: AtomicU64,
    folders: AtomicU64,
    bytes: AtomicU64,
    skipped: AtomicU64,
    cancel: AtomicBool,
}

/// What [`Progress`] has counted so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub files: u64,
    pub folders: u64,
    pub bytes: u64,
    /// Folders that couldn't be read, usually for lack of permission.
    pub skipped: u64,
}

impl Progress {
    pub fn counts(&self) -> Counts {
        Counts {
            files: self.files.load(Ordering::Relaxed),
            folders: self.folders.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            skipped: self.skipped.load(Ordering::Relaxed),
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub path: PathBuf,
    /// Space on disk, which is what frees up when it's deleted.
    pub size: u64,
    /// The length a program reading it sees. Differs for sparse files.
    pub apparent: u64,
    pub modified: Option<SystemTime>,
    pub extension: String,
    pub kind: Kind,
}

#[derive(Clone, Debug)]
pub struct FolderEntry {
    pub path: PathBuf,
    /// Everything inside, at any depth.
    pub size: u64,
    /// Just the files sitting directly in it.
    pub direct: u64,
    /// Files inside, at any depth.
    pub files: u64,
}

/// Every file with one extension added up.
#[derive(Clone, Debug)]
pub struct TypeTotal {
    /// Lowercase, without the dot. Empty for files without one.
    pub extension: String,
    pub kind: Kind,
    pub size: u64,
    pub files: u64,
}

#[derive(Clone, Debug)]
pub struct Scan {
    /// Largest first, at most [`KEEP`].
    pub files: Vec<FileEntry>,
    /// Largest first, at most [`KEEP`], not counting the root.
    pub folders: Vec<FolderEntry>,
    /// Largest first.
    pub types: Vec<TypeTotal>,
    pub counts: Counts,
    pub elapsed: Duration,
    /// Stopped before the end, so the numbers are partial.
    pub stopped: bool,
}

/// Scans `root` without leaving its filesystem. Symlinks aren't followed,
/// hard links count once, and sizes are space on disk, like `du`.
pub fn scan(root: &Path, progress: &Progress) -> Result<Scan, String> {
    let started = Instant::now();
    let meta = fs::metadata(root).map_err(|err| format!("Couldn't open {}: {err}", root.display()))?;
    if !meta.is_dir() {
        return Err(format!("{} isn't a folder", root.display()));
    }
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(MAX_THREADS);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .stack_size(STACK_SIZE)
        .thread_name(|ix| format!("purrtop-scan-{ix}"))
        .build()
        .map_err(|err| format!("Couldn't start the scan: {err}"))?;

    let walker = Walker { device: meta.dev(), progress, files: Top::new(KEEP), folders: Top::new(KEEP), types: Mutex::default(), links: Mutex::default() };
    pool.install(|| walker.walk(root, meta.blocks() * 512, true));

    let mut types: Vec<TypeTotal> = walker
        .types
        .into_inner()
        .unwrap_or_default()
        .into_iter()
        .map(|(extension, (size, files))| TypeTotal { kind: Kind::of(&extension), extension, size, files })
        .collect();
    types.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.extension.cmp(&b.extension)));
    Ok(Scan {
        files: walker.files.into_sorted(),
        folders: walker.folders.into_sorted(),
        types,
        counts: progress.counts(),
        elapsed: started.elapsed(),
        stopped: progress.cancelled(),
    })
}

struct Walker<'a> {
    device: u64,
    progress: &'a Progress,
    files: Top<FileEntry>,
    folders: Top<FolderEntry>,
    /// Size and file count by extension.
    types: Mutex<HashMap<String, (u64, u64)>>,
    /// `(device, inode)` of every file with more than one link seen so far.
    links: Mutex<HashSet<(u64, u64)>>,
}

#[derive(Clone, Copy, Default)]
struct Totals {
    size: u64,
    files: u64,
}

impl Walker<'_> {
    /// Adds up `dir`, whose own entry takes `own` bytes, and everything under it.
    fn walk(&self, dir: &Path, own: u64, root: bool) -> Totals {
        if self.progress.cancelled() {
            return Totals::default();
        }
        let Ok(entries) = fs::read_dir(dir) else {
            self.progress.skipped.fetch_add(1, Ordering::Relaxed);
            return Totals { size: own, files: 0 };
        };

        let mut subfolders = Vec::new();
        let mut direct = Totals::default();
        let mut types: HashMap<String, (u64, u64)> = HashMap::new();
        for entry in entries.flatten() {
            // Doesn't follow symlinks, so a link counts as the link itself.
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let size = meta.blocks() * 512;
            if meta.is_dir() {
                // Another filesystem mounted inside, like /proc under /.
                if meta.dev() == self.device {
                    subfolders.push((entry.path(), size));
                }
                continue;
            }
            if meta.nlink() > 1 && !self.links.lock().is_ok_and(|mut links| links.insert((meta.dev(), meta.ino()))) {
                continue;
            }
            direct.size += size;
            if !meta.is_file() {
                continue;
            }
            direct.files += 1;
            let extension = file_types::extension(&entry.file_name().to_string_lossy());
            let total = types.entry(extension.clone()).or_default();
            total.0 += size;
            total.1 += 1;
            self.files.offer(size, || FileEntry {
                path: entry.path(),
                size,
                apparent: meta.len(),
                modified: meta.modified().ok(),
                kind: Kind::of(&extension),
                extension,
            });
        }
        self.progress.files.fetch_add(direct.files, Ordering::Relaxed);
        self.progress.bytes.fetch_add(direct.size + own, Ordering::Relaxed);
        self.progress.folders.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut all) = self.types.lock() {
            for (extension, (size, files)) in types {
                let total = all.entry(extension).or_default();
                total.0 += size;
                total.1 += files;
            }
        }

        let below = subfolders.par_iter().map(|(path, size)| self.walk(path, *size, false)).reduce(Totals::default, |a, b| Totals { size: a.size + b.size, files: a.files + b.files });
        let totals = Totals { size: own + direct.size + below.size, files: direct.files + below.files };
        if !root {
            self.folders.offer(totals.size, || FolderEntry { path: dir.to_path_buf(), size: totals.size, direct: direct.size, files: totals.files });
        }
        totals
    }
}

/// The `limit` largest items offered from any thread.
struct Top<T> {
    limit: usize,
    /// The smallest size still kept once full, so most offers skip the lock.
    floor: AtomicU64,
    heap: Mutex<BinaryHeap<Reverse<Ranked<T>>>>,
}

struct Ranked<T> {
    size: u64,
    item: T,
}

impl<T> PartialEq for Ranked<T> {
    fn eq(&self, other: &Self) -> bool {
        self.size == other.size
    }
}

impl<T> Eq for Ranked<T> {}

impl<T> PartialOrd for Ranked<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Ranked<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.size.cmp(&other.size)
    }
}

impl<T> Top<T> {
    fn new(limit: usize) -> Self {
        Self { limit, floor: AtomicU64::new(0), heap: Mutex::new(BinaryHeap::with_capacity(limit + 1)) }
    }

    /// Keeps the item if it's among the largest. `make` only runs when it is.
    fn offer(&self, size: u64, make: impl FnOnce() -> T) {
        if size == 0 || size <= self.floor.load(Ordering::Relaxed) {
            return;
        }
        let Ok(mut heap) = self.heap.lock() else {
            return;
        };
        if heap.len() >= self.limit {
            if heap.peek().is_some_and(|Reverse(smallest)| size <= smallest.size) {
                return;
            }
            heap.pop();
        }
        heap.push(Reverse(Ranked { size, item: make() }));
        if heap.len() >= self.limit
            && let Some(Reverse(smallest)) = heap.peek()
        {
            self.floor.store(smallest.size, Ordering::Relaxed);
        }
    }

    /// Largest first.
    fn into_sorted(self) -> Vec<T> {
        let heap = self.heap.into_inner().unwrap_or_default();
        heap.into_sorted_vec().into_iter().map(|Reverse(ranked)| ranked.item).collect()
    }
}

/// Moves a file or folder to the desktop's Trash with `gio trash`, which
/// follows the freedesktop Trash spec, including drives other than home's.
pub fn move_to_trash(path: &Path) -> Result<(), String> {
    let output = std::process::Command::new("gio").arg("trash").arg("--").arg(path).output().map_err(|err| format!("Couldn't run gio: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if message.is_empty() { "gio trash failed".into() } else { message })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway folder that's removed when the test ends.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("purrtop-{name}-{}", std::process::id()));
            _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn file(&self, relative: &str, bytes: usize) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, vec![7u8; bytes]).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn keeps_the_largest_in_order() {
        let top = Top::new(3);
        for size in [5, 1, 9, 0, 7, 3, 9] {
            top.offer(size, || size);
        }
        assert_eq!(top.into_sorted(), vec![9, 9, 7]);
    }

    #[test]
    fn adds_up_files_folders_and_types() {
        let dir = TempDir::new("scan");
        dir.file("videos/big.mkv", 256 * 1024);
        dir.file("videos/small.MKV", 16 * 1024);
        dir.file("docs/notes.txt", 64 * 1024);
        dir.file("docs/deep/er/readme", 8 * 1024);

        let scan = scan(&dir.0, &Progress::default()).unwrap();
        assert!(!scan.stopped);
        assert_eq!(scan.counts.files, 4);
        assert_eq!(scan.counts.skipped, 0);

        let names: Vec<_> = scan.files.iter().map(|file| file.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["big.mkv", "notes.txt", "small.MKV", "readme"]);
        assert!(scan.files[0].size >= 256 * 1024);
        assert_eq!(scan.files[0].apparent, 256 * 1024);
        assert_eq!(scan.files[0].kind, Kind::Video);

        // The root isn't listed, and the biggest folder comes first.
        assert!(scan.folders.iter().all(|folder| folder.path != dir.0));
        assert_eq!(scan.folders[0].path, dir.0.join("videos"));
        assert_eq!(scan.folders[0].files, 2);
        let docs = scan.folders.iter().find(|folder| folder.path == dir.0.join("docs")).unwrap();
        assert_eq!(docs.files, 2);
        assert!(docs.size > docs.direct);

        let mkv = scan.types.iter().find(|total| total.extension == "mkv").unwrap();
        assert_eq!(mkv.files, 2);
        assert!(scan.types.iter().any(|total| total.extension.is_empty() && total.kind == Kind::Other));
    }

    #[test]
    fn counts_hard_links_once() {
        let dir = TempDir::new("links");
        let original = dir.file("a/data.bin", 128 * 1024);
        fs::create_dir_all(dir.0.join("b")).unwrap();
        fs::hard_link(&original, dir.0.join("b/data.bin")).unwrap();

        let scan = scan(&dir.0, &Progress::default()).unwrap();
        assert_eq!(scan.counts.files, 1);
        assert_eq!(scan.files.len(), 1);
    }

    #[test]
    fn doesnt_follow_symlinks() {
        let dir = TempDir::new("symlinks");
        let target = dir.file("real/movie.mp4", 128 * 1024);
        std::os::unix::fs::symlink(&target, dir.0.join("link.mp4")).unwrap();
        std::os::unix::fs::symlink(dir.0.join("real"), dir.0.join("loop")).unwrap();

        let scan = scan(&dir.0, &Progress::default()).unwrap();
        assert_eq!(scan.counts.files, 1);
        assert_eq!(scan.files[0].path, target);
    }

    #[test]
    fn stops_when_cancelled() {
        let dir = TempDir::new("cancel");
        dir.file("a/b.txt", 4096);
        let progress = Progress::default();
        progress.cancel();
        let scan = scan(&dir.0, &progress).unwrap();
        assert!(scan.stopped);
        assert!(scan.files.is_empty());
    }

    #[test]
    fn refuses_files_and_missing_paths() {
        let dir = TempDir::new("refuse");
        let file = dir.file("plain.txt", 10);
        assert!(scan(&file, &Progress::default()).is_err());
        assert!(scan(&dir.0.join("missing"), &Progress::default()).is_err());
    }
}
