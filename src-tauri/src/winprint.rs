//! Impression sous Windows : chaque page est dessinée par GDI à sa taille
//! réelle, centrée, et réduite seulement si elle dépasse la zone imprimable.

use rustscanner_shared::{Printer, tr, trf};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDCW, DIB_RGB_COLORS, DeleteDC, GetDeviceCaps, HALFTONE, HDC, HORZRES,
    LOGPIXELSX, LOGPIXELSY, SRCCOPY, SetBrushOrgEx, SetStretchBltMode, StretchDIBits, VERTRES,
};
use windows::Win32::Graphics::Printing::{EnumPrintersW, GetDefaultPrinterW, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_INFO_4W};
use windows::Win32::Storage::Xps::{AbortDoc, DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};
use windows::core::{HSTRING, PCWSTR, PWSTR, w};

use crate::error::{Error, Result};
use crate::export::Source;

fn default_printer() -> String {
    let mut len = 0u32;
    unsafe {
        let _ = GetDefaultPrinterW(None, &mut len);
        if len == 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize];
        if !GetDefaultPrinterW(Some(PWSTR(buf.as_mut_ptr())), &mut len).as_bool() {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())])
    }
}

/// Imprimantes locales et connexions réseau.
pub fn printers() -> Vec<Printer> {
    let default = default_printer();
    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
    let (mut needed, mut count) = (0u32, 0u32);
    unsafe {
        let _ = EnumPrintersW(flags, PCWSTR::null(), 4, None, &mut needed, &mut count);
    }
    if needed == 0 {
        return Vec::new();
    }
    // tampon aligné : il contient des structures à pointeurs
    let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
    let bytes = unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr().cast::<u8>(), needed as usize) };
    if unsafe { EnumPrintersW(flags, PCWSTR::null(), 4, Some(bytes), &mut needed, &mut count) }.is_err() {
        return Vec::new();
    }
    let infos = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<PRINTER_INFO_4W>(), count as usize) };
    infos
        .iter()
        .filter_map(|i| unsafe { i.pPrinterName.to_string() }.ok())
        .map(|name| Printer {
            default: name == default,
            name,
        })
        .collect()
}

pub fn printing_available() -> bool {
    true
}

fn failed(what: &str) -> Error {
    Error::Other(trf!(
        "Impression impossible ({what}) : {}",
        "Printing failed ({what}): {}",
        windows::core::Error::from_thread().message()
    ))
}

/// Dessine une page sur le contexte d'impression.
fn page(dc: HDC, src: &Source) -> Result<()> {
    let img = image::open(&src.path)?.to_rgb8();
    let (w, h) = img.dimensions();
    // DIB 24 bits : BGR, lignes alignées sur 4 octets, de haut en bas
    let stride = (w as usize * 3).div_ceil(4) * 4;
    let mut bits = vec![0u8; stride * h as usize];
    for (y, row) in img.rows().enumerate() {
        let line = &mut bits[y * stride..];
        for (x, p) in row.enumerate() {
            line[x * 3..x * 3 + 3].copy_from_slice(&[p.0[2], p.0[1], p.0[0]]);
        }
    }
    drop(img);
    let (area_w, area_h, dpi_x, dpi_y) = unsafe {
        (
            GetDeviceCaps(Some(dc), HORZRES) as f64,
            GetDeviceCaps(Some(dc), VERTRES) as f64,
            GetDeviceCaps(Some(dc), LOGPIXELSX) as f64,
            GetDeviceCaps(Some(dc), LOGPIXELSY) as f64,
        )
    };
    let dpi = if src.info.dpi > 0.0 { src.info.dpi } else { 300.0 };
    let (mut dw, mut dh) = (w as f64 / dpi * dpi_x, h as f64 / dpi * dpi_y);
    let scale = (area_w / dw).min(area_h / dh).min(1.0);
    dw *= scale;
    dh *= scale;
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 24,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        if StartPage(dc) <= 0 {
            return Err(failed("StartPage"));
        }
        SetStretchBltMode(dc, HALFTONE);
        let _ = SetBrushOrgEx(dc, 0, 0, None);
        let drawn = StretchDIBits(
            dc,
            ((area_w - dw) / 2.0).round() as i32,
            ((area_h - dh) / 2.0).round() as i32,
            dw.round() as i32,
            dh.round() as i32,
            0,
            0,
            w as i32,
            h as i32,
            Some(bits.as_ptr().cast()),
            &bmi,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
        if drawn == 0 {
            return Err(failed("StretchDIBits"));
        }
        if EndPage(dc) <= 0 {
            return Err(failed("EndPage"));
        }
    }
    Ok(())
}

/// Imprime les pages (copies assemblées) ; renvoie le nom de l'imprimante.
pub fn print(sources: &[Source], printer: &str, copies: u32) -> Result<String> {
    if sources.is_empty() {
        return Err(Error::Other(tr!("Aucune page à imprimer", "No page to print").into()));
    }
    let name = if printer.is_empty() { default_printer() } else { printer.to_string() };
    let dc = unsafe { CreateDCW(PCWSTR::null(), &HSTRING::from(name.as_str()), PCWSTR::null(), None) };
    if dc.is_invalid() {
        return Err(failed(&name));
    }
    let doc = DOCINFOW {
        cbSize: size_of::<DOCINFOW>() as i32,
        lpszDocName: w!("RustScanner"),
        ..Default::default()
    };
    let result = (|| {
        if unsafe { StartDocW(dc, &doc) } <= 0 {
            return Err(failed("StartDoc"));
        }
        for _ in 0..copies.max(1) {
            for src in sources {
                page(dc, src)?;
            }
        }
        Ok(())
    })();
    unsafe {
        if result.is_ok() {
            EndDoc(dc);
        } else {
            AbortDoc(dc);
        }
        let _ = DeleteDC(dc);
    }
    result.map(|_| name)
}
