//! Construction du bundle modifié et écriture dans le dossier de sortie.
//!
//! Le bundle d'origine n'est **jamais** modifié : la sortie est écrite dans
//! `<sortie>/fmod_assets_all.bundle`, accompagnée d'un `cxssm-mod.txt` listant
//! les sons remplacés (utile pour la description Steam Workshop).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use cxssm_unity::{SerializedFile, TextAsset};

use crate::encoder::{self, EncoderConfig};
use crate::error::Result;
use crate::project::ModProject;

pub const BUNDLE_NAME: &str = "fmod_assets_all.bundle";

pub fn export(project: &ModProject, cfg: &EncoderConfig, out_dir: &Path) -> Result<PathBuf> {
    // 1. Ré-encode les sons et reconstruit chaque banque modifiée.
    let mut per_node: BTreeMap<&str, Vec<(i64, Vec<u8>)>> = BTreeMap::new();
    for (b_idx, entry) in project.banks.iter().enumerate() {
        let repl: Vec<_> = project.replacements.iter().filter(|(r, _)| r.bank == b_idx).collect();
        if repl.is_empty() {
            continue;
        }
        let mut fsb = entry.fsb.clone();
        for (r, file) in repl {
            let target = &entry.fsb.samples[r.sample];
            let new = encoder::encode(cfg, file, target, fsb.codec)?;
            fsb.check_compatible(r.sample, fsb.codec, &new)?;
            fsb.replace(r.sample, new)?;
            log::info!("{} ← {}", target.name, file.display());
        }
        let bank_bytes = entry.bank.with_fsb(&fsb.to_bytes());
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
