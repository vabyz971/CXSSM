//! Encodage d'un fichier audio en échantillon FSB5 compatible avec l'original.
//!
//! - **PCM16** : encodage natif (lecture WAV via `hound`).
//! - **Vorbis / FADPCM / Opus** : délégué à `fsbankcl`, l'outil en ligne de commande
//!   fourni avec le *FMOD Engine* (téléchargement gratuit sur fmod.com, compte requis).
//!   Windows : `bin/fsbankcl.exe` ; Linux : `bin/fsbank` (+ ses bibliothèques dans `bin/lib`).
//!   FMOD Vorbis utilise des en-têtes Vorbis pré-calculés identifiés par CRC :
//!   seul l'encodeur officiel garantit un flux lisible par le moteur du jeu.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use cxssm_fmod::{Codec, Fsb5, Sample};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct EncoderConfig {
    /// Chemin de `fsbankcl` (`fsbankcl.exe` sous Windows).
    pub fsbankcl: Option<PathBuf>,
    /// Qualité Vorbis 1–100 (FMOD Studio utilise 37 par défaut côté jeu mobile, 75+ conseillé PC).
    pub quality: u8,
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
    let exe = cfg
        .fsbankcl
        .as_ref()
        .ok_or_else(|| Error::Encoder("chemin de fsbank non configuré (CXSSM_FSBANKCL)".into()))?;
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!("cxssm-{}-{n}.fsb", std::process::id()));
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
    let fsb = Fsb5::parse(&std::fs::read(&tmp)?)?;
    let _ = std::fs::remove_file(&tmp);
    fsb.samples
        .into_iter()
        .next()
        .ok_or_else(|| Error::Encoder("FSB généré vide".into()))
}
