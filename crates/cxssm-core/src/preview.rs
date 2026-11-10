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

fn pcm16(codec: Codec, sample: &Sample) -> Result<Vec<i16>> {
    Ok(match codec {
        Codec::Vorbis => vorbis::decode_pcm16(sample)?,
        Codec::Pcm16 => sample.data.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect(),
        c => return Err(Error::Audio(format!("lecture du codec {c} pas encore prise en charge"))),
    })
}

pub fn decode(codec: Codec, sample: &Sample) -> Result<DecodedSound> {
    let pcm = pcm16(codec, sample)?;
    Ok(DecodedSound { bytes: wav(&pcm, sample.frequency, sample.channels)?, extension: "wav" })
}

/// Plage de fréquences affichée par [`spectrum`] (Hz), sur une échelle logarithmique.
pub const SPECTRUM_MIN_HZ: f32 = 30.;
pub const SPECTRUM_MAX_HZ: f32 = 16_000.;
/// Dynamique affichée : de -`SPECTRUM_DB` dB à 0 dB (pic le plus fort).
const SPECTRUM_DB: f32 = 60.;
/// Taille des fenêtres FFT et nombre maximal de fenêtres analysées par son.
const FFT_SIZE: usize = 2048;
const MAX_FRAMES: usize = 48;

/// Spectre moyen du son (FFT, fenêtre de Hann) : `bins` valeurs 0..1 réparties sur une
/// échelle logarithmique de [`SPECTRUM_MIN_HZ`] à [`SPECTRUM_MAX_HZ`].
pub fn spectrum(codec: Codec, sample: &Sample, bins: usize) -> Result<Vec<f32>> {
    let pcm = pcm16(codec, sample)?;
    Ok(spectrum_of(&pcm, sample.channels.max(1) as usize, sample.frequency, bins))
}

fn spectrum_of(pcm: &[i16], channels: usize, rate: u32, bins: usize) -> Vec<f32> {
    // Mixage mono, normalisé en -1..1.
    let mono: Vec<f32> =
        pcm.chunks(channels).map(|f| f.iter().map(|&x| f32::from(x)).sum::<f32>() / (channels as f32 * 32768.)).collect();
    let mut power = vec![0f32; FFT_SIZE / 2];
    if mono.len() >= FFT_SIZE {
        let frames = ((mono.len() - FFT_SIZE) / FFT_SIZE + 1).min(MAX_FRAMES);
        let span = mono.len() - FFT_SIZE;
        let hann: Vec<f32> = (0..FFT_SIZE)
            .map(|i| 0.5 - 0.5 * (2. * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos())
            .collect();
        for f in 0..frames {
            // Fenêtres réparties sur toute la durée du son.
            let start = if frames > 1 { f * span / (frames - 1) } else { 0 };
            let mut re: Vec<f32> = (0..FFT_SIZE).map(|i| mono[start + i] * hann[i]).collect();
            let mut im = vec![0f32; FFT_SIZE];
            fft(&mut re, &mut im);
            for (k, p) in power.iter_mut().enumerate() {
                *p += re[k] * re[k] + im[k] * im[k];
            }
        }
    }
    // Regroupement en bandes logarithmiques puis passage en décibels.
    let hz_per_bin = rate as f32 / FFT_SIZE as f32;
    let max_hz = SPECTRUM_MAX_HZ.min(rate as f32 / 2.);
    let ratio = max_hz / SPECTRUM_MIN_HZ;
    let db: Vec<f32> = (0..bins)
        .map(|b| {
            let lo = SPECTRUM_MIN_HZ * ratio.powf(b as f32 / bins as f32);
            let hi = SPECTRUM_MIN_HZ * ratio.powf((b + 1) as f32 / bins as f32);
            // Raies FFT couvertes par la bande (au moins une, en basses fréquences).
            let k1 = ((hi / hz_per_bin).ceil() as usize).clamp(1, power.len());
            let k0 = ((lo / hz_per_bin) as usize).min(k1 - 1);
            let p = power[k0..k1].iter().sum::<f32>() / (k1 - k0) as f32;
            10. * (p + 1e-12).log10()
        })
        .collect();
    let top = db.iter().copied().fold(f32::MIN, f32::max);
    db.iter().map(|d| ((d - top + SPECTRUM_DB) / SPECTRUM_DB).clamp(0., 1.)).collect()
}

/// FFT complexe en place (radix 2, taille puissance de 2).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2. * std::f32::consts::PI / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (wr, wi) = ((ang * k as f32).cos(), (ang * k as f32).sin());
                let (a, b) = (start + k, start + k + len / 2);
                let (xr, xi) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                re[b] = re[a] - xr;
                im[b] = im[a] - xi;
                re[a] += xr;
                im[a] += xi;
            }
        }
        len <<= 1;
    }
}

/// Enveloppe du son pour l'affichage : `bins` crêtes (0..1, normalisées sur la plus forte).
pub fn peaks(codec: Codec, sample: &Sample, bins: usize) -> Result<Vec<f32>> {
    Ok(envelope(&pcm16(codec, sample)?, bins))
}

fn envelope(pcm: &[i16], bins: usize) -> Vec<f32> {
    if pcm.is_empty() || bins == 0 {
        return vec![0.; bins];
    }
    let mut v: Vec<f32> = (0..bins)
        .map(|i| {
            let (a, b) = (i * pcm.len() / bins, ((i + 1) * pcm.len() / bins).max(i * pcm.len() / bins + 1));
            pcm[a..b.min(pcm.len())].iter().map(|s| s.unsigned_abs()).max().unwrap_or(0) as f32
        })
        .collect();
    let max = v.iter().copied().fold(0., f32::max);
    if max > 0. {
        v.iter_mut().for_each(|x| *x /= max);
    }
    v
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

#[cfg(test)]
mod tests {
    #[test]
    fn envelope_is_normalised() {
        let pcm: Vec<i16> = (0..1000).map(|i| if i < 500 { 100 } else { -400 }).collect();
        let e = super::envelope(&pcm, 4);
        assert_eq!(e, vec![0.25, 0.25, 1.0, 1.0]);
        assert_eq!(super::envelope(&[], 3), vec![0.; 3]);
    }

    #[test]
    fn spectrum_peaks_at_the_tone_frequency() {
        // Sinus de 1 kHz à 48 kHz : le maximum du spectre tombe dans la bande de 1 kHz.
        let rate = 48_000;
        let pcm: Vec<i16> = (0..rate).map(|i| ((i as f32 * 2. * std::f32::consts::PI * 1000. / rate as f32).sin() * 20_000.) as i16).collect();
        let bins = 64;
        let s = super::spectrum_of(&pcm, 1, rate as u32, bins);
        let peak = s.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        let ratio = super::SPECTRUM_MAX_HZ / super::SPECTRUM_MIN_HZ;
        let (lo, hi) = (super::SPECTRUM_MIN_HZ * ratio.powf(peak as f32 / bins as f32), super::SPECTRUM_MIN_HZ * ratio.powf((peak + 1) as f32 / bins as f32));
        assert!(lo <= 1000. && 1000. <= hi * 1.1, "pic à {lo}-{hi} Hz");
        assert_eq!(s[peak], 1.);
        assert!(s[2] < 0.3, "basses fréquences atténuées : {}", s[2]);
    }
}
