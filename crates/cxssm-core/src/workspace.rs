//! **Projet CXSSM** (`.cxssm`) : un dossier qui regroupe tout ce qu'il faut pour
//! reconstruire et installer le mod.
//!
//! ```text
//! Mon mod/
//! ├─ Mon mod.cxssm                    manifeste (TOML) : sons remplacés, réglages, jeu
//! ├─ original/fmod_assets_all.bundle  copie du bundle d'origine du jeu
//! ├─ mod/fmod_assets_all.bundle       bundle modifié (construit par CXSSM)
//! └─ audio/
//!    ├─ imported/    fichiers importés (copiés dans le projet)
//!    ├─ edited/      pistes rendues par l'éditeur (volume, hauteur, enregistrements)
//!    ├─ exported/    sons d'origine exportés
//!    └─ recordings/  enregistrements de la voiture (accélération, pied levé)
//! ```
//!
//! Le Steam Workshop ne peut pas remplacer `fmod_assets_all.bundle` (le catalogue du jeu
//! le charge sans passer par le chargeur de mods) : le mod s'installe en remplaçant le
//! fichier du jeu, et l'original du projet sert à le restaurer.

use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::export::BUNDLE_NAME;
use crate::game;
use crate::project::{ModProject, SoundRef};

pub const EXTENSION: &str = "cxssm";
const FORMAT: u32 = 1;

/// Sous-dossiers du projet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folder {
    Original,
    Mod,
    Imported,
    Edited,
    Exported,
    Recordings,
}

impl Folder {
    pub const ALL: [Folder; 6] = [Folder::Original, Folder::Mod, Folder::Imported, Folder::Edited, Folder::Exported, Folder::Recordings];

    /// Chemin relatif au dossier du projet.
    pub fn rel(self) -> &'static str {
        match self {
            Folder::Original => "original",
            Folder::Mod => "mod",
            Folder::Imported => "audio/imported",
            Folder::Edited => "audio/edited",
            Folder::Exported => "audio/exported",
            Folder::Recordings => "audio/recordings",
        }
    }
}

/// Manifeste `.cxssm`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub format: u32,
    pub name: String,
    /// Version de CXSSM qui a écrit le fichier.
    pub cxssm: String,
    /// `fmod_assets_all.bundle` du jeu, où le mod est installé.
    pub game_bundle: Option<PathBuf>,
    /// Le bundle de `mod/` ne correspond plus aux sons du projet.
    pub mod_stale: bool,
    pub recordings: Recordings,
    #[serde(rename = "sound")]
    pub sounds: Vec<SoundEntry>,
}

/// Enregistrements de la voiture (chemins relatifs au projet).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Recordings {
    pub acc: Option<String>,
    pub dec: Option<String>,
}

/// Son remplacé. Repéré par les noms de sa banque et de son échantillon, qui restent
/// valables si le bundle du jeu est mis à jour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundEntry {
    pub bank: String,
    pub sample: String,
    /// Fichier utilisé pour construire le mod.
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit: Option<EditEntry>,
}

/// Réglages de l'éditeur de pistes pour un son.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditEntry {
    pub source: EditSource,
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default)]
    pub semitones: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum EditSource {
    /// Son d'origine du jeu (volume et hauteur modifiés).
    Original,
    /// Fichier importé (chemin relatif au projet).
    File { path: String },
    /// Passage d'un enregistrement de la voiture.
    Recording { phase: Phase, rpm: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Acc,
    Dec,
}

/// État du fichier du jeu par rapport au projet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    /// Aucun fichier du jeu associé au projet.
    Unknown,
    /// Le fichier du jeu n'existe plus.
    Missing,
    /// Le jeu a son fichier d'origine, identique à celui du projet.
    Original,
    /// Le mod du projet est installé.
    Installed,
    /// Le jeu a un fichier d'origine plus récent que celui du projet (mise à jour du jeu).
    Updated,
    /// Le fichier du jeu a été modifié, mais ce n'est pas le mod actuel du projet
    /// (ancienne version du mod, autre outil…).
    Modified,
}

/// Projet ouvert : son dossier et son manifeste.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub dir: PathBuf,
    /// Fichier `.cxssm`.
    pub file: PathBuf,
    pub manifest: Manifest,
}

impl Workspace {
    /// Chemin du manifeste choisi par l'utilisateur (`…/Mon mod.cxssm`) → dossier du
    /// projet `…/Mon mod/` qui contiendra `Mon mod.cxssm`.
    pub fn dir_for(chosen: &Path) -> PathBuf {
        let stem = chosen.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "CXSSM".into());
        let parent = chosen.parent().unwrap_or(Path::new("."));
        // Déjà dans un dossier du même nom : on le réutilise.
        if parent.file_name().is_some_and(|n| n.to_string_lossy() == stem) {
            parent.to_path_buf()
        } else {
            parent.join(stem)
        }
    }

    /// Crée le dossier du projet et y copie le bundle d'origine (≈ 200 Mo).
    pub fn create(chosen: &Path, original: &Path, game_bundle: Option<PathBuf>) -> Result<Self> {
        let dir = Self::dir_for(chosen);
        let name = dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "CXSSM".into());
        let ws = Self {
            file: dir.join(format!("{name}.{EXTENSION}")),
            manifest: Manifest { format: FORMAT, name, cxssm: env!("CARGO_PKG_VERSION").into(), game_bundle, ..Default::default() },
            dir,
        };
        if ws.manifest_path().exists() {
            return Err(Error::Project(format!("{} existe déjà", ws.manifest_path().display())));
        }
        for f in Folder::ALL {
            std::fs::create_dir_all(ws.folder(f))?;
        }
        copy_atomic(original, &ws.original_bundle())?;
        ws.save()?;
        Ok(ws)
    }

    /// Ouvre un projet : le manifeste `.cxssm` ou son dossier.
    pub fn open(path: &Path) -> Result<Self> {
        let manifest_path = if path.is_dir() {
            std::fs::read_dir(path)?
                .flatten()
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|e| e == EXTENSION))
                .ok_or_else(|| Error::Project(format!("aucun fichier .{EXTENSION} dans {}", path.display())))?
        } else {
            path.to_path_buf()
        };
        let text = std::fs::read_to_string(&manifest_path)?;
        let manifest: Manifest = toml::from_str(&text).map_err(|e| Error::Project(format!("{} : {e}", manifest_path.display())))?;
        if manifest.format > FORMAT {
            return Err(Error::Project(format!("projet créé par une version plus récente de CXSSM ({})", manifest.cxssm)));
        }
        let dir = manifest_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let ws = Self { dir, file: manifest_path, manifest };
        if !ws.original_bundle().is_file() {
            return Err(Error::Project(format!("bundle d'origine absent : {}", ws.original_bundle().display())));
        }
        Ok(ws)
    }

    pub fn save(&self) -> Result<()> {
        let mut m = self.manifest.clone();
        m.format = FORMAT;
        m.cxssm = env!("CARGO_PKG_VERSION").into();
        let text = toml::to_string_pretty(&m).map_err(|e| Error::Project(e.to_string()))?;
        let tmp = self.manifest_path().with_extension("cxssm.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, self.manifest_path())?;
        Ok(())
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.file.clone()
    }

    pub fn folder(&self, f: Folder) -> PathBuf {
        self.dir.join(f.rel())
    }

    pub fn original_bundle(&self) -> PathBuf {
        self.folder(Folder::Original).join(BUNDLE_NAME)
    }

    pub fn mod_bundle(&self) -> PathBuf {
        self.folder(Folder::Mod).join(BUNDLE_NAME)
    }

    /// Chemin enregistré dans le manifeste : relatif s'il est dans le projet.
    pub fn rel(&self, p: &Path) -> String {
        match p.strip_prefix(&self.dir) {
            Ok(r) => r.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"),
            Err(_) => p.display().to_string(),
        }
    }

    /// Chemin réel d'une entrée du manifeste.
    pub fn abs(&self, s: &str) -> PathBuf {
        let p = Path::new(s);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            // Pas de sortie du dossier du projet (`..`).
            self.dir.join(p.components().filter(|c| matches!(c, Component::Normal(_))).collect::<PathBuf>())
        }
    }

    pub fn contains(&self, p: &Path) -> bool {
        p.starts_with(&self.dir)
    }

    /// Nom de fichier d'un son : `<banque>_<son>`, sans caractère interdit.
    pub fn sound_stem(bank: &str, sample: &str) -> String {
        format!("{bank}_{sample}").chars().map(|c| if c.is_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect()
    }

    /// Copie un fichier importé dans `audio/imported/` (s'il n'y est pas déjà).
    pub fn import(&self, src: &Path, bank: &str, sample: &str) -> Result<PathBuf> {
        if self.contains(src) {
            return Ok(src.to_path_buf());
        }
        let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "wav".into());
        let stem = Self::sound_stem(bank, sample);
        // Un import précédent du même son est gardé (annuler doit pouvoir y revenir) :
        // `<son>.wav`, puis `<son>-2.wav`… ; un fichier identique est réutilisé.
        for n in 1.. {
            let name = if n == 1 { format!("{stem}.{ext}") } else { format!("{stem}-{n}.{ext}") };
            let dst = self.folder(Folder::Imported).join(name);
            if !dst.exists() {
                copy_atomic(src, &dst)?;
                return Ok(dst);
            }
            if same_file_content(src, &dst) {
                return Ok(dst);
            }
        }
        unreachable!("boucle infinie")
    }

    /// Piste rendue par l'éditeur pour ce son.
    pub fn edited_path(&self, bank: &str, sample: &str) -> PathBuf {
        self.folder(Folder::Edited).join(format!("{}.wav", Self::sound_stem(bank, sample)))
    }

    /// Copie un enregistrement de la voiture dans `audio/recordings/`.
    pub fn import_recording(&self, src: &Path) -> Result<PathBuf> {
        if self.contains(src) {
            return Ok(src.to_path_buf());
        }
        let dst = self.folder(Folder::Recordings).join(src.file_name().ok_or_else(|| Error::Project("nom de fichier invalide".into()))?);
        copy_atomic(src, &dst)?;
        Ok(dst)
    }

    /// Sons du manifeste retrouvés dans le bundle ouvert, et ceux introuvables.
    pub fn resolve<'a>(&'a self, project: &ModProject) -> (Vec<(SoundRef, &'a SoundEntry)>, Vec<&'a SoundEntry>) {
        let (mut found, mut missing) = (Vec::new(), Vec::new());
        for e in &self.manifest.sounds {
            let r = project.sounds().find(|(_, b, s)| b.name == e.bank && s.name == e.sample).map(|(r, _, _)| r);
            match r {
                Some(r) => found.push((r, e)),
                None => missing.push(e),
            }
        }
        (found, missing)
    }

    /// Compare le fichier du jeu à l'original et au mod du projet (lit les fichiers).
    pub fn game_state(&self) -> GameState {
        let Some(game) = &self.manifest.game_bundle else { return GameState::Unknown };
        let Ok(meta) = std::fs::metadata(game) else { return GameState::Missing };
        let mod_bundle = self.mod_bundle();
        if mod_bundle.is_file() && same_file_content(game, &mod_bundle) {
            return GameState::Installed;
        }
        let pristine = match game::catalog_bundle_size(game) {
            Some(size) => meta.len() == size,
            None => same_file_content(game, &self.original_bundle()),
        };
        if !pristine {
            GameState::Modified
        } else if same_file_content(game, &self.original_bundle()) {
            GameState::Original
        } else {
            GameState::Updated
        }
    }

    /// L'original du projet correspond à la version installée du jeu (d'après la taille
    /// déclarée par son catalogue ; vrai si le catalogue est illisible).
    pub fn original_is_current(&self) -> bool {
        let Some(game) = &self.manifest.game_bundle else { return true };
        match (game::catalog_bundle_size(game), std::fs::metadata(self.original_bundle())) {
            (Some(size), Ok(m)) => m.len() == size,
            _ => true,
        }
    }

    /// Copie le mod construit à la place du fichier du jeu.
    pub fn install(&self) -> Result<PathBuf> {
        let game = self.manifest.game_bundle.clone().ok_or_else(|| Error::Project("fichier du jeu non défini".into()))?;
        if !self.original_is_current() {
            return Err(Error::Project(
                "le jeu a été mis à jour depuis la création du projet : mettez à jour l'original du projet avant d'installer".into(),
            ));
        }
        if !self.mod_bundle().is_file() {
            return Err(Error::Project("mod pas encore construit".into()));
        }
        copy_atomic(&self.mod_bundle(), &game)?;
        Ok(game)
    }

    /// Remet le fichier d'origine dans le jeu.
    pub fn restore(&self) -> Result<PathBuf> {
        let game = self.manifest.game_bundle.clone().ok_or_else(|| Error::Project("fichier du jeu non défini".into()))?;
        if !self.original_is_current() {
            return Err(Error::Project(
                "l'original du projet est plus ancien que le jeu : utilisez « Vérifier l'intégrité des fichiers » dans Steam".into(),
            ));
        }
        copy_atomic(&self.original_bundle(), &game)?;
        Ok(game)
    }

    /// Remplace l'original du projet par le fichier (d'origine) du jeu mis à jour.
    pub fn update_original(&self) -> Result<()> {
        let game = self.manifest.game_bundle.clone().ok_or_else(|| Error::Project("fichier du jeu non défini".into()))?;
        if let Some(size) = game::catalog_bundle_size(&game) {
            if std::fs::metadata(&game)?.len() != size {
                return Err(Error::Project("le fichier du jeu est modifié : ce n'est pas un original".into()));
            }
        }
        copy_atomic(&game, &self.original_bundle())
    }
}

/// Le bundle est-il le fichier d'origine du jeu ? `None` si on ne peut pas le savoir
/// (pas de catalogue à côté ni dans `game`).
pub fn is_pristine(bundle: &Path, game: Option<&Path>) -> Option<bool> {
    let size = game::catalog_bundle_size(bundle).or_else(|| game.and_then(game::catalog_bundle_size))?;
    Some(std::fs::metadata(bundle).ok()?.len() == size)
}

/// Copie via un fichier temporaire puis renommage : la destination n'est jamais à moitié écrite.
fn copy_atomic(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut name = dst.file_name().unwrap_or_default().to_os_string();
    name.push(".cxssm-tmp");
    let tmp = dst.with_file_name(name);
    std::fs::copy(src, &tmp)?;
    std::fs::rename(&tmp, dst).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(())
}

/// Contenu identique (taille d'abord, puis lecture par blocs).
pub fn same_file_content(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else { return false };
    if ma.len() != mb.len() {
        return false;
    }
    let (Ok(mut fa), Ok(mut fb)) = (File::open(a), File::open(b)) else { return false };
    let (mut ba, mut bb) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    loop {
        let n = match read_full(&mut fa, &mut ba) {
            Ok(n) => n,
            Err(_) => return false,
        };
        if read_full(&mut fb, &mut bb).ok() != Some(n) || ba[..n] != bb[..n] {
            return false;
        }
        if n == 0 {
            return true;
        }
    }
}

fn read_full(f: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cxssm-ws-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn project_folder_roundtrip() {
        let root = tmp("roundtrip");
        let original = root.join("fmod_assets_all.bundle");
        std::fs::write(&original, b"original bundle").unwrap();
        let game = root.join("game.bundle");
        std::fs::write(&game, b"original bundle").unwrap();

        let ws = Workspace::create(&root.join("Mon mod.cxssm"), &original, Some(game.clone())).unwrap();
        assert_eq!(ws.dir, root.join("Mon mod"));
        assert!(ws.manifest_path().is_file());
        assert_eq!(std::fs::read(ws.original_bundle()).unwrap(), b"original bundle");
        for f in Folder::ALL {
            assert!(ws.folder(f).is_dir());
        }
        // Créer deux fois le même projet échoue.
        assert!(Workspace::create(&root.join("Mon mod.cxssm"), &original, None).is_err());

        // Import : copie dans le projet, chemin relatif dans le manifeste.
        let wav = root.join("mon son.wav");
        std::fs::write(&wav, b"RIFF").unwrap();
        let imported = ws.import(&wav, "Flat", "sample/0007").unwrap();
        assert_eq!(imported, ws.folder(Folder::Imported).join("Flat_sample_0007.wav"));
        assert_eq!(ws.rel(&imported), "audio/imported/Flat_sample_0007.wav");
        assert_eq!(ws.abs("audio/imported/Flat_sample_0007.wav"), imported);
        assert_eq!(ws.abs("../../etc/passwd"), ws.dir.join("etc/passwd"));
        assert_eq!(ws.import(&imported, "Flat", "x").unwrap(), imported);
        // Même fichier : réutilisé ; autre fichier : copié à côté, l'ancien est gardé.
        assert_eq!(ws.import(&wav, "Flat", "sample/0007").unwrap(), imported);
        let other = root.join("autre.wav");
        std::fs::write(&other, b"RIFF2").unwrap();
        let second = ws.import(&other, "Flat", "sample/0007").unwrap();
        assert_eq!(second, ws.folder(Folder::Imported).join("Flat_sample_0007-2.wav"));
        assert_eq!(std::fs::read(&imported).unwrap(), b"RIFF");

        let mut ws = ws;
        ws.manifest.sounds.push(SoundEntry {
            bank: "Flat".into(),
            sample: "sample_0007".into(),
            file: ws.rel(&imported),
            edit: Some(EditEntry { source: EditSource::Recording { phase: Phase::Acc, rpm: 6225. }, gain_db: -1.5, semitones: 0.3 }),
        });
        ws.manifest.recordings.acc = Some("audio/recordings/acc.wav".into());
        ws.save().unwrap();
        let back = Workspace::open(&ws.manifest_path()).unwrap();
        assert_eq!(back.manifest, ws.manifest);
        // Ouverture par le dossier.
        assert_eq!(Workspace::open(&ws.dir).unwrap().manifest, ws.manifest);

        // Installation / restauration (sans catalogue : comparaison avec l'original).
        assert_eq!(ws.game_state(), GameState::Original);
        assert!(ws.install().is_err(), "mod pas encore construit");
        std::fs::write(ws.mod_bundle(), b"modded bundle!").unwrap();
        ws.install().unwrap();
        assert_eq!(std::fs::read(&game).unwrap(), b"modded bundle!");
        assert_eq!(ws.game_state(), GameState::Installed);
        ws.restore().unwrap();
        assert_eq!(ws.game_state(), GameState::Original);
        std::fs::write(&game, b"something else").unwrap();
        assert_eq!(ws.game_state(), GameState::Modified);
        std::fs::remove_file(&game).unwrap();
        assert_eq!(ws.game_state(), GameState::Missing);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dir_for_reuses_a_folder_of_the_same_name() {
        assert_eq!(Workspace::dir_for(Path::new("/a/b/Mod.cxssm")), PathBuf::from("/a/b/Mod"));
        assert_eq!(Workspace::dir_for(Path::new("/a/Mod/Mod.cxssm")), PathBuf::from("/a/Mod"));
    }

    #[test]
    fn compares_file_contents() {
        let root = tmp("cmp");
        let (a, b, c) = (root.join("a"), root.join("b"), root.join("c"));
        let big: Vec<u8> = (0..3_000_000u32).map(|i| (i * 7) as u8).collect();
        std::fs::write(&a, &big).unwrap();
        std::fs::write(&b, &big).unwrap();
        let mut other = big.clone();
        other[2_500_000] ^= 1;
        std::fs::write(&c, &other).unwrap();
        assert!(same_file_content(&a, &b));
        assert!(!same_file_content(&a, &c));
        let _ = std::fs::remove_dir_all(&root);
    }
}
