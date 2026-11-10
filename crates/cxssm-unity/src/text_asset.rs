//! `TextAsset` (class 49) : `m_Name: string` puis `m_Script: byte[]`.
//! FMOD for Unity stocke chaque `.bank` dans un TextAsset nommé `<Banque>.bank`
//! (ou `<Banque>` selon la version du plugin) ; `m_Script` contient les octets bruts.

use crate::error::Result;
use crate::io::{Endian, Reader};

#[derive(Debug, Clone)]
pub struct TextAsset {
    pub name: String,
    pub script: Vec<u8>,
}

impl TextAsset {
    pub fn parse(data: &[u8], endian: Endian) -> Result<Self> {
        let mut r = Reader::new(data, endian);
        let name = aligned_bytes(&mut r)?;
        let script = aligned_bytes(&mut r)?;
        Ok(Self { name: String::from_utf8_lossy(&name).into_owned(), script })
    }

    pub fn to_bytes(&self, endian: Endian) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.script.len() + self.name.len() + 16);
        write_aligned(&mut out, self.name.as_bytes(), endian);
        write_aligned(&mut out, &self.script, endian);
        out
    }

    /// Vrai si le contenu ressemble à une banque FMOD Studio (`RIFF....FEV `).
    pub fn is_fmod_bank(&self) -> bool {
        self.script.len() >= 12 && &self.script[0..4] == b"RIFF" && &self.script[8..12] == b"FEV "
    }
}

fn aligned_bytes(r: &mut Reader<'_>) -> Result<Vec<u8>> {
    let len = r.i32()?.max(0) as usize;
    let v = r.take(len)?.to_vec();
    r.align(4);
    Ok(v)
}

fn write_aligned(out: &mut Vec<u8>, b: &[u8], endian: Endian) {
    let len = b.len() as i32;
    out.extend_from_slice(&match endian {
        Endian::Little => len.to_le_bytes(),
        Endian::Big => len.to_be_bytes(),
    });
    out.extend_from_slice(b);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let t = TextAsset { name: "Engine.bank".into(), script: b"RIFF\0\0\0\0FEV xyz".to_vec() };
        let b = t.to_bytes(Endian::Little);
        let back = TextAsset::parse(&b, Endian::Little).unwrap();
        assert_eq!(back.name, "Engine.bank");
        assert!(back.is_fmod_bank());
    }
}
