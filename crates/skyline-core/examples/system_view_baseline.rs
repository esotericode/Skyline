//! Synthetic mixed collection, measuring the desktop's actual membership/query path.
use skyline_core::{
    catalog::parse_dat,
    hash::Cancellation,
    matching::CatalogIndex,
    model::{Digests, FileRecord, MatchResult, Verification},
    systems::collection_systems,
    view::{Sort, query_rows_in_collection_system},
};
use std::{hint::black_box, path::Path, time::Instant};

fn main() -> anyhow::Result<()> {
    let names = [
        "Nintendo - Game Boy Advance",
        "Nintendo - Nintendo 64 (BigEndian)",
        "Nintendo - Nintendo 64 (ByteSwapped)",
        "Toshiba - Visicom",
    ];
    let mut catalogs = vec![];
    for (id, name) in names.into_iter().enumerate() {
        let xml = format!(
            "<datafile><header><name>{name}</name></header><game name=\"Synthetic {id}\"><rom name=\"Game.custom{id}\" size=\"3\" sha256=\"{id:064x}\"/></game></datafile>"
        );
        catalogs.push(parse_dat(
            xml.as_bytes(),
            Path::new("synthetic.dat"),
            "",
            &Cancellation::default(),
        )?);
    }
    let index = CatalogIndex::new(catalogs.clone());
    for count in [10_000, 100_000] {
        let rows: Vec<_> = (0..count)
            .rev()
            .map(|id| {
                let digests = Digests {
                    sha256: Some(format!("{:064x}", id % 4)),
                    ..Default::default()
                };
                FileRecord {
                    path: format!("Fixture {id:06}.custom{}", id % 4).into(),
                    size: Some(3),
                    digests: Some(digests.clone()),
                    header: None,
                    format: None,
                    header_check: None,
                    result: if id % 5 == 0 {
                        MatchResult {
                            status: Verification::Unknown,
                            candidates: vec![],
                            rejected: vec![],
                            explanation: "synthetic unknown".into(),
                        }
                    } else {
                        index.identify(3, &digests)
                    },
                }
            })
            .collect();
        let start = Instant::now();
        let systems = collection_systems(&rows, &index.summary());
        let grouping_ms = start.elapsed().as_secs_f64() * 1000.0;
        let selected = systems
            .iter()
            .find(|system| system.name == "Nintendo 64")
            .unwrap();
        let mut samples = vec![];
        for id in 0..100 {
            let start = Instant::now();
            black_box(query_rows_in_collection_system(
                &rows,
                if id % 2 == 0 { "Fixture 00" } else { "" },
                None,
                Some(selected),
                Sort::Name,
                id % 3 == 0,
            ));
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "rows={count} families={} grouping_ms={grouping_ms:.2} active_N64_query_sort_p95_ms={:.2}",
            systems.len(),
            samples[94]
        );
    }
    Ok(())
}
