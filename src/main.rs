use chrono::{DateTime, Local};
use eframe::egui;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

#[derive(Clone, Debug, Serialize)]
struct FileRecord {
    name: String,
    path: String,
    size_bytes: u64,
    modified: String,
    sha256: String,
}

#[derive(Debug)]
enum WorkerEvent {
    ScanProgress {
        done: usize,
        total: usize,
        record: Option<FileRecord>,
        error: Option<String>,
    },
    ScanFileProgress {
        path: String,
        done: u64,
        total: u64,
    },
    ScanFinished {
        cancelled: bool,
    },
    CompareProgress {
        done: u64,
        total: u64,
    },
    CompareFinished(Result<bool, String>),
}

#[derive(Default)]
struct FileDnaApp {
    scan_paths: Vec<PathBuf>,
    recursive: bool,
    records: Vec<FileRecord>,
    errors: Vec<String>,
    status: String,
    progress: Option<(usize, usize)>,
    file_progress: Option<(String, u64, u64)>,
    scan_running: bool,
    cancel_flag: Option<Arc<AtomicBool>>,
    sender: Option<Sender<WorkerEvent>>,
    receiver: Option<Receiver<WorkerEvent>>,
    compare_a: Option<PathBuf>,
    compare_b: Option<PathBuf>,
    compare_result: Option<String>,
    compare_running: bool,
    compare_cancel_flag: Option<Arc<AtomicBool>>,
    compare_progress: Option<(u64, u64)>,
}

fn format_modified(path: &Path) -> String {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(|time| {
            let local: DateTime<Local> = time.into();
            local.format("%d.%m.%Y, %H:%M:%S").to_string()
        })
        .unwrap_or_else(|| "Nicht verfügbar".into())
}

fn hash_file(
    path: &Path,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err("Analyse abgebrochen".into());
        }
        let count = file
            .read(&mut buffer)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        progress(count as u64);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files(
    paths: &[PathBuf],
    recursive: bool,
    cancelled: &AtomicBool,
) -> (Vec<PathBuf>, Vec<String>) {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let mut pending = paths.to_vec();
    while let Some(path) = pending.pop() {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                errors.push(format!(
                    "Übersprungen (symbolischer Link): {}",
                    path.display()
                ));
            }
            Ok(metadata) if metadata.is_file() => files.push(path),
            Ok(metadata) if metadata.is_dir() => match fs::read_dir(&path) {
                Ok(entries) => {
                    for entry in entries {
                        match entry {
                            Ok(entry) => {
                                let child = entry.path();
                                if child.is_dir() {
                                    if recursive {
                                        pending.push(child);
                                    }
                                } else if child.is_file() {
                                    pending.push(child);
                                }
                            }
                            Err(error) => errors.push(format!(
                                "Eintrag in {} konnte nicht gelesen werden: {error}",
                                path.display()
                            )),
                        }
                    }
                }
                Err(error) => errors.push(format!(
                    "Ordner {} konnte nicht gelesen werden: {error}",
                    path.display()
                )),
            },
            Ok(_) => errors.push(format!(
                "Übersprungen (kein regulärer Dateityp): {}",
                path.display()
            )),
            Err(error) => errors.push(format!("{}: {error}", path.display())),
        }
    }
    files.sort();
    (files, errors)
}

impl FileDnaApp {
    fn add_paths(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            if !self.scan_paths.contains(&path) {
                self.scan_paths.push(path);
            }
        }
    }

    fn start_scan(&mut self) {
        if self.scan_running || self.scan_paths.is_empty() {
            return;
        }
        self.records.clear();
        self.errors.clear();
        self.status = "Dateien werden vorbereitet…".into();
        self.progress = Some((0, 0));
        self.scan_running = true;
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_flag = Some(cancel.clone());
        let paths = self.scan_paths.clone();
        let recursive = self.recursive;
        let (tx, rx) = mpsc::channel();
        self.sender = Some(tx.clone());
        self.receiver = Some(rx);
        thread::spawn(move || {
            let (files, mut errors) = collect_files(&paths, recursive, &cancel);
            let total = files.len();
            let _ = tx.send(WorkerEvent::ScanProgress {
                done: 0,
                total,
                record: None,
                error: None,
            });
            for (index, path) in files.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let result = (|| {
                    let metadata =
                        fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
                    let mut read = 0u64;
                    let hash = hash_file(path, &cancel, |count| {
                        read = read.saturating_add(count);
                        let _ = tx.send(WorkerEvent::ScanFileProgress {
                            path: path.display().to_string(),
                            done: read,
                            total: metadata.len(),
                        });
                    })?;
                    Ok::<_, String>(FileRecord {
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        path: path.display().to_string(),
                        size_bytes: metadata.len(),
                        modified: format_modified(path),
                        sha256: hash,
                    })
                })();
                let (record, error) = match result {
                    Ok(record) => (Some(record), None),
                    Err(_error) if cancel.load(Ordering::Relaxed) => (None, None),
                    Err(error) => (None, Some(error)),
                };
                let _ = tx.send(WorkerEvent::ScanProgress {
                    done: index + 1,
                    total,
                    record,
                    error,
                });
            }
            for error in errors.drain(..) {
                let _ = tx.send(WorkerEvent::ScanProgress {
                    done: total,
                    total,
                    record: None,
                    error: Some(error),
                });
            }
            let _ = tx.send(WorkerEvent::ScanFinished {
                cancelled: cancel.load(Ordering::Relaxed),
            });
        });
    }

    fn start_compare(&mut self) {
        let (Some(a), Some(b)) = (self.compare_a.clone(), self.compare_b.clone()) else {
            return;
        };
        if self.compare_running {
            return;
        }
        let Some(tx) = self.sender.clone() else {
            return;
        };
        self.compare_running = true;
        self.compare_result = Some("Vergleich läuft…".into());
        self.compare_progress = Some((
            0,
            fs::metadata(&a)
                .map(|m| m.len())
                .unwrap_or(0)
                .saturating_add(fs::metadata(&b).map(|m| m.len()).unwrap_or(0)),
        ));
        let cancel = Arc::new(AtomicBool::new(false));
        self.compare_cancel_flag = Some(cancel.clone());
        thread::spawn(move || {
            let total = fs::metadata(&a)
                .map(|m| m.len())
                .unwrap_or(0)
                .saturating_add(fs::metadata(&b).map(|m| m.len()).unwrap_or(0));
            let mut done = 0u64;
            let mut update = |count: u64| {
                done = done.saturating_add(count);
                let _ = tx.send(WorkerEvent::CompareProgress { done, total });
            };
            let result = match (
                hash_file(&a, &cancel, &mut update),
                hash_file(&b, &cancel, &mut update),
            ) {
                (Ok(hash_a), Ok(hash_b)) => Ok(hash_a == hash_b),
                (Err(error), _) | (_, Err(error)) => Err(error),
            };
            let cancelled = cancel.load(Ordering::Relaxed);
            let _ = tx.send(WorkerEvent::CompareFinished(if cancelled {
                Err("Vergleich abgebrochen".into())
            } else {
                result
            }));
        });
    }

    fn poll_worker(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.receiver else {
            return;
        };
        while let Ok(event) = receiver.try_recv() {
            match event {
                WorkerEvent::ScanProgress {
                    done,
                    total,
                    record,
                    error,
                } => {
                    self.progress = Some((done, total));
                    if total > 0 {
                        self.status = format!("Untersuche Datei {done} von {total}…");
                    }
                    if let Some(record) = record {
                        self.records.push(record);
                    }
                    if let Some(error) = error {
                        self.errors.push(error);
                    }
                }
                WorkerEvent::ScanFinished { cancelled } => {
                    self.scan_running = false;
                    self.cancel_flag = None;
                    self.status = if cancelled {
                        "Analyse abgebrochen. Bereits untersuchte Dateien bleiben in der Liste."
                            .into()
                    } else {
                        format!(
                            "Fertig: {} Dateien untersucht, {} Fehler.",
                            self.records.len(),
                            self.errors.len()
                        )
                    };
                }
                WorkerEvent::ScanFileProgress { path, done, total } => {
                    self.file_progress = Some((path, done, total))
                }
                WorkerEvent::CompareFinished(result) => {
                    self.compare_running = false;
                    self.compare_cancel_flag = None;
                    self.compare_result = Some(match result {
                        Ok(true) => "Inhalt identisch (SHA-256 stimmt überein).".into(),
                        Ok(false) => "Inhalt unterschiedlich.".into(),
                        Err(error) => format!("Vergleich fehlgeschlagen: {error}"),
                    });
                }
                WorkerEvent::CompareProgress { done, total } => {
                    self.compare_progress = Some((done, total))
                }
            }
        }
        if self.scan_running || self.compare_running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn duplicate_groups(&self) -> Vec<Vec<&FileRecord>> {
        let mut groups: HashMap<&str, Vec<&FileRecord>> = HashMap::new();
        for record in &self.records {
            groups.entry(&record.sha256).or_default().push(record);
        }
        groups
            .into_values()
            .filter(|group| group.len() > 1)
            .collect()
    }

    fn export_json(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("filedna-ergebnis.json")
            .add_filter("JSON", &["json"])
            .save_file()
        {
            let result = serde_json::to_string_pretty(&self.records)
                .map_err(|error| error.to_string())
                .and_then(|json| fs::write(&path, json).map_err(|error| error.to_string()));
            match result {
                Ok(()) => self.status = format!("JSON gespeichert: {}", path.display()),
                Err(error) => self.status = format!("Export fehlgeschlagen: {error}"),
            }
        }
    }

    fn export_csv(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("filedna-ergebnis.csv")
            .add_filter("CSV", &["csv"])
            .save_file()
        {
            let mut csv = String::from("Name;Pfad;Groesse_Bytes;Geaendert;SHA-256\n");
            for record in &self.records {
                let values = [
                    &record.name,
                    &record.path,
                    &record.size_bytes.to_string(),
                    &record.modified,
                    &record.sha256,
                ];
                csv.push_str(
                    &values
                        .iter()
                        .map(|value| format!("\"{}\"", value.replace('"', "\"\"")))
                        .collect::<Vec<_>>()
                        .join(";"),
                );
                csv.push('\n');
            }
            match fs::write(&path, csv) {
                Ok(()) => self.status = format!("CSV gespeichert: {}", path.display()),
                Err(error) => self.status = format!("Export fehlgeschlagen: {error}"),
            }
        }
    }
}

impl eframe::App for FileDnaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker(ctx);
        egui::TopBottomPanel::top("title_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("FileDNA · Alpha");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Schließen").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("🔬 FileDNA");
                ui.label("Dateien untersuchen, vergleichen und Duplikate finden.");
                ui.separator();

                ui.heading("1. Dateien und Ordner untersuchen");
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Dateien hinzufügen…").clicked() {
                        if let Some(paths) = rfd::FileDialog::new().pick_files() {
                            self.add_paths(paths);
                        }
                    }
                    if ui.button("Ordner hinzufügen…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.add_paths(vec![path]);
                        }
                    }
                    ui.checkbox(&mut self.recursive, "Unterordner einbeziehen");
                    if ui
                        .add_enabled(
                            !self.scan_running && !self.scan_paths.is_empty(),
                            egui::Button::new("Analyse starten"),
                        )
                        .clicked()
                    {
                        self.start_scan();
                    }
                    if ui
                        .add_enabled(self.scan_running, egui::Button::new("Abbrechen"))
                        .clicked()
                    {
                        if let Some(flag) = &self.cancel_flag {
                            flag.store(true, Ordering::Relaxed);
                        }
                    }
                });
                if !self.scan_paths.is_empty() {
                    ui.label(format!("{} Eingabe(n) ausgewählt", self.scan_paths.len()));
                    ui.horizontal(|ui| {
                        if ui.button("Auswahl leeren").clicked() {
                            self.scan_paths.clear();
                        }
                        ui.label(
                            self.scan_paths
                                .iter()
                                .map(|p| p.display().to_string())
                                .collect::<Vec<_>>()
                                .join(" · "),
                        );
                    });
                }
                if let Some((done, total)) = self.progress {
                    let fraction = if total == 0 {
                        if self.scan_running { 0.0 } else { 1.0 }
                    } else {
                        done as f32 / total as f32
                    };
                    ui.add(egui::ProgressBar::new(fraction).show_percentage().text(
                        if total == 0 {
                            "Suche Dateien…".into()
                        } else {
                            format!("{done} / {total}")
                        },
                    ));
                }
                if self.scan_running {
                    if let Some((path, done, total)) = &self.file_progress {
                        let fraction = if *total == 0 {
                            0.0
                        } else {
                            *done as f32 / *total as f32
                        };
                        ui.add(
                            egui::ProgressBar::new(fraction)
                                .show_percentage()
                                .text(format!(
                                    "Lese {}",
                                    Path::new(path)
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                )),
                        );
                    }
                }
                if !self.status.is_empty() {
                    ui.label(&self.status);
                }

                ui.horizontal(|ui| {
                    ui.heading("Ergebnisse");
                    if ui
                        .add_enabled(
                            !self.records.is_empty(),
                            egui::Button::new("JSON exportieren…"),
                        )
                        .clicked()
                    {
                        self.export_json();
                    }
                    if ui
                        .add_enabled(
                            !self.records.is_empty(),
                            egui::Button::new("CSV exportieren…"),
                        )
                        .clicked()
                    {
                        self.export_csv();
                    }
                    if ui
                        .add_enabled(
                            !self.records.is_empty(),
                            egui::Button::new("Ergebnis kopieren"),
                        )
                        .clicked()
                    {
                        let text = self
                            .records
                            .iter()
                            .map(|r| {
                                format!(
                                    "{}\t{}\t{} Bytes\t{}",
                                    r.name, r.path, r.size_bytes, r.sha256
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        ctx.copy_text(text);
                        self.status = "Ergebnisse in die Zwischenablage kopiert.".into();
                    }
                });
                egui::Grid::new("results")
                    .striped(true)
                    .num_columns(4)
                    .show(ui, |ui| {
                        ui.strong("Name");
                        ui.strong("Größe");
                        ui.strong("SHA-256");
                        ui.strong("Pfad");
                        ui.end_row();
                        for record in &self.records {
                            ui.label(&record.name);
                            ui.label(format!("{} Bytes", record.size_bytes));
                            ui.monospace(&record.sha256);
                            ui.label(&record.path);
                            ui.end_row();
                        }
                    });

                ui.separator();
                ui.heading("2. Duplikate");
                let duplicate_groups = self.duplicate_groups();
                if duplicate_groups.is_empty() {
                    ui.label("Noch keine identischen Dateien gefunden.");
                }
                for group in duplicate_groups {
                    ui.collapsing(
                        format!(
                            "{} gleiche Dateien · SHA-256 {}",
                            group.len(),
                            &group[0].sha256[..12]
                        ),
                        |ui| {
                            for record in group {
                                ui.label(&record.path);
                            }
                        },
                    );
                }

                ui.separator();
                ui.heading("3. Zwei Dateien vergleichen");
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Datei A wählen…").clicked() {
                        self.compare_a = rfd::FileDialog::new().pick_file();
                        self.compare_result = None;
                    }
                    ui.label(
                        self.compare_a
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "Keine Datei gewählt".into()),
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Datei B wählen…").clicked() {
                        self.compare_b = rfd::FileDialog::new().pick_file();
                        self.compare_result = None;
                    }
                    ui.label(
                        self.compare_b
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "Keine Datei gewählt".into()),
                    );
                    let enabled = self.compare_a.is_some()
                        && self.compare_b.is_some()
                        && !self.compare_running;
                    if ui
                        .add_enabled(enabled, egui::Button::new("Vergleichen"))
                        .clicked()
                    {
                        self.start_compare();
                    }
                    if ui
                        .add_enabled(
                            self.compare_running,
                            egui::Button::new("Vergleich abbrechen"),
                        )
                        .clicked()
                    {
                        if let Some(flag) = &self.compare_cancel_flag {
                            flag.store(true, Ordering::Relaxed);
                        }
                    }
                });
                if self.compare_running {
                    if let Some((done, total)) = self.compare_progress {
                        let fraction = if total == 0 {
                            1.0
                        } else {
                            done as f32 / total as f32
                        };
                        ui.add(
                            egui::ProgressBar::new(fraction)
                                .show_percentage()
                                .text("Vergleich liest beide Dateien"),
                        );
                    }
                }
                if let Some(result) = &self.compare_result {
                    ui.label(result);
                }

                if !self.errors.is_empty() {
                    ui.separator();
                    ui.heading(format!("Fehler und Hinweise ({})", self.errors.len()));
                    for error in &self.errors {
                        ui.colored_label(egui::Color32::LIGHT_RED, error);
                    }
                }
            });
        });
    }
}

fn main() -> eframe::Result {
    let initial_path = env::args_os().nth(1).map(PathBuf::from);
    let mut app = FileDnaApp::default();
    if let Some(path) = initial_path {
        app.add_paths(vec![path]);
        app.start_scan();
    }
    let icon = image::load_from_memory(include_bytes!("../assets/filedna.png"))
        .expect("Das eingebettete FileDNA-Icon konnte nicht geladen werden.")
        .to_rgba8();
    let (icon_width, icon_height) = icon.dimensions();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_icon(egui::IconData {
            rgba: icon.into_raw(),
            width: icon_width,
            height: icon_height,
        }),
        ..Default::default()
    };
    eframe::run_native("FileDNA", options, Box::new(move |_cc| Ok(Box::new(app))))
}
