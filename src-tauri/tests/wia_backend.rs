//! Scanners WIA réels (Windows) : liste et options du premier scanner installé ;
//! l'aperçu, qui fait fonctionner le scanner, n'est lancé qu'à la demande :
//!
//!   cargo test -p rustscanner --test wia_backend -- --ignored --nocapture
#![cfg(windows)]

use std::sync::{Mutex, MutexGuard, mpsc};
use std::time::Duration;

use rustscanner_lib::scanner::{Output, Scanner};
use rustscanner_shared::{Area, OptValue, ScanEvent, names};
use tokio::runtime::Runtime;

/// Un seul test à la fois sur le scanner (un second accès simultané est refusé).
static DEVICE: Mutex<()> = Mutex::new(());

fn first_scanner() -> Option<(MutexGuard<'static, ()>, Runtime, Scanner)> {
    let guard = DEVICE.lock().unwrap_or_else(|e| e.into_inner());
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let s = Scanner::start();
    assert!(s.available(), "{:?}", s.load_error);
    let devices = rt.block_on(s.devices(false)).expect("liste des scanners");
    println!("{devices:#?}");
    let dev = devices.first()?;
    let opts = rt.block_on(s.open(dev.name.clone(), Vec::new())).expect("ouverture");
    for o in &opts {
        println!("{:>2} {:<12} {:?} = {:?}", o.index, o.name, o.constraint, o.value);
    }
    for name in [names::SOURCE, names::MODE, names::RESOLUTION, names::TL_X, names::BR_Y] {
        assert!(opts.iter().any(|o| o.name == name), "option {name}");
    }
    Some((guard, rt, s))
}

#[test]
fn devices_and_options() {
    let Some((_guard, rt, s)) = first_scanner() else { return };
    let gray = Some(OptValue::Text("Gray".into()));
    let opts = rt.block_on(s.set(names::MODE.into(), gray.clone())).unwrap();
    assert!(opts.iter().any(|o| o.name == names::MODE && o.value == gray));
    rt.block_on(s.close()).unwrap();
}

#[test]
#[ignore]
fn preview() {
    let Some((_guard, rt, s)) = first_scanner() else { return };
    let (tx, rx) = mpsc::channel();
    rt.block_on(s.scan(true, tx)).expect("aperçu");
    loop {
        match rx.recv_timeout(Duration::from_secs(120)).expect("fin de l'aperçu") {
            Output::Event(ScanEvent::Progress { fraction, .. }) => println!("{:.0} %", fraction * 100.0),
            Output::Event(_) => {}
            Output::Image { image, dpi, .. } => println!("image {} × {} à {dpi} dpi", image.width(), image.height()),
            Output::Done { pages, error, .. } => {
                assert_eq!(error, None);
                assert_eq!(pages, 1);
                break;
            }
        }
    }
    rt.block_on(s.close()).unwrap();
}

/// Zone de 50 × 30 mm à 150 dpi en noir et blanc : réglages appliqués au pilote.
#[test]
#[ignore]
fn lineart_area() {
    let Some((_guard, rt, s)) = first_scanner() else { return };
    rt.block_on(s.set(names::MODE.into(), Some(OptValue::Text("Lineart".into())))).unwrap();
    rt.block_on(s.set(names::RESOLUTION.into(), Some(OptValue::Number(150.0)))).unwrap();
    rt.block_on(s.set_area(Area { x: 20.0, y: 20.0, w: 50.0, h: 30.0 })).unwrap();
    let (tx, rx) = mpsc::channel();
    rt.block_on(s.scan(false, tx)).expect("numérisation");
    let mut image = None;
    loop {
        match rx.recv_timeout(Duration::from_secs(120)).expect("fin de la numérisation") {
            Output::Image { image: img, dpi, bilevel, .. } => image = Some((img.width(), img.height(), dpi, bilevel)),
            Output::Done { error, .. } => {
                assert_eq!(error, None);
                break;
            }
            Output::Event(_) => {}
        }
    }
    let (w, h, dpi, bilevel) = image.expect("image");
    println!("{w} × {h} à {dpi} dpi, trait : {bilevel}");
    assert!(bilevel);
    assert_eq!(dpi, 150.0);
    // 50 mm à 150 dpi ≈ 295 px ; 30 mm ≈ 177 px (le pilote peut arrondir)
    assert!((w as i32 - 295).abs() <= 16 && (h as i32 - 177).abs() <= 16, "{w} × {h}");
    rt.block_on(s.close()).unwrap();
}
