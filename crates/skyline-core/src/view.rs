use crate::model::{FileRecord, Verification};

#[derive(Clone, Copy, PartialEq)]
pub enum Sort {
    Name,
    Status,
    Size,
}

/// Shared with the measurement harness so query timings measure actual UI logic.
pub fn query_rows(
    records: &[FileRecord],
    query: &str,
    status: Option<&Verification>,
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
            query.is_empty()
                || row.path.to_string_lossy().to_lowercase().contains(&query)
                || row.result.status.label().to_lowercase().contains(&query)
                || row.result.candidates.iter().any(|candidate| {
                    candidate.entry.game.to_lowercase().contains(&query)
                        || candidate.entry.filename.to_lowercase().contains(&query)
                })
        })
        .map(|(id, _)| id)
        .collect();
    visible.sort_by(|&a, &b| {
        let a = &records[a];
        let b = &records[b];
        let order = match sort {
            Sort::Name => a.path.file_name().cmp(&b.path.file_name()),
            Sort::Status => a.result.status.label().cmp(b.result.status.label()),
            Sort::Size => a.size.cmp(&b.size),
        }
        .then_with(|| a.path.cmp(&b.path));
        if descending { order.reverse() } else { order }
    });
    visible
}
