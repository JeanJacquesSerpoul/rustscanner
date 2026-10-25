//! Document en cours : pages numérisées, conservées sur disque (PNG) avec une
//! vignette et une image d'affichage en mémoire.

use std::fs;
use std::io::{BufWriter, Cursor};
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::imageops::FilterType as Resize;
use image::{DynamicImage, ImageEncoder};
use rustscanner_shared::{PageInfo, PreviewInfo};

use crate::error::{Error, Result};

const THUMB_SIZE: u32 = 360;
const VIEW_SIZE: u32 = 2400;

pub struct Page {
    pub info: PageInfo,
    pub path: PathBuf,
    thumb: Vec<u8>,
    view: Vec<u8>,
}

/// Page préparée hors verrou (encodage coûteux), prête à être insérée.
pub struct Prepared {
    page: Page,
}

pub struct Document {
    dir: PathBuf,
    pages: Vec<Page>,
    /// Pages supprimées, conservées pour « Annuler » (position d'origine).
    trash: Vec<(usize, Page)>,
    next_id: u64,
    preview: Option<(PreviewInfo, Vec<u8>)>,
    /// Modifié depuis le dernier enregistrement.
    pub dirty: bool,
}

fn jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    let rgb;
    let src: &DynamicImage = match img {
        DynamicImage::ImageLuma8(_) | DynamicImage::ImageRgb8(_) => img,
        other => {
            rgb = DynamicImage::ImageRgb8(other.to_rgb8());
            &rgb
        }
    };
    src.write_with_encoder(JpegEncoder::new_with_quality(&mut out, quality))?;
    Ok(out.into_inner())
}

fn scaled(img: &DynamicImage, max: u32) -> DynamicImage {
    if img.width() <= max && img.height() <= max {
        img.clone()
    } else {
        img.resize(max, max, Resize::Triangle)
    }
}

fn save_png(img: &DynamicImage, path: &Path) -> Result<()> {
    let file = BufWriter::new(fs::File::create(path)?);
    let enc = PngEncoder::new_with_quality(file, CompressionType::Fast, FilterType::Sub);
    enc.write_image(img.as_bytes(), img.width(), img.height(), img.color().into())?;
    Ok(())
}

impl Document {
    pub fn new(dir: PathBuf) -> Document {
        let _ = fs::create_dir_all(&dir);
        Document {
            dir,
            pages: Vec::new(),
            trash: Vec::new(),
            next_id: 1,
            preview: None,
            dirty: false,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Encode une page (fichier PNG, vignette, affichage) sans toucher au document.
    pub fn prepare(dir: &Path, id: u64, img: &DynamicImage, dpi: f64, bilevel: bool, rev: u32) -> Result<Prepared> {
        let path = dir.join(format!("page-{id}.png"));
        save_png(img, &path)?;
        let color = matches!(img.color().channel_count(), 3 | 4);
        let page = Page {
            info: PageInfo {
                id,
                width: img.width(),
                height: img.height(),
                dpi,
                rev,
                bilevel,
                color,
            },
            path,
            thumb: jpeg(&scaled(img, THUMB_SIZE), 82)?,
            view: jpeg(&scaled(img, VIEW_SIZE), 88)?,
        };
        Ok(Prepared { page })
    }

    pub fn insert(&mut self, prepared: Prepared, at: Option<usize>) -> PageInfo {
        let info = prepared.page.info.clone();
        let at = at.unwrap_or(self.pages.len()).min(self.pages.len());
        self.pages.insert(at, prepared.page);
        self.dirty = true;
        info
    }

    /// Remplace une page modifiée (rotation…).
    pub fn replace(&mut self, prepared: Prepared) -> Result<PageInfo> {
        let id = prepared.page.info.id;
        let slot = self.pages.iter_mut().find(|p| p.info.id == id).ok_or(Error::NoPage(id))?;
        *slot = prepared.page;
        self.dirty = true;
        Ok(slot.info.clone())
    }

    pub fn pages(&self) -> Vec<PageInfo> {
        self.pages.iter().map(|p| p.info.clone()).collect()
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    pub fn page(&self, id: u64) -> Result<&Page> {
        self.pages.iter().find(|p| p.info.id == id).ok_or(Error::NoPage(id))
    }

    pub fn thumb(&self, id: u64) -> Option<Vec<u8>> {
        self.page(id).ok().map(|p| p.thumb.clone())
    }

    pub fn view(&self, id: u64) -> Option<Vec<u8>> {
        self.page(id).ok().map(|p| p.view.clone())
    }

    pub fn preview(&self) -> Option<Vec<u8>> {
        self.preview.as_ref().map(|(_, b)| b.clone())
    }

    pub fn set_preview(&mut self, img: &DynamicImage) -> Result<PreviewInfo> {
        let rev = self.preview.as_ref().map(|(i, _)| i.rev + 1).unwrap_or(1);
        let bytes = jpeg(&scaled(img, VIEW_SIZE), 85)?;
        let info = PreviewInfo {
            rev,
            width: img.width(),
            height: img.height(),
        };
        self.preview = Some((info.clone(), bytes));
        Ok(info)
    }

    pub fn clear_preview(&mut self) {
        self.preview = None;
    }

    /// Supprime des pages (récupérables par `restore`).
    pub fn delete(&mut self, ids: &[u64]) -> Vec<PageInfo> {
        self.purge_trash();
        let mut removed = Vec::new();
        for &id in ids {
            if let Some(pos) = self.pages.iter().position(|p| p.info.id == id) {
                let page = self.pages.remove(pos);
                removed.push(page.info.clone());
                self.trash.push((pos, page));
            }
        }
        if !removed.is_empty() {
            self.dirty = true;
        }
        removed
    }

    /// Restaure les dernières pages supprimées à leur place.
    pub fn restore(&mut self) -> Vec<PageInfo> {
        let mut restored = Vec::new();
        while let Some((pos, page)) = self.trash.pop() {
            restored.push(page.info.clone());
            let pos = pos.min(self.pages.len());
            self.pages.insert(pos, page);
        }
        restored.reverse();
        restored
    }

    /// Efface définitivement la corbeille (au début d'une nouvelle suppression).
    fn purge_trash(&mut self) {
        for (_, page) in self.trash.drain(..) {
            let _ = fs::remove_file(&page.path);
        }
    }

    /// Réordonne selon la liste d'identifiants (les absents gardent leur ordre, à la fin).
    pub fn reorder(&mut self, order: &[u64]) {
        let mut pages = std::mem::take(&mut self.pages);
        let mut sorted = Vec::with_capacity(pages.len());
        for id in order {
            if let Some(pos) = pages.iter().position(|p| p.info.id == *id) {
                sorted.push(pages.remove(pos));
            }
        }
        sorted.extend(pages);
        if sorted.iter().map(|p| p.info.id).ne(self.pages.iter().map(|p| p.info.id)) {
            self.dirty = true;
        }
        self.pages = sorted;
    }

    /// Supprime toutes les pages (nouveau document).
    pub fn clear(&mut self) {
        self.purge_trash();
        for page in self.pages.drain(..) {
            let _ = fs::remove_file(&page.path);
        }
        self.dirty = false;
    }

    pub fn load(path: &Path) -> Result<DynamicImage> {
        Ok(image::open(path)?)
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        if !self.dir.as_os_str().is_empty() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

/// Dossier de travail de la session ; nettoie ceux des sessions terminées.
/// Le processus `pid` (session d'une autre instance) est encore en cours.
#[cfg(not(windows))]
fn running(pid: &str) -> bool {
    Path::new("/proc").join(pid).exists()
}

#[cfg(windows)]
fn running(pid: &str) -> bool {
    use windows::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    let Ok(pid) = pid.parse::<u32>() else { return false };
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let mut code = 0u32;
        let alive = GetExitCodeProcess(handle, &mut code).is_ok() && code == STILL_ACTIVE.0 as u32;
        let _ = CloseHandle(handle);
        alive
    }
}

pub fn session_dir() -> PathBuf {
    // sous Windows, le dossier de cache (%LOCALAPPDATA%\rustscanner) est aussi
    // celui de l'installation : les sessions vont dans le dossier temporaire
    let cache = if cfg!(windows) { None } else { dirs::cache_dir() };
    let base = cache.unwrap_or_else(std::env::temp_dir).join("rustscanner");
    if let Ok(entries) = fs::read_dir(&base) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some(pid) = name.strip_prefix("session-")
                && !running(pid)
            {
                let _ = fs::remove_dir_all(e.path());
            }
        }
    }
    base.join(format!("session-{}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(doc: &mut Document, w: u32) -> u64 {
        let id = doc.next_id();
        let img = DynamicImage::ImageLuma8(image::GrayImage::new(w, 10));
        let p = Document::prepare(doc.dir(), id, &img, 300.0, false, 0).unwrap();
        doc.insert(p, None);
        id
    }

    #[test]
    fn delete_restore_reorder() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::new(dir.path().join("s"));
        let a = add(&mut doc, 10);
        let b = add(&mut doc, 20);
        let c = add(&mut doc, 30);
        doc.reorder(&[c, a]);
        assert_eq!(doc.pages().iter().map(|p| p.id).collect::<Vec<_>>(), vec![c, a, b]);
        doc.delete(&[a]);
        assert_eq!(doc.len(), 2);
        doc.restore();
        assert_eq!(doc.pages().iter().map(|p| p.id).collect::<Vec<_>>(), vec![c, a, b]);
        assert!(doc.thumb(b).is_some());
        let img = Document::load(&doc.page(b).unwrap().path).unwrap();
        assert_eq!(img.width(), 20);
        doc.clear();
        assert!(doc.is_empty());
    }
}
