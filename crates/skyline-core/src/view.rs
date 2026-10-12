use crate::model::{FileRecord, Verification};
use std::{borrow::Cow, cmp::Ordering, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    Name,
    Status,
    Size,
}

/// Strongest evidence first, so sorting by verification groups results meaningfully.
fn status_rank(status: &Verification) -> u8 {
    match status {
        Verification::Exact => 0,
        Verification::CrcOnly => 1,
        Verification::Ambiguous => 2,
        Verification::Unknown => 3,
        Verification::ReadError => 4,
        Verification::Unsupported => 5,
    }
}

/// Case-insensitive without allocating per comparison.
fn compare_names(a: &Path, b: &Path) -> Ordering {
    let a = a.file_name().unwrap_or_default().to_string_lossy();
    let b = b.file_name().unwrap_or_default().to_string_lossy();
    a.chars()
        .flat_map(char::to_lowercase)
        .cmp(b.chars().flat_map(char::to_lowercase))
}

/// Shared with the measurement harness so query timings measure actual UI logic.
pub fn query_rows(
    records: &[FileRecord],
    query: &str,
    status: Option<&Verification>,
    sort: Sort,
    descending: bool,
) -> Vec<usize> {
    query_rows_in_system(records, query, status, None, sort, descending)
}

/// Filtering an ambiguous row never resolves its identity or discards candidates.
/// Only catalog-supported candidate systems participate; extensions are not evidence.
pub fn query_rows_in_system(
    records: &[FileRecord],
    query: &str,
    status: Option<&Verification>,
    system: Option<&str>,
    sort: Sort,
    descending: bool,
) -> Vec<usize> {
    let query = query.to_lowercase();
    let mut visible: Vec<_> = records
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            if status.is_some_and(|status| row.result.status != *status) {
                return false;
            }
            if system
                .is_some_and(|system| !row.result.candidates.iter().any(|c| c.system == system))
            {
                return false;
            }
            query.is_empty()
                || row.path.to_string_lossy().to_lowercase().contains(&query)
                || row.result.status.label().to_lowercase().contains(&query)
                || row.result.candidates.iter().any(|candidate| {
                    candidate.entry.game.to_lowercase().contains(&query)
                        || candidate.entry.filename.to_lowercase().contains(&query)
                        || candidate.system.to_lowercase().contains(&query)
                })
        })
        .map(|(id, _)| id)
        .collect();
    visible.sort_by(|&a, &b| {
        let a = &records[a];
        let b = &records[b];
        let order = match sort {
            Sort::Name => compare_names(&a.path, &b.path),
            Sort::Status => status_rank(&a.result.status).cmp(&status_rank(&b.result.status)),
            Sort::Size => a.size.cmp(&b.size),
        }
        .then_with(|| a.path.cmp(&b.path));
        if descending { order.reverse() } else { order }
    });
    visible
}

/// Paths for people to read. Windows canonical paths carry a `\\?\` verbatim prefix that
/// is noise in the interface; the stored path keeps it.
pub fn display_path(path: &Path) -> Cow<'_, str> {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        Cow::Owned(format!(r"\\{rest}"))
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        Cow::Owned(rest.to_owned())
    } else {
        text
    }
}

/// Binary units with one decimal place, e.g. "64.0 KiB".
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// "YYYY-MM-DD HH:MM UTC" from Unix seconds (proleptic Gregorian, no time zone data).
pub fn format_utc(unix_seconds: u64) -> String {
    let days = (unix_seconds / 86_400) as i64;
    let seconds = unix_seconds % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        seconds / 3600,
        seconds % 3600 / 60
    )
}
