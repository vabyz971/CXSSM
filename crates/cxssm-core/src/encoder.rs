//! Encodage d'un fichier audio en échantillon FSB5 compatible avec l'original.
//!
//! - **PCM16** : encodage natif (lecture WAV via `hound`).
//! - **Vorbis / FADPCM / Opus** : délégué à l'encodeur FSBank du *FMOD Engine*
//!   (téléchargement gratuit sur fmod.com, compte requis) :
//!   - Windows / Linux : l'outil en ligne de commande `bin/fsbankcl.exe` / `bin/fsbank`
//!     (+ ses bibliothèques dans `bin/lib`), ou à défaut la bibliothèque `api/fsbank` ;
//!   - macOS : la bibliothèque `api/fsbank/lib/libfsbank.dylib`, appelée directement. Le seul
//!     exécutable fourni est l'application `FSBank.app`, qui ouvrirait sa fenêtre à chaque son.
//!
//!   `CXSSM_FSBANK=lib` ou `CXSSM_FSBANK=cli` force l'une ou l'autre voie.
//!   FMOD Vorbis utilise des en-têtes Vorbis pré-calculés identifiés par CRC :
//!   seul l'encodeur officiel garantit un flux lisible par le moteur du jeu.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use cxssm_fmod::{Codec, Fsb5, Sample};
use cxssm_fmod_studio::FsbankFormat;

use crate::error::{Error, Result};
use crate::fmod_install::FmodInstall;

#[derive(Debug, Clone, Default)]
pub struct EncoderConfig {
    /// Chemin de `fsbankcl` (`fsbankcl.exe` sous Windows).
    pub fsbankcl: Option<PathBuf>,
    /// Dossier `api/` du FMOD Engine, si sa bibliothèque FSBank est présente.
    pub fsbank_api: Option<PathBuf>,
    /// Qualité Vorbis 1–100 (FMOD Studio utilise 37 par défaut côté jeu mobile, 75+ conseillé PC).
    pub quality: u8,
}

impl EncoderConfig {
    /// Configuration tirée d'une installation du FMOD Engine.
    pub fn from_install(fmod: Option<&FmodInstall>, quality: u8) -> Self {
        Self {
            fsbankcl: fmod.and_then(|f| f.fsbank.clone()),
            fsbank_api: fmod.filter(|f| f.has_fsbank_lib()).map(|f| f.api.clone()),
            quality,
        }
    }

    /// Un encodeur FSBank (bibliothèque ou ligne de commande) est disponible.
    pub fn available(&self) -> bool {
        self.fsbankcl.is_some() || self.fsbank_api.is_some()
    }

    /// Faut-il passer par la bibliothèque plutôt que par l'exécutable ?
    fn use_library(&self) -> bool {
        match std::env::var("CXSSM_FSBANK").as_deref() {
            Ok("lib") => return self.fsbank_api.is_some(),
            Ok("cli") => return self.fsbankcl.is_none() && self.fsbank_api.is_some(),
            _ => {}
        }
        self.fsbank_api.is_some() && (cfg!(target_os = "macos") || self.fsbankcl.is_none())
    }
}

pub fn encode(cfg: &EncoderConfig, input: &Path, target: &Sample, codec: Codec) -> Result<Sample> {
    match codec {
        Codec::Pcm16 => encode_pcm16(input, target),
        c => match c.fsbank_format() {
            Some(fmt) => encode_with_fsbank(cfg, input, target, fmt),
            None => Err(Error::Encoder(format!("codec {c} non supporté pour l'instant"))),
        },
    }
}

fn encode_pcm16(input: &Path, target: &Sample) -> Result<Sample> {
    let mut r = hound::WavReader::open(input).map_err(|e| Error::Audio(e.to_string()))?;
    let spec = r.spec();
    if spec.bits_per_sample != 16 || spec.sample_format != hound::SampleFormat::Int {
        return Err(Error::Audio("WAV PCM 16 bits requis".into()));
    }
    let mut data = Vec::new();
    for s in r.samples::<i16>() {
        data.extend_from_slice(&s.map_err(|e| Error::Audio(e.to_string()))?.to_le_bytes());
    }
    let frames = data.len() as u64 / (2 * spec.channels as u64);
    Ok(Sample {
        name: target.name.clone(),
        frequency: spec.sample_rate,
        channels: spec.channels,
        num_samples: frames,
        loop_range: target.loop_range.map(|(s, _)| (s, frames.saturating_sub(1) as u32)),
        vorbis_crc: None,
        chunks: Vec::new(),
        data,
        header_bits: None,
    })
}

fn encode_with_fsbank(cfg: &EncoderConfig, input: &Path, target: &Sample, format: &str) -> Result<Sample> {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!("cxssm-{}-{n}.fsb", std::process::id()));
    if cfg.use_library() {
        let api = cfg.fsbank_api.as_deref().expect("use_library vérifie fsbank_api");
        let fmt = FsbankFormat::from_name(format).ok_or_else(|| Error::Encoder(format!("format {format} inconnu")))?;
        let built = cxssm_fmod_studio::build_fsb(api, input, &tmp, fmt, cfg.quality.into(), target.loop_range.is_some());
        if let Err(e) = built {
            let _ = std::fs::remove_file(&tmp);
            return Err(Error::Encoder(format!("FSBank : {e}")));
        }
        return read_single_sample(&tmp);
    }
    let exe = cfg
        .fsbankcl
        .as_ref()
        .ok_or_else(|| Error::Encoder("chemin de fsbank non configuré (CXSSM_FSBANKCL)".into()))?;
    let quality = cfg.quality.clamp(1, 100).to_string();

    let mut cmd = Command::new(exe);
    cmd.args(["-format", format, "-quality", &quality, "-rebuild", "-o"]).arg(&tmp);
    if target.loop_range.is_none() {
        // fsbank encode par défaut « comme une boucle » : inutile pour un son ponctuel.
        cmd.arg("-no_loop");
    }
    cmd.arg(input);
    // Linux : le binaire FMOD cherche ses .so dans `bin/lib`.
    if let Some(lib) = exe.parent().map(|d| d.join("lib")).filter(|d| d.is_dir()) {
        let mut paths = vec![lib];
        if let Some(old) = std::env::var_os("LD_LIBRARY_PATH") {
            paths.extend(std::env::split_paths(&old));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            cmd.env("LD_LIBRARY_PATH", joined);
        }
    }
    let output = cmd.output()?;
    if !output.status.success() || !tmp.exists() {
        let log = String::from_utf8_lossy(&output.stdout).to_string() + &String::from_utf8_lossy(&output.stderr);
        return Err(Error::Encoder(format!("fsbank a échoué ({}) : {}", output.status, log.trim())));
    }
    read_single_sample(&tmp)
}

/// Lit le seul son du FSB produit par FSBank, puis supprime le fichier.
fn read_single_sample(tmp: &Path) -> Result<Sample> {
    let bytes = std::fs::read(tmp);
    let _ = std::fs::remove_file(tmp);
    let fsb = Fsb5::parse(&bytes?)?;
    fsb.samples
        .into_iter()
        .next()
        .ok_or_else(|| Error::Encoder("FSB généré vide".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compare la bibliothèque FSBank et l'outil en ligne de commande sur un WAV généré.
    /// Nécessite un FMOD Engine installé : `cargo test -p cxssm-core -- --ignored fsbank`.
    #[test]
    #[ignore = "nécessite le FMOD Engine"]
    fn fsbank_library_matches_command_line() {
        let fmod = FmodInstall::resolve(None).expect("FMOD Engine introuvable");
        let wav = std::env::temp_dir().join(format!("cxssm-enc-test-{}.wav", std::process::id()));
        let spec = hound::WavSpec { channels: 1, sample_rate: 44100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&wav, spec).unwrap();
        for i in 0..44100u32 {
            w.write_sample(((i as f32 * 440.0 * std::f32::consts::TAU / 44100.0).sin() * 12000.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        let target = Sample {
            name: "test".into(),
            frequency: 44100,
            channels: 1,
            num_samples: 0,
            loop_range: None,
            vorbis_crc: None,
            chunks: Vec::new(),
            data: Vec::new(),
            header_bits: None,
        };
        let lib = EncoderConfig { fsbankcl: None, ..EncoderConfig::from_install(Some(&fmod), 80) };
        let a = encode(&lib, &wav, &target, Codec::Vorbis).expect("bibliothèque FSBank");
        assert_eq!((a.frequency, a.channels, a.num_samples), (44100, 1, 44100));
        assert!(a.vorbis_crc.is_some() && !a.data.is_empty());
        if fmod.fsbank.is_some() {
            let cli = EncoderConfig { fsbank_api: None, ..EncoderConfig::from_install(Some(&fmod), 80) };
            let b = encode(&cli, &wav, &target, Codec::Vorbis).expect("fsbank en ligne de commande");
            assert_eq!((a.num_samples, a.vorbis_crc, a.loop_range), (b.num_samples, b.vorbis_crc, b.loop_range));
            assert_eq!(a.data, b.data, "flux Vorbis identiques");
            // Son en boucle, et bibliothèque réutilisée pour un deuxième encodage.
            let looped = Sample { loop_range: Some((0, 44099)), ..target.clone() };
            let c = encode(&lib, &wav, &looped, Codec::Vorbis).expect("bibliothèque, boucle");
            let d = encode(&cli, &wav, &looped, Codec::Vorbis).expect("ligne de commande, boucle");
            assert_eq!((c.loop_range, &c.data), (d.loop_range, &d.data));
        }
        let _ = std::fs::remove_file(&wav);
    }
}
