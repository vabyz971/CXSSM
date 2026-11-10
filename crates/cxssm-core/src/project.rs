//! Projet de mod : bundle ouvert, banques FMOD détectées, remplacements en attente.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cxssm_fmod::{Bank, Codec, Fsb5, Sample};
use cxssm_unity::{Bundle, SerializedFile, TextAsset, CLASS_TEXT_ASSET};

use crate::error::{Error, Result};

/// Classement indicatif d'après le nom du son ou de la banque.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SoundCategory {
    Engine,
    Turbo,
    Exhaust,
    Music,
    Voice,
    Ui,
    Other,
}

impl SoundCategory {
    pub fn guess(bank: &str, sample: &str) -> Self {
        // Banques moteur de CarX Street : nommées d'après l'architecture (V8, I4, Flat…).
        let b = bank.to_lowercase();
        const ENGINE_BANKS: [&str; 11] = ["v6", "v8", "v10", "v12", "i3", "i4", "i5", "i6", "flat", "rotary", "wankel"];
        if ENGINE_BANKS.contains(&b.as_str()) || b.starts_with("engine") {
            return Self::Engine;
        }
        if b.starts_with("music") {
            return Self::Music;
        }
        let s = format!("{b}/{sample}").to_lowercase();
        let has = |k: &[&str]| k.iter().any(|w| s.contains(w));
        if has(&["turbo", "blowoff", "bov", "wastegate"]) {
            Self::Turbo
        } else if has(&["exhaust", "backfire", "pop", "muffler"]) {
            Self::Exhaust
        } else if has(&["engine", "rpm", "motor", "_on_", "_off_", "idle", "limiter"]) {
            Self::Engine
        } else if has(&["music", "radio", "track", "ost"]) {
            Self::Music
        } else if has(&["voice", "vo_", "dialog", "speech"]) {
            Self::Voice
        } else if has(&["ui", "menu", "click", "button"]) {
            Self::Ui
        } else {
            Self::Other
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Engine => "Moteur",
            Self::Turbo => "Turbo",
            Self::Exhaust => "Échappement",
            Self::Music => "Musique",
            Self::Voice => "Voix",
            Self::Ui => "Interface",
            Self::Other => "Autre",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SoundRef {
    pub bank: usize,
    pub sample: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SoundStatus {
    Original,
    /// Remplacement prévu, fichier compatible.
    Replaced(PathBuf),
    /// Remplacement prévu mais caractéristiques différentes (message).
    Mismatch(PathBuf, String),
}

/// Une banque FMOD trouvée dans le bundle.
#[derive(Debug, Clone)]
pub struct BankEntry {
    pub name: String,
    /// Nœud du bundle (SerializedFile) qui contient le TextAsset.
    pub node: String,
    pub path_id: i64,
    pub bank: Bank,
    pub fsb: Fsb5,
}

#[derive(Debug)]
pub struct ModProject {
    pub source: PathBuf,
    pub bundle: Bundle,
    pub banks: Vec<BankEntry>,
    /// Banques FMOD sans sons (ex. `Master Bank.strings` : noms des évènements).
    /// Nécessaires pour rejouer les évènements avec l'API FMOD Studio.
    pub aux_banks: Vec<(String, Vec<u8>)>,
    pub replacements: BTreeMap<SoundRef, PathBuf>,
    /// Résultat de la vérification de compatibilité, calculé une seule fois.
    statuses: BTreeMap<SoundRef, SoundStatus>,
}

impl ModProject {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let source = path.as_ref().to_path_buf();
        let bundle = Bundle::open(&source)?;
        let mut banks = Vec::new();
        let mut aux_banks = Vec::new();

        for node in &bundle.nodes {
            if node.path.ends_with(".resS") || node.path.ends_with(".resource") {
                continue;
            }
            let sf = match SerializedFile::parse(node.data.clone()) {
                Ok(sf) => sf,
                Err(e) => {
                    log::warn!("nœud {} ignoré : {e}", node.path);
                    continue;
                }
            };
            for obj in sf.objects_of_class(CLASS_TEXT_ASSET) {
                let ta = TextAsset::parse(sf.object_data(obj), sf.endian)?;
                if !ta.is_fmod_bank() {
                    continue;
                }
                log::debug!("TextAsset {} : {} o, chunks {:?}", ta.name, ta.script.len(), riff_chunks(&ta.script));
                // Certaines banques n'ont pas de sons (ex. Master.strings.bank) : on les ignore.
                let bank = match Bank::parse(ta.script.clone()) {
                    Ok(b) => b,
                    Err(e) => {
                        log::info!(
                            "banque {} sans sons ({e}) – chunks : {:?}",
                            ta.name,
                            riff_chunks(&ta.script)
                        );
                        aux_banks.push((ta.name.clone(), ta.script.clone()));
                        continue;
                    }
                };
                match bank.fsb() {
                    Ok(fsb) => banks.push(BankEntry {
                        name: ta.name,
                        node: node.path.clone(),
                        path_id: obj.path_id,
                        bank,
                        fsb,
                    }),
                    Err(e) => log::warn!("banque {} illisible (chiffrée ?) : {e}", ta.name),
                }
            }
        }
        if banks.is_empty() {
            return Err(Error::NoBank);
        }
        log::info!("{} banque(s) FMOD détectée(s)", banks.len());
        Ok(Self { source, bundle, banks, aux_banks, replacements: BTreeMap::new(), statuses: BTreeMap::new() })
    }

    pub fn sounds(&self) -> impl Iterator<Item = (SoundRef, &BankEntry, &Sample)> {
        self.banks.iter().enumerate().flat_map(|(b, e)| {
            e.fsb
                .samples
                .iter()
                .enumerate()
                .map(move |(s, smp)| (SoundRef { bank: b, sample: s }, e, smp))
        })
    }

    pub fn sample(&self, r: SoundRef) -> Option<(&BankEntry, &Sample)> {
        let e = self.banks.get(r.bank)?;
        Some((e, e.fsb.samples.get(r.sample)?))
    }

    pub fn codec(&self, r: SoundRef) -> Option<Codec> {
        self.banks.get(r.bank).map(|b| b.fsb.codec)
    }

    pub fn status(&self, r: SoundRef) -> SoundStatus {
        self.statuses.get(&r).cloned().unwrap_or(SoundStatus::Original)
    }

    fn check(&self, r: SoundRef, path: &Path) -> SoundStatus {
        let Some((_, orig)) = self.sample(r) else {
            return SoundStatus::Original;
        };
        match crate::audio::probe(path) {
            Ok(i) if i.sample_rate == orig.frequency && i.channels == orig.channels => {
                SoundStatus::Replaced(path.to_path_buf())
            }
            Ok(i) => SoundStatus::Mismatch(
                path.to_path_buf(),
                format!(
                    "{} Hz / {} can. — attendu {} Hz / {} can.",
                    i.sample_rate, i.channels, orig.frequency, orig.channels
                ),
            ),
            Err(e) => SoundStatus::Mismatch(path.to_path_buf(), e.to_string()),
        }
    }

    pub fn set_replacement(&mut self, r: SoundRef, file: PathBuf) {
        let st = self.check(r, &file);
        self.statuses.insert(r, st);
        self.replacements.insert(r, file);
    }

    pub fn clear_replacement(&mut self, r: SoundRef) {
        self.statuses.remove(&r);
        self.replacements.remove(&r);
    }
}

/// Identifiants des chunks RIFF de premier niveau (diagnostic).
fn riff_chunks(d: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut p = 12;
    while p + 8 <= d.len() && out.len() < 32 {
        let size = u32::from_le_bytes(d[p + 4..p + 8].try_into().expect("4")) as usize;
        let mut id = String::from_utf8_lossy(&d[p..p + 4]).into_owned();
        if &d[p..p + 4] == b"LIST" && p + 12 <= d.len() {
            id = format!("LIST:{}", String::from_utf8_lossy(&d[p + 8..p + 12]));
        }
        out.push(format!("{id}({size})"));
        p += 8 + size + (size & 1);
    }
    out
}
