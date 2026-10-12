#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use anyhow::{Context, Result};
use eframe::egui::{self, Color32, RichText};
use skyline_core::{
    VERSION,
    catalog::import_path,
    hash::Cancellation,
    matching::CatalogIndex,
    model::{Catalog, FileRecord, Verification},
    scan::{ScanOptions, ScanProgress, scan_folder},
    store::Store,
    view::{Sort, query_rows},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

enum Event {
    Progress(ScanProgress),
    Loaded(Vec<Catalog>, Vec<FileRecord>),
    Imported(Vec<Catalog>),
    Scanned(Vec<FileRecord>, String),
    Failed(String),
}

enum Job {
    Load,
    Import {
        path: PathBuf,
        system: String,
    },
    Scan {
        root: PathBuf,
        exclusions: Vec<PathBuf>,
        catalogs: Arc<Vec<Catalog>>,
        selected: Option<usize>,
    },
}

fn run_job(
    job: Job,
    database: PathBuf,
    cancel: Cancellation,
    sender: SyncSender<Event>,
    ctx: egui::Context,
) {
    let result = (|| -> Result<Event> {
        cancel.check()?;
        let mut store = Store::open(&database)?;
        match job {
            Job::Load => Ok(Event::Loaded(store.catalogs()?, store.collection()?)),
            Job::Import { path, system } => {
                let imported = import_path(&path, &system, &cancel)?;
                cancel.check()?;
                store.import_catalog(&imported)?;
                Ok(Event::Imported(store.catalogs()?))
            }
            Job::Scan {
                root,
                exclusions,
                catalogs,
                selected,
            } => {
                let catalogs = if let Some(id) = selected {
                    vec![catalogs[id].clone()]
                } else {
                    catalogs.as_ref().clone()
                };
                let index = CatalogIndex::new(catalogs);
                let mut last_progress = Instant::now() - Duration::from_secs(1);
                let report = scan_folder(
                    &root,
                    &index,
                    &ScanOptions { exclusions },
                    &cancel,
                    |progress| {
                        if last_progress.elapsed() >= Duration::from_millis(100) {
                            // Progress can be dropped under load; the final result is reliably delivered.
                            let _ = sender.try_send(Event::Progress(progress));
                            ctx.request_repaint();
                            last_progress = Instant::now();
                        }
                    },
                )?;
                let status = if report.cancelled {
                    format!(
                        "Cancelled · {} partial rows · previous saved scan retained",
                        report.records.len()
                    )
                } else {
                    cancel.check()?;
                    store.save_completed_scan(&report.records)?;
                    format!(
                        "Scan saved · {} rows · {:.2} MiB hashed",
                        report.records.len(),
                        report.hashed_bytes as f64 / 1_048_576.0
                    )
                };
                Ok(Event::Scanned(report.records, status))
            }
        }
    })();
    let event = result.unwrap_or_else(|error| Event::Failed(format!("{error:#}")));
    let _ = sender.send(event);
    ctx.request_repaint();
}

struct Skyline {
    database: PathBuf,
    catalogs: Arc<Vec<Catalog>>,
    records: Vec<FileRecord>,
    visible: Vec<usize>,
    duplicates: HashMap<String, usize>,
    search: String,
    status_filter: Option<Verification>,
    sort: Sort,
    descending: bool,
    selected_row: Option<usize>,
    selected_catalog: Option<usize>,
    dat_path: String,
    root_path: String,
    exclusions: String,
    system: String,
    status: String,
    error: Option<String>,
    progress: Option<ScanProgress>,
    receiver: Option<Receiver<Event>>,
    cancel: Cancellation,
    show_about: bool,
}

impl Skyline {
    fn new(cc: &eframe::CreationContext<'_>, database: PathBuf) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(20, 24, 31);
        visuals.window_fill = Color32::from_rgb(25, 30, 39);
        visuals.selection.bg_fill = Color32::from_rgb(32, 91, 103);
        cc.egui_ctx.set_visuals(visuals);
        let mut app = Self {
            database,
            catalogs: Arc::new(vec![]),
            records: vec![],
            visible: vec![],
            duplicates: HashMap::new(),
            search: String::new(),
            status_filter: None,
            sort: Sort::Name,
            descending: false,
            selected_row: None,
            selected_catalog: None,
            dat_path: String::new(),
            root_path: String::new(),
            exclusions: String::new(),
            system: "Game Boy".into(),
            status: "Loading local collection…".into(),
            error: None,
            progress: None,
            receiver: None,
            cancel: Cancellation::default(),
            show_about: false,
        };
        app.start(Job::Load, &cc.egui_ctx);
        app
    }

    fn start(&mut self, job: Job, ctx: &egui::Context) {
        if self.receiver.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::sync_channel(16);
        self.cancel = Cancellation::default();
        self.receiver = Some(receiver);
        self.error = None;
        self.progress = None;
        self.status = "Working…".into();
        let database = self.database.clone();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        let fallback_sender = sender.clone();
        let fallback_ctx = ctx.clone();
        let result = std::thread::Builder::new().name("skyline-job".into()).spawn(move || {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_job(job, database, cancel, sender, ctx))).is_err() {
                let _ = fallback_sender.send(Event::Failed("Background worker failed unexpectedly. Reopen or rescan to refresh saved state.".into()));
                fallback_ctx.request_repaint();
            }
        });
        if let Err(error) = result {
            self.error = Some(format!("Could not start background job: {error}"));
            self.status = "Job could not start".into();
            self.finish();
        }
    }

    fn poll(&mut self) {
        let events: Vec<_> = self
            .receiver
            .as_ref()
            .map(|receiver| receiver.try_iter().take(16).collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Progress(progress) => self.progress = Some(progress),
                Event::Loaded(catalogs, records) => {
                    self.catalogs = Arc::new(catalogs);
                    self.set_records(records);
                    self.status =
                        "Ready · saved results are historical; rescan to refresh byte evidence"
                            .into();
                    self.finish();
                }
                Event::Imported(catalogs) => {
                    self.catalogs = Arc::new(catalogs);
                    self.selected_catalog = None;
                    self.status = "Catalog imported · rescan to use updated references".into();
                    self.finish();
                }
                Event::Scanned(records, status) => {
                    self.set_records(records);
                    self.status = status;
                    self.finish();
                }
                Event::Failed(error) => {
                    self.error = Some(error);
                    self.status = "Job stopped · previous saved data retained".into();
                    self.finish();
                }
            }
        }
    }

    fn finish(&mut self) {
        self.receiver = None;
        self.progress = None;
    }

    fn set_records(&mut self, records: Vec<FileRecord>) {
        self.records = records;
        self.selected_row = None;
        self.duplicates.clear();
        for row in &self.records {
            if let Some(hash) = row.digests.as_ref().and_then(|d| d.sha256.as_ref()) {
                *self.duplicates.entry(hash.clone()).or_default() += 1;
            }
        }
        self.filter_rows();
    }

    fn filter_rows(&mut self) {
        self.visible = query_rows(
            &self.records,
            &self.search,
            self.status_filter.as_ref(),
            self.sort,
            self.descending,
        );
    }

    fn controls(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Skyline").size(24.0).strong());
                ui.label(RichText::new("ROM library").color(Color32::from_gray(160)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(format!("v{VERSION} · About")).clicked() {
                        self.show_about = true;
                    }
                    ui.label("Read-only preview");
                });
            });
            ui.add_space(8.0);
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if self.receiver.is_some() && ui.button("Cancel job").clicked() {
                    self.cancel.cancel();
                    self.status = "Cancelling at the next safe boundary…".into();
                }
                ui.label(&self.status);
                ui.label(format!(
                    "{} / {} rows",
                    self.visible.len(),
                    self.records.len()
                ));
            });
            if let Some(progress) = &self.progress {
                ui.label(format!(
                    "{} complete · {:.1} MiB · {}",
                    progress.completed_files,
                    progress.hashed_bytes as f64 / 1_048_576.0,
                    progress.path.display()
                ));
            }
            if let Some(error) = &self.error {
                ui.colored_label(Color32::from_rgb(255, 170, 140), error);
            }
        });
        egui::SidePanel::left("sources").default_width(245.0).width_range(200.0..=420.0).resizable(true).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(8.0);
                ui.heading("Sources");
                ui.label("References for the next scan");
                ui.radio_value(&mut self.selected_catalog, None, "All imported catalogs");
                for (id, catalog) in self.catalogs.iter().enumerate() {
                    ui.radio_value(&mut self.selected_catalog, Some(id), format!("{}\n{} · {}", catalog.provenance.system, catalog.name, catalog.version));
                }
                if self.catalogs.is_empty() { ui.label("Import a DAT to identify your files. Scans without references show Unknown."); }
                ui.separator();
                ui.heading("Import DAT");
                egui::ComboBox::from_id_salt("system").selected_text(&self.system).show_ui(ui, |ui| {
                    for system in ["Game Boy", "Game Boy Color"] { ui.selectable_value(&mut self.system, system.into(), system); }
                });
                ui.label("Local XML DAT file path");
                ui.add(egui::TextEdit::singleline(&mut self.dat_path).hint_text("C:\\Catalogs\\reference.dat").desired_width(f32::INFINITY));
                if ui.add_enabled(self.receiver.is_none() && !self.dat_path.trim().is_empty(), egui::Button::new("Import catalog")).clicked() {
                    self.start(Job::Import { path: self.dat_path.trim().into(), system: self.system.clone() }, ctx);
                }
                ui.separator();
                ui.heading("Scan folder");
                ui.label("ROM folder path");
                ui.add(egui::TextEdit::singleline(&mut self.root_path).hint_text("C:\\ROMs").desired_width(f32::INFINITY));
                ui.label("Exclude relative prefixes (one per line)");
                ui.add(egui::TextEdit::multiline(&mut self.exclusions).desired_rows(2).desired_width(f32::INFINITY).hint_text("Backups\nUnsorted"));
                if ui.add_enabled(self.receiver.is_none() && !self.root_path.trim().is_empty(), egui::Button::new("Scan and verify")).clicked() {
                    self.start(Job::Scan {
                        root: self.root_path.trim().into(),
                        exclusions: self.exclusions.lines().map(str::trim).filter(|s| !s.is_empty()).map(Into::into).collect(),
                        catalogs: self.catalogs.clone(), selected: self.selected_catalog,
                    }, ctx);
                }
                ui.small("Every scan rereads file bytes. Original files are preserved.");
                ui.add_space(12.0);
                ui.small("ZIP inspection and organization are upcoming milestones.");
            });
        });
    }

    fn details(&self, ctx: &egui::Context) {
        egui::SidePanel::right("details")
            .default_width(330.0)
            .width_range(240.0..=650.0)
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(8.0);
                    ui.heading("File details");
                    let Some(record) = self.selected_row.and_then(|id| self.records.get(id)) else {
                        ui.label("Select a row to inspect byte evidence and catalog provenance.");
                        return;
                    };
                    ui.label(RichText::new(record.display_name()).strong());
                    ui.label(record.path.to_string_lossy());
                    ui.label(record.result.status.label());
                    ui.label(&record.result.explanation);
                    if let Some(size) = record.size {
                        ui.label(format!("{size} bytes"));
                    }
                    if let Some(digests) = &record.digests {
                        ui.separator();
                        ui.strong("Calculated hashes");
                        for (algorithm, digest) in digests.values() {
                            if let Some(digest) = digest {
                                ui.label(algorithm);
                                ui.monospace(digest);
                            }
                        }
                        if let Some(hash) = &digests.sha256
                            && self.duplicates.get(hash).copied().unwrap_or(0) > 1
                        {
                            ui.label(format!(
                                "Identical content in {} scanned paths",
                                self.duplicates[hash]
                            ));
                        }
                    }
                    for (heading, candidates) in [
                        ("Matching references", &record.result.candidates),
                        ("Rejected partial evidence", &record.result.rejected),
                    ] {
                        if candidates.is_empty() {
                            continue;
                        }
                        ui.separator();
                        ui.strong(heading);
                        for candidate in candidates {
                            ui.label(RichText::new(&candidate.entry.game).strong());
                            ui.label(format!("Catalog filename: {}", candidate.entry.filename));
                            ui.label(format!(
                                "{} · {} · {}",
                                candidate.system, candidate.catalog_name, candidate.catalog_version
                            ));
                            ui.label(format!(
                                "Dump flag: {:?} ({})",
                                candidate.entry.quality,
                                if candidate.entry.raw_status.is_empty() {
                                    "unspecified"
                                } else {
                                    &candidate.entry.raw_status
                                }
                            ));
                            ui.label(format!(
                                "Expected size: {:?}; set members: {}",
                                candidate.entry.size, candidate.entry.group_members
                            ));
                            ui.label(format!(
                                "Checked: {}",
                                candidate.checked_algorithms.join(", ")
                            ));
                            if !candidate.entry.unsupported_algorithms.is_empty() {
                                ui.label(format!(
                                    "Unsupported reference attributes: {}",
                                    candidate.entry.unsupported_algorithms.join(", ")
                                ));
                            }
                            for (algorithm, digest) in candidate.entry.digests.values() {
                                if let Some(digest) = digest {
                                    ui.label(format!("Expected {algorithm}"));
                                    ui.monospace(digest);
                                }
                            }
                            for failure in &candidate.failures {
                                ui.colored_label(Color32::from_rgb(255, 170, 140), failure);
                            }
                            ui.label("Snapshot SHA-256");
                            ui.monospace(&candidate.catalog_sha256);
                            if let Some(catalog) = self.catalogs.iter().find(|catalog| {
                                catalog.provenance.snapshot_sha256 == candidate.catalog_sha256
                                    && catalog.provenance.system == candidate.system
                            }) {
                                ui.label(&catalog.provenance.provider);
                                ui.label(format!(
                                    "Origin: {}",
                                    catalog.provenance.origin.display()
                                ));
                                ui.label(format!(
                                    "Import time: {} (Unix UTC seconds)",
                                    catalog.provenance.imported_unix_seconds
                                ));
                                ui.label(format!(
                                    "License: {}",
                                    catalog
                                        .provenance
                                        .license_reference
                                        .as_deref()
                                        .unwrap_or("not established by this local import")
                                ));
                            }
                            ui.add_space(12.0);
                        }
                    }
                });
            });
    }

    fn collection(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("Collection");
                let mut changed = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("Search files or catalog titles")
                            .desired_width(250.0),
                    )
                    .changed();
                egui::ComboBox::from_id_salt("verification")
                    .selected_text(
                        self.status_filter
                            .as_ref()
                            .map_or("All results", Verification::label),
                    )
                    .show_ui(ui, |ui| {
                        changed |= ui
                            .selectable_value(&mut self.status_filter, None, "All results")
                            .changed();
                        for status in [
                            Verification::Exact,
                            Verification::CrcOnly,
                            Verification::Ambiguous,
                            Verification::Unknown,
                            Verification::ReadError,
                            Verification::Unsupported,
                        ] {
                            let label = status.label();
                            changed |= ui
                                .selectable_value(&mut self.status_filter, Some(status), label)
                                .changed();
                        }
                    });
                if changed {
                    self.filter_rows();
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Sort:");
                for (sort, label) in [
                    (Sort::Name, "Filename"),
                    (Sort::Status, "Verification"),
                    (Sort::Size, "Size"),
                ] {
                    if ui.selectable_label(self.sort == sort, label).clicked() {
                        self.sort = sort;
                        self.filter_rows();
                    }
                }
                if ui.checkbox(&mut self.descending, "Descending").changed() {
                    self.filter_rows();
                }
            });
            ui.separator();
            if self.records.is_empty() {
                ui.add_space(60.0);
                ui.heading("Your collection, understood.");
                ui.label("Import a local DAT, enter a ROM folder, then scan and verify.");
                ui.label(
                    "Skyline compares file bytes with your selected references, entirely offline.",
                );
                return;
            }
            if self.visible.is_empty() {
                ui.label("No rows match this search and filter.");
            }
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show_rows(ui, 46.0, self.visible.len(), |ui, range| {
                    for position in range {
                        let id = self.visible[position];
                        let record = &self.records[id];
                        let title = record
                            .result
                            .candidates
                            .first()
                            .map(|candidate| candidate.entry.filename.as_str())
                            .unwrap_or("No unique catalog filename");
                        let size = record.size.map_or("—".into(), |size| format!("{size} B"));
                        let ambiguous = record.result.status == Verification::Ambiguous;
                        let label = format!(
                            "{}    ·    {}    ·    {}\n{}",
                            record.display_name(),
                            record.result.status.label(),
                            size,
                            if ambiguous {
                                "Multiple identities — inspect candidates"
                            } else {
                                title
                            }
                        );
                        let response = ui.add_sized(
                            [ui.available_width().max(360.0), 44.0],
                            egui::Button::selectable(self.selected_row == Some(id), label),
                        );
                        if response.clicked() {
                            self.selected_row = Some(id);
                        }
                    }
                });
        });
    }
}

impl eframe::App for Skyline {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) && self.receiver.is_some() {
            self.cancel.cancel();
        }
        if ctx.input(|input| input.viewport().close_requested()) {
            self.cancel.cancel();
        }
        self.controls(ctx);
        self.details(ctx);
        self.collection(ctx);
        egui::Window::new("About Skyline").open(&mut self.show_about).resizable(false).show(ctx, |ui| {
            ui.heading(format!("Skyline {VERSION}"));
            ui.label("Development build · offline ROM verification");
            ui.label("Loose-file read-only prototype. ZIP and file organization are not implemented yet.");
            ui.label(format!("Local database: {}", self.database.display()));
            ui.label("Application license: pending owner selection.");
        });
        // eframe repaints on input and worker notifications. No continuous idle repaint loop.
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.cancel.cancel();
    }
}

fn database_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SKYLINE_DATA_DIR") {
        return Ok(PathBuf::from(path).join("skyline.sqlite3"));
    }
    #[cfg(windows)]
    let base = PathBuf::from(
        std::env::var_os("LOCALAPPDATA")
            .context("LOCALAPPDATA is unavailable; set SKYLINE_DATA_DIR")?,
    );
    #[cfg(not(windows))]
    let base = if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(path)
    } else {
        PathBuf::from(
            std::env::var_os("HOME").context("HOME is unavailable; set SKYLINE_DATA_DIR")?,
        )
        .join(".local/share")
    };
    Ok(base.join("Skyline").join("skyline.sqlite3"))
}

fn main() -> Result<()> {
    if std::env::args().any(|arg| arg == "--version") {
        println!("Skyline {VERSION}");
        return Ok(());
    }
    let database = database_path()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("Skyline {VERSION}"))
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([1000.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Skyline",
        options,
        Box::new(move |cc| Ok(Box::new(Skyline::new(cc, database)))),
    )
    .map_err(|error| anyhow::anyhow!("Desktop startup failed: {error}"))
}
