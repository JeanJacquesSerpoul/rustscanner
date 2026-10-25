//! Accès à libsane (chargée dynamiquement : l'application démarre même sans SANE).
//!
//! Les appels doivent tous venir d'un même fil (voir `scanner.rs`), sauf
//! `sane_cancel`, que la norme autorise depuis un autre fil.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;

use libloading::Library;
use rustscanner_shared::{Constraint, DeviceInfo, OptKind, OptUnit, OptValue, ScanOption};

use crate::error::{Error, Result};

/// Scanner ouvert (pointeur opaque de SANE).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Handle(pub(crate) *mut c_void);

// le pointeur n'est qu'un jeton transmis à libsane
unsafe impl Send for Handle {}

type Word = c_int;

const STATUS_GOOD: Word = 0;
pub const STATUS_CANCELLED: Word = 2;
pub const STATUS_EOF: Word = 5;
pub const STATUS_NO_DOCS: Word = 7;

const TYPE_BOOL: Word = 0;
const TYPE_INT: Word = 1;
const TYPE_FIXED: Word = 2;
const TYPE_STRING: Word = 3;
const TYPE_BUTTON: Word = 4;
const TYPE_GROUP: Word = 5;

const CONSTRAINT_RANGE: Word = 1;
const CONSTRAINT_WORD_LIST: Word = 2;
const CONSTRAINT_STRING_LIST: Word = 3;

const CAP_SOFT_SELECT: Word = 1;
const CAP_AUTOMATIC: Word = 16;
const CAP_INACTIVE: Word = 32;
const CAP_ADVANCED: Word = 64;

const ACTION_GET: Word = 0;
const ACTION_SET: Word = 1;
const ACTION_AUTO: Word = 2;

pub const INFO_RELOAD_OPTIONS: Word = 2;

pub const FRAME_GRAY: Word = 0;
pub const FRAME_RGB: Word = 1;
pub const FRAME_RED: Word = 2;
pub const FRAME_GREEN: Word = 3;
pub const FRAME_BLUE: Word = 4;

#[repr(C)]
struct RawDevice {
    name: *const c_char,
    vendor: *const c_char,
    model: *const c_char,
    kind: *const c_char,
}

#[repr(C)]
struct RawRange {
    min: Word,
    max: Word,
    quant: Word,
}

#[repr(C)]
struct RawDescriptor {
    name: *const c_char,
    title: *const c_char,
    desc: *const c_char,
    kind: Word,
    unit: Word,
    size: Word,
    cap: Word,
    constraint_type: Word,
    constraint: *const c_void,
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct Parameters {
    pub format: Word,
    pub last_frame: Word,
    pub bytes_per_line: Word,
    pub pixels_per_line: Word,
    pub lines: Word,
    pub depth: Word,
}

type AuthCallback = Option<extern "C" fn(*const c_char, *mut c_char, *mut c_char)>;

struct Api {
    init: unsafe extern "C" fn(*mut Word, AuthCallback) -> Word,
    exit: unsafe extern "C" fn(),
    get_devices: unsafe extern "C" fn(*mut *const *const RawDevice, Word) -> Word,
    open: unsafe extern "C" fn(*const c_char, *mut *mut c_void) -> Word,
    close: unsafe extern "C" fn(*mut c_void),
    get_option_descriptor: unsafe extern "C" fn(*mut c_void, Word) -> *const RawDescriptor,
    control_option: unsafe extern "C" fn(*mut c_void, Word, Word, *mut c_void, *mut Word) -> Word,
    get_parameters: unsafe extern "C" fn(*mut c_void, *mut Parameters) -> Word,
    start: unsafe extern "C" fn(*mut c_void) -> Word,
    read: unsafe extern "C" fn(*mut c_void, *mut u8, Word, *mut Word) -> Word,
    cancel: unsafe extern "C" fn(*mut c_void),
    strstatus: unsafe extern "C" fn(Word) -> *const c_char,
}

/// Bibliothèque SANE initialisée.
pub struct Sane {
    api: Api,
    pub version: String,
    _lib: Library,
}

// la bibliothèque n'est utilisée que depuis le fil du scanner ; seule
// `cancel` (autorisée par la norme) est appelée depuis un autre fil
unsafe impl Send for Sane {}
unsafe impl Sync for Sane {}

fn fixed_to_f64(w: Word) -> f64 {
    w as f64 / 65536.0
}

fn f64_to_fixed(v: f64) -> Word {
    (v * 65536.0).round() as Word
}

unsafe fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

impl Sane {
    pub fn load() -> Result<Sane> {
        let candidates = ["libsane.so.1", "libsane.so", "libsane.1.dylib", "libsane.dylib"];
        let mut last = String::new();
        for name in candidates {
            match unsafe { Library::new(name) } {
                Ok(lib) => return Self::from_library(lib),
                Err(e) => last = e.to_string(),
            }
        }
        Err(Error::NoSane(last))
    }

    fn from_library(lib: Library) -> Result<Sane> {
        macro_rules! sym {
            ($name:literal) => {
                *unsafe { lib.get($name) }.map_err(|e| Error::NoSane(e.to_string()))?
            };
        }
        let api = Api {
            init: sym!(b"sane_init\0"),
            exit: sym!(b"sane_exit\0"),
            get_devices: sym!(b"sane_get_devices\0"),
            open: sym!(b"sane_open\0"),
            close: sym!(b"sane_close\0"),
            get_option_descriptor: sym!(b"sane_get_option_descriptor\0"),
            control_option: sym!(b"sane_control_option\0"),
            get_parameters: sym!(b"sane_get_parameters\0"),
            start: sym!(b"sane_start\0"),
            read: sym!(b"sane_read\0"),
            cancel: sym!(b"sane_cancel\0"),
            strstatus: sym!(b"sane_strstatus\0"),
        };
        let mut code: Word = 0;
        let status = unsafe { (api.init)(&mut code, None) };
        let mut sane = Sane {
            api,
            version: String::new(),
            _lib: lib,
        };
        sane.check(status)?;
        sane.version = format!("{}.{}.{}", (code >> 24) & 0xff, (code >> 16) & 0xff, code & 0xffff);
        Ok(sane)
    }

    fn status_text(&self, status: Word) -> String {
        unsafe { cstr((self.api.strstatus)(status)) }
    }

    pub fn check(&self, status: Word) -> Result<()> {
        if status == STATUS_GOOD {
            Ok(())
        } else {
            Err(Error::Sane {
                status,
                message: self.status_text(status),
            })
        }
    }

    pub fn devices(&self, local_only: bool) -> Result<Vec<DeviceInfo>> {
        let mut list: *const *const RawDevice = ptr::null();
        self.check(unsafe { (self.api.get_devices)(&mut list, local_only as Word) })?;
        let mut out = Vec::new();
        if list.is_null() {
            return Ok(out);
        }
        let mut i = 0;
        loop {
            let dev = unsafe { *list.add(i) };
            if dev.is_null() {
                break;
            }
            let d = unsafe { &*dev };
            out.push(unsafe {
                DeviceInfo {
                    name: cstr(d.name),
                    vendor: cstr(d.vendor),
                    model: cstr(d.model),
                    kind: cstr(d.kind),
                }
            });
            i += 1;
        }
        Ok(out)
    }

    pub fn open(&self, name: &str) -> Result<Handle> {
        let c = CString::new(name).map_err(|_| Error::Other("nom invalide".into()))?;
        let mut h: *mut c_void = ptr::null_mut();
        self.check(unsafe { (self.api.open)(c.as_ptr(), &mut h) })?;
        Ok(Handle(h))
    }

    pub fn close(&self, h: Handle) {
        unsafe { (self.api.close)(h.0) }
    }

    fn descriptor(&self, h: Handle, index: Word) -> Option<&RawDescriptor> {
        let p = unsafe { (self.api.get_option_descriptor)(h.0, index) };
        if p.is_null() { None } else { Some(unsafe { &*p }) }
    }

    fn option_count(&self, h: Handle) -> Word {
        let mut n: Word = 0;
        let status = unsafe {
            (self.api.control_option)(h.0, 0, ACTION_GET, &mut n as *mut Word as *mut c_void, ptr::null_mut())
        };
        if status == STATUS_GOOD { n } else { 0 }
    }

    /// Liste toutes les options (groupes compris dans `group`).
    pub fn options(&self, h: Handle, translate: &dyn Fn(&str) -> String) -> Vec<ScanOption> {
        let mut out = Vec::new();
        let mut group = String::new();
        for i in 1..self.option_count(h) {
            let Some(d) = self.descriptor(h, i) else { continue };
            let title = unsafe { cstr(d.title) };
            if d.kind == TYPE_GROUP {
                group = translate(&title);
                continue;
            }
            let kind = match d.kind {
                TYPE_BOOL => OptKind::Bool,
                TYPE_INT => OptKind::Int,
                TYPE_FIXED => OptKind::Fixed,
                TYPE_STRING => OptKind::Str,
                TYPE_BUTTON => OptKind::Button,
                _ => continue,
            };
            let unit = match d.unit {
                1 => OptUnit::Pixel,
                2 => OptUnit::Bit,
                3 => OptUnit::Mm,
                4 => OptUnit::Dpi,
                5 => OptUnit::Percent,
                6 => OptUnit::Microsecond,
                _ => OptUnit::None,
            };
            let number = |w: Word| if kind == OptKind::Fixed { fixed_to_f64(w) } else { w as f64 };
            let constraint = unsafe {
                match d.constraint_type {
                    CONSTRAINT_RANGE if !d.constraint.is_null() => {
                        let r = &*(d.constraint as *const RawRange);
                        Constraint::Range {
                            min: number(r.min),
                            max: number(r.max),
                            step: number(r.quant),
                        }
                    }
                    CONSTRAINT_WORD_LIST if !d.constraint.is_null() => {
                        let w = d.constraint as *const Word;
                        let n = *w as usize;
                        Constraint::Numbers {
                            values: (1..=n).map(|k| number(*w.add(k))).collect(),
                        }
                    }
                    CONSTRAINT_STRING_LIST if !d.constraint.is_null() => {
                        let s = d.constraint as *const *const c_char;
                        let mut values = Vec::new();
                        let mut k = 0;
                        while !(*s.add(k)).is_null() {
                            values.push(cstr(*s.add(k)));
                            k += 1;
                        }
                        Constraint::Strings { values }
                    }
                    _ => Constraint::None,
                }
            };
            let word = std::mem::size_of::<Word>() as Word;
            let array = matches!(kind, OptKind::Int | OptKind::Fixed) && d.size > word;
            let active = d.cap & CAP_INACTIVE == 0;
            let value = if active && !array && kind != OptKind::Button {
                self.get_value(h, i, d).ok()
            } else {
                None
            };
            out.push(ScanOption {
                index: i,
                name: unsafe { cstr(d.name) },
                desc: translate(&unsafe { cstr(d.desc) }),
                title: translate(&title),
                group: group.clone(),
                kind,
                unit,
                constraint,
                value,
                active,
                settable: d.cap & CAP_SOFT_SELECT != 0,
                advanced: d.cap & CAP_ADVANCED != 0,
                array,
                automatic: d.cap & CAP_AUTOMATIC != 0,
            });
        }
        out
    }

    fn get_value(&self, h: Handle, index: Word, d: &RawDescriptor) -> Result<OptValue> {
        let size = d.size.max(4) as usize;
        let mut buf = vec![0u8; size + 1];
        self.check(unsafe {
            (self.api.control_option)(h.0, index, ACTION_GET, buf.as_mut_ptr() as *mut c_void, ptr::null_mut())
        })?;
        let word = Word::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]);
        Ok(match d.kind {
            TYPE_BOOL => OptValue::Bool(word != 0),
            TYPE_INT => OptValue::Number(word as f64),
            TYPE_FIXED => OptValue::Number(fixed_to_f64(word)),
            _ => {
                let end = buf.iter().position(|&b| b == 0).unwrap_or(size);
                OptValue::Text(String::from_utf8_lossy(&buf[..end]).into_owned())
            }
        })
    }

    /// Modifie une option ; renvoie les indicateurs SANE (`INFO_*`).
    pub fn set_value(&self, h: Handle, index: Word, value: &OptValue) -> Result<Word> {
        let d = self
            .descriptor(h, index)
            .ok_or_else(|| Error::Other(format!("option {index} inconnue")))?;
        let mut info: Word = 0;
        let status = match (d.kind, value) {
            (TYPE_BUTTON, _) => unsafe {
                (self.api.control_option)(h.0, index, ACTION_SET, ptr::null_mut(), &mut info)
            },
            (TYPE_STRING, OptValue::Text(s)) => {
                let mut buf = vec![0u8; (d.size as usize).max(s.len() + 1)];
                let n = s.len().min(buf.len() - 1);
                buf[..n].copy_from_slice(&s.as_bytes()[..n]);
                unsafe {
                    (self.api.control_option)(h.0, index, ACTION_SET, buf.as_mut_ptr() as *mut c_void, &mut info)
                }
            }
            (kind, v) => {
                let mut w: Word = match (kind, v) {
                    (TYPE_BOOL, OptValue::Bool(b)) => *b as Word,
                    (TYPE_INT, OptValue::Number(n)) => n.round() as Word,
                    (TYPE_FIXED, OptValue::Number(n)) => f64_to_fixed(*n),
                    (TYPE_BOOL, OptValue::Number(n)) => (*n != 0.0) as Word,
                    _ => return Err(Error::Other("type de valeur incompatible".into())),
                };
                unsafe {
                    (self.api.control_option)(h.0, index, ACTION_SET, &mut w as *mut Word as *mut c_void, &mut info)
                }
            }
        };
        self.check(status)?;
        Ok(info)
    }

    pub fn set_auto(&self, h: Handle, index: Word) -> Result<Word> {
        let mut info: Word = 0;
        self.check(unsafe { (self.api.control_option)(h.0, index, ACTION_AUTO, ptr::null_mut(), &mut info) })?;
        Ok(info)
    }

    pub fn start(&self, h: Handle) -> Word {
        unsafe { (self.api.start)(h.0) }
    }

    pub fn parameters(&self, h: Handle) -> Result<Parameters> {
        let mut p = Parameters::default();
        self.check(unsafe { (self.api.get_parameters)(h.0, &mut p) })?;
        Ok(p)
    }

    /// Lit un bloc : `Ok(0)` en fin de trame.
    pub fn read(&self, h: Handle, buf: &mut [u8]) -> Result<usize> {
        let mut len: Word = 0;
        let status = unsafe { (self.api.read)(h.0, buf.as_mut_ptr(), buf.len() as Word, &mut len) };
        if status == STATUS_EOF {
            return Ok(0);
        }
        self.check(status)?;
        Ok(len.max(0) as usize)
    }

    pub fn cancel(&self, h: Handle) {
        unsafe { (self.api.cancel)(h.0) }
    }
}

impl Drop for Sane {
    fn drop(&mut self) {
        unsafe { (self.api.exit)() }
    }
}
