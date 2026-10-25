//! Enregistrement du document : PDF (une image par page, à la taille réelle)
//! ou images séparées ; impression par CUPS (par GDI sous Windows, voir `winprint.rs`).

use std::fs;
use std::io::{BufWriter, Cursor, Write};
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::Command;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use image::codecs::jpeg::{JpegEncoder, PixelDensity};
use image::codecs::webp::WebPEncoder;
use image::{DynamicImage, ImageFormat};
#[cfg(not(windows))]
use rustscanner_shared::Printer;
use rustscanner_shared::{ExportFormat, PageInfo};

use crate::error::{Error, Result};

/// Page à enregistrer : informations et image source (PNG de la session).
pub struct Source {
    pub info: PageInfo,
    pub path: PathBuf,
}

fn load(src: &Source) -> Result<DynamicImage> {
    Ok(image::open(&src.path)?)
}

/// Noms des fichiers d'images : `nom.ext` pour une page, `nom-001.ext`… sinon.
pub fn image_paths(base: &Path, format: ExportFormat, count: usize) -> Vec<PathBuf> {
    let ext = format.extension();
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "scan".into());
    let dir = base.parent().map(Path::to_path_buf).unwrap_or_default();
    if count <= 1 {
        return vec![dir.join(format!("{stem}.{ext}"))];
    }
    let width = count.to_string().len().max(3);
    (1..=count).map(|i| dir.join(format!("{stem}-{i:0width$}.{ext}"))).collect()
}

/// Enregistre les pages ; `progress(i, n)` après chaque page.
pub fn export(
    sources: &[Source],
    path: &Path,
    format: ExportFormat,
    quality: u8,
    progress: &dyn Fn(usize, usize),
) -> Result<Vec<PathBuf>> {
    if sources.is_empty() {
        return Err(Error::Other(rustscanner_shared::tr!("Aucune page à enregistrer", "No page to save").into()));
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let quality = quality.clamp(1, 100);
    match format {
        ExportFormat::Pdf => {
            let path = path.with_extension("pdf");
            write_pdf(sources, &path, quality, progress)?;
            Ok(vec![path])
        }
        _ => {
            let paths = image_paths(path, format, sources.len());
            for (i, (src, out)) in sources.iter().zip(&paths).enumerate() {
                let img = load(src)?;
                write_image(&img, src.info.dpi, out, format, quality)?;
                progress(i + 1, sources.len());
            }
            Ok(paths)
        }
    }
}

fn write_image(img: &DynamicImage, dpi: f64, out: &Path, format: ExportFormat, quality: u8) -> Result<()> {
    let tmp = out.with_extension("part");
    {
        let mut file = BufWriter::new(fs::File::create(&tmp)?);
        match format {
            ExportFormat::Jpeg => {
                let mut enc = JpegEncoder::new_with_quality(&mut file, quality);
                if dpi > 0.0 {
                    let d = dpi.round() as u16;
                    enc.set_pixel_density(PixelDensity::dpi(d));
                }
                let rgb;
                let src = if img.color().has_alpha() || !matches!(img, DynamicImage::ImageLuma8(_) | DynamicImage::ImageRgb8(_)) {
                    rgb = DynamicImage::ImageRgb8(img.to_rgb8());
                    &rgb
                } else {
                    img
                };
                src.write_with_encoder(enc)?;
            }
            ExportFormat::Webp => {
                let rgb = img.to_rgb8();
                DynamicImage::ImageRgb8(rgb).write_with_encoder(WebPEncoder::new_lossless(&mut file))?;
            }
            ExportFormat::Png => img.write_to(&mut file, ImageFormat::Png)?,
            ExportFormat::Tiff => img.write_to(&mut file, ImageFormat::Tiff)?,
            ExportFormat::Pdf => unreachable!(),
        }
        file.flush()?;
    }
    fs::rename(&tmp, out)?;
    Ok(())
}

/// Image encodée pour le PDF.
struct PdfImage {
    width: u32,
    height: u32,
    color_space: &'static str,
    bits: u8,
    filter: &'static str,
    data: Vec<u8>,
}

fn pdf_image(img: &DynamicImage, bilevel: bool, quality: u8) -> Result<PdfImage> {
    let (width, height) = (img.width(), img.height());
    if bilevel {
        // 1 bit par pixel, compressé sans perte (texte net, fichier léger)
        let gray = img.to_luma8();
        let row_bytes = width.div_ceil(8) as usize;
        let mut packed = vec![0u8; row_bytes * height as usize];
        for (y, row) in gray.rows().enumerate() {
            for (x, px) in row.enumerate() {
                if px.0[0] > 127 {
                    packed[y * row_bytes + x / 8] |= 0x80 >> (x % 8);
                }
            }
        }
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        z.write_all(&packed)?;
        return Ok(PdfImage {
            width,
            height,
            color_space: "/DeviceGray",
            bits: 1,
            filter: "/FlateDecode",
            data: z.finish()?,
        });
    }
    let gray = matches!(img, DynamicImage::ImageLuma8(_) | DynamicImage::ImageLuma16(_));
    let src = if gray {
        DynamicImage::ImageLuma8(img.to_luma8())
    } else {
        DynamicImage::ImageRgb8(img.to_rgb8())
    };
    let mut out = Cursor::new(Vec::new());
    src.write_with_encoder(JpegEncoder::new_with_quality(&mut out, quality))?;
    Ok(PdfImage {
        width,
        height,
        color_space: if gray { "/DeviceGray" } else { "/DeviceRGB" },
        bits: 8,
        filter: "/DCTDecode",
        data: out.into_inner(),
    })
}

/// Écrit un PDF minimal : catalogue, arbre de pages, une image par page.
struct PdfWriter<W: Write> {
    out: W,
    pos: usize,
    offsets: Vec<usize>,
}

impl<W: Write> PdfWriter<W> {
    fn raw(&mut self, bytes: &[u8]) -> Result<()> {
        self.out.write_all(bytes)?;
        self.pos += bytes.len();
        Ok(())
    }

    fn object(&mut self, id: usize, body: &str) -> Result<()> {
        self.begin(id)?;
        self.raw(body.as_bytes())?;
        self.raw(b"\nendobj\n")
    }

    fn begin(&mut self, id: usize) -> Result<()> {
        if self.offsets.len() < id {
            self.offsets.resize(id, 0);
        }
        self.offsets[id - 1] = self.pos;
        self.raw(format!("{id} 0 obj\n").as_bytes())
    }

    fn stream(&mut self, id: usize, dict: &str, data: &[u8]) -> Result<()> {
        self.begin(id)?;
        self.raw(format!("<< {dict} /Length {} >>\nstream\n", data.len()).as_bytes())?;
        self.raw(data)?;
        self.raw(b"\nendstream\nendobj\n")
    }
}

fn pdf_string(s: &str) -> String {
    let mut out = String::from("(");
    for c in s.chars() {
        match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            _ => out.push('?'),
        }
    }
    out.push(')');
    out
}

pub fn write_pdf(sources: &[Source], path: &Path, quality: u8, progress: &dyn Fn(usize, usize)) -> Result<()> {
    let tmp = path.with_extension("part");
    let file = BufWriter::new(fs::File::create(&tmp)?);
    let mut w = PdfWriter {
        out: file,
        pos: 0,
        offsets: Vec::new(),
    };
    w.raw(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")?;
    let n = sources.len();
    // 1 : catalogue, 2 : pages, 3 : infos ; puis 3 objets par page
    let page_id = |i: usize| 4 + i * 3;
    for (i, src) in sources.iter().enumerate() {
        let img = load(src)?;
        let encoded = pdf_image(&img, src.info.bilevel, quality)?;
        drop(img);
        let dpi = if src.info.dpi > 0.0 { src.info.dpi } else { 300.0 };
        let pw = encoded.width as f64 * 72.0 / dpi;
        let ph = encoded.height as f64 * 72.0 / dpi;
        let (pid, cid, iid) = (page_id(i), page_id(i) + 1, page_id(i) + 2);
        w.object(
            pid,
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.3} {ph:.3}] /Resources << /XObject << /Im0 {iid} 0 R >> /ProcSet [/PDF /ImageB /ImageC] >> /Contents {cid} 0 R >>"
            ),
        )?;
        let content = format!("q {pw:.3} 0 0 {ph:.3} 0 0 cm /Im0 Do Q");
        w.stream(cid, "", content.as_bytes())?;
        w.stream(
            iid,
            &format!(
                "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {} /BitsPerComponent {} /Filter {}",
                encoded.width, encoded.height, encoded.color_space, encoded.bits, encoded.filter
            ),
            &encoded.data,
        )?;
        progress(i + 1, n);
    }
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", page_id(i))).collect();
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>")?;
    w.object(2, &format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")))?;
    let date = chrono::Local::now().format("D:%Y%m%d%H%M%S").to_string();
    let title = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    w.object(
        3,
        &format!(
            "<< /Producer {} /Creator {} /Title {} /CreationDate {} >>",
            pdf_string(&format!("RustScanner {}", env!("CARGO_PKG_VERSION"))),
            pdf_string("RustScanner"),
            pdf_string(&title),
            pdf_string(&date)
        ),
    )?;
    let xref = w.pos;
    let count = w.offsets.len() + 1;
    let mut table = format!("xref\n0 {count}\n0000000000 65535 f \n");
    for off in &w.offsets {
        table.push_str(&format!("{off:010} 00000 n \n"));
    }
    table.push_str(&format!("trailer\n<< /Size {count} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n"));
    w.raw(table.as_bytes())?;
    w.out.flush()?;
    drop(w);
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(windows)]
pub use crate::winprint::{printers, printing_available};

/// Imprime les pages : PDF temporaire (dans `dir`) envoyé à CUPS, ou GDI sous Windows.
pub fn print_pages(sources: &[Source], dir: &Path, printer: &str, copies: u32) -> Result<String> {
    #[cfg(windows)]
    {
        let _ = dir;
        crate::winprint::print(sources, printer, copies)
    }
    #[cfg(not(windows))]
    {
        let pdf = dir.join("print.pdf");
        write_pdf(sources, &pdf, 90, &|_, _| {})?;
        print(&pdf, printer, copies)
    }
}

/// Imprimantes CUPS (`lpstat`).
#[cfg(not(windows))]
pub fn printers() -> Vec<Printer> {
    let default = Command::new("lpstat")
        .arg("-d")
        .env("LC_ALL", "C")
        .output()
        .ok()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.split(':').nth(1).map(|n| n.trim().to_string())
        })
        .unwrap_or_default();
    let Ok(out) = Command::new("lpstat").arg("-e").output() else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|name| Printer {
            default: name == default,
            name: name.to_string(),
        })
        .collect()
}

#[cfg(not(windows))]
pub fn printing_available() -> bool {
    Command::new("lp").arg("--help").output().is_ok()
}

/// Envoie un PDF à l'imprimante.
#[cfg(not(windows))]
pub fn print(pdf: &Path, printer: &str, copies: u32) -> Result<String> {
    let mut cmd = Command::new("lp");
    if !printer.is_empty() {
        cmd.arg("-d").arg(printer);
    }
    let out = cmd
        .arg("-n")
        .arg(copies.max(1).to_string())
        .arg("-o")
        .arg("fit-to-page")
        .arg(pdf)
        .output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(Error::Other(String::from_utf8_lossy(&out.stderr).trim().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(dir: &Path, id: u64, img: DynamicImage, bilevel: bool) -> Source {
        let path = dir.join(format!("p{id}.png"));
        img.save(&path).unwrap();
        Source {
            info: PageInfo {
                id,
                width: img.width(),
                height: img.height(),
                dpi: 150.0,
                rev: 0,
                bilevel,
                color: false,
            },
            path,
        }
    }

    #[test]
    fn names() {
        let p = image_paths(Path::new("/tmp/a/scan.png"), ExportFormat::Jpeg, 2);
        assert_eq!(p[0], Path::new("/tmp/a/scan-001.jpg"));
        assert_eq!(image_paths(Path::new("/x/doc"), ExportFormat::Png, 1)[0], Path::new("/x/doc.png"));
    }

    #[test]
    fn pdf_and_images() {
        let dir = tempfile::tempdir().unwrap();
        let rgb = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(300, 150, image::Rgb([200, 10, 10])));
        let bw = DynamicImage::ImageLuma8(image::GrayImage::from_pixel(37, 20, image::Luma([255])));
        let sources = vec![source(dir.path(), 1, rgb, false), source(dir.path(), 2, bw, true)];
        let out = dir.path().join("out/doc.pdf");
        let files = export(&sources, &out, ExportFormat::Pdf, 80, &|_, _| {}).unwrap();
        let data = fs::read(&files[0]).unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/Count 2"));
        // 300 px à 150 dpi = 2 pouces = 144 pt
        assert!(text.contains("/MediaBox [0 0 144.000 72.000]"));
        assert!(text.trim_end().ends_with("%%EOF"));
        // chaque entrée de la table xref pointe sur le début de son objet
        let xref_pos: usize = text.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
        let table = String::from_utf8(data[xref_pos..].to_vec()).unwrap();
        for (i, line) in table.lines().skip(3).take(9).enumerate() {
            let off: usize = line[..10].parse().unwrap();
            assert!(data[off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()), "objet {}", i + 1);
        }

        let imgs = export(&sources, &dir.path().join("img/page.webp"), ExportFormat::Webp, 80, &|_, _| {}).unwrap();
        assert_eq!(imgs.len(), 2);
        assert!(imgs.iter().all(|p| p.exists()));
        let jpg = export(&sources[..1], &dir.path().join("one.jpg"), ExportFormat::Jpeg, 80, &|_, _| {}).unwrap();
        assert_eq!(image::open(&jpg[0]).unwrap().width(), 300);
    }
}
