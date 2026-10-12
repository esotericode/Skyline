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
    view::{Sort, display_path, format_size, format_utc, query_rows},
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
    scroll_offset: f32,
    viewport_height: f32,
    scroll_request: Option<f32>,
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
            scroll_offset: 0.0,
            viewport_height: 0.0,
            scroll_request: None,
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
        self.scroll_request = Some(0.0);
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
                if self.receiver.is_some() {
                    ui.spinner();
                }
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
                    display_path(&progress.path)
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
            .default_width(300.0)
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
                    ui.label(RichText::new(display_path(&record.path)).weak());
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(record.result.status.label())
                            .color(status_color(&record.result.status))
                            .strong(),
                    );
                    ui.label(&record.result.explanation);
                    if let Some(size) = record.size {
                        ui.label(format!("{} ({size} bytes)", format_size(size)));
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
                                "Expected size: {}; set members: {}",
                                candidate
                                    .entry
                                    .size
                                    .map_or("not supplied".into(), |size| format!("{size} bytes")),
                                candidate.entry.group_members
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
                                    display_path(&catalog.provenance.origin)
                                ));
                                ui.label(format!(
                                    "Imported: {}",
                                    format_utc(catalog.provenance.imported_unix_seconds)
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

    /// Arrow/Page/Home/End move the row selection when no text field or popup has the keys.
    fn keyboard_navigation(&mut self, ctx: &egui::Context) {
        if self.visible.is_empty() || ctx.wants_keyboard_input() || ctx.is_popup_open() {
            return;
        }
        let page = ((self.viewport_height / ROW_HEIGHT) as isize - 1).max(1);
        let step = ctx.input(|input| {
            use egui::Key;
            let last = self.visible.len() as isize;
            [
                (Key::ArrowDown, 1),
                (Key::ArrowUp, -1),
                (Key::PageDown, page),
                (Key::PageUp, -page),
                (Key::End, last),
                (Key::Home, -last),
            ]
            .into_iter()
            .filter(|(key, _)| input.key_pressed(*key))
            .map(|(_, step)| step)
            .sum::<isize>()
        });
        if step == 0 {
            return;
        }
        let current = self
            .selected_row
            .and_then(|id| self.visible.iter().position(|&row| row == id));
        let position = match current {
            Some(position) => (position as isize + step).clamp(0, self.visible.len() as isize - 1),
            None if step > 0 => 0,
            None => self.visible.len() as isize - 1,
        } as usize;
        self.selected_row = Some(self.visible[position]);
        let top = position as f32 * ROW_HEIGHT;
        if top < self.scroll_offset {
            self.scroll_request = Some(top);
        } else if top + ROW_HEIGHT > self.scroll_offset + self.viewport_height {
            self.scroll_request = Some(top + ROW_HEIGHT - self.viewport_height);
        }
    }

    fn collection(&mut self, ctx: &egui::Context) {
        self.keyboard_navigation(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Collection");
                ui.add_space(8.0);
                let mut changed = false;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                            for status in STATUSES {
                                let label = status.label();
                                changed |= ui
                                    .selectable_value(&mut self.status_filter, Some(status), label)
                                    .changed();
                            }
                        });
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut self.search)
                                .hint_text("Search files or catalog titles")
                                .desired_width(ui.available_width()),
                        )
                        .changed();
                });
                if changed {
                    self.filter_rows();
                }
            });
            ui.add_space(6.0);
            if self.records.is_empty() {
                ui.add_space(60.0);
                ui.vertical_centered(|ui| {
                    ui.heading("Your collection, understood.");
                    ui.add_space(4.0);
                    ui.label("Import a local DAT, enter a ROM folder, then scan and verify.");
                    ui.label(
                        RichText::new(
                            "Skyline compares file bytes with your selected references, entirely offline.",
                        )
                        .weak(),
                    );
                });
                return;
            }
            let columns = Columns::new(ui.available_width());
            self.table_header(ui, &columns);
            if self.visible.is_empty() {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| ui.label(RichText::new("No rows match this search and filter.").weak()));
                return;
            }
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
            if let Some(offset) = self.scroll_request.take() {
                area = area.vertical_scroll_offset(offset);
            }
            let output = area.show_rows(ui, ROW_HEIGHT, self.visible.len(), |ui, range| {
                for position in range {
                    let id = self.visible[position];
                    if self.table_row(ui, &columns, id, position % 2 == 1) {
                        self.selected_row = Some(id);
                    }
                }
            });
            self.scroll_offset = output.state.offset.y;
            self.viewport_height = output.inner_rect.height();
        });
    }

    fn table_header(&mut self, ui: &mut egui::Ui, columns: &Columns) {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(columns.width, HEADER_HEIGHT),
            egui::Sense::hover(),
        );
        let visuals = ui.visuals().clone();
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            visuals.widgets.noninteractive.bg_stroke,
        );
        for (index, (title, sort)) in [
            ("Status", Some(Sort::Status)),
            ("File", Some(Sort::Name)),
            ("Catalog match", None),
            ("Size", Some(Sort::Size)),
        ]
        .into_iter()
        .enumerate()
        {
            let cell = columns.cell(rect, index);
            let active = sort.is_some_and(|sort| sort == self.sort);
            let mut color = visuals.weak_text_color();
            if let Some(sort) = sort {
                let response = ui
                    .interact(cell, ui.id().with(("sort", index)), egui::Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if response.hovered() || active {
                    color = visuals.strong_text_color();
                }
                if response.clicked() {
                    if self.sort == sort {
                        self.descending = !self.descending;
                    } else {
                        self.sort = sort;
                        self.descending = false;
                    }
                    self.filter_rows();
                }
            }
            let text = paint_cell(
                ui,
                cell,
                RichText::new(title).strong().color(color),
                index == 3,
            );
            if active && let Some(text) = text {
                // Default fonts lack arrow glyphs, so the sort direction is painted.
                let x = if index == 3 {
                    text.left() - 12.0
                } else {
                    text.right() + 6.0
                };
                let (tip, base) = if self.descending {
                    (4.0, -3.0)
                } else {
                    (-4.0, 3.0)
                };
                let y = text.center().y;
                ui.painter().add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(x, y + base),
                        egui::pos2(x + 8.0, y + base),
                        egui::pos2(x + 4.0, y + tip),
                    ],
                    color,
                    egui::Stroke::NONE,
                ));
            }
        }
    }

    /// Returns true when the row was clicked.
    fn table_row(&self, ui: &mut egui::Ui, columns: &Columns, id: usize, stripe: bool) -> bool {
        let record = &self.records[id];
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(columns.width, ROW_HEIGHT), egui::Sense::CLICK);
        let selected = self.selected_row == Some(id);
        let visuals = ui.visuals();
        let fill = if selected {
            visuals.selection.bg_fill
        } else if response.hovered() {
            visuals.widgets.hovered.weak_bg_fill
        } else if stripe {
            visuals.faint_bg_color
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 3.0, fill);
        let status = &record.result.status;
        let status_cell = columns.cell(rect, 0);
        ui.painter().circle_filled(
            egui::pos2(status_cell.left() + 4.0, status_cell.center().y),
            4.0,
            status_color(status),
        );
        // Tinted text loses contrast on the selection fill; the dot keeps the color cue.
        let strong = if selected {
            visuals.selection.stroke.color
        } else {
            visuals.strong_text_color()
        };
        paint_cell(
            ui,
            status_cell.with_min_x(status_cell.left() + 14.0),
            RichText::new(status.label()).color(if selected {
                strong
            } else {
                status_color(status)
            }),
            false,
        );
        paint_cell(
            ui,
            columns.cell(rect, 1),
            RichText::new(record.display_name()).color(strong),
            false,
        );
        let matched = match record.result.candidates.as_slice() {
            [] => RichText::new("—").weak(),
            _ if *status == Verification::Ambiguous => RichText::new(format!(
                "{} candidates · inspect details",
                record.result.candidates.len()
            ))
            .color(status_color(status)),
            [first, ..] => RichText::new(&first.entry.game),
        };
        paint_cell(ui, columns.cell(rect, 2), matched, false);
        let size = record.size.map_or("—".into(), format_size);
        paint_cell(ui, columns.cell(rect, 3), RichText::new(size).weak(), true);
        let label = format!("{} · {}", record.display_name(), status.label());
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &label)
        });
        response
            .on_hover_text_at_pointer(display_path(&record.path))
            .clicked()
    }
}

const ROW_HEIGHT: f32 = 30.0;
const HEADER_HEIGHT: f32 = 28.0;
const CELL_PADDING: f32 = 8.0;
const STATUSES: [Verification; 6] = [
    Verification::Exact,
    Verification::CrcOnly,
    Verification::Ambiguous,
    Verification::Unknown,
    Verification::ReadError,
    Verification::Unsupported,
];

/// Status text always accompanies the color; the color only speeds up scanning.
fn status_color(status: &Verification) -> Color32 {
    match status {
        Verification::Exact => Color32::from_rgb(110, 205, 150),
        Verification::CrcOnly => Color32::from_rgb(226, 190, 104),
        Verification::Ambiguous => Color32::from_rgb(196, 160, 236),
        Verification::Unknown => Color32::from_rgb(160, 170, 184),
        Verification::ReadError => Color32::from_rgb(244, 136, 120),
        Verification::Unsupported => Color32::from_rgb(132, 142, 156),
    }
}

/// Fixed status/size columns; file and catalog match share the remaining width. Narrow
/// panels (small windows, 125–150% display scaling) drop the catalog column, which the
/// details panel repeats.
struct Columns {
    width: f32,
    edges: [f32; 5],
}

impl Columns {
    fn new(width: f32) -> Self {
        let status = 164.0;
        let size = 84.0;
        let flexible = (width - status - size).max(120.0);
        let file = if width >= 560.0 {
            flexible * 0.55
        } else {
            flexible
        };
        let edges = [
            0.0,
            status,
            status + file,
            status + flexible,
            status + flexible + size,
        ];
        Self {
            width: edges[4].max(width),
            edges,
        }
    }

    fn cell(&self, row: egui::Rect, index: usize) -> egui::Rect {
        egui::Rect::from_x_y_ranges(
            row.left() + self.edges[index] + CELL_PADDING
                ..=row.left() + self.edges[index + 1] - CELL_PADDING,
            row.y_range(),
        )
    }
}

/// Single-line text, truncated with an ellipsis to fit the cell. Returns the text bounds.
fn paint_cell(
    ui: &egui::Ui,
    cell: egui::Rect,
    text: RichText,
    right_align: bool,
) -> Option<egui::Rect> {
    if cell.width() < 1.0 {
        return None;
    }
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        cell.width().max(0.0),
        egui::TextStyle::Body,
    );
    let x = if right_align {
        cell.right() - galley.size().x
    } else {
        cell.left()
    };
    let pos = egui::pos2(x, cell.center().y - galley.size().y / 2.0);
    let bounds = egui::Rect::from_min_size(pos, galley.size());
    ui.painter()
        .with_clip_rect(cell.intersect(ui.clip_rect()))
        .galley(pos, galley, ui.visuals().text_color());
    Some(bounds)
}

impl eframe::App for Skyline {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        if self.receiver.is_some()
            && !ctx.wants_keyboard_input()
            && !ctx.is_popup_open()
            && ctx.input(|input| input.key_pressed(egui::Key::Escape))
        {
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

fn main() {
    if std::env::args().any(|arg| arg == "--version") {
        println!("Skyline {VERSION}");
        return;
    }
    if let Err(error) = run() {
        report_startup_error(&format!("{error:#}"));
        std::process::exit(1);
    }
}

/// Release builds on Windows have no console, so stderr alone would make a failed start
/// (for example, no usable OpenGL driver) look like the app never launched.
fn report_startup_error(message: &str) {
    eprintln!("Skyline could not start: {message}");
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let text = wide(&format!("Skyline could not start.\n\n{message}"));
        let title = wide("Skyline");
        // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}

fn run() -> Result<()> {
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
