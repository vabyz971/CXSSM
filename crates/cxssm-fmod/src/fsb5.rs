//! Format **FSB5** (FMOD Sound Bank v5), little endian.
//!
//! En-tête (v1 = 0x3C octets, v0 = 0x40) :
//! `"FSB5" | version | n_samples | taille_entetes | taille_noms | taille_donnees | codec | …`
//!
//! En-tête d'échantillon : un `u64` compacté (cf. vgmstream `fsb5.c`) :
//! - bit 0       : chunks supplémentaires présents
//! - bits 1..4   : index de fréquence
//! - bits 5..6   : canaux (0=1, 1=2, 2=6, 3=8)
//! - bits 7..33  : offset des données / 32
//! - bits 34..63 : nombre d'échantillons PCM (durée)
//!
//! Suivi éventuellement de chunks `u32` : bit 0 = suivant, bits 1..24 = taille, bits 25..31 = type.

use crate::error::{Error, Result};

const FREQUENCIES: [u32; 11] = [4000, 8000, 11000, 11025, 16000, 22050, 24000, 32000, 44100, 48000, 96000];
const CHANNELS: [u16; 4] = [1, 2, 6, 8];
const DATA_ALIGN: usize = 32;

/// Types de chunks d'en-tête d'échantillon.
const CHUNK_CHANNELS: u8 = 1;
const CHUNK_FREQUENCY: u8 = 2;
const CHUNK_LOOP: u8 = 3;
const CHUNK_VORBIS: u8 = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Pcm8,
    Pcm16,
    Pcm24,
    Pcm32,
    PcmFloat,
    GcAdpcm,
    ImaAdpcm,
    Vag,
    HeVag,
    Xma,
    Mpeg,
    Celt,
    At9,
    Xwma,
    Vorbis,
    FAdpcm,
    Opus,
    Unknown(u32),
}

impl Codec {
    pub fn from_mode(m: u32) -> Self {
        use Codec::*;
        match m {
            1 => Pcm8,
            2 => Pcm16,
            3 => Pcm24,
            4 => Pcm32,
            5 => PcmFloat,
            6 => GcAdpcm,
            7 => ImaAdpcm,
            8 => Vag,
            9 => HeVag,
            10 => Xma,
            11 => Mpeg,
            12 => Celt,
            13 => At9,
            14 => Xwma,
            15 => Vorbis,
            16 => FAdpcm,
            17 => Opus,
            n => Unknown(n),
        }
    }

    /// Nom de format attendu par `fsbankcl -format`.
    pub fn fsbank_format(self) -> Option<&'static str> {
        match self {
            Codec::Pcm16 | Codec::Pcm8 | Codec::Pcm24 | Codec::Pcm32 | Codec::PcmFloat => Some("pcm"),
            Codec::Vorbis => Some("vorbis"),
            Codec::FAdpcm => Some("fadpcm"),
            Codec::Opus => Some("opus"),
            _ => None,
        }
    }
}

impl std::fmt::Display for Codec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Codec::Unknown(n) => write!(f, "Inconnu({n})"),
            c => write!(f, "{c:?}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sample {
    pub name: String,
    pub frequency: u32,
    pub channels: u16,
    pub num_samples: u64,
    pub loop_range: Option<(u32, u32)>,
    /// CRC de l'en-tête Vorbis (FMOD remplace les en-têtes Vorbis par un CRC
    /// pointant vers une table d'en-têtes pré-calculés).
    pub vorbis_crc: Option<u32>,
    /// Chunks supplémentaires bruts (type, contenu) – réécrits tels quels.
    pub chunks: Vec<(u8, Vec<u8>)>,
    pub data: Vec<u8>,
    /// Bits bruts (index fréquence, index canaux) lus dans un FSB existant.
    /// `Some` ⇒ l'en-tête est réécrit à l'identique (bits + chunks verbatim) ;
    /// `None` ⇒ échantillon créé par CXSSM, bits et chunks calculés.
    pub header_bits: Option<(u8, u8)>,
}

impl Sample {
    pub fn duration_secs(&self) -> f64 {
        if self.frequency == 0 {
            0.0
        } else {
            self.num_samples as f64 / self.frequency as f64
        }
    }
}

#[derive(Debug, Clone)]
pub struct Fsb5 {
    pub version: u32,
    pub codec: Codec,
    mode: u32,
    /// Octets 0x1C.. de l'en-tête (zéros + hash) conservés à l'identique.
    header_tail: Vec<u8>,
    /// Le FSB d'origine contient une table des noms (CarX Street : non, noms supprimés).
    has_names: bool,
    pub samples: Vec<Sample>,
}

impl Fsb5 {
    pub fn parse(d: &[u8]) -> Result<Self> {
        if d.get(0..4) != Some(b"FSB5") {
            return Err(Error::BadSignature("FSB5".into()));
        }
        let version = u32_at(d, 4)?;
        let n = u32_at(d, 8)? as usize;
        let shdr_size = u32_at(d, 12)? as usize;
        let name_size = u32_at(d, 16)? as usize;
        let data_size = u32_at(d, 20)? as usize;
        let mode = u32_at(d, 24)?;
        let header_size = if version == 0 { 0x40 } else { 0x3C };
        let header_tail = slice(d, 28, header_size - 28)?.to_vec();

        // --- en-têtes d'échantillons ---
        struct Raw {
            offset: usize,
            s: Sample,
        }
        let mut raws = Vec::with_capacity(n);
        let mut p = header_size;
        for _ in 0..n {
            let m = u64_at(d, p)?;
            p += 8;
            let mut s = Sample {
                name: String::new(),
                frequency: FREQUENCIES.get(((m >> 1) & 0xF) as usize).copied().unwrap_or(44100),
                channels: CHANNELS[((m >> 5) & 0x3) as usize],
                num_samples: (m >> 34) & 0x3FFF_FFFF,
                loop_range: None,
                vorbis_crc: None,
                chunks: Vec::new(),
                data: Vec::new(),
                header_bits: Some((((m >> 1) & 0xF) as u8, ((m >> 5) & 0x3) as u8)),
            };
            let offset = (((m >> 7) & 0x07FF_FFFF) << 5) as usize;
            let mut more = m & 1 == 1;
            while more {
                let c = u32_at(d, p)?;
                p += 4;
                more = c & 1 == 1;
                let size = ((c >> 1) & 0x00FF_FFFF) as usize;
                let ty = (c >> 25) as u8;
                let body = slice(d, p, size)?;
                p += size;
                match ty {
                    CHUNK_CHANNELS if !body.is_empty() => s.channels = body[0] as u16,
                    CHUNK_FREQUENCY if size >= 4 => s.frequency = u32_at(body, 0)?,
                    CHUNK_LOOP if size >= 8 => s.loop_range = Some((u32_at(body, 0)?, u32_at(body, 4)?)),
                    CHUNK_VORBIS if size >= 4 => s.vorbis_crc = Some(u32_at(body, 0)?),
                    _ => {}
                }
                s.chunks.push((ty, body.to_vec()));
            }
            raws.push(Raw { offset, s });
        }

        // --- noms ---
        let names_start = header_size + shdr_size;
        if name_size > 0 {
            for (i, r) in raws.iter_mut().enumerate() {
                let off = u32_at(d, names_start + i * 4)? as usize;
                let start = names_start + off;
                let rest = d.get(start..names_start + name_size).ok_or(Error::UnexpectedEof(start))?;
                let len = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
                r.s.name = String::from_utf8_lossy(&rest[..len]).into_owned();
            }
        }
        for (i, r) in raws.iter_mut().enumerate() {
            if r.s.name.is_empty() {
                r.s.name = format!("sample_{i:04}");
            }
        }

        // --- données ---
        let data_start = names_start + name_size;
        let offsets: Vec<usize> = raws.iter().map(|r| r.offset).collect();
        for (i, r) in raws.iter_mut().enumerate() {
            let end = offsets.get(i + 1).copied().unwrap_or(data_size);
            r.s.data = slice(d, data_start + r.offset, end.saturating_sub(r.offset))?.to_vec();
        }

        Ok(Self {
            version,
            codec: Codec::from_mode(mode),
            mode,
            header_tail,
            has_names: name_size > 0,
            samples: raws.into_iter().map(|r| r.s).collect(),
        })
    }

    /// FSB ne contenant que l'échantillon `index` (pour décoder un seul son rapidement).
    pub fn single(&self, index: usize) -> Result<Self> {
        let s = self.samples.get(index).ok_or(Error::BadIndex(index))?.clone();
        Ok(Self { samples: vec![s], ..self.clone_header() })
    }

    fn clone_header(&self) -> Self {
        Self {
            version: self.version,
            codec: self.codec,
            mode: self.mode,
            header_tail: self.header_tail.clone(),
            has_names: self.has_names,
            samples: Vec::new(),
        }
    }

    /// Vérifie qu'un échantillon de remplacement est compatible avec l'original.
    pub fn check_compatible(&self, index: usize, other_codec: Codec, new: &Sample) -> Result<()> {
        let orig = self.samples.get(index).ok_or(Error::BadIndex(index))?;
        if other_codec != self.codec {
            return Err(Error::Incompatible(format!("codec {other_codec} ≠ {}", self.codec)));
        }
        if new.channels != orig.channels {
            return Err(Error::Incompatible(format!("{} canal(aux) ≠ {}", new.channels, orig.channels)));
        }
        if new.frequency != orig.frequency {
            return Err(Error::Incompatible(format!("{} Hz ≠ {} Hz", new.frequency, orig.frequency)));
        }
        Ok(())
    }

    /// Remplace l'échantillon `index` en conservant son nom (référencé par la banque).
    pub fn replace(&mut self, index: usize, mut new: Sample) -> Result<()> {
        let slot = self.samples.get_mut(index).ok_or(Error::BadIndex(index))?;
        new.name = slot.name.clone();
        *slot = new;
        Ok(())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        // Données : chaque échantillon commence sur un multiple de 32 octets.
        // Pas de padding après le dernier (comme FMOD).
        let mut data = Vec::new();
        let mut offsets = Vec::with_capacity(self.samples.len());
        for s in &self.samples {
            while !data.len().is_multiple_of(DATA_ALIGN) {
                data.push(0);
            }
            offsets.push(data.len());
            data.extend_from_slice(&s.data);
        }

        let mut shdr = Vec::new();
        for (s, &off) in self.samples.iter().zip(&offsets) {
            let (freq_idx, ch_idx, chunks) = header_fields(s);
            let m: u64 = (!chunks.is_empty() as u64)
                | ((freq_idx as u64) << 1)
                | ((ch_idx as u64) << 5)
                | (((off as u64) >> 5) << 7)
                | ((s.num_samples & 0x3FFF_FFFF) << 34);
            shdr.extend_from_slice(&m.to_le_bytes());
            let last = chunks.len().saturating_sub(1);
            for (i, (ty, body)) in chunks.iter().enumerate() {
                let c: u32 = (i != last) as u32 | ((body.len() as u32 & 0x00FF_FFFF) << 1) | ((*ty as u32) << 25);
                shdr.extend_from_slice(&c.to_le_bytes());
                shdr.extend_from_slice(body);
            }
        }

        // Table des noms : offsets puis chaînes terminées par \0 (seulement si l'original en a une).
        let mut names = Vec::new();
        if self.has_names {
            names.resize(self.samples.len() * 4, 0);
            for (i, s) in self.samples.iter().enumerate() {
                let off = names.len() as u32;
                names[i * 4..i * 4 + 4].copy_from_slice(&off.to_le_bytes());
                names.extend_from_slice(s.name.as_bytes());
                names.push(0);
            }
            while !names.len().is_multiple_of(16) {
                names.push(0);
            }
        }

        // FMOD aligne le début des données audio sur 32 octets : le padding est
        // compté dans la taille des en-têtes d'échantillons.
        let header_size = if self.version == 0 { 0x40 } else { 0x3C };
        while !(header_size + shdr.len() + names.len()).is_multiple_of(DATA_ALIGN) {
            shdr.push(0);
        }

        let mut out = Vec::with_capacity(0x40 + shdr.len() + names.len() + data.len());
        out.extend_from_slice(b"FSB5");
        for v in [
            self.version,
            self.samples.len() as u32,
            shdr.len() as u32,
            names.len() as u32,
            data.len() as u32,
            self.mode,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.header_tail);
        out.extend(shdr);
        out.extend(names);
        out.extend(data);
        out
    }
}

/// (index fréquence, index canaux, chunks) à écrire pour un échantillon.
/// Chunks d'en-tête : (type, contenu).
type Chunks<'a> = std::borrow::Cow<'a, [(u8, Vec<u8>)]>;

fn header_fields(s: &Sample) -> (u8, u8, Chunks<'_>) {
    use std::borrow::Cow;
    if let Some((f, c)) = s.header_bits {
        return (f, c, Cow::Borrowed(&s.chunks));
    }
    let freq = FREQUENCIES.iter().position(|&f| f == s.frequency);
    let ch = CHANNELS.iter().position(|&c| c == s.channels);
    let mut chunks: Vec<(u8, Vec<u8>)> = s
        .chunks
        .iter()
        .filter(|(t, _)| *t != CHUNK_FREQUENCY && *t != CHUNK_CHANNELS)
        .cloned()
        .collect();
    if freq.is_none() {
        chunks.insert(0, (CHUNK_FREQUENCY, s.frequency.to_le_bytes().to_vec()));
    }
    if ch.is_none() {
        chunks.insert(0, (CHUNK_CHANNELS, vec![s.channels as u8]));
    }
    (freq.unwrap_or(8) as u8, ch.unwrap_or(0) as u8, Cow::Owned(chunks))
}

fn slice(d: &[u8], p: usize, n: usize) -> Result<&[u8]> {
    d.get(p..p + n).ok_or(Error::UnexpectedEof(p))
}
fn u32_at(d: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(slice(d, p, 4)?.try_into().expect("4")))
}
fn u64_at(d: &[u8], p: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(slice(d, p, 8)?.try_into().expect("8")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str, len: usize) -> Sample {
        Sample {
            name: name.into(),
            frequency: 48000,
            channels: 2,
            num_samples: 1234,
            loop_range: Some((0, 1233)),
            vorbis_crc: None,
            chunks: vec![(CHUNK_LOOP, [0u32.to_le_bytes(), 1233u32.to_le_bytes()].concat())],
            data: vec![0xAB; len],
            header_bits: None,
        }
    }

    #[test]
    fn roundtrip() {
        let f = Fsb5 {
            version: 1,
            codec: Codec::Pcm16,
            mode: 2,
            header_tail: vec![0; 0x3C - 28],
            has_names: true,
            samples: vec![sample("engine_on_3000", 100), sample("engine_off_3000", 64)],
        };
        let back = Fsb5::parse(&f.to_bytes()).unwrap();
        assert_eq!(back.samples.len(), 2);
        assert_eq!(back.samples[1].name, "engine_off_3000");
        assert_eq!(back.samples[0].frequency, 48000);
        assert_eq!(back.samples[0].channels, 2);
        assert_eq!(back.samples[0].loop_range, Some((0, 1233)));
        assert_eq!(&back.samples[0].data[..100], &[0xAB; 100][..]);
    }
}
