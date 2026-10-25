//! Accès aux scanners sous Windows par WIA 2.0 (Windows Image Acquisition).
//!
//! Les scanners sont présentés comme par `sane.rs` : des options portant les
//! noms normalisés par SANE (`source`, `mode`, `resolution`, `tl-x`…), que le
//! reste de l'application manipule sans connaître WIA. Les réglages sont
//! conservés ici et appliqués à l'élément WIA choisi (vitre, chargeur, film)
//! au début de chaque numérisation.
//!
//! Tous les appels viennent du fil du scanner, initialisé en COM multithread ;
//! les rappels de transfert de WIA arrivent sur d'autres fils de ce même
//! appartement.

use std::ffi::c_void;
use std::sync::{Arc, Mutex};

use image::{DynamicImage, ImageFormat};
use rustscanner_shared::{Constraint, DeviceInfo, OptKind, OptUnit, OptValue, ScanOption, names, tr};
use windows::Win32::Devices::ImageAcquisition::*;
use windows::Win32::Foundation::{E_ABORT, E_ACCESSDENIED, E_OUTOFMEMORY, HGLOBAL, S_FALSE};
use windows::Win32::System::Com::StructuredStorage::{
    CreateStreamOnHGlobal, PROPSPEC, PROPSPEC_0, PROPVARIANT, PRSPEC_PROPID, PropVariantClear,
};
use windows::Win32::System::Com::{
    CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemAlloc, IStream, STREAM_SEEK_SET,
    TYMED_FILE,
};
use windows::Win32::System::Variant::{VT_BSTR, VT_CLSID, VT_I4, VT_LPWSTR, VT_VECTOR};
use windows::core::{BSTR, GUID, Interface, implement};

use crate::error::{Error, Result, STATUS_CANCELLED, STATUS_NO_DOCS};
use crate::scanner::Sink;

/// Scanner ouvert : pointeur vers son état (`Device`), propre au fil du scanner.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Handle(pub(crate) *mut c_void);

// le pointeur n'est déréférencé que par le fil du scanner
unsafe impl Send for Handle {}

const STI_DEVICE_TYPE_SCANNER: i32 = 1;

// numéros des options présentées à l'interface
const OPT_SOURCE: i32 = 1;
const OPT_MODE: i32 = 2;
const OPT_RESOLUTION: i32 = 3;
const OPT_TL_X: i32 = 4;
const OPT_TL_Y: i32 = 5;
const OPT_BR_X: i32 = 6;
const OPT_BR_Y: i32 = 7;
const OPT_PREVIEW: i32 = 8;
const OPT_BRIGHTNESS: i32 = 9;
const OPT_CONTRAST: i32 = 10;
const OPT_THRESHOLD: i32 = 11;

/// Modes proposés : nom (au sens de SANE), type de données et profondeur WIA.
const MODES: [(&str, u32, i32); 3] = [
    ("Color", WIA_DATA_COLOR, 24),
    ("Gray", WIA_DATA_GRAYSCALE, 8),
    ("Lineart", WIA_DATA_THRESHOLD, 1),
];

thread_local! {
    static COM: () = {
        // le fil du scanner ne sert qu'à WIA : appartement multithread, jamais libéré
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    };
}

fn manager() -> Result<IWiaDevMgr2> {
    COM.with(|_| ());
    unsafe { CoCreateInstance(&WiaDevMgr2, None, CLSCTX_LOCAL_SERVER) }.map_err(|e| Error::NoSane(e.message()))
}

/// Erreur WIA ramenée aux états de SANE (bourrage, chargeur vide…).
fn wia_error(e: windows::core::Error) -> Error {
    let code = e.code();
    let status = match code {
        WIA_ERROR_PAPER_EMPTY => STATUS_NO_DOCS,
        WIA_ERROR_PAPER_JAM | WIA_ERROR_MULTI_FEED | WIA_ERROR_PAPER_PROBLEM => 6,
        WIA_ERROR_BUSY | WIA_ERROR_DEVICE_LOCKED | WIA_ERROR_WARMING_UP => 3,
        WIA_ERROR_COVER_OPEN => 8,
        WIA_ERROR_OFFLINE | WIA_ERROR_DEVICE_COMMUNICATION | WIA_S_NO_DEVICE_AVAILABLE => 9,
        E_OUTOFMEMORY => 10,
        E_ACCESSDENIED => 11,
        E_ABORT => STATUS_CANCELLED,
        _ => 4,
    };
    let text = e.message();
    let message = if text.trim().is_empty() {
        format!("WIA 0x{:08X}", code.0 as u32)
    } else {
        format!("{} (0x{:08X})", text.trim(), code.0 as u32)
    };
    Error::Sane { status, message }
}

// ---- propriétés WIA ----

fn spec(id: u32) -> PROPSPEC {
    PROPSPEC {
        ulKind: PRSPEC_PROPID,
        Anonymous: PROPSPEC_0 { propid: id },
    }
}

fn read<T>(props: &IWiaPropertyStorage, id: u32, convert: impl FnOnce(&PROPVARIANT) -> Option<T>) -> Option<T> {
    let spec = spec(id);
    let mut pv = PROPVARIANT::default();
    unsafe { props.ReadMultiple(1, &spec, &mut pv) }.ok()?;
    let value = convert(&pv);
    let _ = unsafe { PropVariantClear(&mut pv) };
    value
}

fn read_int(props: &IWiaPropertyStorage, id: u32) -> Option<i32> {
    read(props, id, |pv| unsafe {
        let v = &pv.Anonymous.Anonymous;
        // entiers signés ou non (VT_I4, VT_UI4, VT_INT…) : même représentation
        (v.vt.0 != 0 && v.vt.0 & VT_VECTOR.0 == 0 && v.vt != VT_BSTR && v.vt != VT_CLSID).then(|| v.Anonymous.lVal)
    })
}

fn read_text(props: &IWiaPropertyStorage, id: u32) -> Option<String> {
    read(props, id, |pv| unsafe {
        let v = &pv.Anonymous.Anonymous;
        match v.vt {
            VT_BSTR => Some(v.Anonymous.bstrVal.to_string()),
            VT_LPWSTR => v.Anonymous.pwszVal.to_string().ok(),
            _ => None,
        }
    })
}

fn write(props: &IWiaPropertyStorage, id: u32, pv: &PROPVARIANT) -> Result<()> {
    unsafe { props.WriteMultiple(1, &spec(id), pv, WIA_IPA_FIRST) }.map_err(wia_error)
}

fn write_int(props: &IWiaPropertyStorage, id: u32, value: i32) -> Result<()> {
    let mut pv = PROPVARIANT::default();
    unsafe {
        let v = &mut *pv.Anonymous.Anonymous;
        v.vt = VT_I4;
        v.Anonymous.lVal = value;
    }
    write(props, id, &pv)
}

fn write_guid(props: &IWiaPropertyStorage, id: u32, value: GUID) -> Result<()> {
    // le GUID doit venir de l'allocateur COM (un pointeur sur la pile corrompt
    // le tas dans le mandataire WIA) ; PropVariantClear le libère ensuite
    let mut pv = PROPVARIANT::default();
    unsafe {
        let guid = CoTaskMemAlloc(size_of::<GUID>()).cast::<GUID>();
        if guid.is_null() {
            return Err(Error::Sane {
                status: 10,
                message: "CoTaskMemAlloc".into(),
            });
        }
        guid.write(value);
        let v = &mut *pv.Anonymous.Anonymous;
        v.vt = VT_CLSID;
        v.Anonymous.puuid = guid;
    }
    let result = write(props, id, &pv);
    let _ = unsafe { PropVariantClear(&mut pv) };
    result
}

/// Valeurs admises d'une propriété entière.
#[derive(Clone, Debug, PartialEq)]
enum Valid {
    Range { min: i32, max: i32, step: i32 },
    List(Vec<i32>),
    /// Combinaison de drapeaux (masque des drapeaux possibles).
    Flags(i32),
    Any,
}

struct Attr {
    writable: bool,
    valid: Valid,
}

fn attributes(props: &IWiaPropertyStorage, id: u32) -> Option<Attr> {
    let spec = spec(id);
    let mut flags = 0u32;
    let mut pv = PROPVARIANT::default();
    unsafe { props.GetPropertyAttributes(1, &spec, &mut flags, &mut pv) }.ok()?;
    if flags == 0 {
        return None;
    }
    let elems: Vec<i32> = unsafe {
        let v = &pv.Anonymous.Anonymous;
        let cal = &v.Anonymous.cal;
        if v.vt.0 & VT_VECTOR.0 != 0 && v.vt.0 & !VT_VECTOR.0 != VT_CLSID.0 && !cal.pElems.is_null() {
            std::slice::from_raw_parts(cal.pElems, cal.cElems as usize).to_vec()
        } else {
            Vec::new()
        }
    };
    let _ = unsafe { PropVariantClear(&mut pv) };
    // dispositions des tableaux WIA : intervalle (min, nominal, max, pas),
    // liste (nombre, nominal, valeurs…), drapeaux (nominal, masque)
    let valid = if flags & WIA_PROP_RANGE != 0 && elems.len() >= 4 {
        Valid::Range {
            min: elems[0],
            max: elems[2],
            step: elems[3],
        }
    } else if flags & WIA_PROP_LIST != 0 && elems.len() >= 2 {
        Valid::List(elems[2..].to_vec())
    } else if flags & WIA_PROP_FLAG != 0 && elems.len() >= 2 {
        Valid::Flags(elems[1])
    } else {
        Valid::Any
    };
    Some(Attr {
        writable: flags & WIA_PROP_WRITE != 0,
        valid,
    })
}

impl Valid {
    fn constraint(&self) -> Constraint {
        match self {
            Valid::Range { min, max, step } => Constraint::Range {
                min: *min as f64,
                max: *max as f64,
                step: *step as f64,
            },
            Valid::List(values) => Constraint::Numbers {
                values: values.iter().map(|v| *v as f64).collect(),
            },
            _ => Constraint::None,
        }
    }

    fn clamp(&self, v: i32) -> i32 {
        match self {
            Valid::Range { min, max, .. } if min <= max => v.clamp(*min, *max),
            Valid::List(values) => values.iter().copied().min_by_key(|x| (x - v).abs()).unwrap_or(v),
            _ => v,
        }
    }
}

/// Écrit une valeur ramenée aux bornes que le pilote annonce à cet instant.
fn write_clamped(props: &IWiaPropertyStorage, id: u32, value: i32) -> Result<()> {
    let value = attributes(props, id).map(|a| a.valid.clamp(value)).unwrap_or(value);
    write_int(props, id, value)
}

/// Écriture facultative : le pilote peut ignorer ou refuser certaines propriétés.
fn try_write(what: &str, r: Result<()>) {
    if let Err(e) = r {
        log::warn!("WIA {what} : {e}");
    }
}

// ---- scanner ouvert ----

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Flatbed,
    Feeder,
    Film,
}

struct Source {
    name: &'static str,
    kind: Kind,
    duplex: bool,
    item: IWiaItem2,
}

/// Possibilités de la source choisie.
struct Caps {
    modes: Vec<&'static str>,
    resolution: Constraint,
    /// Surface numérisable (mm).
    bed: (f64, f64),
    brightness: Option<Constraint>,
    contrast: Option<Constraint>,
    threshold: Option<Constraint>,
}

/// Réglages courants, appliqués à l'élément WIA au moment de numériser.
#[derive(Clone)]
struct Config {
    mode: &'static str,
    resolution: f64,
    /// tl-x, tl-y, br-x, br-y (mm).
    area: [f64; 4],
    preview: bool,
    brightness: f64,
    contrast: f64,
    threshold: f64,
}

struct Device {
    root: IWiaItem2,
    sources: Vec<Source>,
    current: usize,
    caps: Caps,
    cfg: Config,
}

fn props_of(item: &IWiaItem2) -> Result<IWiaPropertyStorage> {
    item.cast().map_err(wia_error)
}

fn mm(thousandths_inch: i32) -> f64 {
    thousandths_inch as f64 * 25.4 / 1000.0
}

fn sources(root: &IWiaItem2) -> Result<Vec<Source>> {
    let list = unsafe { root.EnumChildItems(None) }.map_err(wia_error)?;
    let mut items = Vec::new();
    loop {
        let mut item = None;
        let mut fetched = 0u32;
        if unsafe { list.Next(1, &mut item, &mut fetched) }.is_err() || fetched == 0 {
            break;
        }
        if let Some(item) = item {
            items.push(item);
        }
    }
    let mut out = Vec::new();
    for item in &items {
        let category = unsafe { item.GetItemCategory() }.unwrap_or_default();
        match category {
            WIA_CATEGORY_FLATBED => out.push(Source {
                name: "Flatbed",
                kind: Kind::Flatbed,
                duplex: false,
                item: item.clone(),
            }),
            WIA_CATEGORY_FEEDER => {
                out.push(Source {
                    name: "ADF",
                    kind: Kind::Feeder,
                    duplex: false,
                    item: item.clone(),
                });
                let props = props_of(item)?;
                let caps = read_int(&props, WIA_DPS_DOCUMENT_HANDLING_CAPABILITIES).unwrap_or(0);
                let select = attributes(&props, WIA_IPS_DOCUMENT_HANDLING_SELECT);
                let duplex = caps & DUPLEX as i32 != 0
                    || matches!(select, Some(Attr { valid: Valid::Flags(mask), .. }) if mask & DUPLEX as i32 != 0);
                if duplex {
                    out.push(Source {
                        name: "ADF Duplex",
                        kind: Kind::Feeder,
                        duplex: true,
                        item: item.clone(),
                    });
                }
            }
            WIA_CATEGORY_FILM => out.push(Source {
                name: "Transparency",
                kind: Kind::Film,
                duplex: false,
                item: item.clone(),
            }),
            _ => {}
        }
    }
    // pilote sans catégories reconnues : le premier élément sert de vitre
    if out.is_empty()
        && let Some(item) = items.into_iter().next()
    {
        out.push(Source {
            name: "Flatbed",
            kind: Kind::Flatbed,
            duplex: false,
            item,
        });
    }
    out.sort_by_key(|s| (s.kind, s.duplex));
    Ok(out)
}

fn int_option(props: &IWiaPropertyStorage, id: u32) -> Option<(Constraint, f64)> {
    let attr = attributes(props, id)?;
    let current = read_int(props, id)?;
    (attr.writable && matches!(attr.valid, Valid::Range { .. } | Valid::List(_)))
        .then(|| (attr.valid.constraint(), current as f64))
}

impl Device {
    fn source(&self) -> &Source {
        &self.sources[self.current]
    }

    /// Choisit une source : relit ses possibilités et reprend ses réglages par
    /// défaut (zone, réglages fins), en gardant le mode et la résolution si possible.
    fn select(&mut self, index: usize, keep: Option<&Config>) -> Result<()> {
        self.current = index;
        let props = props_of(&self.source().item)?;
        let datatypes = match attributes(&props, WIA_IPA_DATATYPE) {
            Some(Attr { valid: Valid::List(v), .. }) => v,
            _ => read_int(&props, WIA_IPA_DATATYPE).into_iter().collect(),
        };
        let mut modes: Vec<&'static str> = MODES
            .iter()
            .filter(|(_, dt, _)| datatypes.contains(&(*dt as i32)))
            .map(|(name, _, _)| *name)
            .collect();
        if modes.is_empty() {
            modes.push(MODES[0].0);
        }
        let xres = read_int(&props, WIA_IPS_XRES).unwrap_or(300).max(1);
        let resolution = match attributes(&props, WIA_IPS_XRES) {
            Some(Attr { valid: v @ (Valid::Range { .. } | Valid::List(_)), .. }) => v.constraint(),
            _ => Constraint::Numbers {
                values: vec![xres as f64],
            },
        };
        // surface : propriétés WIA 2.0 de l'élément, sinon taille de la vitre
        // (WIA 1), sinon étendue maximale à la résolution courante
        let root = props_of(&self.root).ok();
        let size = |item_prop, root_prop, extent_prop| {
            read_int(&props, item_prop)
                .or_else(|| root.as_ref().and_then(|r| read_int(r, root_prop)))
                .filter(|v| *v > 0)
                .map(mm)
                .or_else(|| match attributes(&props, extent_prop)?.valid {
                    Valid::Range { max, .. } if max > 0 => Some(max as f64 / xres as f64 * 25.4),
                    _ => None,
                })
        };
        let bed = (
            size(WIA_IPS_MAX_HORIZONTAL_SIZE, WIA_DPS_HORIZONTAL_BED_SIZE, WIA_IPS_XEXTENT).unwrap_or(215.9),
            size(WIA_IPS_MAX_VERTICAL_SIZE, WIA_DPS_VERTICAL_BED_SIZE, WIA_IPS_YEXTENT).unwrap_or(297.0),
        );
        // zone par défaut du pilote (pour le chargeur : souvent A4 ou Letter)
        let px = |id| read_int(&props, id).map(|v| v as f64 / xres as f64 * 25.4);
        let (x, y) = (px(WIA_IPS_XPOS).unwrap_or(0.0), px(WIA_IPS_YPOS).unwrap_or(0.0));
        let (w, h) = (
            px(WIA_IPS_XEXTENT).filter(|v| *v > 0.0).unwrap_or(bed.0),
            px(WIA_IPS_YEXTENT).filter(|v| *v > 0.0).unwrap_or(bed.1),
        );
        let area = [
            x.clamp(0.0, bed.0),
            y.clamp(0.0, bed.1),
            (x + w).clamp(0.0, bed.0),
            (y + h).clamp(0.0, bed.1),
        ];
        let brightness = int_option(&props, WIA_IPS_BRIGHTNESS);
        let contrast = int_option(&props, WIA_IPS_CONTRAST);
        let threshold = int_option(&props, WIA_IPS_THRESHOLD);
        let current_mode = read_int(&props, WIA_IPA_DATATYPE)
            .and_then(|dt| MODES.iter().find(|(_, d, _)| *d as i32 == dt))
            .map(|(name, _, _)| *name)
            .filter(|name| modes.contains(name))
            .unwrap_or(modes[0]);
        let mode = keep.map(|k| k.mode).filter(|m| modes.contains(m)).unwrap_or(current_mode);
        let resolution_value = keep.map(|k| k.resolution).unwrap_or(xres as f64);
        self.cfg = Config {
            mode,
            resolution: crate::scanner::nearest(&number_option(OPT_RESOLUTION, &resolution), resolution_value),
            area,
            preview: false,
            brightness: brightness.as_ref().map(|b| b.1).unwrap_or(0.0),
            contrast: contrast.as_ref().map(|c| c.1).unwrap_or(0.0),
            threshold: threshold.as_ref().map(|t| t.1).unwrap_or(0.0),
        };
        self.caps = Caps {
            modes,
            resolution,
            bed,
            brightness: brightness.map(|b| b.0),
            contrast: contrast.map(|c| c.0),
            threshold: threshold.map(|t| t.0),
        };
        Ok(())
    }

    fn options(&self) -> Vec<ScanOption> {
        let standard = tr!("Standard", "Standard");
        let geometry = tr!("Géométrie", "Geometry");
        let enhancement = tr!("Amélioration", "Enhancement");
        let mut out = vec![
            ScanOption {
                title: tr!("Source", "Source").into(),
                desc: tr!("Origine des documents", "Where the documents come from").into(),
                group: standard.into(),
                constraint: Constraint::Strings {
                    values: self.sources.iter().map(|s| s.name.to_string()).collect(),
                },
                value: Some(OptValue::Text(self.source().name.into())),
                ..text_option(OPT_SOURCE, names::SOURCE)
            },
            ScanOption {
                title: tr!("Mode", "Mode").into(),
                desc: tr!("Couleur, niveaux de gris ou noir et blanc", "Color, grayscale or black & white").into(),
                group: standard.into(),
                constraint: Constraint::Strings {
                    values: self.caps.modes.iter().map(|m| m.to_string()).collect(),
                },
                value: Some(OptValue::Text(self.cfg.mode.into())),
                ..text_option(OPT_MODE, names::MODE)
            },
            ScanOption {
                title: tr!("Résolution", "Resolution").into(),
                desc: tr!("Points par pouce", "Dots per inch").into(),
                group: standard.into(),
                unit: OptUnit::Dpi,
                value: Some(OptValue::Number(self.cfg.resolution)),
                ..number_option(OPT_RESOLUTION, &self.caps.resolution)
            },
            ScanOption {
                title: tr!("Aperçu", "Preview").into(),
                group: standard.into(),
                kind: OptKind::Bool,
                value: Some(OptValue::Bool(self.cfg.preview)),
                ..text_option(OPT_PREVIEW, names::PREVIEW)
            },
        ];
        let corners = [
            (OPT_TL_X, names::TL_X, tr!("Gauche", "Left"), self.caps.bed.0),
            (OPT_TL_Y, names::TL_Y, tr!("Haut", "Top"), self.caps.bed.1),
            (OPT_BR_X, names::BR_X, tr!("Droite", "Right"), self.caps.bed.0),
            (OPT_BR_Y, names::BR_Y, tr!("Bas", "Bottom"), self.caps.bed.1),
        ];
        for (i, (index, name, title, max)) in corners.into_iter().enumerate() {
            out.push(ScanOption {
                name: name.into(),
                title: title.into(),
                group: geometry.into(),
                kind: OptKind::Fixed,
                unit: OptUnit::Mm,
                value: Some(OptValue::Number(self.cfg.area[i])),
                ..number_option(index, &Constraint::Range { min: 0.0, max, step: 0.0 })
            });
        }
        let fine = [
            (OPT_BRIGHTNESS, "brightness", tr!("Luminosité", "Brightness"), &self.caps.brightness, self.cfg.brightness, true),
            (OPT_CONTRAST, "contrast", tr!("Contraste", "Contrast"), &self.caps.contrast, self.cfg.contrast, true),
            (
                OPT_THRESHOLD,
                "threshold",
                tr!("Seuil", "Threshold"),
                &self.caps.threshold,
                self.cfg.threshold,
                self.cfg.mode == "Lineart",
            ),
        ];
        for (index, name, title, constraint, value, active) in fine {
            if let Some(c) = constraint {
                out.push(ScanOption {
                    name: name.into(),
                    title: title.into(),
                    group: enhancement.into(),
                    value: Some(OptValue::Number(value)),
                    active,
                    ..number_option(index, c)
                });
            }
        }
        out.sort_by_key(|o| o.index);
        out
    }

    fn set(&mut self, index: i32, value: &OptValue) -> Result<()> {
        let invalid = || Error::Other(tr!("Valeur refusée par le scanner", "Value rejected by the scanner").into());
        let number = || value.as_f64().or_else(|| value.as_bool().map(|b| b as i32 as f64)).ok_or_else(invalid);
        match index {
            OPT_SOURCE => {
                let name = value.as_str().ok_or_else(invalid)?;
                let pos = self.sources.iter().position(|s| s.name.eq_ignore_ascii_case(name)).ok_or_else(invalid)?;
                if pos != self.current {
                    let keep = self.cfg.clone();
                    self.select(pos, Some(&keep))?;
                }
            }
            OPT_MODE => {
                let name = value.as_str().ok_or_else(invalid)?;
                self.cfg.mode = self.caps.modes.iter().copied().find(|m| m.eq_ignore_ascii_case(name)).ok_or_else(invalid)?;
            }
            OPT_RESOLUTION => self.cfg.resolution = number()?,
            OPT_TL_X | OPT_TL_Y | OPT_BR_X | OPT_BR_Y => {
                let i = (index - OPT_TL_X) as usize;
                let max = if i.is_multiple_of(2) { self.caps.bed.0 } else { self.caps.bed.1 };
                self.cfg.area[i] = number()?.clamp(0.0, max);
            }
            OPT_PREVIEW => self.cfg.preview = value.as_bool().or_else(|| value.as_f64().map(|n| n != 0.0)).ok_or_else(invalid)?,
            OPT_BRIGHTNESS => self.cfg.brightness = number()?,
            OPT_CONTRAST => self.cfg.contrast = number()?,
            OPT_THRESHOLD => self.cfg.threshold = number()?,
            _ => return Err(Error::Other(format!("option {index} inconnue"))),
        }
        Ok(())
    }

    /// Applique les réglages à l'élément WIA de la source choisie.
    fn apply(&self, props: &IWiaPropertyStorage) -> Result<()> {
        let src = self.source();
        let cfg = &self.cfg;
        write_guid(props, WIA_IPA_FORMAT, WiaImgFmt_BMP)?;
        write_int(props, WIA_IPA_TYMED, TYMED_FILE.0)?;
        let (_, datatype, depth) = MODES.iter().find(|(name, _, _)| *name == cfg.mode).copied().unwrap_or(MODES[0]);
        write_int(props, WIA_IPA_DATATYPE, datatype as i32)?;
        try_write("profondeur", write_int(props, WIA_IPA_DEPTH, depth));
        let res = cfg.resolution.round() as i32;
        write_clamped(props, WIA_IPS_XRES, res)?;
        try_write("résolution verticale", write_clamped(props, WIA_IPS_YRES, res));
        if src.kind == Kind::Feeder {
            let select = if src.duplex { DUPLEX } else { FRONT_ONLY };
            try_write("recto-verso", write_int(props, WIA_IPS_DOCUMENT_HANDLING_SELECT, select as i32));
            try_write("pages", write_int(props, WIA_IPS_PAGES, ALL_PAGES as i32));
            try_write("format de page", write_int(props, WIA_IPS_PAGE_SIZE, WIA_PAGE_CUSTOM as i32));
        }
        try_write(
            "aperçu",
            write_int(props, WIA_IPS_PREVIEW, (if cfg.preview { WIA_PREVIEW_SCAN } else { WIA_FINAL_SCAN }) as i32),
        );
        // zone en pixels à la résolution choisie : l'origine d'abord (le pilote
        // réduit alors l'étendue permise à ce qui reste), puis l'étendue, que
        // certains pilotes rognent si l'origine change après elle
        let px = |v: f64| (v / 25.4 * res as f64).round() as i32;
        let [x0, y0, x1, y1] = cfg.area;
        let (x, y) = (px(x0.min(x1)), px(y0.min(y1)));
        let (w, h) = (px((x1 - x0).abs()).max(1), px((y1 - y0).abs()).max(1));
        try_write("zone", write_clamped(props, WIA_IPS_XPOS, x));
        try_write("zone", write_clamped(props, WIA_IPS_YPOS, y));
        try_write("zone", write_clamped(props, WIA_IPS_XEXTENT, w));
        try_write("zone", write_clamped(props, WIA_IPS_YEXTENT, h));
        let fine = [
            (WIA_IPS_BRIGHTNESS, self.caps.brightness.is_some(), cfg.brightness),
            (WIA_IPS_CONTRAST, self.caps.contrast.is_some(), cfg.contrast),
            (WIA_IPS_THRESHOLD, self.caps.threshold.is_some() && cfg.mode == "Lineart", cfg.threshold),
        ];
        for (id, present, value) in fine {
            if present {
                try_write("réglage", write_clamped(props, id, value.round() as i32));
            }
        }
        Ok(())
    }
}

fn text_option(index: i32, name: &str) -> ScanOption {
    ScanOption {
        index,
        name: name.into(),
        title: String::new(),
        desc: String::new(),
        group: String::new(),
        kind: OptKind::Str,
        unit: OptUnit::None,
        constraint: Constraint::None,
        value: None,
        active: true,
        settable: true,
        advanced: false,
        array: false,
        automatic: false,
    }
}

fn number_option(index: i32, constraint: &Constraint) -> ScanOption {
    let name = match index {
        OPT_RESOLUTION => names::RESOLUTION,
        _ => "",
    };
    ScanOption {
        kind: OptKind::Int,
        constraint: constraint.clone(),
        ..text_option(index, name)
    }
}

// ---- transfert ----

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pixels {
    Color,
    Gray,
    Lineart,
}

/// État partagé entre la numérisation et les rappels de WIA.
struct Transfer {
    sink: Arc<Sink>,
    pixels: Pixels,
    stream: Mutex<Option<IStream>>,
    streams: Mutex<u32>,
    error: Mutex<Option<Error>>,
}

// les rappels arrivent sur les fils RPC de l'appartement multithread, où le
// flux COM peut être utilisé ; tout accès passe par les verrous
unsafe impl Send for Transfer {}
unsafe impl Sync for Transfer {}

impl Transfer {
    /// Page reçue en entier : décodage et remise au document.
    fn finish_page(&self) {
        let Some(stream) = self.stream.lock().unwrap().take() else { return };
        let decoded = read_stream(&stream).and_then(|bytes| {
            if bytes.is_empty() {
                // page vide (blanche, écartée par le pilote)
                Ok(None)
            } else {
                decode(&bytes, self.pixels).map(Some)
            }
        });
        match decoded {
            Ok(Some((image, bilevel))) => self.sink.page(image, bilevel),
            Ok(None) => {}
            Err(e) => {
                let mut slot = self.error.lock().unwrap();
                slot.get_or_insert(e);
            }
        }
    }
}

#[implement(IWiaTransferCallback)]
struct Callback {
    transfer: Arc<Transfer>,
}

impl IWiaTransferCallback_Impl for Callback_Impl {
    fn TransferCallback(&self, _flags: i32, params: *const WiaTransferParams) -> windows::core::Result<()> {
        let t = &self.transfer;
        if let Some(p) = unsafe { params.as_ref() } {
            match p.lMessage as u32 {
                WIA_TRANSFER_MSG_STATUS => t.sink.progress((p.lPercentComplete as f64 / 100.0).clamp(0.0, 1.0)),
                WIA_TRANSFER_MSG_END_OF_STREAM => t.finish_page(),
                _ => {}
            }
        }
        // S_FALSE demande à WIA d'interrompre le transfert
        if t.sink.cancelled() { Err(S_FALSE.into()) } else { Ok(()) }
    }

    fn GetNextStream(&self, _flags: i32, _name: &BSTR, _full_name: &BSTR) -> windows::core::Result<IStream> {
        let t = &self.transfer;
        if t.sink.cancelled() {
            return Err(E_ABORT.into());
        }
        t.finish_page();
        {
            // la première page est annoncée avant le lancement du transfert
            let mut n = t.streams.lock().unwrap();
            if *n > 0 {
                t.sink.started();
            }
            *n += 1;
        }
        let stream = unsafe { CreateStreamOnHGlobal(HGLOBAL::default(), true) }?;
        *t.stream.lock().unwrap() = Some(stream.clone());
        Ok(stream)
    }
}

fn read_stream(stream: &IStream) -> Result<Vec<u8>> {
    unsafe { stream.Seek(0, STREAM_SEEK_SET, None) }.map_err(wia_error)?;
    let mut out = Vec::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let mut n = 0u32;
        unsafe { stream.Read(buf.as_mut_ptr().cast(), buf.len() as u32, Some(&mut n)) }
            .ok()
            .map_err(wia_error)?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    Ok(out)
}

/// Complète un DIB sans en-tête de fichier (certains pilotes) en BMP lisible.
fn with_file_header(dib: &[u8]) -> Vec<u8> {
    let u32_at = |o: usize| dib.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0);
    let header = u32_at(0) as usize;
    let bits = dib.get(14..16).map(|b| u16::from_le_bytes([b[0], b[1]])).unwrap_or(24);
    let used = u32_at(32) as usize;
    let palette = if used > 0 { used } else if bits <= 8 { 1 << bits } else { 0 };
    // BI_BITFIELDS avec un en-tête court : trois masques après l'en-tête
    let masks = if u32_at(16) == 3 && header == 40 { 12 } else { 0 };
    let offset = 14 + header + masks + palette * 4;
    let mut out = Vec::with_capacity(dib.len() + 14);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((dib.len() + 14) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(offset as u32).to_le_bytes());
    out.extend_from_slice(dib);
    out
}

fn decode(bytes: &[u8], pixels: Pixels) -> Result<(DynamicImage, bool)> {
    let img = if bytes.starts_with(b"BM") {
        image::load_from_memory_with_format(bytes, ImageFormat::Bmp)?
    } else {
        image::load_from_memory_with_format(&with_file_header(bytes), ImageFormat::Bmp)?
    };
    Ok(match pixels {
        Pixels::Color => (DynamicImage::ImageRgb8(img.to_rgb8()), false),
        Pixels::Gray => (DynamicImage::ImageLuma8(img.to_luma8()), false),
        Pixels::Lineart => {
            let mut g = img.to_luma8();
            for p in g.pixels_mut() {
                p.0[0] = if p.0[0] > 127 { 255 } else { 0 };
            }
            (DynamicImage::ImageLuma8(g), true)
        }
    })
}

// ---- API commune avec `sane.rs` ----

/// SAFETY : `h` vient de `open`, n'est utilisé que par le fil du scanner, et pas après `close`.
unsafe fn device<'a>(h: Handle) -> &'a mut Device {
    unsafe { &mut *(h.0 as *mut Device) }
}

/// Accès à WIA ; les objets COM sont créés à la demande sur le fil du scanner.
pub struct Wia {
    pub version: String,
}

impl Wia {
    /// WIA fait partie de Windows : sa disponibilité se vérifie à la première recherche.
    pub fn load() -> Result<Wia> {
        Ok(Wia { version: "2.0".into() })
    }

    /// Scanners installés dans Windows (USB et réseau, WSD ou eSCL) ; les scanners
    /// partagés à distance par WIA ne sont listés que si `local_only` est faux.
    pub fn devices(&self, local_only: bool) -> Result<Vec<DeviceInfo>> {
        let mgr = manager()?;
        let flags = if local_only { WIA_DEVINFO_ENUM_LOCAL as i32 } else { 0 };
        let list = unsafe { mgr.EnumDeviceInfo(flags) }.map_err(wia_error)?;
        let mut out = Vec::new();
        loop {
            let mut props = None;
            let mut fetched = 0u32;
            if unsafe { list.Next(1, &mut props, &mut fetched) }.is_err() || fetched == 0 {
                break;
            }
            let Some(props) = props else { break };
            let kind = read_int(&props, WIA_DIP_DEV_TYPE).unwrap_or(0);
            if kind >> 16 != STI_DEVICE_TYPE_SCANNER {
                continue;
            }
            let Some(id) = read_text(&props, WIA_DIP_DEV_ID) else { continue };
            out.push(DeviceInfo {
                name: format!("wia:{id}"),
                vendor: read_text(&props, WIA_DIP_VEND_DESC).unwrap_or_default(),
                model: read_text(&props, WIA_DIP_DEV_NAME)
                    .or_else(|| read_text(&props, WIA_DIP_DEV_DESC))
                    .unwrap_or_else(|| id.clone()),
                kind: "scanner".into(),
            });
        }
        Ok(out)
    }

    pub fn open(&self, name: &str) -> Result<Handle> {
        let id = name.strip_prefix("wia:").unwrap_or(name);
        let root = unsafe { manager()?.CreateDevice(0, &BSTR::from(id)) }.map_err(wia_error)?;
        let sources = sources(&root)?;
        if sources.is_empty() {
            return Err(Error::Other(
                tr!("Ce périphérique ne propose aucune source de numérisation", "This device offers no scan source").into(),
            ));
        }
        let mut dev = Device {
            root,
            sources,
            current: 0,
            caps: Caps {
                modes: Vec::new(),
                resolution: Constraint::None,
                bed: (0.0, 0.0),
                brightness: None,
                contrast: None,
                threshold: None,
            },
            cfg: Config {
                mode: MODES[0].0,
                resolution: 300.0,
                area: [0.0; 4],
                preview: false,
                brightness: 0.0,
                contrast: 0.0,
                threshold: 0.0,
            },
        };
        dev.select(0, None)?;
        Ok(Handle(Box::into_raw(Box::new(dev)).cast()))
    }

    pub fn close(&self, h: Handle) {
        // SAFETY : `close` n'est appelé qu'une fois par scanner ouvert
        drop(unsafe { Box::from_raw(h.0 as *mut Device) });
    }

    /// Options dans le vocabulaire de SANE ; les titres sont déjà traduits.
    pub fn options(&self, h: Handle, _translate: &dyn Fn(&str) -> String) -> Vec<ScanOption> {
        unsafe { device(h) }.options()
    }

    pub fn set_value(&self, h: Handle, index: i32, value: &OptValue) -> Result<i32> {
        unsafe { device(h) }.set(index, value)?;
        Ok(0)
    }

    /// Aucune option WIA n'est annoncée comme automatique.
    pub fn set_auto(&self, _h: Handle, _index: i32) -> Result<i32> {
        Ok(0)
    }

    /// L'annulation passe par le rappel de transfert (voir `Sink::cancelled`).
    pub fn cancel(&self, _h: Handle) {}

    /// Numérise une page, ou toutes celles du chargeur en un seul transfert.
    pub fn scan(&self, h: Handle, _feeder: bool, sink: &Arc<Sink>) -> Result<()> {
        let dev = unsafe { device(h) };
        let item = dev.source().item.clone();
        dev.apply(&props_of(&item)?)?;
        let pixels = match dev.cfg.mode {
            "Gray" => Pixels::Gray,
            "Lineart" => Pixels::Lineart,
            _ => Pixels::Color,
        };
        let transfer = Arc::new(Transfer {
            sink: sink.clone(),
            pixels,
            stream: Mutex::new(None),
            streams: Mutex::new(0),
            error: Mutex::new(None),
        });
        let callback: IWiaTransferCallback = Callback {
            transfer: transfer.clone(),
        }
        .into();
        let wia: IWiaTransfer = item.cast().map_err(wia_error)?;
        sink.started();
        let result = unsafe { wia.Download(0, &callback) };
        // dernière page si le pilote n'a pas signalé la fin du flux
        transfer.finish_page();
        if sink.cancelled() {
            return Err(Error::cancelled());
        }
        if let Some(e) = transfer.error.lock().unwrap().take() {
            return Err(e);
        }
        result.map_err(wia_error)?;
        // transfert « réussi » sans aucune image : chargeur vide pour certains pilotes
        if sink.pages() == 0 && dev.source().kind == Kind::Feeder {
            return Err(Error::Sane {
                status: STATUS_NO_DOCS,
                message: "no documents".into(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_values() {
        let range = Valid::Range { min: 0, max: 100, step: 1 };
        assert_eq!(range.clamp(150), 100);
        assert_eq!(Valid::List(vec![75, 150, 300]).clamp(200), 150);
        assert_eq!(
            Valid::List(vec![75, 150]).constraint(),
            Constraint::Numbers { values: vec![75.0, 150.0] }
        );
    }

    #[test]
    fn dib_without_file_header() {
        // 2 × 1 pixels, 24 bits : un pixel rouge, un pixel bleu
        let mut dib = Vec::new();
        for v in [40u32, 2, 1] {
            dib.extend_from_slice(&v.to_le_bytes());
        }
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&24u16.to_le_bytes());
        dib.extend_from_slice(&[0; 24]);
        dib.extend_from_slice(&[0, 0, 255, 255, 0, 0, 0, 0]);
        let (img, bilevel) = decode(&dib, Pixels::Color).unwrap();
        assert!(!bilevel);
        let rgb = img.to_rgb8();
        assert_eq!(rgb.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(rgb.get_pixel(1, 0).0, [0, 0, 255]);
        let (img, bilevel) = decode(&dib, Pixels::Lineart).unwrap();
        assert!(bilevel);
        assert!(img.to_luma8().pixels().all(|p| p.0[0] == 0 || p.0[0] == 255));
    }
}
