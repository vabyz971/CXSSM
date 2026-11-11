//! Construction du bundle modifié et écriture dans le dossier de sortie.
//!
//! Le bundle d'origine n'est **jamais** modifié : la sortie est écrite dans
//! `<sortie>/fmod_assets_all.bundle`, accompagnée d'un `cxssm-mod.txt` listant
//! les sons remplacés (utile pour décrire le mod).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use cxssm_unity::{SerializedFile, TextAsset};

use crate::encoder::{self, EncoderConfig};
use crate::error::Result;
use crate::project::{BankEntry, ModProject};

pub const BUNDLE_NAME: &str = "fmod_assets_all.bundle";

/// Reconstruit la banque `b_idx` avec ses sons remplacés (ré-encodés).
/// `None` si aucun son de cette banque n'est remplacé.
pub fn build_bank(project: &ModProject, cfg: &EncoderConfig, b_idx: usize) -> Result<Option<Vec<u8>>> {
    let repl = bank_replacements(project, b_idx);
    if repl.is_empty() {
        return Ok(None);
    }
    build_bank_entry(&project.banks[b_idx], &repl, cfg).map(Some)
}

/// Remplacements d'une banque : (index du son, fichier).
pub fn bank_replacements(project: &ModProject, b_idx: usize) -> Vec<(usize, PathBuf)> {
    project
        .replacements
        .iter()
        .filter(|(r, _)| r.bank == b_idx)
        .map(|(r, f)| (r.sample, f.clone()))
        .collect()
}

/// Variante sans `ModProject` (utilisable en arrière-plan avec une copie de la banque).
pub fn build_bank_entry(entry: &BankEntry, repl: &[(usize, PathBuf)], cfg: &EncoderConfig) -> Result<Vec<u8>> {
    let mut fsb = entry.fsb.clone();
    for (sample, file) in repl {
        let target = &entry.fsb.samples[*sample];
        let new = encoder::encode(cfg, file, target, fsb.codec)?;
        fsb.check_compatible(*sample, fsb.codec, &new)?;
        fsb.replace(*sample, new)?;
        log::info!("{} / {} ← {}", entry.name, target.name, file.display());
    }
    Ok(entry.bank.with_fsb(&fsb.to_bytes()))
}

pub fn export(project: &ModProject, cfg: &EncoderConfig, out_dir: &Path) -> Result<PathBuf> {
    // 1. Ré-encode les sons et reconstruit chaque banque modifiée.
    let mut per_node: BTreeMap<&str, Vec<(i64, Vec<u8>)>> = BTreeMap::new();
    for (b_idx, entry) in project.banks.iter().enumerate() {
        let Some(bank_bytes) = build_bank(project, cfg, b_idx)? else { continue };
        let node = project.bundle.node(&entry.node).expect("nœud existant");
        let sf = SerializedFile::parse(node.data.clone())?;
        let ta = TextAsset { name: entry.name.clone(), script: bank_bytes };
        per_node.entry(entry.node.as_str()).or_default().push((entry.path_id, ta.to_bytes(sf.endian)));
    }

    // 2. Reconstruit les SerializedFile concernés puis le bundle.
    let mut bundle = project.bundle.clone();
    for (node_path, repl) in per_node {
        let node = bundle.node_mut(node_path).expect("nœud existant");
        let sf = SerializedFile::parse(std::mem::take(&mut node.data))?;
        node.data = sf.rebuild(&repl);
    }

    let out = out_dir.join(BUNDLE_NAME);
    bundle.save(&out)?;
    std::fs::write(out_dir.join("cxssm-mod.txt"), manifest(project))?;
    Ok(out)
}

fn manifest(p: &ModProject) -> String {
    let mut s = String::from("Sons remplacés par CXSSM\n========================\n");
    for (r, file) in &p.replacements {
        if let Some((bank, smp)) = p.sample(*r) {
            let _ = writeln!(s, "{} / {}  ←  {}", bank.name, smp.name, file.display());
        }
    }
    s
}
