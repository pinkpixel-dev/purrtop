//! Everything `/proc` will tell us about one process, for the details dialog.
//! Read once when the dialog opens. Parts the kernel won't show for another
//! user's process come back as `Err` with the reason.

use std::path::Path;

/// Open files listed before the dialog stops counting.
const MAX_OPEN_FILES: usize = 2000;

pub struct ProcessDetails {
    pub cwd: Result<String, String>,
    /// `Key: value` pairs from `/proc/<pid>/status`.
    pub status: Vec<(String, String)>,
    /// `(limit, soft, hard, units)` rows from `/proc/<pid>/limits`.
    pub limits: Result<Vec<[String; 4]>, String>,
    pub environment: Result<Vec<String>, String>,
    /// `(fd, target)` pairs.
    pub open_files: Result<Vec<(String, String)>, String>,
}

impl ProcessDetails {
    pub fn read(pid: u32) -> Self {
        let dir = Path::new("/proc").join(pid.to_string());
        Self {
            cwd: std::fs::read_link(dir.join("cwd")).map(|path| path.to_string_lossy().into_owned()).map_err(reason),
            status: std::fs::read_to_string(dir.join("status")).map(|text| parse_status(&text)).unwrap_or_default(),
            limits: std::fs::read_to_string(dir.join("limits")).map(|text| parse_limits(&text)).map_err(reason),
            environment: std::fs::read(dir.join("environ"))
                .map(|bytes| {
                    let mut vars: Vec<String> = bytes
                        .split(|&b| b == 0)
                        .filter(|var| !var.is_empty())
                        .map(|var| String::from_utf8_lossy(var).into_owned())
                        .collect();
                    vars.sort();
                    vars
                })
                .map_err(reason),
            open_files: read_open_files(&dir).map_err(reason),
        }
    }
}

fn reason(err: std::io::Error) -> String {
    match err.kind() {
        std::io::ErrorKind::PermissionDenied => "Access denied (it belongs to another user)".into(),
        std::io::ErrorKind::NotFound => "The process has exited".into(),
        _ => err.to_string(),
    }
}

fn parse_status(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_string(), value.split_whitespace().collect::<Vec<_>>().join(" ")))
        .collect()
}

/// The limits file is a fixed-width table: the name can contain spaces, so
/// columns are cut at the header's positions.
fn parse_limits(text: &str) -> Vec<[String; 4]> {
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let (Some(soft), Some(hard), Some(units)) = (header.find("Soft Limit"), header.find("Hard Limit"), header.find("Units")) else {
        return Vec::new();
    };
    let cut = |line: &str, from: usize, to: usize| line.get(from..to.min(line.len())).unwrap_or_default().trim().to_string();
    lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| [cut(line, 0, soft), cut(line, soft, hard), cut(line, hard, units), cut(line, units, line.len())])
        .collect()
}

fn read_open_files(dir: &Path) -> std::io::Result<Vec<(String, String)>> {
    let mut files: Vec<(u32, String)> = std::fs::read_dir(dir.join("fd"))?
        .filter_map(Result::ok)
        .take(MAX_OPEN_FILES)
        .filter_map(|entry| {
            let fd = entry.file_name().to_str()?.parse().ok()?;
            let target = std::fs::read_link(entry.path()).map(|path| path.to_string_lossy().into_owned()).unwrap_or_default();
            Some((fd, target))
        })
        .collect();
    files.sort_by_key(|(fd, _)| *fd);
    Ok(files.into_iter().map(|(fd, target)| (fd.to_string(), target)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_limits_table() {
        let text = "Limit                     Soft Limit           Hard Limit           Units     \n\
                    Max cpu time              unlimited            unlimited            seconds   \n\
                    Max open files            1024                 524288               files     \n";
        let limits = parse_limits(text);
        assert_eq!(limits.len(), 2);
        assert_eq!(limits[1], ["Max open files", "1024", "524288", "files"].map(String::from));
    }

    #[test]
    fn reads_our_own_process() {
        let details = ProcessDetails::read(std::process::id());
        assert!(details.cwd.is_ok());
        assert!(details.status.iter().any(|(key, _)| key == "Name"));
        assert!(details.open_files.is_ok_and(|files| !files.is_empty()));
        assert!(ProcessDetails::read(1).environment.is_err(), "init's environment belongs to root");
    }
}
