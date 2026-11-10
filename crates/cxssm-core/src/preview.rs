//! Décodage d'un son du jeu en **WAV** (pré-écoute / export des originaux).

use cxssm_fmod::{vorbis, Codec, Sample};

use crate::error::{Error, Result};

/// Son décodé, prêt à être lu ou écrit sur disque.
#[derive(Debug, Clone)]
pub struct DecodedSound {
    /// Fichier WAV complet (PCM 16 bits).
    pub bytes: Vec<u8>,
    pub extension: &'static str,
}

pub fn decode(codec: Codec, sample: &Sample) -> Result<DecodedSound> {
    let pcm: Vec<i16> = match codec {
        Codec::Vorbis => vorbis::decode_pcm16(sample)?,
        Codec::Pcm16 => sample.data.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect(),
        c => return Err(Error::Audio(format!("lecture du codec {c} pas encore prise en charge"))),
    };
    Ok(DecodedSound { bytes: wav(&pcm, sample.frequency, sample.channels)?, extension: "wav" })
}

fn wav(pcm: &[i16], rate: u32, channels: u16) -> Result<Vec<u8>> {
    let spec = hound::WavSpec { channels, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut cursor = std::io::Cursor::new(Vec::with_capacity(44 + pcm.len() * 2));
    {
        let mut w = hound::WavWriter::new(&mut cursor, spec).map_err(|e| Error::Audio(e.to_string()))?;
        let mut w16 = w.get_i16_writer(pcm.len() as u32);
        for &s in pcm {
            w16.write_sample(s);
        }
        w16.flush().map_err(|e| Error::Audio(e.to_string()))?;
        w.finalize().map_err(|e| Error::Audio(e.to_string()))?;
    }
    Ok(cursor.into_inner())
}
