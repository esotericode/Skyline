use anyhow::{Result, bail};
use skyline_core::{
    VERSION,
    catalog::import_path,
    hash::Cancellation,
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
            let system = if args[2] == "auto" { "" } else { &args[2] };
            let imported = import_path(Path::new(&args[3]), system, &cancel)?;
            let count = imported.catalog.entries.len();
            Store::open(Path::new(&args[1]))?.import_catalog(&imported)?;
            println!("Imported {count} entries into {}", args[1]);
        }
        Some("import-folder") if args.len() == 3 => {
            let mut store = Store::open(Path::new(&args[1]))?;
            let report = store.sync_catalog_folder(Path::new(&args[2]), &cancel, |_, _, _| {})?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Some("scan") if args.len() >= 3 => {
            let mut store = Store::open(Path::new(&args[1]))?;
            for warning in store.warnings() {
                eprintln!("warning: {warning}");
            }
            // `--dats <folder>` scans against that DAT folder only, as the desktop does.
            let mut rest = &args[3..];
            let mut dats = None;
            if rest.first().map(String::as_str) == Some("--dats") {
                let Some(folder) = rest.get(1) else {
                    bail!("--dats needs a DAT folder");
                };
                dats = Some(Path::new(folder));
                rest = &rest[2..];
            }
            let index = store.catalog_index(dats)?;
            let options = ScanOptions {
                exclusions: rest.iter().map(Into::into).collect(),
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
                "Skyline {VERSION}\nUsage:\n  skyline-cli --version\n  skyline-cli import <database> <system|auto> <DAT>\n  skyline-cli import-folder <database> <DAT-folder>\n  skyline-cli scan <database> <folder> [--dats <DAT-folder>] [excluded-relative-prefix ...]\nUse auto to take the system label from the DAT header. Scans use all imported catalogs,\nor only those of one indexed DAT folder with --dats.\nAll scans force rehash and are read-only. The database stores the last completed scan."
            );
            if !args.is_empty() {
                bail!("Invalid command or arguments");
            }
        }
    }
    Ok(())
}
