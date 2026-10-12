#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod preferences;

use anyhow::{Context, Result};
use eframe::egui::{self, Color32, RichText};
use skyline_core::{
    VERSION,
    catalog_folder::IntakeReport,
    hash::Cancellation,
    matching::CatalogIndex,
    model::{Catalog, FileRecord, Verification},
    scan::{ScanOptions, ScanProgress, scan_folder},
    settings::{AppSettings, SETTINGS_REVISION},
    store::Store,
    systems::{CollectionSystem, collection_systems, system_family},
    view::{Sort, display_path, format_size, format_utc, query_rows_in_collection_system},
};
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

enum Event {
    Progress(ScanProgress),
    Loaded(Box<LoadedState>),
    References(Box<ReferenceState>),
    CatalogProgress(usize, usize, PathBuf),
    Scanned(Vec<FileRecord>, Vec<CollectionSystem>, String),
    Failed(String),
    SettingsSaved(Box<AppSettings>),
    SettingsFailed(String),
    PathPicked(PathChoice, Option<PathBuf>),
}

struct LoadedState {
    catalogs: Arc<Vec<Catalog>>,
    records: Vec<FileRecord>,
    settings: AppSettings,
    settings_warning: Option<String>,
    intake: Option<IntakeReport>,
    systems: Vec<CollectionSystem>,
}

struct ReferenceState {
    catalogs: Arc<Vec<Catalog>>,
    intake: Option<IntakeReport>,
    warning: Option<String>,
}

#[derive(Clone, Copy)]
enum PathChoice {
    Dats,
    Folder,
}

enum Job {
    Load,
    SyncCatalogFolder(PathBuf),
    Scan {
        root: PathBuf,
        exclusions: Vec<PathBuf>,
        catalogs: Arc<Vec<Catalog>>,
        dat_folder: Option<PathBuf>,
    },
    SaveSettings(AppSettings),
    PickPath {
        kind: PathChoice,
        dialog: rfd::FileDialog,
    },
}

fn run_job(
    job: Job,
    database: PathBuf,
    cancel: Cancellation,
    sender: SyncSender<Event>,
    ctx: egui::Context,
) {
    let settings_job = matches!(&job, Job::SaveSettings(_));
    let result = (|| -> Result<Event> {
        cancel.check()?;
        match job {
            Job::Load => {
                let store = Store::open_cancellable(&database, &cancel)?;
                let (settings, settings_warning) = match store.settings() {
                    Ok(settings) => (settings, None),
                    Err(error) => (
                        AppSettings::default(),
                        Some(format!(
                            "Saved preferences could not be restored; using defaults. {error:#}"
                        )),
                    ),
                };
                let catalogs = store.active_catalogs(
                    (!settings.dat_folder.is_empty())
                        .then(|| std::path::Path::new(&settings.dat_folder)),
                )?;
                let records = store.collection()?;
                let systems = collection_systems(&records, &catalogs);
                Ok(Event::Loaded(Box::new(LoadedState {
                    catalogs: Arc::new(catalogs),
                    records,
                    systems,
                    intake: if settings.dat_folder.is_empty() {
                        None
                    } else {
                        store.catalog_folder_report(std::path::Path::new(&settings.dat_folder))?
                    },
                    settings,
                    settings_warning,
                })))
            }
            Job::SyncCatalogFolder(path) => {
                let mut store = Store::open_cancellable(&database, &cancel)?;
                let intake = sync_references(&mut store, &path, &cancel, &sender, &ctx)?;
                Ok(Event::References(Box::new(ReferenceState {
                    catalogs: Arc::new(store.active_catalogs(Some(&path))?),
                    intake: Some(intake),
                    warning: None,
                })))
            }
            Job::Scan {
                root,
                exclusions,
                catalogs,
                dat_folder,
            } => {
                let mut store = Store::open_cancellable(&database, &cancel)?;
                let catalogs = if let Some(path) = dat_folder {
                    let (intake, warning) =
                        match sync_references(&mut store, &path, &cancel, &sender, &ctx) {
                            Ok(report) => (Some(report), None),
                            Err(error) => {
                                cancel.check()?;
                                let cached = store.active_catalogs(Some(&path))?;
                                if cached.is_empty() {
                                    return Err(error);
                                }
                                (
                                    store.catalog_folder_report(&path)?,
                                    Some(format!(
                                        "DAT folder unavailable; using saved references. {error:#}"
                                    )),
                                )
                            }
                        };
                    let active = Arc::new(store.active_catalogs(Some(&path))?);
                    sender.send(Event::References(Box::new(ReferenceState {
                        catalogs: active.clone(),
                        intake,
                        warning,
                    })))?;
                    ctx.request_repaint();
                    active
                } else {
                    catalogs
                };
                let index = CatalogIndex::from_shared(catalogs);
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
                let systems = index.collection_systems(&report.records);
                Ok(Event::Scanned(report.records, systems, status))
            }
            Job::SaveSettings(settings) => {
                Store::open_cancellable(&database, &cancel)?.save_settings(&settings)?;
                Ok(Event::SettingsSaved(Box::new(settings)))
            }
            Job::PickPath { kind, dialog } => {
                let path = match kind {
                    PathChoice::Dats | PathChoice::Folder => dialog.pick_folder(),
                };
                Ok(Event::PathPicked(
                    kind,
                    if cancel.is_cancelled() { None } else { path },
                ))
            }
        }
    })();
    let event = result.unwrap_or_else(|error| {
        if settings_job {
            Event::SettingsFailed(format!("{error:#}"))
        } else {
            Event::Failed(format!("{error:#}"))
        }
    });
    let _ = sender.send(event);
    ctx.request_repaint();
}

fn sync_references(
    store: &mut Store,
    path: &std::path::Path,
    cancel: &Cancellation,
    sender: &SyncSender<Event>,
    ctx: &egui::Context,
) -> Result<IntakeReport> {
    let mut last = Instant::now() - Duration::from_secs(1);
    store.sync_catalog_folder(path, cancel, |done, total, path| {
        if last.elapsed() >= Duration::from_millis(100) || done == total {
            let _ = sender.try_send(Event::CatalogProgress(done, total, path.into()));
            ctx.request_repaint();
            last = Instant::now();
        }
    })
}

struct Skyline {
    database: PathBuf,
    catalogs: Arc<Vec<Catalog>>,
    records: Vec<FileRecord>,
    visible: Vec<usize>,
    duplicates: HashMap<String, usize>,
    search: String,
    status_filter: Option<Verification>,
    collection_system: Option<String>,
    matched_systems: Vec<CollectionSystem>,
    sort: Sort,
    descending: bool,
    selected_row: Option<usize>,
    dat_folder: String,
    intake: Option<IntakeReport>,
    catalog_progress: Option<(usize, usize, PathBuf)>,
    syncing_only: bool,
    root_path: String,
    exclusions: String,
    status: String,
    error: Option<String>,
    progress: Option<ScanProgress>,
    receiver: Option<Receiver<Event>>,
    cancel: Cancellation,
    show_about: bool,
    scroll_offset: f32,
    viewport_height: f32,
    scroll_request: Option<f32>,
    settings_loaded: bool,
    last_saved_settings: AppSettings,
    tracked_settings: AppSettings,
    settings_change_at: Option<Instant>,
    settings_save_failed: bool,
    picker_open: bool,
    worker: Option<std::thread::JoinHandle<()>>,
    saving_preferences: bool,
    quit_after_picker: bool,
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
            collection_system: None,
            matched_systems: vec![],
            sort: Sort::Name,
            descending: false,
            selected_row: None,
            dat_folder: String::new(),
            intake: None,
            catalog_progress: None,
            syncing_only: false,
            root_path: String::new(),
            exclusions: String::new(),
            status: "Loading local collection…".into(),
            error: None,
            progress: None,
            receiver: None,
            cancel: Cancellation::default(),
            show_about: false,
            scroll_offset: 0.0,
            viewport_height: 0.0,
            scroll_request: None,
            settings_loaded: false,
            last_saved_settings: AppSettings::default(),
            tracked_settings: AppSettings::default(),
            settings_change_at: None,
            settings_save_failed: false,
            picker_open: false,
            worker: None,
            saving_preferences: false,
            quit_after_picker: false,
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
        let quiet = matches!(&job, Job::SaveSettings(_));
        self.syncing_only = matches!(&job, Job::SyncCatalogFolder(_));
        self.catalog_progress = None;
        self.saving_preferences = quiet;
        self.picker_open = matches!(&job, Job::PickPath { .. });
        if !quiet {
            self.error = None;
        }
        self.progress = None;
        if !quiet {
            self.status = if self.picker_open {
                "Choose a path in the dialog, or cancel there.".into()
            } else {
                "Working…".into()
            };
        }
        let database = self.database.clone();
        let cancel = self.cancel.clone();
        let ctx = ctx.clone();
        let fallback_sender = sender.clone();
        let fallback_ctx = ctx.clone();
        let result = std::thread::Builder::new().name("skyline-job".into()).spawn(move || {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_job(job, database, cancel, sender, ctx))).is_err() {
                let message = "Background worker failed unexpectedly. Reopen or rescan to refresh saved state.".into();
                let _ = fallback_sender.send(if quiet { Event::SettingsFailed(message) } else { Event::Failed(message) });
                fallback_ctx.request_repaint();
            }
        });
        if let Err(error) = result {
            self.error = Some(format!("Could not start background job: {error}"));
            self.status = "Job could not start".into();
            if quiet {
                self.settings_save_failed = true;
            }
            self.finish();
        } else {
            self.worker = result.ok();
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let mut sync_folder = false;
        let events: Vec<_> = self
            .receiver
            .as_ref()
            .map(|receiver| receiver.try_iter().take(16).collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Progress(progress) => {
                    self.catalog_progress = None;
                    self.progress = Some(progress);
                }
                Event::CatalogProgress(done, total, path) => {
                    self.catalog_progress = Some((done, total, path))
                }
                Event::Loaded(state) => {
                    self.catalogs = state.catalogs;
                    self.intake = state.intake;
                    self.apply_settings(&state.settings);
                    self.last_saved_settings = state.settings;
                    self.tracked_settings = self.current_settings();
                    self.settings_loaded = true;
                    self.error = state.settings_warning;
                    self.set_records(state.records, state.systems);
                    self.status =
                        "Ready · saved results are historical; rescan to refresh byte evidence"
                            .into();
                    self.finish();
                }
                Event::References(state) => {
                    self.catalogs = state.catalogs;
                    self.intake = state.intake;
                    self.error = state.warning;
                    self.refresh_system_catalog_counts();
                    self.status = format!(
                        "{} catalogs ready · variants selected automatically",
                        self.catalogs.len()
                    );
                    if self.syncing_only {
                        self.finish();
                    }
                }
                Event::Scanned(records, systems, status) => {
                    self.set_records(records, systems);
                    self.status = status;
                    self.finish();
                }
                Event::Failed(error) => {
                    self.error = Some(error);
                    self.status = "Job stopped · previous saved data retained".into();
                    self.finish();
                }
                Event::SettingsSaved(settings) => {
                    self.last_saved_settings = *settings;
                    self.finish();
                }
                Event::SettingsFailed(error) => {
                    self.error = Some(format!("Preferences could not be saved: {error}"));
                    self.settings_save_failed = true;
                    self.finish();
                }
                Event::PathPicked(kind, path) => {
                    if let Some(path) = path {
                        let text = path.into_os_string().into_string();
                        match text {
                            Ok(text) => {
                                match kind {
                                    PathChoice::Dats => {
                                        self.dat_folder = text;
                                        self.catalogs = Arc::new(vec![]);
                                        self.intake = None;
                                        self.matched_systems =
                                            collection_systems(&self.records, &self.catalogs);
                                        sync_folder = true;
                                    }
                                    PathChoice::Folder => self.root_path = text,
                                }
                                self.status = "Path selected · ready when you are".into();
                            }
                            Err(_) => {
                                self.error = Some("This path cannot be represented as UTF-8; enter a supported path.".into());
                                self.status = "Path selection stopped".into();
                            }
                        }
                    } else {
                        self.status = "Path selection cancelled".into();
                    }
                    self.finish();
                }
            }
        }
        if sync_folder && !self.quit_after_picker {
            self.start(Job::SyncCatalogFolder(self.dat_folder.clone().into()), ctx);
        }
    }

    fn finish(&mut self) {
        self.receiver = None;
        self.progress = None;
        self.catalog_progress = None;
        self.picker_open = false;
        // Final events arrive only after the worker's database writes have finished.
        self.worker = None;
        self.saving_preferences = false;
    }

    fn current_settings(&self) -> AppSettings {
        AppSettings {
            revision: SETTINGS_REVISION,
            dat_folder: self.dat_folder.clone(),
            dat_path: String::new(),
            root_path: self.root_path.clone(),
            exclusions: self.exclusions.clone(),
            system: String::new(),
            preferred_catalog: None,
            search: self.search.clone(),
            status_filter: self.status_filter.clone(),
            collection_system: self.collection_system.clone(),
            sort: self.sort,
            descending: self.descending,
        }
    }

    fn apply_settings(&mut self, settings: &AppSettings) {
        self.dat_folder = settings.dat_folder.clone();
        self.root_path = settings.root_path.clone();
        self.exclusions = settings.exclusions.clone();
        self.search = settings.search.clone();
        self.status_filter = settings.status_filter.clone();
        self.collection_system = settings.collection_system.clone();
        self.sort = settings.sort;
        self.descending = settings.descending;
    }

    fn save_preferences_when_idle(&mut self, ctx: &egui::Context) {
        if !self.settings_loaded || self.quit_after_picker {
            return;
        }
        let current = self.current_settings();
        if current != self.tracked_settings {
            self.tracked_settings = current.clone();
            self.settings_change_at = Some(Instant::now());
            self.settings_save_failed = false;
        }
        if current == self.last_saved_settings
            || self.settings_save_failed
            || self.receiver.is_some()
        {
            return;
        }
        let elapsed = self
            .settings_change_at
            .map_or(Duration::from_millis(500), |at| at.elapsed());
        if elapsed >= Duration::from_millis(500) {
            self.start(Job::SaveSettings(current), ctx);
        } else {
            ctx.request_repaint_after(Duration::from_millis(500) - elapsed);
        }
    }

    fn pick_path(&mut self, kind: PathChoice, ctx: &egui::Context, frame: &eframe::Frame) {
        let dialog = rfd::FileDialog::new().set_parent(frame);
        let dialog = match kind {
            PathChoice::Dats => {
                let dialog = dialog.set_title("Choose your DAT folder");
                if self.dat_folder.is_empty() {
                    dialog
                } else {
                    dialog.set_directory(&self.dat_folder)
                }
            }
            PathChoice::Folder => {
                let dialog = dialog.set_title("Choose a ROM folder");
                if self.root_path.is_empty() {
                    dialog
                } else {
                    dialog.set_directory(&self.root_path)
                }
            }
        };
        self.start(Job::PickPath { kind, dialog }, ctx);
    }

    fn set_records(&mut self, records: Vec<FileRecord>, systems: Vec<CollectionSystem>) {
        self.matched_systems = systems;
        if self.collection_system.as_ref().is_some_and(|family| {
            !self
                .matched_systems
                .iter()
                .any(|system| &system.name == family)
        }) {
            self.collection_system = None;
        }
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

    fn refresh_system_catalog_counts(&mut self) {
        for system in &mut self.matched_systems {
            system.available_catalogs = self
                .catalogs
                .iter()
                .filter(|catalog| {
                    !catalog.entries.is_empty()
                        && system_family(&catalog.provenance.system) == system.name
                })
                .count();
        }
    }

    fn filter_rows(&mut self) {
        self.scroll_request = Some(0.0);
        self.visible = query_rows_in_collection_system(
            &self.records,
            &self.search,
            self.status_filter.as_ref(),
            self.collection_system.as_ref().and_then(|name| {
                self.matched_systems
                    .iter()
                    .find(|system| &system.name == name)
            }),
            self.sort,
            self.descending,
        );
    }

    fn controls(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
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
                if self.saving_preferences {
                    ui.small("Saving preferences…");
                }
                if self.receiver.is_some()
                    && !self.picker_open
                    && !self.saving_preferences
                    && ui.button("Cancel job").clicked()
                {
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
            if let Some((done, total, path)) = &self.catalog_progress {
                ui.label(format!(
                    "Indexing DATs · {done}/{total} · {}",
                    display_path(path)
                ));
            }
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
                // Restoring preferences must finish before the user can edit them.
                if !self.settings_loaded && self.receiver.is_some() { ui.disable(); }
                ui.add_space(8.0);
                ui.heading("Reference folder");
                ui.small("Add your DAT collection once. Skyline checks every catalog and chooses matching variants automatically.");
                ui.add(egui::TextEdit::singleline(&mut self.dat_folder).hint_text("C:\\Catalogs").desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.receiver.is_none(), egui::Button::new("Choose DAT folder…")).clicked() {
                        self.pick_path(PathChoice::Dats, ctx, frame);
                    }
                    if ui.add_enabled(self.receiver.is_none() && !self.dat_folder.trim().is_empty(), egui::Button::new("Refresh")).clicked() {
                        self.start(Job::SyncCatalogFolder(self.dat_folder.trim().into()), ctx);
                    }
                });
                ui.small(format!("{} catalogs ready", self.catalogs.len()));
                if self.catalogs.is_empty() && self.dat_folder.is_empty() { ui.small("Choose a DAT folder to identify files. You can also scan first."); }
                ui.collapsing("Reference details", |ui| {
                    if let Some(report) = &self.intake {
                        ui.small(format!("{} files found · {} parsed · {} cached · {} previous versions retained", report.discovered, report.parsed, report.cached, report.retained));
                        for issue in &report.issues { ui.label(format!("{}: {}", display_path(&issue.path), issue.explanation)); }
                        if report.omitted_issues > 0 { ui.label(format!("{} further issues omitted", report.omitted_issues)); }
                    }
                    ui.collapsing("All available catalogs", |ui| {
                        for catalog in self.catalogs.iter() { ui.small(format!("{} · {}", catalog.name, catalog.version)); }
                    });
                });
                ui.separator();
                ui.heading("Your systems");
                let mut changed = ui.selectable_value(&mut self.collection_system, None, "All files").changed();
                for system in &self.matched_systems {
                    changed |= ui.selectable_value(&mut self.collection_system, Some(system.name.clone()), &system.name).changed();
                    ui.small(format!("{} matched · {} awaiting identification", system.matched_files, system.suggested_files));
                    if system.available_catalogs == 0 && system.suggested_files > 0 { ui.small("No catalog available for this system. File extension suggests this family; bytes remain unverified."); }
                    else if !system.used_catalogs.is_empty() { ui.small(format!("{} catalog variants used automatically", system.used_catalogs.len())); }
                }
                if self.matched_systems.is_empty() { ui.small("Systems appear after scanning your files."); }
                if changed { self.filter_rows(); }
                ui.separator();
                ui.heading("Scan folder");
                ui.label("ROM folder path");
                ui.add(egui::TextEdit::singleline(&mut self.root_path).hint_text("C:\\ROMs").desired_width(f32::INFINITY));
                if ui.add_enabled(self.receiver.is_none(), egui::Button::new("Choose folder…")).clicked() {
                    self.pick_path(PathChoice::Folder, ctx, frame);
                }
                ui.collapsing("Scan options", |ui| {
                    ui.label("Exclude relative prefixes (one per line)");
                    ui.add(egui::TextEdit::multiline(&mut self.exclusions).desired_rows(2).desired_width(f32::INFINITY).hint_text("Backups\nUnsorted"));
                });
                if ui.add_enabled(self.receiver.is_none() && !self.root_path.trim().is_empty(), egui::Button::new("Scan and verify")).clicked() {
                    self.start(Job::Scan {
                        root: self.root_path.trim().into(),
                        exclusions: self.exclusions.lines().map(str::trim).filter(|s| !s.is_empty()).map(Into::into).collect(),
                        catalogs: self.catalogs.clone(), dat_folder: (!self.dat_folder.trim().is_empty()).then(|| self.dat_folder.trim().into()),
                    }, ctx);
                }
                ui.small("Every scan rereads file bytes. Original files are preserved.");
                ui.small("Loose ROMs from many systems are checked against active DATs. System identity comes from matching bytes.");
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
                    ui.separator();
                    ui.strong("Format & headers");
                    for note in skyline_core::formats::header_notes(record) { ui.label(note); }
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
                    if let Some(header) = &record.header {
                        ui.separator();
                        ui.strong("Content without header");
                        ui.label(format!(
                            "{} detected ({} bytes). Calculated digests for {} after it; the file is unchanged.",
                            header.format,
                            header.header_bytes,
                            format_size(header.size)
                        ));
                        for (algorithm, digest) in header.digests.values() {
                            if let Some(digest) = digest {
                                ui.label(algorithm);
                                ui.monospace(digest);
                            }
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
                            if let Some(header) = &candidate.entry.reference_header { ui.label("Catalog header metadata"); ui.monospace(header); }
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
                if !self.settings_loaded && self.receiver.is_some() {
                    ui.disable();
                }
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
                                .hint_text("Search files, systems or catalog titles")
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
                    ui.label("Choose your DAT folder and ROM folder, then scan and verify.");
                    ui.label(
                        RichText::new(
                            "Skyline picks matching catalogs automatically, entirely offline.",
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
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No rows match this search and filter.").weak())
                });
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
            ("System / catalog match", None),
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
            [first, ..] => RichText::new(format!(
                "{} · {}",
                system_family(&first.system),
                first.entry.game
            )),
        };
        paint_cell(ui, columns.cell(rect, 2), matched, false);
        let size = record.size.map_or("—".into(), format_size);
        paint_cell(ui, columns.cell(rect, 3), RichText::new(size).weak(), true);
        let systems = record
            .result
            .candidates
            .iter()
            .map(|candidate| system_family(&candidate.system))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ");
        let label = format!(
            "{} · {} · {}",
            record.display_name(),
            status.label(),
            systems
        );
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
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.poll(ctx);
        if self.receiver.is_some()
            && !self.picker_open
            && !self.saving_preferences
            && !ctx.wants_keyboard_input()
            && !ctx.is_popup_open()
            && ctx.input(|input| input.key_pressed(egui::Key::Escape))
        {
            self.cancel.cancel();
        }
        if ctx.input(|input| input.viewport().close_requested()) {
            self.cancel.cancel();
            if self.picker_open {
                // rfd keeps the parent's raw native handles. Keep the window alive
                // until the dialog returns, then honor the user's quit request.
                self.quit_after_picker = true;
                self.status = "Close the file dialog to finish quitting Skyline.".into();
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        if self.quit_after_picker && !self.picker_open {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.controls(ctx, frame);
        self.details(ctx);
        self.collection(ctx);
        egui::Window::new("About Skyline").open(&mut self.show_about).resizable(false).show(ctx, |ui| {
            ui.heading(format!("Skyline {VERSION}"));
            ui.label("Development build · offline ROM verification");
            ui.label("Loose-file read-only prototype. ZIP and file organization are not implemented yet.");
            ui.label(format!("Local database: {}", self.database.display()));
            ui.label("Application license: pending owner selection.");
        });
        self.save_preferences_when_idle(ctx);
        // eframe repaints on input and worker notifications. No continuous idle repaint loop.
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.cancel.cancel();
        self.receiver = None;
        // An older debounced save must finish before the final snapshot is written.
        // Dropping the receiver above also unblocks its terminal event send.
        if self.settings_loaded
            && (self.saving_preferences || self.current_settings() != self.last_saved_settings)
        {
            let settings = self.current_settings();
            let database = self.database.clone();
            // Rendering is finished. Flush the last edit on a worker, including edits
            // made during a job or inside the debounce window, then wait for its receipt.
            let pending = if self.saving_preferences {
                self.worker.take()
            } else {
                None
            };
            let result = preferences::flush(database, settings, pending);
            if let Err(error) = result {
                report_error(&format!(
                    "Skyline could not save your latest preferences.\n\n{error:#}"
                ));
            }
        }
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
        report_error(&format!("Skyline could not start.\n\n{error:#}"));
        std::process::exit(1);
    }
}

/// Release builds on Windows have no console, so stderr alone would make a failed start
/// (for example, no usable OpenGL driver) look like the app never launched.
fn report_error(message: &str) {
    eprintln!("{message}");
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let text = wide(message);
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
