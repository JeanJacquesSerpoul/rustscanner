//! Commandes appelées par l'interface (`invoke`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};

use rustscanner_shared::{
    AppInfo, Area, DeviceInfo, ExportFormat, ExportRequest, ExportResult, OptValue, PageInfo, Printer, ScanEvent,
    ScanOption, Settings, expand_pattern, i18n, names, tr,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;

use crate::config::{self, DeviceOptions};
use crate::document::{self, Document};
use crate::error::{Error, Result};
use crate::export::{self, Source};
use crate::scanner::{Output, Scanner};

pub struct AppState {
    pub scanner: Scanner,
    pub doc: Arc<Mutex<Document>>,
    pub settings: Mutex<Settings>,
    device_opts: Mutex<DeviceOptions>,
    device: Mutex<Option<String>>,
    ingest: mpsc::Sender<Output>,
}

impl AppState {
    pub fn new(app: AppHandle) -> AppState {
        let settings = config::load_settings();
        i18n::set_lang(i18n::resolve(&settings.language, &config::system_locale()));
        let doc = Arc::new(Mutex::new(Document::new(document::session_dir())));
        let (tx, rx) = mpsc::channel();
        let ingest_doc = doc.clone();
        std::thread::Builder::new()
            .name("ingest".into())
            .spawn(move || ingest(app, ingest_doc, rx))
            .expect("fil d'intégration");
        AppState {
            scanner: Scanner::start(),
            doc,
            settings: Mutex::new(settings),
            device_opts: Mutex::new(DeviceOptions::load()),
            device: Mutex::new(None),
            ingest: tx,
        }
    }

    pub fn unsaved_pages(&self) -> usize {
        let doc = self.doc.lock().unwrap();
        if doc.dirty { doc.len() } else { 0 }
    }

    fn remember(&self, name: &str, value: Option<OptValue>) {
        let enabled = self.settings.lock().unwrap().remember_options;
        let Some(device) = self.device.lock().unwrap().clone() else { return };
        if enabled {
            let mut opts = self.device_opts.lock().unwrap();
            opts.remember(&device, name, value);
            let _ = opts.save();
        }
    }
}

/// Intègre les images numérisées au document (dans l'ordre) et prévient l'interface.
fn ingest(app: AppHandle, doc: Arc<Mutex<Document>>, rx: mpsc::Receiver<Output>) {
    let emit = |ev: ScanEvent| {
        let _ = app.emit("scan", ev);
    };
    while let Ok(out) = rx.recv() {
        match out {
            Output::Event(ev) => emit(ev),
            Output::Image { image, preview: true, .. } => {
                match doc.lock().unwrap().set_preview(&image) {
                    Ok(preview) => emit(ScanEvent::Preview { preview }),
                    Err(e) => log::warn!("aperçu : {e}"),
                }
            }
            Output::Image { image, dpi, bilevel, .. } => {
                let (id, dir) = {
                    let mut d = doc.lock().unwrap();
                    (d.next_id(), d.dir().to_path_buf())
                };
                match Document::prepare(&dir, id, &image, dpi, bilevel, 0) {
                    Ok(p) => {
                        let page = doc.lock().unwrap().insert(p, None);
                        emit(ScanEvent::Page { page });
                    }
                    Err(e) => log::warn!("page : {e}"),
                }
            }
            Output::Done {
                pages,
                cancelled,
                error,
            } => emit(ScanEvent::Finished {
                pages,
                cancelled,
                error,
            }),
        }
    }
}

fn info(state: &AppState) -> AppInfo {
    let settings = state.settings.lock().unwrap().clone();
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        lang: i18n::lang().code().into(),
        save_dir: config::save_dir(&settings).to_string_lossy().into_owned(),
        settings,
        sane: state.scanner.available(),
        sane_version: state.scanner.version(),
        printing: export::printing_available(),
    }
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    info(&state)
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<AppInfo> {
    i18n::set_lang(i18n::resolve(&settings.language, &config::system_locale()));
    config::save_settings(&settings)?;
    *state.settings.lock().unwrap() = settings;
    Ok(info(&state))
}

#[tauri::command]
pub async fn list_devices(state: State<'_, AppState>) -> Result<Vec<DeviceInfo>> {
    let local_only = !state.settings.lock().unwrap().network_devices;
    let mut list = state.scanner.devices(local_only).await?;
    // les caméras (v4l) ne sont pas des scanners de documents : en fin de liste
    list.sort_by_key(|d| (d.backend() == "v4l", d.label().to_lowercase()));
    Ok(list)
}

#[tauri::command]
pub async fn open_device(state: State<'_, AppState>, name: String) -> Result<Vec<ScanOption>> {
    let saved = {
        let remember = state.settings.lock().unwrap().remember_options;
        if remember { state.device_opts.lock().unwrap().get(&name) } else { Vec::new() }
    };
    let opts = state.scanner.open(name.clone(), saved).await?;
    *state.device.lock().unwrap() = Some(name.clone());
    state.doc.lock().unwrap().clear_preview();
    let mut s = state.settings.lock().unwrap();
    if s.last_device.as_deref() != Some(name.as_str()) {
        s.last_device = Some(name);
        let _ = config::save_settings(&s);
    }
    Ok(opts)
}

#[tauri::command]
pub async fn close_device(state: State<'_, AppState>) -> Result<()> {
    *state.device.lock().unwrap() = None;
    state.scanner.close().await
}

#[tauri::command]
pub async fn get_options(state: State<'_, AppState>) -> Result<Vec<ScanOption>> {
    state.scanner.options().await
}

/// Modifie une option ; `value` absent : valeur automatique.
#[tauri::command]
pub async fn set_option(state: State<'_, AppState>, name: String, value: Option<OptValue>) -> Result<Vec<ScanOption>> {
    let opts = state.scanner.set(name.clone(), value).await?;
    let applied = opts.iter().find(|o| o.name == name).and_then(|o| o.value.clone());
    state.remember(&name, applied);
    Ok(opts)
}

#[tauri::command]
pub async fn set_area(state: State<'_, AppState>, area: Area) -> Result<Vec<ScanOption>> {
    let opts = state.scanner.set_area(area).await?;
    for name in [names::TL_X, names::TL_Y, names::BR_X, names::BR_Y] {
        let v = opts.iter().find(|o| o.name == name).and_then(|o| o.value.clone());
        state.remember(name, v);
    }
    Ok(opts)
}

/// Oublie les réglages mémorisés et rouvre le scanner avec ses valeurs d'usine.
#[tauri::command]
pub async fn reset_options(state: State<'_, AppState>) -> Result<Vec<ScanOption>> {
    let device = state.device.lock().unwrap().clone().ok_or(Error::NoDevice)?;
    {
        let mut opts = state.device_opts.lock().unwrap();
        opts.forget(&device);
        let _ = opts.save();
    }
    state.scanner.open(device, Vec::new()).await
}

#[tauri::command]
pub async fn start_scan(state: State<'_, AppState>, preview: bool) -> Result<()> {
    state.scanner.scan(preview, state.ingest.clone()).await
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) {
    state.scanner.cancel();
}

#[tauri::command]
pub fn list_pages(state: State<'_, AppState>) -> Vec<PageInfo> {
    state.doc.lock().unwrap().pages()
}

#[derive(serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum Transform {
    RotateLeft,
    RotateRight,
    Rotate180,
    FlipHorizontal,
    FlipVertical,
}

/// Applique une transformation aux pages (en parallèle, hors verrou).
#[tauri::command]
pub async fn transform_pages(state: State<'_, AppState>, ids: Vec<u64>, transform: Transform) -> Result<Vec<PageInfo>> {
    let jobs: Vec<(PageInfo, PathBuf, PathBuf)> = {
        let doc = state.doc.lock().unwrap();
        ids.iter()
            .filter_map(|id| doc.page(*id).ok())
            .map(|p| (p.info.clone(), p.path.clone(), doc.dir().to_path_buf()))
            .collect()
    };
    let doc = state.doc.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        let results: Vec<Result<()>> = std::thread::scope(|s| {
            let handles: Vec<_> = jobs
                .iter()
                .map(|(info, path, dir)| {
                    let doc = doc.clone();
                    s.spawn(move || -> Result<()> {
                        let img = Document::load(path)?;
                        let img = match transform {
                            Transform::RotateLeft => img.rotate270(),
                            Transform::RotateRight => img.rotate90(),
                            Transform::Rotate180 => img.rotate180(),
                            Transform::FlipHorizontal => img.fliph(),
                            Transform::FlipVertical => img.flipv(),
                        };
                        let p = Document::prepare(dir, info.id, &img, info.dpi, info.bilevel, info.rev + 1)?;
                        doc.lock().unwrap().replace(p)?;
                        Ok(())
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap_or(Ok(()))).collect()
        });
        results.into_iter().collect()
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))??;
    Ok(state.doc.lock().unwrap().pages())
}

#[tauri::command]
pub fn delete_pages(state: State<'_, AppState>, ids: Vec<u64>) -> Vec<PageInfo> {
    let mut doc = state.doc.lock().unwrap();
    doc.delete(&ids);
    doc.pages()
}

#[tauri::command]
pub fn restore_pages(state: State<'_, AppState>) -> Vec<PageInfo> {
    let mut doc = state.doc.lock().unwrap();
    doc.restore();
    doc.pages()
}

#[tauri::command]
pub fn reorder_pages(state: State<'_, AppState>, order: Vec<u64>) -> Vec<PageInfo> {
    let mut doc = state.doc.lock().unwrap();
    doc.reorder(&order);
    doc.pages()
}

#[tauri::command]
pub fn clear_document(state: State<'_, AppState>) {
    let mut doc = state.doc.lock().unwrap();
    doc.clear();
    doc.clear_preview();
}

/// Chemin proposé pour l'enregistrement : dossier et modèle de nom des réglages,
/// numéro choisi pour ne rien écraser.
#[tauri::command]
pub fn suggest_path(state: State<'_, AppState>, format: ExportFormat) -> String {
    let settings = state.settings.lock().unwrap().clone();
    let dir = config::save_dir(&settings);
    let now = chrono::Local::now();
    let (date, time) = (now.format("%Y-%m-%d").to_string(), now.format("%H-%M-%S").to_string());
    let has_n = settings.name_pattern.contains("{n}");
    for n in 1..10_000 {
        let name = expand_pattern(&settings.name_pattern, &date, &time, n);
        let base = dir.join(&name);
        let taken = if format == ExportFormat::Pdf {
            base.with_extension("pdf").exists()
        } else {
            export::image_paths(&base, format, 1)[0].exists() || export::image_paths(&base, format, 2)[0].exists()
        };
        if !taken {
            return base.with_extension(format.extension()).to_string_lossy().into_owned();
        }
        if !has_n {
            let name = format!("{name}-{n}");
            let base = dir.join(name);
            if !base.with_extension(format.extension()).exists() {
                return base.with_extension(format.extension()).to_string_lossy().into_owned();
            }
        }
    }
    dir.join("scan").with_extension(format.extension()).to_string_lossy().into_owned()
}

fn sources(state: &AppState, ids: &[u64]) -> Vec<Source> {
    let doc = state.doc.lock().unwrap();
    let pages = doc.pages();
    let ids: Vec<u64> = if ids.is_empty() { pages.iter().map(|p| p.id).collect() } else { ids.to_vec() };
    ids.iter()
        .filter_map(|id| doc.page(*id).ok())
        .map(|p| Source {
            info: p.info.clone(),
            path: p.path.clone(),
        })
        .collect()
}

#[derive(serde::Serialize, Clone)]
struct ExportProgress {
    done: usize,
    total: usize,
}

#[tauri::command]
pub async fn export_document(app: AppHandle, state: State<'_, AppState>, request: ExportRequest) -> Result<ExportResult> {
    let srcs = sources(&state, &request.pages);
    let all = srcs.len() == state.doc.lock().unwrap().len();
    let path = PathBuf::from(&request.path);
    let files = tauri::async_runtime::spawn_blocking(move || {
        export::export(&srcs, &path, request.format, request.quality, &|done, total| {
            let _ = app.emit("export-progress", ExportProgress { done, total });
        })
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))??;
    if all {
        state.doc.lock().unwrap().dirty = false;
    }
    // le dossier choisi devient la proposition suivante
    if let Some(dir) = files.first().and_then(|f| f.parent()) {
        let mut s = state.settings.lock().unwrap();
        let dir = dir.to_string_lossy().into_owned();
        if s.save_dir != dir {
            s.save_dir = dir;
            s.format = request.format;
            s.quality = request.quality;
            let _ = config::save_settings(&s);
        } else if s.format != request.format || s.quality != request.quality {
            s.format = request.format;
            s.quality = request.quality;
            let _ = config::save_settings(&s);
        }
    }
    let bytes = files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
    Ok(ExportResult {
        files: files.iter().map(|f| f.to_string_lossy().into_owned()).collect(),
        bytes,
    })
}

/// Boîte de dialogue « Enregistrer sous ».
#[tauri::command]
pub async fn pick_save_path(app: AppHandle, path: String, format: ExportFormat) -> Result<Option<String>> {
    let p = Path::new(&path);
    let (tx, rx) = oneshot::channel();
    let mut dialog = app
        .dialog()
        .file()
        .set_title(tr!("Enregistrer le document", "Save document"))
        .add_filter(format.label(), &[format.extension()]);
    if let Some(dir) = p.parent() {
        dialog = dialog.set_directory(dir);
    }
    if let Some(name) = p.file_name() {
        dialog = dialog.set_file_name(name.to_string_lossy());
    }
    dialog.save_file(move |f| {
        let _ = tx.send(f.and_then(|f| f.into_path().ok()));
    });
    let chosen = rx.await.map_err(|e| Error::Other(e.to_string()))?;
    Ok(chosen.map(|mut p| {
        if p.extension().and_then(|e| ExportFormat::from_extension(&e.to_string_lossy())) != Some(format) {
            let name = format!("{}.{}", p.file_name().unwrap_or_default().to_string_lossy(), format.extension());
            p.set_file_name(name);
        }
        p.to_string_lossy().into_owned()
    }))
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle, start: String) -> Result<Option<String>> {
    let (tx, rx) = oneshot::channel();
    let mut dialog = app.dialog().file().set_title(tr!("Dossier d'enregistrement", "Save folder"));
    if !start.is_empty() {
        dialog = dialog.set_directory(start);
    }
    dialog.pick_folder(move |f| {
        let _ = tx.send(f.and_then(|f| f.into_path().ok()));
    });
    let chosen = rx.await.map_err(|e| Error::Other(e.to_string()))?;
    Ok(chosen.map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn list_printers() -> Vec<Printer> {
    tauri::async_runtime::spawn_blocking(export::printers).await.unwrap_or_default()
}

#[tauri::command]
pub async fn print_document(
    state: State<'_, AppState>,
    pages: Vec<u64>,
    printer: String,
    copies: u32,
) -> Result<String> {
    let srcs = sources(&state, &pages);
    let dir = state.doc.lock().unwrap().dir().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let pdf = dir.join("print.pdf");
        export::write_pdf(&srcs, &pdf, 90, &|_, _| {})?;
        export::print(&pdf, &printer, copies)
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))?
}

#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> Result<()> {
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| Error::Other(e.to_string()))
}

#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> Result<()> {
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| Error::Other(e.to_string()))
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    let state = app.state::<AppState>();
    state.scanner.cancel();
    state.doc.lock().unwrap().dirty = false;
    app.exit(0);
}
