//! Localisation du **FMOD Engine** installé par l'utilisateur (CXSSM ne le redistribue pas).
//!
//! Dispositions connues :
//! - Linux  : `fmodstudioapi20400linux/{api/{core,studio}/lib/x86_64/*.so, bin/fsbank}`
//! - Windows: `C:\Program Files (x86)\FMOD SoundSystem\FMOD Studio API Windows\{api\…\x64\*.dll, bin\fsbankcl.exe}`
//! - macOS  : `FMOD Programmers API/{api/{core,studio}/lib/*.dylib, bin/fsbankcl}`
//!
//! Ordre de résolution : variables `CXSSM_FMOD_API` / `CXSSM_FSBANKCL`, puis dossier choisi
//! dans les préférences, puis détection automatique.

use std::path::{Path, PathBuf};

/// Une installation du FMOD Engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FmodInstall {
    /// Dossier racine (contient `api/` et `bin/`).
    pub root: PathBuf,
    /// Dossier `api/` (bibliothèques FMOD Core / Studio / FSBank).
    pub api: PathBuf,
    /// Encodeur en ligne de commande (`fsbank` / `fsbankcl`), s'il est présent.
    pub fsbank: Option<PathBuf>,
}

/// Bibliothèque FMOD Studio relative au dossier `api/`, selon le système.
fn studio_lib(api: &Path) -> PathBuf {
    if cfg!(windows) {
        api.join("studio/lib/x64/fmodstudio.dll")
    } else if cfg!(target_os = "macos") {
        api.join("studio/lib/libfmodstudio.dylib")
    } else {
        api.join("studio/lib/x86_64/libfmodstudio.so")
    }
}

fn fsbank_exe(root: &Path) -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["fsbankcl.exe"] } else { &["fsbank", "fsbankcl"] };
    names.iter().map(|n| root.join("bin").join(n)).find(|p| p.is_file())
}

impl FmodInstall {
    /// Reconnaît une installation à partir de n'importe quel dossier de l'utilisateur :
    /// la racine, son `api/`, son `bin/`, ou le chemin de l'exécutable `fsbank`.
    pub fn from_path(path: &Path) -> Option<Self> {
        let mut candidates = vec![path.to_path_buf()];
        let mut p = path;
        for _ in 0..3 {
            if let Some(parent) = p.parent() {
                candidates.push(parent.to_path_buf());
                p = parent;
            }
        }
        candidates.into_iter().find_map(|root| {
            let api = root.join("api");
            studio_lib(&api).is_file().then(|| Self { fsbank: fsbank_exe(&root), api, root })
        })
    }

    /// Installations trouvées aux emplacements habituels (téléchargements, Program Files, /opt…).
    pub fn detect() -> Vec<Self> {
        let mut bases: Vec<PathBuf> = Vec::new();
        if cfg!(windows) {
            for var in ["ProgramFiles(x86)", "ProgramFiles"] {
                if let Some(p) = std::env::var_os(var) {
                    bases.push(PathBuf::from(p).join("FMOD SoundSystem"));
                }
            }
        }
        if cfg!(target_os = "macos") {
            bases.push(PathBuf::from("/Applications"));
        }
        if cfg!(all(unix, not(target_os = "macos"))) {
            bases.push(PathBuf::from("/opt"));
        }
        if let Some(d) = dirs::download_dir() {
            bases.push(d);
        }
        if let Some(h) = dirs::home_dir() {
            bases.push(h.join("Downloads"));
            bases.push(h.join("Téléchargements"));
            bases.push(h.clone());
        }

        let mut found: Vec<Self> = Vec::new();
        for base in bases {
            let Ok(entries) = std::fs::read_dir(&base) else { continue };
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_lowercase();
                if !name.contains("fmod") || !e.path().is_dir() {
                    continue;
                }
                if let Some(inst) = Self::from_path(&e.path()) {
                    if !found.iter().any(|f| f.root == inst.root) {
                        found.push(inst);
                    }
                }
            }
        }
        // Les versions les plus récentes en premier (ex. fmodstudioapi20400 > 20300).
        found.sort_by(|a, b| b.root.cmp(&a.root));
        found
    }

    /// Installation à utiliser : variables d'environnement, puis `preferred`
    /// (préférences), puis détection automatique.
    pub fn resolve(preferred: Option<&Path>) -> Option<Self> {
        let env_api = std::env::var_os("CXSSM_FMOD_API").map(PathBuf::from);
        let env_fsbank = std::env::var_os("CXSSM_FSBANKCL").map(PathBuf::from);
        let mut inst = env_api
            .as_deref()
            .and_then(Self::from_path)
            .or_else(|| env_fsbank.as_deref().and_then(Self::from_path))
            .or_else(|| preferred.and_then(Self::from_path))
            .or_else(|| Self::detect().into_iter().next())?;
        if let Some(f) = env_fsbank.filter(|f| f.is_file()) {
            inst.fsbank = Some(f);
        }
        Some(inst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_root_api_and_bin() {
        let tmp = std::env::temp_dir().join(format!("cxssm-fmod-test-{}", std::process::id()));
        let root = tmp.join("fmodstudioapi20400test");
        let lib = studio_lib(&root.join("api"));
        std::fs::create_dir_all(lib.parent().unwrap()).unwrap();
        std::fs::write(&lib, b"").unwrap();
        std::fs::create_dir_all(root.join("bin")).unwrap();

        for p in [root.clone(), root.join("api"), root.join("bin"), root.join("api/studio")] {
            let inst = FmodInstall::from_path(&p).unwrap_or_else(|| panic!("{}", p.display()));
            assert_eq!(inst.root, root);
        }
        assert!(FmodInstall::from_path(&tmp).is_none());
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
