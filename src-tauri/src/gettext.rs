//! Lecture des catalogues gettext (.mo) de sane-backends pour traduire les
//! titres et descriptions des options, comme le fait KSane pour Skanpage.

use std::collections::HashMap;
use std::path::Path;

#[derive(Default)]
pub struct Catalog {
    map: HashMap<String, String>,
}

impl Catalog {
    /// Catalogue de sane-backends pour la langue (`fr`) ; vide s'il est absent.
    pub fn sane_backends(lang: &str) -> Catalog {
        let dirs = ["/usr/share/locale", "/usr/local/share/locale"];
        for dir in dirs {
            let path = Path::new(dir).join(lang).join("LC_MESSAGES/sane-backends.mo");
            if let Ok(bytes) = std::fs::read(&path)
                && let Some(c) = Catalog::parse(&bytes)
            {
                return c;
            }
        }
        Catalog::default()
    }

    pub fn parse(data: &[u8]) -> Option<Catalog> {
        let le = match data.get(0..4)? {
            [0xde, 0x12, 0x04, 0x95] => true,
            [0x95, 0x04, 0x12, 0xde] => false,
            _ => return None,
        };
        let u32_at = |off: usize| -> Option<usize> {
            let b: [u8; 4] = data.get(off..off + 4)?.try_into().ok()?;
            Some(if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) } as usize)
        };
        let n = u32_at(8)?;
        let orig = u32_at(12)?;
        let trans = u32_at(16)?;
        let string = |table: usize, i: usize| -> Option<&str> {
            let len = u32_at(table + i * 8)?;
            let off = u32_at(table + i * 8 + 4)?;
            std::str::from_utf8(data.get(off..off + len)?).ok()
        };
        let mut map = HashMap::with_capacity(n);
        for i in 0..n {
            let (Some(k), Some(v)) = (string(orig, i), string(trans, i)) else { continue };
            if k.is_empty() || v.is_empty() {
                continue;
            }
            // formes plurielles : seule la première nous intéresse
            let k = k.split('\0').next().unwrap_or(k);
            let v = v.split('\0').next().unwrap_or(v);
            map.insert(k.to_string(), v.to_string());
        }
        Some(Catalog { map })
    }

    pub fn translate(&self, s: &str) -> String {
        if s.is_empty() {
            return String::new();
        }
        self.map.get(s).cloned().unwrap_or_else(|| s.to_string())
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_catalog() {
        // présent sur la plupart des systèmes Linux francophones ; sinon le test est neutre
        let c = Catalog::sane_backends("fr");
        if !c.is_empty() {
            assert_ne!(c.translate("Scan resolution"), "");
        }
        assert_eq!(c.translate("zzz-not-there"), "zzz-not-there");
    }
}
