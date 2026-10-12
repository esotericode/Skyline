use anyhow::{Result, bail};
use skyline_core::{
    VERSION,
    catalog::import_path,
    hash::Cancellation,
    matching::CatalogIndex,
    scan::{ScanOptions, scan_folder},
    store::Store,
};
use std::path::Path;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cancel = Cancellation::default();
    match args.first().map(String::as_str) {
        Some("--version") => println!("Skyline {VERSION}"),
        Some("import") if args.len() == 4 => {
            let imported = import_path(Path::new(&args[3]), &args[2], &cancel)?;
            let count = imported.catalog.entries.len();
            Store::open(Path::new(&args[1]))?.import_catalog(&imported)?;
            println!("Imported {count} entries into {}", args[1]);
        }
        Some("scan") if args.len() >= 3 => {
            let mut store = Store::open(Path::new(&args[1]))?;
            let index = CatalogIndex::new(store.catalogs()?);
            let options = ScanOptions {
                exclusions: args[3..].iter().map(Into::into).collect(),
            };
            let report = scan_folder(Path::new(&args[2]), &index, &options, &cancel, |_| {})?;
            if report.cancelled {
                bail!("Cancelled scan was not saved");
            }
            store.save_completed_scan(&report.records)?;
            println!("{}", serde_json::to_string_pretty(&report.records)?);
        }
        _ => {
            eprintln!(
                "Skyline {VERSION}\nUsage:\n  skyline-cli --version\n  skyline-cli import <database> <system> <DAT>\n  skyline-cli scan <database> <folder> [excluded-relative-prefix ...]\nAll scans force rehash and are read-only. The database stores the last completed scan."
            );
            if !args.is_empty() {
                bail!("Invalid command or arguments");
            }
        }
    }
    Ok(())
}
