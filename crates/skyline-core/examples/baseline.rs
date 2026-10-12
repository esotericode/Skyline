use skyline_core::{
    catalog::parse_dat,
    hash::{Cancellation, hash_stream},
    matching::CatalogIndex,
    model::*,
    store::Store,
    view::{Sort, query_rows},
};
use std::{hint::black_box, path::Path, time::Instant};

fn main() -> anyhow::Result<()> {
    println!(
        "Skyline {} · synthetic core baseline · release recommended",
        skyline_core::VERSION
    );
    for count in [10_000, 100_000] {
        let mut xml =
            "<datafile><header><name>Synthetic baseline</name><version>1</version></header>"
                .to_owned();
        for id in 0..count {
            xml.push_str(&format!("<game name=\"Fixture {id:06}\"><rom name=\"Fixture {id:06}.gb\" size=\"3\" sha256=\"{id:064x}\"/></game>"));
        }
        xml.push_str("</datafile>");
        let started = Instant::now();
        let catalog = parse_dat(
            xml.as_bytes(),
            Path::new("synthetic-baseline.dat"),
            "Game Boy",
            &Cancellation::default(),
        )?;
        let import_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let index = CatalogIndex::new(vec![catalog]);
        let index_ms = started.elapsed().as_secs_f64() * 1000.0;
        let mut samples = vec![];
        for id in 0..1000 {
            let digests = Digests {
                sha256: Some(format!("{id:064x}")),
                ..Default::default()
            };
            let started = Instant::now();
            let matched = black_box(index.identify(3, &digests));
            assert_eq!(matched.status, Verification::Exact);
            samples.push(started.elapsed().as_secs_f64() * 1_000_000.0);
        }
        samples.sort_by(f64::total_cmp);
        let rows: Vec<_> = (0..count)
            .rev()
            .map(|id| {
                let digests = Digests {
                    sha256: Some(format!("{id:064x}")),
                    ..Default::default()
                };
                let result = if id % 5 == 0 {
                    MatchResult {
                        status: Verification::Unknown,
                        candidates: vec![],
                        rejected: vec![],
                        explanation: "synthetic unknown".into(),
                    }
                } else {
                    index.identify(3, &digests)
                };
                FileRecord {
                    path: format!("synthetic/Fixture {id:06}.gb").into(),
                    size: Some(3),
                    digests: Some(digests),
                    header: None,
                    format: None,
                    result,
                }
            })
            .collect();
        let mut queries = vec![];
        for id in 0..100 {
            let started = Instant::now();
            let query = if id % 2 == 0 { "Fixture 00" } else { "" };
            black_box(query_rows(&rows, query, None, Sort::Name, id % 3 == 0));
            queries.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        queries.sort_by(f64::total_cmp);
        let temporary = tempfile::tempdir()?;
        let mut store = Store::open(&temporary.path().join("baseline.sqlite3"))?;
        let started = Instant::now();
        store.save_completed_scan(&rows)?;
        let save_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        assert_eq!(store.collection()?.len(), count);
        let load_ms = started.elapsed().as_secs_f64() * 1000.0;
        println!(
            "entries={count} DAT_bytes={} import_ms={import_ms:.2} index_ms={index_ms:.2} match_p95_us={:.2} query_sort_p95_ms={:.2} save_ms={save_ms:.2} load_ms={load_ms:.2}",
            xml.len(),
            samples[949],
            queries[94]
        );
    }
    let bytes = vec![42_u8; 64 * 1024 * 1024];
    let started = Instant::now();
    black_box(hash_stream(&bytes[..], &Cancellation::default(), |_| {})?);
    println!(
        "four_hashes_in_memory_MiB_per_second={:.2}",
        64.0 / started.elapsed().as_secs_f64()
    );
    Ok(())
}
