//! Conversion des trames SANE (gris, RVB entrelacé ou trois passes R/V/B ;
//! 1, 8 ou 16 bits) en image.

use image::{DynamicImage, GrayImage, RgbImage};

use crate::error::{Error, Result};
use crate::sane::{FRAME_BLUE, FRAME_GRAY, FRAME_GREEN, FRAME_RED, FRAME_RGB, Parameters};

pub struct Frame {
    pub params: Parameters,
    pub data: Vec<u8>,
}

impl Frame {
    fn lines(&self) -> usize {
        let bpl = self.params.bytes_per_line.max(1) as usize;
        if self.params.lines > 0 {
            (self.params.lines as usize).min(self.data.len() / bpl)
        } else {
            // scanner à main / chargeur : hauteur connue seulement à la fin
            self.data.len() / bpl
        }
    }

    fn row(&self, y: usize) -> &[u8] {
        let bpl = self.params.bytes_per_line as usize;
        &self.data[y * bpl..(y + 1) * bpl]
    }

    /// Échantillon `i` d'une ligne, ramené sur 8 bits.
    fn sample(&self, row: &[u8], i: usize) -> u8 {
        match self.params.depth {
            1 => {
                // en SANE, un bit à 1 est noir
                let bit = row[i / 8] >> (7 - (i % 8)) & 1;
                if bit == 1 { 0 } else { 255 }
            }
            16 => {
                let v = u16::from_ne_bytes([row[i * 2], row[i * 2 + 1]]);
                (v >> 8) as u8
            }
            _ => row[i],
        }
    }
}

/// Assemble les trames d'une page ; renvoie l'image et si elle est au trait.
pub fn assemble(frames: &[Frame]) -> Result<(DynamicImage, bool)> {
    let first = frames.first().ok_or_else(|| Error::Other("aucune donnée reçue".into()))?;
    let width = first.params.pixels_per_line.max(0) as u32;
    let height = frames.iter().map(|f| f.lines()).min().unwrap_or(0) as u32;
    if width == 0 || height == 0 {
        return Err(Error::Other("image vide".into()));
    }
    let depth = first.params.depth;
    match first.params.format {
        FRAME_GRAY => {
            let mut img = GrayImage::new(width, height);
            for y in 0..height as usize {
                let row = first.row(y);
                for x in 0..width as usize {
                    img.put_pixel(x as u32, y as u32, image::Luma([first.sample(row, x)]));
                }
            }
            Ok((DynamicImage::ImageLuma8(img), depth == 1))
        }
        FRAME_RGB => {
            let mut img = RgbImage::new(width, height);
            for y in 0..height as usize {
                let row = first.row(y);
                for x in 0..width as usize {
                    let i = x * 3;
                    let px = [first.sample(row, i), first.sample(row, i + 1), first.sample(row, i + 2)];
                    img.put_pixel(x as u32, y as u32, image::Rgb(px));
                }
            }
            Ok((DynamicImage::ImageRgb8(img), false))
        }
        FRAME_RED | FRAME_GREEN | FRAME_BLUE => {
            let channel = |format| frames.iter().find(|f| f.params.format == format);
            let (Some(r), Some(g), Some(b)) = (channel(FRAME_RED), channel(FRAME_GREEN), channel(FRAME_BLUE))
            else {
                return Err(Error::Other("trame couleur incomplète".into()));
            };
            let mut img = RgbImage::new(width, height);
            for y in 0..height as usize {
                let (rr, gr, br) = (r.row(y), g.row(y), b.row(y));
                for x in 0..width as usize {
                    let px = [r.sample(rr, x), g.sample(gr, x), b.sample(br, x)];
                    img.put_pixel(x as u32, y as u32, image::Rgb(px));
                }
            }
            Ok((DynamicImage::ImageRgb8(img), false))
        }
        other => Err(Error::Other(format!("format de trame non pris en charge ({other})"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(format: i32, depth: i32, ppl: i32, bpl: i32, lines: i32) -> Parameters {
        Parameters {
            format,
            last_frame: 1,
            bytes_per_line: bpl,
            pixels_per_line: ppl,
            lines,
            depth,
        }
    }

    #[test]
    fn lineart() {
        // 10 pixels → 2 octets par ligne ; premier pixel noir
        let f = Frame {
            params: params(FRAME_GRAY, 1, 10, 2, 2),
            data: vec![0b1000_0000, 0, 0xff, 0xc0],
        };
        let (img, bilevel) = assemble(&[f]).unwrap();
        assert!(bilevel);
        let g = img.to_luma8();
        assert_eq!(g.dimensions(), (10, 2));
        assert_eq!(g.get_pixel(0, 0).0[0], 0);
        assert_eq!(g.get_pixel(1, 0).0[0], 255);
        assert_eq!(g.get_pixel(9, 1).0[0], 0);
    }

    #[test]
    fn rgb16_unknown_height() {
        let px = 0xab00u16.to_ne_bytes();
        let row: Vec<u8> = (0..6).flat_map(|_| px).collect();
        let f = Frame {
            params: params(FRAME_RGB, 16, 2, 12, -1),
            data: [row.clone(), row].concat(),
        };
        let (img, _) = assemble(&[f]).unwrap();
        let rgb = img.to_rgb8();
        assert_eq!(rgb.dimensions(), (2, 2));
        assert_eq!(rgb.get_pixel(1, 1).0, [0xab, 0xab, 0xab]);
    }

    #[test]
    fn three_pass() {
        let mk = |format, v| Frame {
            params: params(format, 8, 1, 1, 1),
            data: vec![v],
        };
        let (img, _) = assemble(&[mk(FRAME_GREEN, 2), mk(FRAME_RED, 1), mk(FRAME_BLUE, 3)]).unwrap();
        assert_eq!(img.to_rgb8().get_pixel(0, 0).0, [1, 2, 3]);
    }
}
