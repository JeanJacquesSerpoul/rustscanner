//! État global de l'interface et actions (appels au cœur).

use std::collections::BTreeSet;

use leptos::prelude::*;
use leptos::task::spawn_local;
use rustscanner_shared::{
    AppInfo, Area, Constraint, DeviceInfo, OptValue, PageInfo, PreviewInfo, ScanEvent, ScanOption, Settings, names,
    tr, trf,
};
use serde_json::json;

use crate::api::{self, ApiError};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Zoom {
    /// Page entière visible.
    Fit,
    /// Largeur de la zone d'affichage.
    Width,
    /// Facteur relatif à l'image d'affichage.
    Scale(f64),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum View {
    Page,
    Area,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub enum Scan {
    #[default]
    Idle,
    Running {
        page: u32,
        fraction: f64,
        preview: bool,
        /// Pages obtenues dans la série en cours.
        done: u32,
        cancelling: bool,
    },
}

impl Scan {
    pub fn is_running(&self) -> bool {
        matches!(self, Scan::Running { .. })
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum DeviceState {
    /// Recherche des scanners en cours.
    Searching,
    None,
    Opening(String),
    Open(String),
    Failed(String, String),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Dialog {
    Save,
    Print,
    Settings,
    Shortcuts,
    /// Fermeture demandée avec des pages non enregistrées.
    Close(usize),
    /// Nouveau document alors que le document courant n'est pas enregistré.
    NewDocument,
}

#[derive(Clone, PartialEq, Debug)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

#[derive(Clone, PartialEq, Debug)]
pub enum ToastAction {
    Undo,
    Open(String),
    Reveal(String),
    Retry,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub text: String,
    pub actions: Vec<(String, ToastAction)>,
}

#[derive(Clone, Copy)]
pub struct AppState {
    pub info: RwSignal<Option<AppInfo>>,
    pub settings: RwSignal<Settings>,
    pub lang: RwSignal<rustscanner_shared::Lang>,
    pub devices: RwSignal<Vec<DeviceInfo>>,
    pub device: RwSignal<DeviceState>,
    pub options: RwSignal<Vec<ScanOption>>,
    /// Option en cours d'application (nom).
    pub applying: RwSignal<Option<String>>,
    pub pages: RwSignal<Vec<PageInfo>>,
    pub current: RwSignal<Option<u64>>,
    pub selected: RwSignal<BTreeSet<u64>>,
    /// Ancre des sélections étendues (Maj + clic).
    pub anchor: RwSignal<Option<u64>>,
    pub preview: RwSignal<Option<PreviewInfo>>,
    pub scan: RwSignal<Scan>,
    pub view: RwSignal<View>,
    pub zoom: RwSignal<Zoom>,
    pub dialog: RwSignal<Option<Dialog>>,
    pub toasts: RwSignal<Vec<Toast>>,
    pub busy: RwSignal<Option<String>>,
    pub dirty: RwSignal<bool>,
    /// Avancement de l'enregistrement (pages faites, total).
    pub export_progress: RwSignal<Option<(usize, usize)>>,
    next_toast: StoredValue<u64>,
}

pub fn use_state() -> AppState {
    expect_context::<AppState>()
}

impl AppState {
    pub fn new() -> AppState {
        AppState {
            info: RwSignal::new(None),
            settings: RwSignal::new(Settings::default()),
            lang: RwSignal::new(rustscanner_shared::i18n::lang()),
            devices: RwSignal::new(Vec::new()),
            device: RwSignal::new(DeviceState::None),
            options: RwSignal::new(Vec::new()),
            applying: RwSignal::new(None),
            pages: RwSignal::new(Vec::new()),
            current: RwSignal::new(None),
            selected: RwSignal::new(BTreeSet::new()),
            anchor: RwSignal::new(None),
            preview: RwSignal::new(None),
            scan: RwSignal::new(Scan::Idle),
            view: RwSignal::new(View::Page),
            zoom: RwSignal::new(Zoom::Fit),
            dialog: RwSignal::new(None),
            toasts: RwSignal::new(Vec::new()),
            busy: RwSignal::new(None),
            dirty: RwSignal::new(false),
            export_progress: RwSignal::new(None),
            next_toast: StoredValue::new(1),
        }
    }

    // ---- notifications ----

    pub fn toast(&self, kind: ToastKind, text: impl Into<String>, actions: Vec<(String, ToastAction)>) {
        let id = self.next_toast.get_value();
        self.next_toast.set_value(id + 1);
        let long = !actions.is_empty() || kind == ToastKind::Error;
        self.toasts.update(|t| {
            t.push(Toast {
                id,
                kind,
                text: text.into(),
                actions,
            });
            if t.len() > 4 {
                t.remove(0);
            }
        });
        let toasts = self.toasts;
        set_timeout(
            move || toasts.update(|t| t.retain(|x| x.id != id)),
            std::time::Duration::from_millis(if long { 7000 } else { 3500 }),
        );
    }

    pub fn error(&self, e: &ApiError) {
        self.toast(ToastKind::Error, e.message.clone(), Vec::new());
    }

    pub fn dismiss(&self, id: u64) {
        self.toasts.update(|t| t.retain(|x| x.id != id));
    }

    // ---- démarrage ----

    pub fn boot(&self, info: AppInfo) {
        self.settings.set(info.settings.clone());
        let sane = info.sane;
        self.info.set(Some(info));
        let s = *self;
        spawn_local(async move {
            if let Ok(pages) = api::call::<Vec<PageInfo>>("list_pages", json!({})).await {
                s.set_pages(pages);
            }
            if sane {
                s.refresh_devices(true).await;
            }
        });
    }

    // ---- scanners ----

    pub async fn refresh_devices(&self, auto_open: bool) {
        let previous = self.device.get_untracked();
        if !matches!(previous, DeviceState::Open(_)) {
            self.device.set(DeviceState::Searching);
        }
        match api::call::<Vec<DeviceInfo>>("list_devices", json!({})).await {
            Ok(list) => {
                let settings = self.settings.get_untracked();
                self.devices.set(list.clone());
                if let DeviceState::Open(_) = previous {
                    return;
                }
                self.device.set(DeviceState::None);
                if !auto_open || list.is_empty() {
                    return;
                }
                // dernier scanner utilisé, sinon le premier qui n'est pas une caméra
                let choice = settings
                    .last_device
                    .as_ref()
                    .filter(|_| settings.auto_open)
                    .and_then(|n| list.iter().find(|d| &d.name == n))
                    .or_else(|| list.iter().find(|d| d.backend() != "v4l" && d.backend() != "test"))
                    .or_else(|| list.first());
                if let Some(d) = choice {
                    self.open_device(d.name.clone()).await;
                }
            }
            Err(e) => {
                self.device.set(DeviceState::None);
                self.error(&e);
            }
        }
    }

    pub async fn open_device(&self, name: String) {
        if self.scan.get_untracked().is_running() {
            return;
        }
        self.device.set(DeviceState::Opening(name.clone()));
        self.options.set(Vec::new());
        self.preview.set(None);
        match api::call::<Vec<ScanOption>>("open_device", json!({ "name": name })).await {
            Ok(opts) => {
                self.options.set(opts);
                self.device.set(DeviceState::Open(name));
            }
            Err(e) => {
                self.device.set(DeviceState::Failed(name, e.message.clone()));
            }
        }
    }

    pub fn device_info(&self) -> Option<DeviceInfo> {
        let name = match self.device.get() {
            DeviceState::Open(n) | DeviceState::Opening(n) | DeviceState::Failed(n, _) => n,
            _ => return None,
        };
        self.devices.with(|l| l.iter().find(|d| d.name == name).cloned())
    }

    pub fn is_open(&self) -> bool {
        matches!(self.device.get(), DeviceState::Open(_))
    }

    pub fn is_open_untracked(&self) -> bool {
        matches!(self.device.get_untracked(), DeviceState::Open(_))
    }

    // ---- options ----

    pub fn option(&self, name: &str) -> Option<ScanOption> {
        self.options.with(|o| o.iter().find(|x| x.name == name && x.active).cloned())
    }

    pub fn set_option(&self, name: String, value: Option<OptValue>) {
        let s = *self;
        // affichage immédiat, confirmé (ou corrigé) par la réponse du scanner
        if let Some(v) = value.clone() {
            self.options.update(|opts| {
                if let Some(o) = opts.iter_mut().find(|o| o.name == name) {
                    o.value = Some(v);
                }
            });
        }
        self.applying.set(Some(name.clone()));
        spawn_local(async move {
            let res = api::call::<Vec<ScanOption>>("set_option", json!({ "name": name, "value": value })).await;
            s.applying.set(None);
            match res {
                Ok(opts) => s.options.set(opts),
                Err(e) => {
                    s.error(&e);
                    if let Ok(opts) = api::call::<Vec<ScanOption>>("get_options", json!({})).await {
                        s.options.set(opts);
                    }
                }
            }
        });
    }

    pub fn reset_options(&self) {
        let s = *self;
        spawn_local(async move {
            match api::call::<Vec<ScanOption>>("reset_options", json!({})).await {
                Ok(opts) => {
                    s.options.set(opts);
                    s.toast(ToastKind::Info, tr!("Réglages du scanner réinitialisés", "Scanner settings reset"), Vec::new());
                }
                Err(e) => s.error(&e),
            }
        });
    }

    /// Surface de la vitre (mm) et zone sélectionnée.
    pub fn geometry(&self) -> Option<(f64, f64, Area)> {
        self.options.with(|opts| {
            let get = |n: &str| opts.iter().find(|o| o.name == n && o.active);
            let (tlx, tly, brx, bry) = (get(names::TL_X)?, get(names::TL_Y)?, get(names::BR_X)?, get(names::BR_Y)?);
            let max = |o: &ScanOption| match &o.constraint {
                Constraint::Range { max, .. } => *max,
                Constraint::Numbers { values } => values.iter().cloned().fold(0.0, f64::max),
                _ => o.value.as_ref().and_then(OptValue::as_f64).unwrap_or(0.0),
            };
            let v = |o: &ScanOption| o.value.as_ref().and_then(OptValue::as_f64).unwrap_or(0.0);
            let (gw, gh) = (max(brx), max(bry));
            if gw <= 0.0 || gh <= 0.0 {
                return None;
            }
            let (x, y) = (v(tlx), v(tly));
            Some((
                gw,
                gh,
                Area {
                    x,
                    y,
                    w: (v(brx) - x).max(0.0),
                    h: (v(bry) - y).max(0.0),
                },
            ))
        })
    }

    pub fn set_area(&self, area: Area) {
        let s = *self;
        // mise à jour locale immédiate
        self.options.update(|opts| {
            for o in opts.iter_mut() {
                let v = match o.name.as_str() {
                    names::TL_X => area.x,
                    names::TL_Y => area.y,
                    names::BR_X => area.x + area.w,
                    names::BR_Y => area.y + area.h,
                    _ => continue,
                };
                o.value = Some(OptValue::Number(v));
            }
        });
        spawn_local(async move {
            match api::call::<Vec<ScanOption>>("set_area", json!({ "area": area })).await {
                Ok(opts) => s.options.set(opts),
                Err(e) => s.error(&e),
            }
        });
    }

    pub fn resolution(&self) -> f64 {
        self.option(names::RESOLUTION)
            .or_else(|| self.option("x-resolution"))
            .and_then(|o| o.value.and_then(|v| v.as_f64()))
            .unwrap_or(300.0)
    }

    // ---- numérisation ----

    pub fn start_scan(&self, preview: bool) {
        if !self.is_open() || self.scan.get_untracked().is_running() {
            return;
        }
        self.scan.set(Scan::Running {
            page: 1,
            fraction: -1.0,
            preview,
            done: 0,
            cancelling: false,
        });
        if preview {
            self.view.set(View::Area);
        } else {
            self.view.set(View::Page);
        }
        let s = *self;
        spawn_local(async move {
            if let Err(e) = api::call::<()>("start_scan", json!({ "preview": preview })).await {
                s.scan.set(Scan::Idle);
                s.error(&e);
            }
        });
    }

    pub fn cancel_scan(&self) {
        self.scan.update(|s| {
            if let Scan::Running { cancelling, .. } = s {
                *cancelling = true;
            }
        });
        api::fire("cancel_scan", json!({}));
    }

    pub fn on_scan_event(&self, ev: ScanEvent) {
        match ev {
            ScanEvent::Started { page, preview } => self.scan.update(|s| match s {
                Scan::Running { page: p, fraction, .. } => {
                    *p = page;
                    *fraction = -1.0;
                }
                Scan::Idle => {
                    *s = Scan::Running {
                        page,
                        fraction: -1.0,
                        preview,
                        done: 0,
                        cancelling: false,
                    }
                }
            }),
            ScanEvent::Progress { page, fraction } => self.scan.update(|s| {
                if let Scan::Running { page: p, fraction: f, .. } = s {
                    *p = page;
                    *f = fraction;
                }
            }),
            ScanEvent::Page { page } => {
                let id = page.id;
                self.pages.update(|p| p.push(page));
                self.current.set(Some(id));
                self.selected.set(BTreeSet::from([id]));
                self.anchor.set(Some(id));
                self.dirty.set(true);
                self.scan.update(|s| {
                    if let Scan::Running { done, .. } = s {
                        *done += 1;
                    }
                });
            }
            ScanEvent::Preview { preview } => {
                self.preview.set(Some(preview));
                self.view.set(View::Area);
            }
            ScanEvent::Finished {
                pages,
                cancelled,
                error,
            } => {
                let preview = matches!(self.scan.get_untracked(), Scan::Running { preview: true, .. });
                self.scan.set(Scan::Idle);
                if let Some(e) = error {
                    self.toast(ToastKind::Error, e, vec![(tr!("Réessayer", "Retry").into(), ToastAction::Retry)]);
                } else if cancelled {
                    self.toast(ToastKind::Info, tr!("Numérisation annulée", "Scan cancelled"), Vec::new());
                } else if !preview && pages > 1 {
                    self.toast(
                        ToastKind::Success,
                        trf!("{pages} pages numérisées", "{pages} pages scanned"),
                        Vec::new(),
                    );
                }
            }
        }
    }

    // ---- pages ----

    pub fn set_pages(&self, pages: Vec<PageInfo>) {
        let ids: BTreeSet<u64> = pages.iter().map(|p| p.id).collect();
        self.selected.update(|s| s.retain(|id| ids.contains(id)));
        let current = self.current.get_untracked();
        if current.is_none_or(|c| !ids.contains(&c)) {
            // page voisine de celle qui a disparu
            let index = current
                .and_then(|c| self.pages.with_untracked(|old| old.iter().position(|p| p.id == c)))
                .unwrap_or(0);
            let next = pages.get(index.min(pages.len().saturating_sub(1))).map(|p| p.id);
            self.current.set(next);
            if let Some(n) = next
                && self.selected.with_untracked(|s| s.is_empty())
            {
                self.selected.set(BTreeSet::from([n]));
            }
        }
        if pages.is_empty() {
            self.dirty.set(false);
        }
        self.pages.set(pages);
    }

    pub fn current_page(&self) -> Option<PageInfo> {
        let id = self.current.get()?;
        self.pages.with(|p| p.iter().find(|x| x.id == id).cloned())
    }

    pub fn current_index(&self) -> Option<usize> {
        let id = self.current.get()?;
        self.pages.with(|p| p.iter().position(|x| x.id == id))
    }

    /// Pages visées par une action : la sélection, sinon la page affichée.
    pub fn targets(&self) -> Vec<u64> {
        let sel = self.selected.get_untracked();
        let ordered: Vec<u64> =
            self.pages.with_untracked(|p| p.iter().map(|x| x.id).filter(|id| sel.contains(id)).collect());
        if !ordered.is_empty() {
            return ordered;
        }
        self.current.get_untracked().into_iter().collect()
    }

    /// Clic sur une vignette (Ctrl : ajout/retrait, Maj : plage).
    pub fn click_page(&self, id: u64, ctrl: bool, shift: bool) {
        if shift && let Some(anchor) = self.anchor.get_untracked() {
            let range: BTreeSet<u64> = self.pages.with_untracked(|p| {
                let a = p.iter().position(|x| x.id == anchor).unwrap_or(0);
                let b = p.iter().position(|x| x.id == id).unwrap_or(0);
                let (lo, hi) = (a.min(b), a.max(b));
                p[lo..=hi].iter().map(|x| x.id).collect()
            });
            self.selected.set(range);
        } else if ctrl {
            self.selected.update(|s| {
                if !s.remove(&id) {
                    s.insert(id);
                }
            });
            self.anchor.set(Some(id));
        } else {
            self.selected.set(BTreeSet::from([id]));
            self.anchor.set(Some(id));
        }
        self.current.set(Some(id));
        self.view.set(View::Page);
    }

    pub fn go(&self, delta: i64) {
        let Some(i) = self.current_index() else { return };
        let n = self.pages.with_untracked(|p| p.len()) as i64;
        let j = (i as i64 + delta).clamp(0, n - 1) as usize;
        if let Some(id) = self.pages.with_untracked(|p| p.get(j).map(|x| x.id)) {
            self.click_page(id, false, false);
        }
    }

    pub fn select_all(&self) {
        let all: BTreeSet<u64> = self.pages.with_untracked(|p| p.iter().map(|x| x.id).collect());
        self.selected.set(all);
    }

    pub fn transform(&self, transform: &'static str) {
        let ids = self.targets();
        if ids.is_empty() {
            return;
        }
        let s = *self;
        self.busy.set(Some(tr!("Transformation…", "Transforming…").into()));
        spawn_local(async move {
            let res = api::call::<Vec<PageInfo>>("transform_pages", json!({ "ids": ids, "transform": transform })).await;
            s.busy.set(None);
            match res {
                Ok(p) => {
                    s.set_pages(p);
                    s.dirty.set(true);
                }
                Err(e) => s.error(&e),
            }
        });
    }

    pub fn delete_pages(&self) {
        let ids = self.targets();
        if ids.is_empty() {
            return;
        }
        let n = ids.len();
        let s = *self;
        spawn_local(async move {
            match api::call::<Vec<PageInfo>>("delete_pages", json!({ "ids": ids })).await {
                Ok(p) => {
                    s.selected.set(BTreeSet::new());
                    s.set_pages(p);
                    s.dirty.set(true);
                    let text = if n == 1 {
                        tr!("Page supprimée", "Page deleted").to_string()
                    } else {
                        trf!("{n} pages supprimées", "{n} pages deleted")
                    };
                    s.toast(ToastKind::Info, text, vec![(tr!("Annuler", "Undo").into(), ToastAction::Undo)]);
                }
                Err(e) => s.error(&e),
            }
        });
    }

    pub fn undo_delete(&self) {
        let s = *self;
        spawn_local(async move {
            if let Ok(p) = api::call::<Vec<PageInfo>>("restore_pages", json!({})).await {
                s.set_pages(p);
            }
        });
    }

    pub fn reorder(&self, order: Vec<u64>) {
        let s = *self;
        // ordre affiché tout de suite
        self.pages.update(|p| p.sort_by_key(|x| order.iter().position(|id| *id == x.id).unwrap_or(usize::MAX)));
        self.dirty.set(true);
        spawn_local(async move {
            if let Ok(p) = api::call::<Vec<PageInfo>>("reorder_pages", json!({ "order": order })).await {
                s.set_pages(p);
            }
        });
    }

    /// Déplace les pages ciblées d'un cran (`delta` = -1 ou 1).
    pub fn move_pages(&self, delta: i64) {
        let ids = self.targets();
        let mut order: Vec<u64> = self.pages.with_untracked(|p| p.iter().map(|x| x.id).collect());
        if ids.is_empty() {
            return;
        }
        let mut positions: Vec<usize> = ids.iter().filter_map(|id| order.iter().position(|x| x == id)).collect();
        positions.sort();
        if delta < 0 {
            if positions.first() == Some(&0) {
                return;
            }
            for p in positions {
                order.swap(p, p - 1);
            }
        } else {
            if positions.last() == Some(&(order.len() - 1)) {
                return;
            }
            for p in positions.into_iter().rev() {
                order.swap(p, p + 1);
            }
        }
        self.reorder(order);
    }

    /// Déplace les pages glissées avant la page `before` (ou à la fin).
    pub fn drop_pages(&self, dragged: Vec<u64>, before: Option<u64>) {
        let mut order: Vec<u64> = self.pages.with_untracked(|p| p.iter().map(|x| x.id).collect());
        order.retain(|id| !dragged.contains(id));
        let at = before.and_then(|b| order.iter().position(|x| *x == b)).unwrap_or(order.len());
        for (k, id) in dragged.iter().enumerate() {
            order.insert(at + k, *id);
        }
        self.reorder(order);
    }

    pub fn new_document(&self, force: bool) {
        if self.pages.with_untracked(|p| p.is_empty()) {
            return;
        }
        if self.dirty.get_untracked() && !force {
            self.dialog.set(Some(Dialog::NewDocument));
            return;
        }
        let s = *self;
        spawn_local(async move {
            let _ = api::call::<()>("clear_document", json!({})).await;
            s.preview.set(None);
            s.set_pages(Vec::new());
            s.current.set(None);
            s.selected.set(BTreeSet::new());
            s.dirty.set(false);
        });
    }

    pub fn has_pages(&self) -> bool {
        self.pages.with(|p| !p.is_empty())
    }

    pub fn save_settings(&self, settings: Settings) {
        let s = *self;
        spawn_local(async move {
            match api::call::<AppInfo>("save_settings", json!({ "settings": settings })).await {
                Ok(info) => {
                    let lang = if info.lang == "fr" {
                        rustscanner_shared::Lang::Fr
                    } else {
                        rustscanner_shared::Lang::En
                    };
                    s.settings.set(info.settings.clone());
                    s.info.set(Some(info));
                    crate::apply_theme(&s.settings.get_untracked().theme);
                    if lang != s.lang.get_untracked() {
                        rustscanner_shared::set_lang(lang);
                        crate::set_document_lang(lang);
                        // titres des options SANE dans la nouvelle langue
                        if s.is_open_untracked()
                            && let Ok(opts) = api::call::<Vec<ScanOption>>("get_options", json!({})).await
                        {
                            s.options.set(opts);
                        }
                        // reconstruit l'interface (voir `app::Root`)
                        s.lang.set(lang);
                    }
                }
                Err(e) => s.error(&e),
            }
        });
    }
}
