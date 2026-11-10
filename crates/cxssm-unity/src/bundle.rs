//! Format conteneur **UnityFS** (Unity 5.3+).
//!
//! Disposition (tout en big endian) :
//! ```text
//! "UnityFS\0" | u32 version | cstr unity_ver | cstr unity_rev | i64 taille totale
//! u32 taille_blocksinfo_compressée | u32 taille_blocksinfo | u32 flags
//! [alignement 16 si version >= 7]
//! BlocksInfo (compressé) : hash[16] | i32 n_blocs | {u32 usize, u32 csize, u16 flags}*
//!                          i32 n_noeuds | {i64 offset, i64 taille, u32 flags, cstr chemin}*
//! [alignement 16 si flag 0x200]
//! Blocs de données (compressés individuellement)
//! ```

use std::path::Path;

use crate::error::{Error, Result};
use crate::io::{BeWriter, Endian, Reader};

const SIGNATURE: &str = "UnityFS";
/// Taille de bloc utilisée par Unity lors de la réécriture (128 Kio).
const BLOCK_SIZE: usize = 0x2_0000;

const FLAG_COMPRESSION_MASK: u32 = 0x3F;
const FLAG_BLOCKS_INFO_AT_END: u32 = 0x80;
const FLAG_PADDING_AT_START: u32 = 0x200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    Lzma,
    Lz4,
    Lz4Hc,
}

impl Compression {
    fn from_flags(flags: u32) -> Result<Self> {
        match flags & FLAG_COMPRESSION_MASK {
            0 => Ok(Self::None),
            1 => Ok(Self::Lzma),
            2 => Ok(Self::Lz4),
            3 => Ok(Self::Lz4Hc),
            n => Err(Error::UnsupportedCompression(n)),
        }
    }

    fn code(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Lzma => 1,
            Self::Lz4 => 2,
            Self::Lz4Hc => 3,
        }
    }
}

/// Fichier virtuel contenu dans le bundle (ex. `CAB-1a2b...`, `CAB-1a2b....resS`).
#[derive(Debug, Clone)]
pub struct Node {
    pub path: String,
    pub flags: u32,
    pub data: Vec<u8>,
}

/// AssetBundle décompressé en mémoire.
#[derive(Debug, Clone)]
pub struct Bundle {
    pub format_version: u32,
    pub unity_version: String,
    pub unity_revision: String,
    pub flags: u32,
    pub compression: Compression,
    pub nodes: Vec<Node>,
}

impl Bundle {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::parse(&std::fs::read(path)?)
    }

    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data, Endian::Big);
        let sig = r.cstr()?;
        if sig != SIGNATURE {
            return Err(Error::BadSignature { expected: SIGNATURE, found: sig });
        }
        let format_version = r.u32()?;
        if !(6..=8).contains(&format_version) {
            return Err(Error::UnsupportedVersion(format_version));
        }
        let unity_version = r.cstr()?;
        let unity_revision = r.cstr()?;
        let _total_size = r.i64()?;
        let info_csize = r.u32()? as usize;
        let info_usize = r.u32()? as usize;
        let flags = r.u32()?;
        if format_version >= 7 {
            r.align(16);
        }

        let info_raw = if flags & FLAG_BLOCKS_INFO_AT_END != 0 {
            let start = data.len().checked_sub(info_csize).ok_or(Error::UnexpectedEof(0))?;
            &data[start..]
        } else {
            r.take(info_csize)?
        };
        let info = decompress(info_raw, info_usize, Compression::from_flags(flags)?)?;
        if flags & FLAG_PADDING_AT_START != 0 {
            r.align(16);
        }

        // --- BlocksInfo ---
        let mut ir = Reader::new(&info, Endian::Big);
        ir.take(16)?; // hash non vérifié par Unity
        let block_count = ir.i32()?;
        let mut blocks = Vec::with_capacity(block_count.max(0) as usize);
        for _ in 0..block_count {
            let usize_ = ir.u32()? as usize;
            let csize = ir.u32()? as usize;
            let bflags = ir.u16()? as u32;
            blocks.push((usize_, csize, bflags));
        }

        // Décompression de tous les blocs en un flux continu.
        let total: usize = blocks.iter().map(|b| b.0).sum();
        let mut stream = Vec::with_capacity(total);
        for (usize_, csize, bflags) in blocks {
            let raw = r.take(csize)?;
            stream.extend(decompress(raw, usize_, Compression::from_flags(bflags)?)?);
        }

        let node_count = ir.i32()?;
        let mut nodes = Vec::with_capacity(node_count.max(0) as usize);
        for _ in 0..node_count {
            let offset = ir.i64()? as usize;
            let size = ir.i64()? as usize;
            let nflags = ir.u32()?;
            let path = ir.cstr()?;
            let data = stream
                .get(offset..offset + size)
                .ok_or(Error::UnexpectedEof(offset))?
                .to_vec();
            nodes.push(Node { path, flags: nflags, data });
        }

        log::info!(
            "bundle UnityFS v{format_version} ({unity_revision}), {} nœud(s), compression {:?}",
            nodes.len(),
            Compression::from_flags(flags)?
        );

        Ok(Self {
            format_version,
            unity_version,
            unity_revision,
            flags,
            compression: Compression::from_flags(flags)?,
            nodes,
        })
    }

    pub fn node(&self, path: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.path == path)
    }

    pub fn node_mut(&mut self, path: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.path == path)
    }

    /// Sérialise le bundle. Les données sont recompressées en LZ4 (rapide,
    /// supporté par toutes les versions de Unity) ; les BlocksInfo aussi.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        // 1. Flux continu des nœuds + table des nœuds.
        let mut stream = Vec::new();
        let mut node_table = Vec::with_capacity(self.nodes.len());
        for n in &self.nodes {
            node_table.push((stream.len() as i64, n.data.len() as i64, n.flags, n.path.as_str()));
            stream.extend_from_slice(&n.data);
        }

        // 2. Découpage en blocs compressés LZ4.
        let mut blocks_data = Vec::new();
        let mut block_table = Vec::new();
        for chunk in stream.chunks(BLOCK_SIZE) {
            let c = lz4_flex::block::compress(chunk);
            // Unity garde le bloc brut si la compression n'apporte rien.
            if c.len() < chunk.len() {
                block_table.push((chunk.len() as u32, c.len() as u32, Compression::Lz4.code() as u16));
                blocks_data.extend(c);
            } else {
                block_table.push((chunk.len() as u32, chunk.len() as u32, 0));
                blocks_data.extend_from_slice(chunk);
            }
        }

        // 3. BlocksInfo.
        let mut info = BeWriter::default();
        info.bytes(&[0u8; 16]);
        info.i32(block_table.len() as i32);
        for (u, c, f) in &block_table {
            info.u32(*u);
            info.u32(*c);
            info.u16(*f);
        }
        info.i32(node_table.len() as i32);
        for (off, size, flags, path) in &node_table {
            info.i64(*off);
            info.i64(*size);
            info.u32(*flags);
            info.cstr(path);
        }
        let info_c = lz4_flex::block::compress(&info.0);

        // 4. En-tête. BlocksInfo placé juste après l'en-tête.
        let flags = (self.flags & !FLAG_COMPRESSION_MASK & !FLAG_BLOCKS_INFO_AT_END)
            | Compression::Lz4.code();
        let mut out = BeWriter::default();
        out.cstr(SIGNATURE);
        out.u32(self.format_version);
        out.cstr(&self.unity_version);
        out.cstr(&self.unity_revision);
        let size_pos = out.0.len();
        out.i64(0); // taille totale, corrigée plus bas
        out.u32(info_c.len() as u32);
        out.u32(info.0.len() as u32);
        out.u32(flags);
        if self.format_version >= 7 {
            out.align(16);
        }
        out.bytes(&info_c);
        if flags & FLAG_PADDING_AT_START != 0 {
            out.align(16);
        }
        out.bytes(&blocks_data);

        let total = out.0.len() as i64;
        out.0[size_pos..size_pos + 8].copy_from_slice(&total.to_be_bytes());
        Ok(out.0)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_bytes()?)?;
        Ok(())
    }
}

fn decompress(raw: &[u8], usize_: usize, c: Compression) -> Result<Vec<u8>> {
    match c {
        Compression::None => Ok(raw.to_vec()),
        Compression::Lz4 | Compression::Lz4Hc => lz4_flex::block::decompress(raw, usize_)
            .map_err(|e| Error::Decompress(e.to_string())),
        Compression::Lzma => {
            // Unity : 5 octets de propriétés LZMA, sans taille décompressée.
            let mut out = Vec::with_capacity(usize_);
            let opts = lzma_rs::decompress::Options {
                unpacked_size: lzma_rs::decompress::UnpackedSize::UseProvided(Some(usize_ as u64)),
                ..Default::default()
            };
            lzma_rs::lzma_decompress_with_options(&mut std::io::Cursor::new(raw), &mut out, &opts)
                .map_err(|e| Error::Decompress(e.to_string()))?;
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_lz4() {
        let b = Bundle {
            format_version: 8,
            unity_version: "5.x.x".into(),
            unity_revision: "2022.3.0f1".into(),
            flags: 0x43,
            compression: Compression::Lz4Hc,
            nodes: vec![
                Node { path: "CAB-test".into(), flags: 4, data: vec![7u8; 300_000] },
                Node { path: "CAB-test.resS".into(), flags: 0, data: (0..=255).collect() },
            ],
        };
        let bytes = b.to_bytes().unwrap();
        let back = Bundle::parse(&bytes).unwrap();
        assert_eq!(back.nodes.len(), 2);
        assert_eq!(back.nodes[0].data, b.nodes[0].data);
        assert_eq!(back.nodes[1].path, "CAB-test.resS");
    }
}
