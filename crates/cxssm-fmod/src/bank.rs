//! Banque FMOD Studio : conteneur RIFF de forme `FEV `.

use crate::error::{Error, Result};
use crate::fsb5::Fsb5;

#[derive(Debug, Clone)]
pub struct Bank {
    raw: Vec<u8>,
    /// Offset absolu du chunk `SND ` (son identifiant).
    snd_chunk: usize,
    /// Offset absolu du début du FSB5 dans le chunk `SND ` (après padding).
    fsb_offset: usize,
    fsb_len: usize,
}

impl Bank {
    pub fn parse(raw: Vec<u8>) -> Result<Self> {
        if raw.len() < 12 || &raw[0..4] != b"RIFF" || &raw[8..12] != b"FEV " {
            return Err(Error::BadSignature("RIFF/FEV attendu".into()));
        }
        let snd_chunk = find_chunk(&raw, 12, raw.len(), b"SND ").ok_or(Error::ChunkNotFound("SND "))?;
        let size = u32_at(&raw, snd_chunk + 4)? as usize;
        let payload = snd_chunk + 8;
        let end = (payload + size).min(raw.len());
        // Le FSB5 est aligné (souvent sur 32 octets) : on cherche la signature.
        let rel = raw[payload..end]
            .windows(4)
            .position(|w| w == b"FSB5")
            .ok_or(Error::BadSignature("FSB5 absent du chunk SND".into()))?;
        let fsb_offset = payload + rel;
        Ok(Self { raw, snd_chunk, fsb_offset, fsb_len: end - fsb_offset })
    }

    /// Octets bruts du FSB5 contenu dans le chunk `SND `.
    pub fn fsb_bytes(&self) -> &[u8] {
        &self.raw[self.fsb_offset..self.fsb_offset + self.fsb_len]
    }

    pub fn fsb(&self) -> Result<Fsb5> {
        Fsb5::parse(self.fsb_bytes())
    }

    /// Remplace le FSB5 en recalculant la taille du chunk `SND `, du RIFF et l'entrée
    /// de l'index `SNDH` (position et taille du FSB, lues par FMOD au chargement : une
    /// taille périmée fait refuser la banque ou tronque ses derniers sons).
    pub fn with_fsb(&self, fsb: &[u8]) -> Vec<u8> {
        let old_end = self.fsb_offset + self.fsb_len;
        let mut out = Vec::with_capacity(self.raw.len() - self.fsb_len + fsb.len());
        out.extend_from_slice(&self.raw[..self.fsb_offset]);
        out.extend_from_slice(fsb);
        let new_chunk_size = (out.len() - (self.snd_chunk + 8)) as u32;
        out.extend_from_slice(&self.raw[old_end..]);
        out[self.snd_chunk + 4..self.snd_chunk + 8].copy_from_slice(&new_chunk_size.to_le_bytes());
        let riff = (out.len() - 8) as u32;
        out[4..8].copy_from_slice(&riff.to_le_bytes());
        // L'index précède le chunk `SND ` : ses offsets ne bougent pas.
        if let Some(sndh) = find_chunk(&out, 12, self.snd_chunk, b"SNDH") {
            let size = u32_at(&out, sndh + 4).unwrap_or(0) as usize;
            let body = sndh + 8;
            // En-tête de 4 octets puis des paires (offset, taille) en u32.
            let mut e = body + 4;
            while e + 8 <= body + size {
                if u32_at(&out, e).ok() == Some(self.fsb_offset as u32) {
                    out[e + 4..e + 8].copy_from_slice(&(fsb.len() as u32).to_le_bytes());
                }
                e += 8;
            }
        }
        out
    }

    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
}

/// Parcours récursif des chunks RIFF (les `LIST` contiennent d'autres chunks).
fn find_chunk(data: &[u8], mut pos: usize, end: usize, id: &[u8; 4]) -> Option<usize> {
    while pos + 8 <= end {
        let cid = &data[pos..pos + 4];
        let size = u32_at(data, pos + 4).ok()? as usize;
        if cid == id {
            return Some(pos);
        }
        if cid == b"LIST" {
            if let Some(p) = find_chunk(data, pos + 12, (pos + 8 + size).min(end), id) {
                return Some(p);
            }
        }
        pos += 8 + size + (size & 1);
    }
    None
}

fn u32_at(d: &[u8], p: usize) -> Result<u32> {
    d.get(p..p + 4)
        .map(|b| u32::from_le_bytes(b.try_into().expect("4 octets")))
        .ok_or(Error::UnexpectedEof(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut c = id.to_vec();
        c.extend((body.len() as u32).to_le_bytes());
        c.extend(body);
        c
    }

    /// Banque minimale : `LIST:PROJ { SNDH }` puis `SND ` (padding + FSB5).
    fn bank(fsb: &[u8]) -> Vec<u8> {
        // RIFF(12) + LIST(8) + "PROJ"(4) + SNDH(8 + 12) = 44, puis SND (8) + 4 de padding.
        let fsb_offset = 44 + 8 + 4;
        let mut sndh = 0x0008_0003u32.to_le_bytes().to_vec();
        sndh.extend((fsb_offset as u32).to_le_bytes());
        sndh.extend((fsb.len() as u32).to_le_bytes());
        let mut proj = b"PROJ".to_vec();
        proj.extend(chunk(b"SNDH", &sndh));
        let mut snd = vec![0u8; 4];
        snd.extend(fsb);
        let mut body = b"FEV ".to_vec();
        body.extend(chunk(b"LIST", &proj));
        body.extend(chunk(b"SND ", &snd));
        let raw = chunk(b"RIFF", &body);
        assert_eq!(&raw[fsb_offset..fsb_offset + 4], b"FSB5");
        raw
    }

    #[test]
    fn with_fsb_updates_sizes_and_sndh_index() {
        let raw = bank(b"FSB5 ancien");
        let b = Bank::parse(raw).unwrap();
        for new in [&b"FSB5 nouveau, plus long"[..], &b"FSB5"[..]] {
            let out = b.with_fsb(new);
            let back = Bank::parse(out.clone()).unwrap();
            assert_eq!(back.fsb_bytes(), new);
            assert_eq!(u32_at(&out, 4).unwrap() as usize, out.len() - 8);
            let sndh = find_chunk(&out, 12, out.len(), b"SNDH").unwrap();
            assert_eq!(u32_at(&out, sndh + 12).unwrap() as usize, back.fsb_offset);
            assert_eq!(u32_at(&out, sndh + 16).unwrap() as usize, new.len());
        }
    }
}
