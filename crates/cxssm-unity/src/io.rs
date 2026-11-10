//! Petit lecteur/écrivain binaire avec boutisme (endianness) dynamique.

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    pub endian: Endian,
}

macro_rules! read_num {
    ($name:ident, $t:ty) => {
        pub fn $name(&mut self) -> Result<$t> {
            let b = self.take(std::mem::size_of::<$t>())?;
            let arr = b.try_into().expect("taille vérifiée");
            Ok(match self.endian {
                Endian::Little => <$t>::from_le_bytes(arr),
                Endian::Big => <$t>::from_be_bytes(arr),
            })
        }
    };
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8], endian: Endian) -> Self {
        Self { data, pos: 0, endian }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(Error::UnexpectedEof(self.pos))?;
        let s = self.data.get(self.pos..end).ok_or(Error::UnexpectedEof(self.pos))?;
        self.pos = end;
        Ok(s)
    }

    pub fn align(&mut self, n: usize) {
        self.pos = self.pos.div_ceil(n) * n;
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    read_num!(u16, u16);
    read_num!(i16, i16);
    read_num!(u32, u32);
    read_num!(i32, i32);
    read_num!(i64, i64);

    /// Chaîne terminée par `\0`.
    pub fn cstr(&mut self) -> Result<String> {
        let rest = &self.data[self.pos.min(self.data.len())..];
        let len = rest.iter().position(|&b| b == 0).ok_or(Error::UnexpectedEof(self.pos))?;
        let s = String::from_utf8_lossy(&rest[..len]).into_owned();
        self.pos += len + 1;
        Ok(s)
    }
}

/// Écrivain gros-boutiste (big endian), utilisé pour l'en-tête UnityFS.
#[derive(Default)]
pub struct BeWriter(pub Vec<u8>);

impl BeWriter {
    pub fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    pub fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    pub fn bytes(&mut self, b: &[u8]) {
        self.0.extend_from_slice(b);
    }
    pub fn cstr(&mut self, s: &str) {
        self.0.extend_from_slice(s.as_bytes());
        self.0.push(0);
    }
    pub fn align(&mut self, n: usize) {
        while !self.0.len().is_multiple_of(n) {
            self.0.push(0);
        }
    }
}
