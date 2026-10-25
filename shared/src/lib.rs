//! Types échangés entre le cœur (Tauri) et l'interface (Leptos).

#[macro_use]
pub mod i18n;

pub use i18n::{Lang, set_lang};

use serde::{Deserialize, Serialize};

/// Scanner détecté par SANE.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DeviceInfo {
    /// Nom SANE (`pixma:MG3600_192.168.1.102`, `escl:http://…`).
    pub name: String,
    pub vendor: String,
    pub model: String,
    /// Type annoncé (« flatbed scanner », « multi-function peripheral »…).
    pub kind: String,
}

impl DeviceInfo {
    /// Backend SANE (partie avant le premier « : »).
    pub fn backend(&self) -> &str {
        self.name.split(':').next().unwrap_or("")
    }

    /// Libellé lisible : « Canon PIXMA MG3600 Series ».
    pub fn label(&self) -> String {
        let vendor = self.vendor.trim();
        let model = self.model.trim();
        if vendor.is_empty()
            || vendor.eq_ignore_ascii_case("noname")
            || model.to_lowercase().starts_with(&vendor.to_lowercase())
        {
            model.to_string()
        } else {
            format!("{vendor} {model}")
        }
    }

    /// Libellé du mode de connexion d'après le backend.
    pub fn transport(&self) -> &'static str {
        match self.backend() {
            "escl" | "airscan" => tr!("Réseau (eSCL)", "Network (eSCL)"),
            "net" => tr!("Réseau (saned)", "Network (saned)"),
            "v4l" => tr!("Caméra", "Camera"),
            "test" => tr!("Virtuel", "Virtual"),
            _ if self.name.contains("192.168.") || self.name.contains("://") => {
                tr!("Réseau", "Network")
            }
            _ => tr!("Local", "Local"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OptKind {
    Bool,
    Int,
    Fixed,
    Str,
    Button,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OptUnit {
    None,
    Pixel,
    Bit,
    Mm,
    Dpi,
    Percent,
    Microsecond,
}

impl OptUnit {
    pub fn suffix(self) -> &'static str {
        match self {
            OptUnit::None => "",
            OptUnit::Pixel => " px",
            OptUnit::Bit => " bit",
            OptUnit::Mm => " mm",
            OptUnit::Dpi => " dpi",
            OptUnit::Percent => " %",
            OptUnit::Microsecond => " µs",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Constraint {
    None,
    Range { min: f64, max: f64, step: f64 },
    Numbers { values: Vec<f64> },
    Strings { values: Vec<String> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum OptValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

impl OptValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            OptValue::Number(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            OptValue::Text(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            OptValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// Option d'un scanner, telle que décrite par le backend.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ScanOption {
    pub index: i32,
    /// Nom technique (`resolution`, `mode`, `tl-x`…) ; vide pour certaines options.
    pub name: String,
    /// Titre (traduit lorsque la traduction de sane-backends est disponible).
    pub title: String,
    pub desc: String,
    /// Groupe d'appartenance (« Standard », « Géométrie »…).
    pub group: String,
    pub kind: OptKind,
    pub unit: OptUnit,
    pub constraint: Constraint,
    /// Valeur courante (absente pour les boutons, tableaux et options inactives).
    pub value: Option<OptValue>,
    pub active: bool,
    pub settable: bool,
    pub advanced: bool,
    /// Option à plusieurs valeurs (table gamma…) : non éditable dans l'interface.
    pub array: bool,
    /// Le scanner peut choisir la valeur lui-même.
    pub automatic: bool,
}

/// Noms d'options normalisés par SANE.
pub mod names {
    pub const SOURCE: &str = "source";
    pub const MODE: &str = "mode";
    pub const RESOLUTION: &str = "resolution";
    pub const TL_X: &str = "tl-x";
    pub const TL_Y: &str = "tl-y";
    pub const BR_X: &str = "br-x";
    pub const BR_Y: &str = "br-y";
    pub const PREVIEW: &str = "preview";

    /// Options déjà présentées dans les réglages essentiels (ou gérées à part).
    pub fn is_essential(name: &str) -> bool {
        matches!(
            name,
            SOURCE | MODE | RESOLUTION | TL_X | TL_Y | BR_X | BR_Y | PREVIEW
        ) || name == "x-resolution"
            || name == "y-resolution"
    }
}

/// Page numérisée du document en cours.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PageInfo {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub dpi: f64,
    /// Incrémenté à chaque modification (invalide le cache des images).
    pub rev: u32,
    /// Image en noir et blanc pur (trait).
    pub bilevel: bool,
    pub color: bool,
}

impl PageInfo {
    /// Format physique en millimètres.
    pub fn size_mm(&self) -> (f64, f64) {
        let dpi = if self.dpi > 0.0 { self.dpi } else { 300.0 };
        (
            self.width as f64 / dpi * 25.4,
            self.height as f64 / dpi * 25.4,
        )
    }
}

/// Aperçu basse résolution de toute la vitre.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PreviewInfo {
    pub rev: u32,
    pub width: u32,
    pub height: u32,
}

/// Zone de numérisation en millimètres.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Formats de page prédéfinis (largeur × hauteur en mm).
pub fn page_sizes() -> Vec<(&'static str, &'static str, f64, f64)> {
    vec![
        ("a4", "A4", 210.0, 297.0),
        ("a5", "A5", 148.0, 210.0),
        ("a6", "A6", 105.0, 148.0),
        ("b5", "B5", 176.0, 250.0),
        ("letter", "US Letter", 215.9, 279.4),
        ("legal", "US Legal", 215.9, 355.6),
        ("photo10x15", tr!("Photo 10 × 15", "Photo 4 × 6\""), 101.6, 152.4),
        ("photo13x18", tr!("Photo 13 × 18", "Photo 5 × 7\""), 127.0, 177.8),
        ("card", tr!("Carte de visite", "Business card"), 85.0, 55.0),
        ("idcard", tr!("Carte d'identité", "ID card"), 85.6, 54.0),
    ]
}

/// Événements émis pendant la numérisation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ScanEvent {
    /// Début d'une page (numéro dans la série, à partir de 1).
    Started { page: u32, preview: bool },
    /// Avancement de la page en cours (0 – 1 ; négatif si inconnu).
    Progress { page: u32, fraction: f64 },
    /// Page ajoutée au document.
    Page { page: PageInfo },
    /// Nouvel aperçu disponible.
    Preview { preview: PreviewInfo },
    /// Fin de la série : `error` renseigné en cas d'échec.
    Finished {
        pages: u32,
        cancelled: bool,
        error: Option<String>,
    },
}

/// Formats d'enregistrement.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    #[default]
    Pdf,
    Png,
    Jpeg,
    Tiff,
    Webp,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 5] = [
        ExportFormat::Pdf,
        ExportFormat::Png,
        ExportFormat::Jpeg,
        ExportFormat::Tiff,
        ExportFormat::Webp,
    ];

    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Pdf => "pdf",
            ExportFormat::Png => "png",
            ExportFormat::Jpeg => "jpg",
            ExportFormat::Tiff => "tiff",
            ExportFormat::Webp => "webp",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Pdf => "PDF",
            ExportFormat::Png => "PNG",
            ExportFormat::Jpeg => "JPEG",
            ExportFormat::Tiff => "TIFF",
            ExportFormat::Webp => "WebP",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            ExportFormat::Pdf => tr!("Un seul document, toutes les pages", "One document, all pages"),
            ExportFormat::Png => tr!("Sans perte, une image par page", "Lossless, one image per page"),
            ExportFormat::Jpeg => tr!("Compact, idéal pour les photos", "Compact, best for photos"),
            ExportFormat::Tiff => tr!("Archivage sans perte", "Lossless archiving"),
            ExportFormat::Webp => tr!("Sans perte, très compact", "Lossless, very compact"),
        }
    }

    pub fn from_extension(ext: &str) -> Option<ExportFormat> {
        match ext.to_ascii_lowercase().as_str() {
            "pdf" => Some(ExportFormat::Pdf),
            "png" => Some(ExportFormat::Png),
            "jpg" | "jpeg" => Some(ExportFormat::Jpeg),
            "tif" | "tiff" => Some(ExportFormat::Tiff),
            "webp" => Some(ExportFormat::Webp),
            _ => None,
        }
    }
}

/// Demande d'enregistrement.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExportRequest {
    /// Chemin complet du fichier (pour les images : base des noms numérotés).
    pub path: String,
    pub format: ExportFormat,
    /// Pages à enregistrer, dans l'ordre (vide : toutes).
    pub pages: Vec<u64>,
    /// Qualité JPEG (PDF et JPEG), 1 – 100.
    pub quality: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExportResult {
    pub files: Vec<String>,
    pub bytes: u64,
}

/// Réglages de l'application.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// "auto" | "fr" | "en"
    pub language: String,
    /// "auto" | "light" | "dark"
    pub theme: String,
    /// Dossier d'enregistrement (vide : Documents).
    pub save_dir: String,
    /// Modèle de nom : {date}, {time}, {n}.
    pub name_pattern: String,
    pub format: ExportFormat,
    pub quality: u8,
    pub last_device: Option<String>,
    /// Mémoriser les réglages de numérisation de chaque scanner.
    pub remember_options: bool,
    /// Rechercher aussi les scanners réseau.
    pub network_devices: bool,
    /// Afficher les options avancées du scanner.
    pub show_advanced: bool,
    /// Rouvrir automatiquement le dernier scanner utilisé.
    pub auto_open: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: "auto".into(),
            theme: "auto".into(),
            save_dir: String::new(),
            name_pattern: "scan-{date}-{n}".into(),
            format: ExportFormat::Pdf,
            quality: 85,
            last_device: None,
            remember_options: true,
            network_devices: true,
            show_advanced: false,
            auto_open: true,
        }
    }
}

/// Informations de démarrage pour l'interface.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AppInfo {
    pub version: String,
    pub lang: String,
    pub settings: Settings,
    /// SANE est disponible sur le système.
    pub sane: bool,
    pub sane_version: String,
    /// Dossier d'enregistrement effectif.
    pub save_dir: String,
    pub printing: bool,
}

/// Imprimante CUPS.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Printer {
    pub name: String,
    pub default: bool,
}

/// Remplace {date}, {time} et {n} dans le modèle de nom de fichier.
pub fn expand_pattern(pattern: &str, date: &str, time: &str, n: u32) -> String {
    let name = pattern
        .replace("{date}", date)
        .replace("{time}", time)
        .replace("{n}", &n.to_string());
    let clean: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c })
        .collect();
    let clean = clean.trim().to_string();
    if clean.is_empty() { "scan".into() } else { clean }
}

/// Libellé traduit des valeurs normalisées (mode, source).
pub fn value_label(option: &str, value: &str) -> String {
    let v = value.to_lowercase();
    let known = match option {
        names::MODE => match v.as_str() {
            "color" | "colour" | "24bit color" | "color24" => Some(tr!("Couleur", "Color")),
            "gray" | "grayscale" | "grey" | "8bit gray" | "gray8" => {
                Some(tr!("Niveaux de gris", "Grayscale"))
            }
            "lineart" | "binary" | "black & white" | "black and white" | "bw" => {
                Some(tr!("Noir et blanc", "Black & white"))
            }
            "halftone" => Some(tr!("Tramé", "Halftone")),
            _ => None,
        },
        names::SOURCE => match v.as_str() {
            "flatbed" | "platen" | "normal" | "document table" => Some(tr!("Vitre", "Flatbed")),
            "adf" | "automatic document feeder" | "adf front" | "adf simplex" | "feeder" => {
                Some(tr!("Chargeur", "Document feeder"))
            }
            "adf duplex" | "duplex" | "automatic document feeder(left aligned,duplex)" => {
                Some(tr!("Chargeur recto-verso", "Feeder (duplex)"))
            }
            "adf back" => Some(tr!("Chargeur (verso)", "Feeder (back)")),
            "transparency" | "transparency adapter" | "tma" | "film" => {
                Some(tr!("Diapositive / film", "Transparency"))
            }
            _ => None,
        },
        _ => None,
    };
    known.map(str::to_string).unwrap_or_else(|| value.to_string())
}

/// La source est un chargeur de documents (numérisation en série).
pub fn is_feeder(source: &str) -> bool {
    let s = source.to_lowercase();
    s.contains("adf") || s.contains("feeder") || s.contains("duplex") || s.contains("chargeur")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern() {
        assert_eq!(expand_pattern("scan-{date}-{n}", "2026-10-25", "14-30", 3), "scan-2026-10-25-3");
        assert_eq!(expand_pattern("a/b:{time}", "d", "t", 1), "a-b-t");
        assert_eq!(expand_pattern("  ", "d", "t", 1), "scan");
    }

    #[test]
    fn device_label() {
        let d = DeviceInfo {
            name: "pixma:MG3600".into(),
            vendor: "CANON".into(),
            model: "Canon PIXMA MG3600 Series".into(),
            kind: "multi-function peripheral".into(),
        };
        assert_eq!(d.label(), "Canon PIXMA MG3600 Series");
        assert_eq!(d.backend(), "pixma");
    }

    #[test]
    fn values() {
        let _g = i18n::TEST_LOCK.lock().unwrap();
        set_lang(Lang::Fr);
        assert_eq!(value_label("mode", "Color"), "Couleur");
        assert_eq!(value_label("source", "ADF Duplex"), "Chargeur recto-verso");
        assert_eq!(value_label("mode", "Weird"), "Weird");
        assert!(is_feeder("Automatic Document Feeder"));
        assert!(!is_feeder("Flatbed"));
        set_lang(Lang::En);
    }
}
