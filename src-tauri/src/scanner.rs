//! Fil dédié au scanner : tous les appels SANE (sauf l'annulation) y sont
//! sérialisés ; les commandes arrivent par un canal et répondent par `oneshot`.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Instant;

use image::DynamicImage;
use rustscanner_shared::{Area, Constraint, DeviceInfo, OptKind, OptUnit, OptValue, ScanEvent, ScanOption, i18n, is_feeder, names};
use tokio::sync::oneshot;

use crate::error::{Error, Result};
use crate::frames::{Frame, assemble};
use crate::gettext::Catalog;
use crate::sane::{self, Handle, Sane};

type Reply<T> = oneshot::Sender<Result<T>>;

/// Sortie d'une numérisation, transmise dans l'ordre au fil d'intégration.
pub enum Output {
    Event(ScanEvent),
    Image {
        image: DynamicImage,
        dpi: f64,
        bilevel: bool,
        preview: bool,
    },
    Done {
        pages: u32,
        cancelled: bool,
        error: Option<String>,
    },
}

enum Job {
    Devices { local_only: bool, reply: Reply<Vec<DeviceInfo>> },
    Open { name: String, saved: Vec<(String, OptValue)>, reply: Reply<Vec<ScanOption>> },
    Close { reply: Reply<()> },
    Options { reply: Reply<Vec<ScanOption>> },
    Set { name: String, value: Option<OptValue>, reply: Reply<Vec<ScanOption>> },
    SetArea { area: Area, reply: Reply<Vec<ScanOption>> },
    Scan { preview: bool, out: mpsc::Sender<Output>, reply: Reply<()> },
}

#[derive(Default)]
struct Cancel {
    handle: AtomicPtr<c_void>,
    requested: AtomicBool,
}

pub struct Scanner {
    tx: mpsc::Sender<Job>,
    sane: Option<Arc<Sane>>,
    cancel: Arc<Cancel>,
    pub load_error: Option<String>,
}

impl Scanner {
    pub fn start() -> Scanner {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(Cancel::default());
        let (sane, load_error) = match Sane::load() {
            Ok(s) => (Some(Arc::new(s)), None),
            Err(e) => (None, Some(e.to_string())),
        };
        if let Some(sane) = sane.clone() {
            let cancel = cancel.clone();
            std::thread::Builder::new()
                .name("scanner".into())
                .spawn(move || Worker::new(sane, cancel).run(rx))
                .expect("fil du scanner");
        }
        Scanner {
            tx,
            sane,
            cancel,
            load_error,
        }
    }

    pub fn available(&self) -> bool {
        self.sane.is_some()
    }

    pub fn version(&self) -> String {
        self.sane.as_ref().map(|s| s.version.clone()).unwrap_or_default()
    }

    async fn ask<T>(&self, make: impl FnOnce(Reply<T>) -> Job) -> Result<T> {
        if self.sane.is_none() {
            return Err(Error::NoSane(self.load_error.clone().unwrap_or_default()));
        }
        let (tx, rx) = oneshot::channel();
        self.tx.send(make(tx)).map_err(|_| Error::Other("fil du scanner arrêté".into()))?;
        rx.await.map_err(|_| Error::Other("fil du scanner arrêté".into()))?
    }

    pub async fn devices(&self, local_only: bool) -> Result<Vec<DeviceInfo>> {
        self.ask(|reply| Job::Devices { local_only, reply }).await
    }

    pub async fn open(&self, name: String, saved: Vec<(String, OptValue)>) -> Result<Vec<ScanOption>> {
        self.ask(|reply| Job::Open { name, saved, reply }).await
    }

    pub async fn close(&self) -> Result<()> {
        self.ask(|reply| Job::Close { reply }).await
    }

    pub async fn options(&self) -> Result<Vec<ScanOption>> {
        self.ask(|reply| Job::Options { reply }).await
    }

    pub async fn set(&self, name: String, value: Option<OptValue>) -> Result<Vec<ScanOption>> {
        self.ask(|reply| Job::Set { name, value, reply }).await
    }

    pub async fn set_area(&self, area: Area) -> Result<Vec<ScanOption>> {
        self.ask(|reply| Job::SetArea { area, reply }).await
    }

    /// Lance une numérisation ; rend la main une fois la série commencée.
    pub async fn scan(&self, preview: bool, out: mpsc::Sender<Output>) -> Result<()> {
        if !self.cancel.handle.load(Ordering::SeqCst).is_null() {
            return Err(Error::Busy);
        }
        self.cancel.requested.store(false, Ordering::SeqCst);
        self.ask(|reply| Job::Scan { preview, out, reply }).await
    }

    pub fn cancel(&self) {
        self.cancel.requested.store(true, Ordering::SeqCst);
        let h = self.cancel.handle.load(Ordering::SeqCst);
        if let Some(sane) = &self.sane
            && !h.is_null()
        {
            sane.cancel(Handle(h));
        }
    }

    pub fn scanning(&self) -> bool {
        !self.cancel.handle.load(Ordering::SeqCst).is_null()
    }
}

struct Worker {
    sane: Arc<Sane>,
    cancel: Arc<Cancel>,
    handle: Option<Handle>,
    catalog: Option<(i18n::Lang, Catalog)>,
}

impl Worker {
    fn new(sane: Arc<Sane>, cancel: Arc<Cancel>) -> Worker {
        Worker {
            sane,
            cancel,
            handle: None,
            catalog: None,
        }
    }

    fn run(mut self, rx: mpsc::Receiver<Job>) {
        while let Ok(job) = rx.recv() {
            match job {
                Job::Devices { local_only, reply } => {
                    let _ = reply.send(self.sane.devices(local_only));
                }
                Job::Open { name, saved, reply } => {
                    let _ = reply.send(self.open(&name, &saved));
                }
                Job::Close { reply } => {
                    self.close();
                    let _ = reply.send(Ok(()));
                }
                Job::Options { reply } => {
                    let _ = reply.send(self.handle().map(|h| self.options(h)));
                }
                Job::Set { name, value, reply } => {
                    let _ = reply.send(self.set(&name, value.as_ref()));
                }
                Job::SetArea { area, reply } => {
                    let _ = reply.send(self.set_area(area));
                }
                Job::Scan { preview, out, reply } => match self.handle() {
                    Ok(h) => {
                        let _ = reply.send(Ok(()));
                        self.scan(h, preview, &out);
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                },
            }
        }
        self.close();
    }

    fn handle(&self) -> Result<Handle> {
        self.handle.ok_or(Error::NoDevice)
    }

    fn close(&mut self) {
        if let Some(h) = self.handle.take() {
            self.sane.close(h);
        }
    }

    fn open(&mut self, name: &str, saved: &[(String, OptValue)]) -> Result<Vec<ScanOption>> {
        self.close();
        let h = self.sane.open(name)?;
        self.handle = Some(h);
        if !saved.is_empty() {
            // deux passes : certaines options n'apparaissent qu'après d'autres (mode → profondeur)
            for _ in 0..2 {
                let opts = self.options(h);
                for opt in &opts {
                    if let Some((_, v)) = saved.iter().find(|(n, _)| *n == opt.name)
                        && opt.active
                        && opt.settable
                        && opt.value.as_ref() != Some(v)
                    {
                        let _ = self.sane.set_value(h, opt.index, v);
                    }
                }
            }
        }
        Ok(self.options(h))
    }

    fn options(&mut self, h: Handle) -> Vec<ScanOption> {
        let lang = i18n::lang();
        if self.catalog.as_ref().map(|(l, _)| *l) != Some(lang) {
            let c = if lang == i18n::Lang::Fr { Catalog::sane_backends("fr") } else { Catalog::default() };
            self.catalog = Some((lang, c));
        }
        let catalog = &self.catalog.as_ref().unwrap().1;
        self.sane.options(h, &|s| catalog.translate(s))
    }

    fn find(&mut self, h: Handle, name: &str) -> Option<ScanOption> {
        self.options(h).into_iter().find(|o| o.name == name)
    }

    fn set(&mut self, name: &str, value: Option<&OptValue>) -> Result<Vec<ScanOption>> {
        let h = self.handle()?;
        let opt = self.find(h, name).ok_or_else(|| Error::Other(format!("option « {name} » inconnue")))?;
        match value {
            Some(v) => self.sane.set_value(h, opt.index, &coerce(&opt, v))?,
            None => self.sane.set_auto(h, opt.index)?,
        };
        Ok(self.options(h))
    }

    fn set_area(&mut self, area: Area) -> Result<Vec<ScanOption>> {
        let h = self.handle()?;
        let res = self.resolution(h);
        let order = [
            (names::TL_X, area.x),
            (names::TL_Y, area.y),
            (names::BR_X, area.x + area.w),
            (names::BR_Y, area.y + area.h),
        ];
        // l'origine d'abord, sinon le scanner peut refuser une largeur négative
        for (name, mm) in order {
            if let Some(opt) = self.find(h, name)
                && opt.active
            {
                let v = if opt.unit == OptUnit::Pixel { mm / 25.4 * res } else { mm };
                self.sane.set_value(h, opt.index, &coerce(&opt, &OptValue::Number(v)))?;
            }
        }
        Ok(self.options(h))
    }

    fn resolution(&mut self, h: Handle) -> f64 {
        let opts = self.options(h);
        ["resolution", "x-resolution"]
            .iter()
            .find_map(|n| opts.iter().find(|o| o.name == *n).and_then(|o| o.value.as_ref()?.as_f64()))
            .unwrap_or(300.0)
    }

    /// Réglages de l'aperçu (basse résolution, toute la vitre) ; renvoie les
    /// valeurs à rétablir ensuite.
    fn enter_preview(&mut self, h: Handle) -> Vec<(i32, OptValue)> {
        let mut restore = Vec::new();
        let opts = self.options(h);
        let mut set = |opt: &ScanOption, v: OptValue| {
            if opt.active && opt.settable && let Some(old) = &opt.value && *old != v
                && self.sane.set_value(h, opt.index, &v).is_ok()
            {
                restore.push((opt.index, old.clone()));
            }
        };
        for opt in &opts {
            match opt.name.as_str() {
                names::PREVIEW => set(opt, OptValue::Bool(true)),
                names::RESOLUTION => set(opt, OptValue::Number(nearest(opt, 100.0))),
                names::TL_X | names::TL_Y => set(opt, OptValue::Number(bounds(opt).0)),
                names::BR_X | names::BR_Y => set(opt, OptValue::Number(bounds(opt).1)),
                _ => {}
            }
        }
        restore
    }

    fn scan(&mut self, h: Handle, preview: bool, out: &mpsc::Sender<Output>) {
        self.cancel.handle.store(h.0, Ordering::SeqCst);
        let restore = if preview { self.enter_preview(h) } else { Vec::new() };
        let source = self
            .find(h, names::SOURCE)
            .and_then(|o| o.value.and_then(|v| v.as_str().map(str::to_string)))
            .unwrap_or_default();
        let feeder = !preview && is_feeder(&source);
        let dpi = self.resolution(h);
        let mut pages = 0u32;
        let mut cancelled = false;
        let mut error = None;
        loop {
            if self.cancel.requested.load(Ordering::SeqCst) {
                cancelled = true;
                break;
            }
            let page = pages + 1;
            let _ = out.send(Output::Event(ScanEvent::Started { page, preview }));
            match self.acquire(h, page, out) {
                Ok((image, bilevel)) => {
                    pages += 1;
                    let _ = out.send(Output::Image {
                        image,
                        dpi,
                        bilevel,
                        preview,
                    });
                }
                Err(e) => {
                    match e.kind() {
                        "cancelled" => cancelled = true,
                        // chargeur vide après au moins une page : fin normale de la série
                        "no-docs" if pages > 0 => {}
                        _ => error = Some(e.user_message()),
                    }
                    break;
                }
            }
            if !feeder {
                break;
            }
        }
        self.sane.cancel(h);
        self.cancel.handle.store(std::ptr::null_mut(), Ordering::SeqCst);
        for (index, v) in restore {
            let _ = self.sane.set_value(h, index, &v);
        }
        let _ = out.send(Output::Done {
            pages,
            cancelled,
            error,
        });
    }

    fn acquire(&mut self, h: Handle, page: u32, out: &mpsc::Sender<Output>) -> Result<(DynamicImage, bool)> {
        let mut frames = Vec::new();
        let mut buf = vec![0u8; 256 * 1024];
        let mut last_report = Instant::now();
        loop {
            self.sane.check(self.sane.start(h))?;
            let params = self.sane.parameters(h)?;
            // trois passes : la progression couvre l'ensemble des trames
            let passes = if params.format >= sane::FRAME_RED { 3.0 } else { 1.0 };
            let done_before = frames.len() as f64;
            let expected = if params.lines > 0 {
                params.bytes_per_line as usize * params.lines as usize
            } else {
                0
            };
            let mut data = Vec::with_capacity(expected);
            loop {
                if self.cancel.requested.load(Ordering::SeqCst) {
                    return Err(Error::Sane {
                        status: sane::STATUS_CANCELLED,
                        message: "cancelled".into(),
                    });
                }
                let n = self.sane.read(h, &mut buf)?;
                if n == 0 {
                    break;
                }
                data.extend_from_slice(&buf[..n]);
                if last_report.elapsed().as_millis() >= 80 {
                    last_report = Instant::now();
                    let fraction = if expected > 0 {
                        ((done_before + data.len() as f64 / expected as f64) / passes).min(1.0)
                    } else {
                        -1.0
                    };
                    let _ = out.send(Output::Event(ScanEvent::Progress { page, fraction }));
                }
            }
            // certains backends terminent la trame (EOF) au lieu de signaler l'annulation
            if self.cancel.requested.load(Ordering::SeqCst) {
                return Err(Error::Sane {
                    status: sane::STATUS_CANCELLED,
                    message: "cancelled".into(),
                });
            }
            let last = params.last_frame != 0;
            frames.push(Frame { params, data });
            if last {
                break;
            }
        }
        let _ = out.send(Output::Event(ScanEvent::Progress { page, fraction: 1.0 }));
        assemble(&frames)
    }
}

/// Bornes d'une option numérique.
fn bounds(opt: &ScanOption) -> (f64, f64) {
    match &opt.constraint {
        Constraint::Range { min, max, .. } => (*min, *max),
        Constraint::Numbers { values } if !values.is_empty() => (
            values.iter().cloned().fold(f64::INFINITY, f64::min),
            values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        ),
        _ => {
            let v = opt.value.as_ref().and_then(OptValue::as_f64).unwrap_or(0.0);
            (v, v)
        }
    }
}

/// Valeur autorisée la plus proche de `target`.
pub fn nearest(opt: &ScanOption, target: f64) -> f64 {
    match &opt.constraint {
        Constraint::Range { min, max, step } => {
            let v = target.clamp(*min, *max);
            if *step > 0.0 { min + ((v - min) / step).round() * step } else { v }
        }
        Constraint::Numbers { values } if !values.is_empty() => values
            .iter()
            .cloned()
            .min_by(|a, b| (a - target).abs().total_cmp(&(b - target).abs()))
            .unwrap_or(target),
        _ => target,
    }
}

/// Adapte la valeur au type et aux contraintes de l'option.
fn coerce(opt: &ScanOption, v: &OptValue) -> OptValue {
    match (opt.kind, v) {
        (OptKind::Int | OptKind::Fixed, OptValue::Number(n)) => {
            let n = nearest(opt, *n);
            OptValue::Number(if opt.kind == OptKind::Int { n.round() } else { n })
        }
        (OptKind::Bool, OptValue::Number(n)) => OptValue::Bool(*n != 0.0),
        _ => v.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(constraint: Constraint) -> ScanOption {
        ScanOption {
            index: 1,
            name: "resolution".into(),
            title: String::new(),
            desc: String::new(),
            group: String::new(),
            kind: OptKind::Int,
            unit: OptUnit::Dpi,
            constraint,
            value: Some(OptValue::Number(300.0)),
            active: true,
            settable: true,
            advanced: false,
            array: false,
            automatic: false,
        }
    }

    #[test]
    fn nearest_values() {
        let list = opt(Constraint::Numbers {
            values: vec![75.0, 150.0, 300.0, 600.0],
        });
        assert_eq!(nearest(&list, 100.0), 75.0);
        assert_eq!(nearest(&list, 130.0), 150.0);
        let range = opt(Constraint::Range {
            min: 50.0,
            max: 1200.0,
            step: 25.0,
        });
        assert_eq!(nearest(&range, 101.0), 100.0);
        assert_eq!(nearest(&range, 5000.0), 1200.0);
        assert_eq!(coerce(&range, &OptValue::Number(312.4)), OptValue::Number(300.0));
    }
}
