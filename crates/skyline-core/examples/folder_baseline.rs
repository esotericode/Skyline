//! Optional local inputs; never bundles third-party catalogs or ROMs.
use anyhow::{Context, Result};
use skyline_core::{hash::Cancellation, matching::CatalogIndex, store::Store};
use std::{path::PathBuf, time::Instant};

fn main() -> Result<()> {
    let folder = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Usage: folder_baseline <DAT-folder>")?,
    );
    let temp = tempfile::tempdir()?;
    let mut store = Store::open(&temp.path().join("baseline.db"))?;
    for pass in ["first", "repeat"] {
        let start = Instant::now();
        let report = store.sync_catalog_folder(&folder, &Cancellation::default(), |_, _, _| {})?;
        for issue in &report.issues {
            eprintln!("{}: {}", issue.path.display(), issue.explanation);
        }
        let intake_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let catalogs = store.active_catalogs(Some(&folder))?;
        let load_ms = start.elapsed().as_secs_f64() * 1000.0;
        let samples: Vec<_> = catalogs
            .iter()
            .filter_map(|catalog| {
                let entry = catalog
                    .entries
                    .iter()
                    .find(|entry| entry.size.is_some() && entry.digests.strong())?;
                Some((
                    catalog.provenance.snapshot_sha256.clone(),
                    entry.size.unwrap(),
                    entry.digests.clone(),
                ))
            })
            .collect();
        let start = Instant::now();
        let index = CatalogIndex::new(catalogs);
        let index_ms = start.elapsed().as_secs_f64() * 1000.0;
        // Metadata lookup check only: no commercial ROM bytes are obtained or verified.
        for (sha, size, digests) in &samples {
            assert!(
                index
                    .identify(*size, digests)
                    .candidates
                    .iter()
                    .any(|candidate| &candidate.catalog_sha256 == sha)
            );
        }
        println!(
            "{pass}: files={} catalogs={} entries={} parsed={} cached={} issues={} intake_ms={intake_ms:.2} load_ms={load_ms:.2} index_ms={index_ms:.2} metadata_lookup_catalogs={}",
            report.discovered,
            report.catalogs,
            report.entries,
            report.parsed,
            report.cached,
            report.issues.len(),
            samples.len()
        );
    }
    Ok(())
}
