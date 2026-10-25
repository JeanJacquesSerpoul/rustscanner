//! Erreurs du cœur, transmises à l'interface sous la forme `{ kind, message }`.

use rustscanner_shared::{tr, trf};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}", trf!("SANE est introuvable sur ce système ({0})", "SANE is not available on this system ({0})", .0))]
    NoSane(String),
    #[error("{message}")]
    Sane { status: i32, message: String },
    #[error("{}", tr!("Aucun scanner n'est ouvert", "No scanner is open"))]
    NoDevice,
    #[error("{}", tr!("Une numérisation est déjà en cours", "A scan is already running"))]
    Busy,
    #[error("{}", trf!("Page introuvable ({0})", "Page not found ({0})", .0))]
    NoPage(u64),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Image(#[from] image::ImageError),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn kind(&self) -> &'static str {
        match self {
            Error::NoSane(_) => "no-sane",
            Error::Sane { status, .. } => match *status {
                crate::sane::STATUS_CANCELLED => "cancelled",
                crate::sane::STATUS_NO_DOCS => "no-docs",
                3 => "busy",
                6 => "jammed",
                8 => "cover-open",
                _ => "sane",
            },
            Error::NoDevice => "no-device",
            Error::Busy => "busy",
            Error::NoPage(_) => "no-page",
            Error::Io(_) => "io",
            Error::Image(_) => "image",
            Error::Other(_) => "other",
        }
    }

    /// Message pour l'utilisateur, traduit pour les états SANE courants.
    pub fn user_message(&self) -> String {
        match self {
            Error::Sane { status, message } => match *status {
                2 => tr!("Numérisation annulée", "Scan cancelled").into(),
                3 => tr!("Le scanner est occupé", "The scanner is busy").into(),
                6 => tr!("Bourrage papier dans le chargeur", "Paper jam in the feeder").into(),
                7 => tr!("Le chargeur de documents est vide", "The document feeder is empty").into(),
                8 => tr!("Le capot du scanner est ouvert", "The scanner cover is open").into(),
                9 => trf!("Erreur de communication avec le scanner ({message})", "Communication error with the scanner ({message})"),
                10 => tr!("Mémoire insuffisante", "Out of memory").into(),
                11 => tr!("Accès au scanner refusé", "Access to the scanner denied").into(),
                _ => message.clone(),
            },
            other => other.to_string(),
        }
    }
}

impl Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Error", 2)?;
        st.serialize_field("kind", self.kind())?;
        st.serialize_field("message", &self.user_message())?;
        st.end()
    }
}

pub type Result<T> = std::result::Result<T, Error>;
