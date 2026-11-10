//! **SerializedFile** Unity (le fichier `CAB-xxxx` à l'intérieur d'un bundle).
//!
//! On ne lit que ce qui est nécessaire pour localiser et remplacer des objets :
//! en-tête, table des types (pour connaître le `class_id`) et table des objets.
//! Les TypeTrees sont sautés sans être interprétés : la disposition d'un
//! `TextAsset` est stable et connue (voir [`crate::text_asset`]).

use crate::error::{Error, Result};
use crate::io::{Endian, Reader};

#[derive(Debug, Clone)]
pub struct ObjectInfo {
    pub path_id: i64,
    pub class_id: i32,
    /// Offset absolu des données de l'objet dans le fichier.
    pub offset: usize,
    pub size: usize,
    /// Positions (absolues) des champs `byte_start` / `byte_size` dans les métadonnées,
    /// pour pouvoir les réécrire sans re-sérialiser toute la table.
    field_start_pos: usize,
    field_size_pos: usize,
}

#[derive(Debug, Clone)]
pub struct SerializedFile {
    pub version: u32,
    pub unity_version: String,
    pub endian: Endian,
    data_offset: usize,
    raw: Vec<u8>,
    pub objects: Vec<ObjectInfo>,
}

impl SerializedFile {
    pub fn parse(raw: Vec<u8>) -> Result<Self> {
        let mut r = Reader::new(&raw, Endian::Big);
        let _meta_size = r.u32()?;
        let _file_size = r.u32()?;
        let version = r.u32()?;
        let mut data_offset = r.u32()? as usize;
        if !(17..=23).contains(&version) {
            // 17+ = Unity 2017.x et plus (CarX Street : Unity 2021/2022).
            return Err(Error::UnsupportedVersion(version));
        }
        let endian = if r.u8()? == 0 { Endian::Little } else { Endian::Big };
        r.take(3)?;
        if version >= 22 {
            let _meta_size = r.u32()?;
            let _file_size = r.i64()?;
            data_offset = r.i64()? as usize;
            r.i64()?;
        }

        r.endian = endian;
        let unity_version = r.cstr()?;
        let _platform = r.i32()?;
        let type_tree = r.u8()? != 0;

        // --- Types ---
        let type_count = r.i32()?;
        let mut class_ids = Vec::with_capacity(type_count.max(0) as usize);
        for _ in 0..type_count {
            let class_id = r.i32()?;
            r.u8()?; // is_stripped
            r.i16()?; // script_type_index
            if class_id == 114 {
                r.take(16)?; // script id (MonoBehaviour)
            }
            r.take(16)?; // old type hash
            if type_tree {
                skip_type_tree(&mut r, version)?;
                if version >= 21 {
                    let deps = r.i32()?;
                    r.take(deps.max(0) as usize * 4)?;
                }
            }
            class_ids.push(class_id);
        }

        // --- Objets ---
        let obj_count = r.i32()?;
        let mut objects = Vec::with_capacity(obj_count.max(0) as usize);
        for _ in 0..obj_count {
            r.align(4);
            let path_id = r.i64()?;
            let field_start_pos = r.pos();
            let start = if version >= 22 { r.i64()? as usize } else { r.u32()? as usize };
            let field_size_pos = r.pos();
            let size = r.u32()? as usize;
            let type_id = r.i32()?;
            let class_id = *class_ids
                .get(type_id as usize)
                .ok_or_else(|| Error::NotFound(format!("type #{type_id}")))?;
            objects.push(ObjectInfo {
                path_id,
                class_id,
                offset: data_offset + start,
                size,
                field_start_pos,
                field_size_pos,
            });
        }

        Ok(Self { version, unity_version, endian, data_offset, raw, objects })
    }

    pub fn object_data(&self, obj: &ObjectInfo) -> &[u8] {
        &self.raw[obj.offset..obj.offset + obj.size]
    }

    pub fn objects_of_class(&self, class_id: i32) -> impl Iterator<Item = &ObjectInfo> {
        self.objects.iter().filter(move |o| o.class_id == class_id)
    }

    /// Reconstruit le fichier en remplaçant les données de certains objets.
    /// `replacements` : (path_id, nouvelles données).
    pub fn rebuild(&self, replacements: &[(i64, Vec<u8>)]) -> Vec<u8> {
        let mut out = self.raw[..self.data_offset].to_vec();
        let mut order: Vec<&ObjectInfo> = self.objects.iter().collect();
        order.sort_by_key(|o| o.offset);

        let mut new_fields = Vec::with_capacity(order.len());
        for obj in order {
            while !(out.len() - self.data_offset).is_multiple_of(8) {
                out.push(0);
            }
            let start = out.len() - self.data_offset;
            let data = replacements
                .iter()
                .find(|(id, _)| *id == obj.path_id)
                .map(|(_, d)| d.as_slice())
                .unwrap_or_else(|| self.object_data(obj));
            out.extend_from_slice(data);
            new_fields.push((obj, start, data.len()));
        }

        for (obj, start, size) in new_fields {
            if self.version >= 22 {
                put(&mut out, obj.field_start_pos, &(start as i64), self.endian);
            } else {
                put(&mut out, obj.field_start_pos, &(start as u32), self.endian);
            }
            put(&mut out, obj.field_size_pos, &(size as u32), self.endian);
        }

        // Taille totale dans l'en-tête (toujours big endian).
        let total = out.len();
        if self.version >= 22 {
            out[24..32].copy_from_slice(&(total as i64).to_be_bytes());
        } else {
            out[4..8].copy_from_slice(&(total as u32).to_be_bytes());
        }
        out
    }
}

fn skip_type_tree(r: &mut Reader<'_>, version: u32) -> Result<()> {
    let node_count = r.i32()?.max(0) as usize;
    let string_size = r.i32()?.max(0) as usize;
    let node_size = if version >= 19 { 32 } else { 24 };
    r.take(node_count * node_size + string_size)?;
    Ok(())
}

trait ToBytes {
    fn le(&self) -> Vec<u8>;
    fn be(&self) -> Vec<u8>;
}
impl ToBytes for u32 {
    fn le(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
    fn be(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}
impl ToBytes for i64 {
    fn le(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
    fn be(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

fn put(buf: &mut [u8], pos: usize, v: &dyn ToBytes, e: Endian) {
    let b = match e {
        Endian::Little => v.le(),
        Endian::Big => v.be(),
    };
    buf[pos..pos + b.len()].copy_from_slice(&b);
}
