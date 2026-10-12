//! Folder discovery and intake reports. Catalog activation lives in the store;
//! display families never replace the catalog's exact verification identity.
use crate::hash::{Cancellation, is_link};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MAX_FOLDER_FILES: usize = 4096;
pub const MAX_FOLDER_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_FOLDER_ENTRIES: usize = 10_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IntakeIssue {
    pub path: PathBuf,
    pub explanation: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IntakeReport {
    pub folder: PathBuf,
    pub discovered: usize,
    pub parsed: usize,
    pub cached: usize,
    pub retained: usize,
    pub catalogs: usize,
    pub entries: usize,
    pub issues: Vec<IntakeIssue>,
    pub omitted_issues: usize,
}

impl IntakeReport {
    pub(crate) fn issue(&mut self, path: PathBuf, explanation: String) {
        if self.issues.len() < 256 {
            // Reports are JSON; an unrepresentable filename must not turn one
            // skipped input into a batch serialization failure.
            let path = PathBuf::from(path.to_string_lossy().into_owned());
            self.issues.push(IntakeIssue { path, explanation });
        } else {
            self.omitted_issues += 1;
        }
    }
}

/// Absolute keys remain available when a previously selected folder is offline.
pub(crate) fn folder_key(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    let mut ancestor = absolute.as_path();
    let mut tail = vec![];
    let resolved = loop {
        if let Ok(mut resolved) = std::fs::canonicalize(ancestor) {
            for part in tail.iter().rev() {
                resolved.push(part);
            }
            break resolved;
        }
        let Some(name) = ancestor.file_name() else {
            break absolute.clone();
        };
        tail.push(name.to_os_string());
        let Some(parent) = ancestor.parent() else {
            break absolute.clone();
        };
        ancestor = parent;
    };
    // Windows canonical paths acquire a verbatim prefix; absolute offline paths
    // do not. Existing parents also resolve 8.3 aliases used by temporary paths.
    // Use the same key both online and offline, including UNC shares.
    #[cfg(windows)]
    let resolved = PathBuf::from(crate::view::display_path(&resolved).into_owned());
    Ok(resolved)
}

pub(crate) struct Discovery {
    pub files: Vec<PathBuf>,
    pub report: IntakeReport,
    pub complete: bool,
}

pub(crate) fn discover(path: &Path, cancel: &Cancellation) -> Result<Discovery> {
    cancel.check()?;
    let metadata = std::fs::symlink_metadata(path).context("Opening DAT folder")?;
    if is_link(&metadata) || !metadata.is_dir() {
        bail!("Choose a regular DAT folder; directory links are skipped");
    }
    let folder = folder_key(path)?;
    if folder.to_str().is_none() {
        bail!("The DAT folder path must be representable as UTF-8");
    }
    let mut found = Discovery {
        files: vec![],
        report: IntakeReport {
            folder: folder.clone(),
            ..Default::default()
        },
        complete: true,
    };
    let mut pending = vec![(folder, 0)];
    let mut visited = 0;
    while let Some((directory, depth)) = pending.pop() {
        cancel.check()?;
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                found.complete = false;
                found.report.issue(
                    directory,
                    format!("Folder unreadable; previous references retained: {error}"),
                );
                continue;
            }
        };
        for entry in entries {
            cancel.check()?;
            visited += 1;
            if visited > 100_000 {
                bail!("DAT folder exceeds 100,000 filesystem entries");
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    found.complete = false;
                    found.report.issue(
                        directory.clone(),
                        format!("Directory entry unreadable: {error}"),
                    );
                    continue;
                }
            };
            let path = entry.path();
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    found.complete = false;
                    found
                        .report
                        .issue(path, format!("Cannot inspect entry: {error}"));
                    continue;
                }
            };
            if is_link(&metadata) {
                continue;
            }
            if metadata.is_dir() {
                if depth >= 64 {
                    bail!("DAT folder nesting exceeds 64 levels");
                }
                pending.push((path, depth + 1));
            } else if metadata.is_file()
                && path.extension().is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("dat") || ext.eq_ignore_ascii_case("xml")
                })
            {
                if found.files.len() >= MAX_FOLDER_FILES {
                    bail!("DAT folder exceeds 4,096 catalogs");
                }
                found.files.push(path);
            }
        }
    }
    found.files.sort();
    found.report.discovered = found.files.len();
    Ok(found)
}
