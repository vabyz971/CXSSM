//! Analyse d'un fichier audio utilisateur (WAV, OGG, FLAC, MP3) via symphonia.

use std::path::Path;

use symphonia::core::{formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct AudioInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: Option<u64>,
}

impl AudioInfo {
    pub fn duration_secs(&self) -> Option<f64> {
        self.frames.map(|f| f as f64 / self.sample_rate as f64)
    }
}

pub fn probe(path: &Path) -> Result<AudioInfo> {
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| Error::Audio(e.to_string()))?;
    let track = probed
        .format
        .default_track()
        .ok_or_else(|| Error::Audio("aucune piste audio".into()))?;
    let p = &track.codec_params;
    Ok(AudioInfo {
        sample_rate: p.sample_rate.ok_or_else(|| Error::Audio("fréquence inconnue".into()))?,
        channels: p.channels.map(|c| c.count() as u16).unwrap_or(1),
        frames: p.n_frames,
    })
}
