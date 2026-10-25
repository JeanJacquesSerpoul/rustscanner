//! Numérisation réelle avec le scanner virtuel « test » de sane-backends
//! (ignoré si le backend n'est pas installé).

use std::sync::mpsc;
use std::time::Duration;

use rustscanner_lib::scanner::{Output, Scanner};
use rustscanner_shared::{Area, OptValue, ScanEvent};

fn test_scanner() -> Option<Scanner> {
    let installed = ["/usr/lib/x86_64-linux-gnu/sane", "/usr/lib/sane", "/usr/lib64/sane"]
        .iter()
        .any(|d| std::path::Path::new(d).join("libsane-test.so.1").exists());
    if !installed {
        return None;
    }
    let dir = std::env::temp_dir().join(format!("rustscanner-sane-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("dll.conf"), "test\n").unwrap();
    // SAFETY : positionné avant tout autre fil, au début du seul test du fichier
    unsafe { std::env::set_var("SANE_CONFIG_DIR", &dir) };
    let s = Scanner::start();
    s.available().then_some(s)
}

/// Images (largeur, hauteur, trait), événements, erreur, nombre de pages.
type Collected = (Vec<(u32, u32, bool)>, Vec<ScanEvent>, Option<String>, u32);

fn collect(rx: &mpsc::Receiver<Output>) -> Collected {
    let mut images = Vec::new();
    let mut events = Vec::new();
    loop {
        match rx.recv_timeout(Duration::from_secs(60)).expect("fin de numérisation") {
            Output::Event(e) => events.push(e),
            Output::Image { image, bilevel, .. } => images.push((image.width(), image.height(), bilevel)),
            Output::Done { pages, error, .. } => return (images, events, error, pages),
        }
    }
}

#[test]
fn scan_with_test_backend() {
    let Some(scanner) = test_scanner() else {
        eprintln!("backend SANE « test » absent : test ignoré");
        return;
    };
    tauri::async_runtime::block_on(async {
        let devices = scanner.devices(true).await.unwrap();
        assert!(devices.iter().any(|d| d.name == "test:0"), "{devices:?}");
        let opts = scanner.open("test:0".into(), vec![]).await.unwrap();
        assert!(opts.iter().any(|o| o.name == "resolution"));

        scanner.set("mode".into(), Some(OptValue::Text("Color".into()))).await.unwrap();
        scanner.set("resolution".into(), Some(OptValue::Number(72.0))).await.unwrap();
        scanner.set("test-picture".into(), Some(OptValue::Text("Color pattern".into()))).await.unwrap();
        // 50,8 × 25,4 mm à 72 dpi → 144 × 72 px (au pas de 1 mm près)
        scanner.set_area(Area { x: 10.0, y: 10.0, w: 50.8, h: 25.4 }).await.unwrap();

        let (tx, rx) = mpsc::channel();
        scanner.scan(false, tx).await.unwrap();
        let (images, events, error, pages) = collect(&rx);
        assert_eq!(error, None);
        assert_eq!(pages, 1);
        let (w, h, bilevel) = images[0];
        assert!((143..=145).contains(&w) && (69..=73).contains(&h), "{w}×{h}");
        assert!(!bilevel);
        assert!(events.iter().any(|e| matches!(e, ScanEvent::Started { page: 1, .. })));

        // trait 1 bit, trois passes
        scanner.set("mode".into(), Some(OptValue::Text("Gray".into()))).await.unwrap();
        scanner.set("depth".into(), Some(OptValue::Number(1.0))).await.unwrap();
        let (tx, rx) = mpsc::channel();
        scanner.scan(false, tx).await.unwrap();
        let (images, _, error, _) = collect(&rx);
        assert_eq!(error, None);
        assert!(images[0].2, "image au trait attendue");

        // aperçu : toute la vitre, réglages rétablis ensuite
        let (tx, rx) = mpsc::channel();
        scanner.scan(true, tx).await.unwrap();
        let (images, _, error, _) = collect(&rx);
        assert_eq!(error, None);
        assert!(images[0].0 > w, "l'aperçu couvre toute la vitre");
        let opts = scanner.options().await.unwrap();
        let res = opts.iter().find(|o| o.name == "resolution").unwrap().value.clone();
        assert_eq!(res, Some(OptValue::Number(72.0)));

        // chargeur : le scanner virtuel est « vide » après 10 feuilles (numérisations
        // précédentes comprises)
        scanner.set("depth".into(), Some(OptValue::Number(8.0))).await.unwrap();
        scanner.set("source".into(), Some(OptValue::Text("Automatic Document Feeder".into()))).await.unwrap();
        let (tx, rx) = mpsc::channel();
        scanner.scan(false, tx).await.unwrap();
        let (images, _, error, pages) = collect(&rx);
        assert_eq!(error, None);
        assert!(pages >= 2, "{pages} feuille(s)");
        assert_eq!(images.len(), pages as usize);

        // erreur du scanner (bourrage simulé) : message pour l'utilisateur
        scanner.set("source".into(), Some(OptValue::Text("Flatbed".into()))).await.unwrap();
        scanner.set("read-return-value".into(), Some(OptValue::Text("SANE_STATUS_JAMMED".into()))).await.unwrap();
        let (tx, rx) = mpsc::channel();
        scanner.scan(false, tx).await.unwrap();
        let (_, _, error, pages) = collect(&rx);
        assert_eq!(pages, 0);
        assert!(error.is_some_and(|e| !e.is_empty()));
        scanner.set("read-return-value".into(), Some(OptValue::Text("Default".into()))).await.unwrap();

        // annulation pendant la lecture
        scanner.set("resolution".into(), Some(OptValue::Number(600.0))).await.unwrap();
        scanner.set_area(Area { x: 0.0, y: 0.0, w: 200.0, h: 200.0 }).await.unwrap();
        scanner.set("read-delay".into(), Some(OptValue::Bool(true))).await.unwrap();
        scanner.set("read-delay-duration".into(), Some(OptValue::Number(20000.0))).await.unwrap();
        scanner.set("read-limit".into(), Some(OptValue::Bool(true))).await.unwrap();
        scanner.set("read-limit-size".into(), Some(OptValue::Number(65536.0))).await.unwrap();
        let (tx, rx) = mpsc::channel();
        scanner.scan(false, tx).await.unwrap();
        std::thread::sleep(Duration::from_millis(300));
        scanner.cancel();
        let cancelled;
        loop {
            if let Output::Done { cancelled: c, pages, .. } = rx.recv_timeout(Duration::from_secs(30)).unwrap() {
                cancelled = c;
                assert_eq!(pages, 0);
                break;
            }
        }
        assert!(cancelled);
        scanner.close().await.unwrap();
    });
}
