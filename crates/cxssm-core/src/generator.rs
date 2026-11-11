//! **Expérimental** : recrée les boucles moteur d'une voiture à partir d'un
//! enregistrement continu (balayage du régime en accélération ou pied levé).
//!
//! 1. [`track_rpm`] suit le régime tout au long de l'enregistrement : la raie la plus
//!    forte du spectre est l'harmonique d'allumage (ordre 3 pour un 6 cylindres
//!    4 temps : `fréquence = régime / 60 × 3`), suivie image par image par
//!    programmation dynamique pour ne pas sauter d'une raie à l'autre.
//! 2. Pour chaque son du jeu, le régime à reproduire est son régime « naturel »
//!    ([`MapEntry::native_rpm`]) : celui que FMOD restitue quand il en change la hauteur.
//! 3. [`make_loop`] extrait le passage de l'enregistrement à ce régime, le « dé-chirpe »
//!    (rééchantillonnage variable qui ramène la hauteur à un régime constant), puis en
//!    fait une boucle sans couture : longueur et fondu en nombre entier de cycles moteur.

use std::path::Path;

use cxssm_fmod::{Codec, Sample};

use crate::error::{Error, Result};
use crate::preview;

/// Ordre de l'harmonique dominante : nombre d'explosions par tour (cylindres / 2 en 4 temps).
pub const DEFAULT_ORDER: f32 = 3.;
/// Écart de hauteur maximal accepté pour atteindre un régime hors de l'enregistrement.
pub const MAX_SHIFT: f32 = 1.35;

/// Plage de fréquences suivie (Hz) et résolution de la grille (cases par octave).
const F_MIN: f32 = 25.;
const F_MAX: f32 = 800.;
const BINS_PER_OCTAVE: f32 = 96.;
/// Saut maximal d'une image à la suivante (cases) et pénalité par case (dB).
const MAX_JUMP: i32 = 4;
const JUMP_COST: f32 = 1.5;

/// Enregistrement mono, échantillons -1..1.
#[derive(Debug, Clone)]
pub struct Recording {
    pub rate: u32,
    pub mono: Vec<f32>,
}

impl Recording {
    /// Lit un WAV (PCM entier ou flottant) et le mixe en mono.
    pub fn load(path: &Path) -> Result<Self> {
        let err = |e: hound::Error| Error::Audio(format!("{} : {e}", path.display()));
        let mut r = hound::WavReader::open(path).map_err(err)?;
        let spec = r.spec();
        let ch = usize::from(spec.channels.max(1));
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => r.samples::<f32>().collect::<std::result::Result<_, _>>().map_err(err)?,
            hound::SampleFormat::Int => {
                let scale = 1. / (1u64 << (spec.bits_per_sample - 1)) as f32;
                r.samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect::<std::result::Result<_, _>>().map_err(err)?
            }
        };
        let mono = samples.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect();
        Ok(Self { rate: spec.sample_rate, mono })
    }

    pub fn duration_secs(&self) -> f32 {
        self.mono.len() as f32 / self.rate as f32
    }
}

/// Régime estimé image par image.
#[derive(Debug, Clone)]
pub struct RpmTrack {
    /// Position (échantillons) du centre de la première image et pas entre images.
    offset: usize,
    hop: usize,
    pub rpm: Vec<f32>,
    /// Image exploitable : raie nettement au-dessus du bruit et signal assez fort.
    pub valid: Vec<bool>,
    /// Émergence (dB) de la raie suivie et de son harmonique double au-dessus de la médiane.
    pub salience: Vec<(f32, f32)>,
}

impl RpmTrack {
    /// Instant (s) du centre de l'image `i`.
    pub fn time(&self, i: usize, rate: u32) -> f32 {
        (self.offset + i * self.hop) as f32 / rate as f32
    }

    /// Régime interpolé à une position (échantillons) de l'enregistrement.
    pub fn at(&self, pos: f64) -> f32 {
        let x = ((pos - self.offset as f64) / self.hop as f64).clamp(0., (self.rpm.len() - 1) as f64);
        let i = x.floor() as usize;
        let j = (i + 1).min(self.rpm.len() - 1);
        let t = (x - i as f64) as f32;
        self.rpm[i] + (self.rpm[j] - self.rpm[i]) * t
    }

    /// Régimes minimal et maximal couverts par les images exploitables.
    pub fn range(&self) -> Option<(f32, f32)> {
        let v = self.rpm.iter().zip(&self.valid).filter(|(_, &ok)| ok).map(|(&r, _)| r);
        v.fold(None, |acc, r| Some(acc.map_or((r, r), |(a, b): (f32, f32)| (a.min(r), b.max(r)))))
    }

    /// Toutes les images recouvrant `[start, start + len]` (échantillons) sont exploitables.
    fn all_valid(&self, start: f64, len: f64) -> bool {
        let first = ((start - self.offset as f64) / self.hop as f64).floor().max(0.) as usize;
        let last = (((start + len - self.offset as f64) / self.hop as f64).ceil().max(0.) as usize).min(self.valid.len() - 1);
        first <= last && self.valid[first..=last].iter().all(|&v| v)
    }

    /// Image exploitable dont le régime est le plus proche de `rpm`.
    fn nearest(&self, rpm: f32) -> Option<usize> {
        (0..self.rpm.len()).filter(|&i| self.valid[i]).min_by(|&a, &b| (self.rpm[a] - rpm).abs().total_cmp(&(self.rpm[b] - rpm).abs()))
    }
}

/// Suit le régime dans `rec` ; `order` = harmonique dominante (voir [`DEFAULT_ORDER`]).
pub fn track_rpm(rec: &Recording, order: f32) -> Result<RpmTrack> {
    // Fenêtre ≈ 0,34 s : résolution de ~3 Hz, nécessaire au ralenti (raie vers 50 Hz).
    let n = ((rec.rate as f32 * 0.34) as usize).next_power_of_two();
    let hop = n / 8;
    if rec.mono.len() < n * 2 {
        return Err(Error::Audio("enregistrement trop court".into()));
    }
    let frames = (rec.mono.len() - n) / hop + 1;
    let grid = ((F_MAX / F_MIN).log2() * BINS_PER_OCTAVE).ceil() as usize;
    let freq = |g: f32| F_MIN * (g / BINS_PER_OCTAVE).exp2();
    let hann: Vec<f32> = (0..n).map(|i| 0.5 - 0.5 * (2. * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos()).collect();

    // Score (dB, 0 = maximum de l'image) de chaque case, saillance et énergie.
    let mut score = vec![vec![0f32; grid]; frames];
    let mut energy = vec![0f32; frames];
    let (mut re, mut im) = (vec![0f32; n], vec![0f32; n]);
    for (t, row) in score.iter_mut().enumerate() {
        let seg = &rec.mono[t * hop..t * hop + n];
        energy[t] = seg.iter().map(|x| x * x).sum::<f32>() / n as f32;
        for i in 0..n {
            re[i] = seg[i] * hann[i];
            im[i] = 0.;
        }
        preview::fft(&mut re, &mut im);
        let mag = |k: usize| (re[k] * re[k] + im[k] * im[k]).sqrt();
        for (g, cell) in row.iter_mut().enumerate() {
            let k = freq(g as f32) * n as f32 / rec.rate as f32;
            let (k0, frac) = (k.floor() as usize, k.fract());
            let m = mag(k0) * (1. - frac) + mag(k0 + 1) * frac;
            *cell = 20. * (m + 1e-9).log10();
        }
        let top = row.iter().copied().fold(f32::MIN, f32::max);
        row.iter_mut().for_each(|c| *c -= top);
    }

    // Programmation dynamique : chemin de score maximal à variation bornée.
    let mut cost = score[0].clone();
    let mut back = vec![vec![0u16; grid]; frames];
    for t in 1..frames {
        let mut next = vec![f32::MIN; grid];
        for g in 0..grid {
            let mut best = (f32::MIN, g);
            for d in -MAX_JUMP..=MAX_JUMP {
                let p = g as i32 + d;
                if p < 0 || p >= grid as i32 {
                    continue;
                }
                let c = cost[p as usize] - JUMP_COST * d.unsigned_abs() as f32;
                if c > best.0 {
                    best = (c, p as usize);
                }
            }
            next[g] = best.0 + score[t][g];
            back[t][g] = best.1 as u16;
        }
        cost = next;
    }
    let mut path = vec![0usize; frames];
    path[frames - 1] = (0..grid).max_by(|&a, &b| cost[a].total_cmp(&cost[b])).unwrap_or(0);
    for t in (1..frames).rev() {
        path[t - 1] = usize::from(back[t][path[t]]);
    }

    // Affinage parabolique, saillance (raie vs médiane de l'image) et énergie.
    let loudest = energy.iter().copied().fold(0f32, f32::max).max(1e-12);
    let mut rpm = Vec::with_capacity(frames);
    let mut valid = Vec::with_capacity(frames);
    let mut saliences = Vec::with_capacity(frames);
    for t in 0..frames {
        let g = path[t];
        let row = &score[t];
        let mut gf = g as f32;
        if g > 0 && g + 1 < grid {
            let (a, b, c) = (row[g - 1], row[g], row[g + 1]);
            let den = a - 2. * b + c;
            if den < 0. {
                gf += (0.5 * (a - c) / den).clamp(-0.5, 0.5);
            }
        }
        rpm.push(freq(gf) * 60. / order);
        let mut sorted = row.clone();
        sorted.sort_by(f32::total_cmp);
        let median = sorted[grid / 2];
        let salience = row[g] - median;
        // Harmonique double (une octave = BINS_PER_OCTAVE cases), meilleure case voisine.
        let h = g + BINS_PER_OCTAVE as usize;
        let second = if h + 2 < grid { row[h - 2..=h + 2].iter().copied().fold(f32::MIN, f32::max) - median } else { salience };
        saliences.push((salience, second));
        valid.push(salience > 18. && second > 8. && energy[t] > loudest * 1e-3);
    }
    // Lissage : médiane glissante (élimine les accrocs) puis moyenne.
    let med: Vec<f32> = (0..frames)
        .map(|t| {
            let mut w: Vec<f32> = rpm[t.saturating_sub(3)..(t + 4).min(frames)].to_vec();
            w.sort_by(f32::total_cmp);
            w[w.len() / 2]
        })
        .collect();
    let smooth = (0..frames)
        .map(|t| {
            let w = &med[t.saturating_sub(2)..(t + 3).min(frames)];
            w.iter().sum::<f32>() / w.len() as f32
        })
        .collect();
    Ok(RpmTrack { offset: n / 2, hop, rpm: smooth, valid, salience: saliences })
}

/// Caractéristiques de la boucle à produire (celles du son d'origine).
#[derive(Debug, Clone, Copy)]
pub struct LoopSpec {
    /// Régime à reproduire.
    pub rpm: f32,
    pub rate: u32,
    pub channels: u16,
    /// Durée visée (ajustée à un nombre entier de cycles moteur).
    pub seconds: f32,
    /// Niveau RMS visé (-1..1), celui du son d'origine.
    pub rms: f32,
}

/// Boucle générée.
#[derive(Debug, Clone)]
pub struct Generated {
    /// PCM 16 bits entrelacé.
    pub pcm: Vec<i16>,
    pub rate: u32,
    pub channels: u16,
    /// Régime de l'enregistrement au centre du passage utilisé.
    pub source_rpm: f32,
    /// Instant (s) de ce passage dans l'enregistrement.
    pub source_time: f32,
    /// Rapport de hauteur appliqué (cible / source).
    pub shift: f32,
}

/// Interpolation cubique (Catmull-Rom) de `x` à la position fractionnaire `pos`.
fn cubic(x: &[f32], pos: f64) -> f32 {
    let i = pos.floor() as isize;
    let t = (pos - i as f64) as f32;
    let at = |k: isize| x[k.clamp(0, x.len() as isize - 1) as usize];
    let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
    p1 + 0.5 * t * (p2 - p0 + t * (2. * p0 - 5. * p1 + 4. * p2 - p3 + t * (3. * (p1 - p2) + p3 - p0)))
}

/// Longueur de boucle et de fondu (échantillons de sortie) : nombres entiers de cycles
/// moteur (2 tours en 4 temps) pour que la boucle se raccorde en phase.
fn loop_lengths(spec: &LoopSpec, seconds: f32) -> (usize, usize) {
    let cycle = f64::from(spec.rate) * 120. / f64::from(spec.rpm);
    let cycles = (f64::from(seconds) * f64::from(spec.rate) / cycle).round().max(4.);
    let fade = (0.06 * f64::from(spec.rate) / cycle).round().max(2.);
    ((cycles * cycle).round() as usize, (fade * cycle).round() as usize)
}

/// Positions de lecture (échantillons de `rec`) pour `total` échantillons de sortie à
/// partir de `start` : vitesse `cible / régime source`, qui ramène la hauteur à `spec.rpm`.
fn read_positions(rec: &Recording, track: &RpmTrack, spec: &LoopSpec, start: f64, total: usize) -> Vec<f64> {
    let ratio = f64::from(rec.rate) / f64::from(spec.rate);
    let mut pos = start;
    (0..total)
        .map(|_| {
            let p = pos;
            pos += ratio * f64::from(spec.rpm) / f64::from(track.at(pos));
            p
        })
        .collect()
}

/// Extrait de `rec` une boucle au régime `spec.rpm` (voir le module). Aux extrémités
/// de l'enregistrement, la boucle est raccourcie pour rester au plus près du régime visé.
pub fn make_loop(rec: &Recording, track: &RpmTrack, spec: LoopSpec) -> Result<Generated> {
    let frame = track.nearest(spec.rpm).ok_or_else(|| Error::Audio("régime introuvable dans l'enregistrement".into()))?;
    let nearest = track.rpm[frame];
    if !(1. / MAX_SHIFT..=MAX_SHIFT).contains(&(spec.rpm / nearest)) {
        return Err(Error::Audio(format!(
            "{:.0} tr/min hors de l'enregistrement (au plus près : {nearest:.0} tr/min)",
            spec.rpm
        )));
    }
    let centre = (track.offset + frame * track.hop) as f64;
    let end = rec.mono.len() as f64 - 3.;

    // Meilleur passage : longueur décroissante, puis décalage autour de l'image choisie
    // en évitant les images inexploitables (bruit, changement de rapport…).
    let mut best: Option<(f32, Vec<f64>, usize, usize)> = None;
    for seconds in [spec.seconds, spec.seconds * 0.6, 0.8, 0.5] {
        let (len, fade) = loop_lengths(&spec, seconds);
        let total = len + fade;
        let probe = read_positions(rec, track, &spec, centre, total);
        let span = probe[total - 1] - centre;
        let ideal = centre - span / 2.;
        let step = track.hop as f64;
        let found = (0..2 * (rec.mono.len() / track.hop))
            .map(|k| ideal + if k % 2 == 0 { 1. } else { -1. } * (k.div_ceil(2) as f64) * step)
            .filter(|&p| p >= 0.)
            .find_map(|p| {
                let pos = read_positions(rec, track, &spec, p, total);
                (pos[total - 1] < end && track.all_valid(p, pos[total - 1] - p)).then_some(pos)
            });
        let Some(pos) = found else { continue };
        let mid = track.at(pos[total / 2]);
        let err = (spec.rpm / mid).ln().abs();
        if best.as_ref().is_none_or(|b| err < b.0 - 0.01) {
            best = Some((err, pos, len, fade));
        }
        // Assez près du régime visé : inutile de raccourcir davantage.
        if err < 0.03 {
            break;
        }
    }
    let (_, pos, len, fade) = best.ok_or_else(|| Error::Audio("pas de passage continu assez long à ce régime".into()))?;
    let source_rpm = track.at(pos[pos.len() / 2]);
    let source_time = pos[pos.len() / 2] as f32 / rec.rate as f32;
    let shift = spec.rpm / source_rpm;
    if !(1. / MAX_SHIFT..=MAX_SHIFT).contains(&shift) {
        return Err(Error::Audio(format!("{:.0} tr/min trop loin du passage utilisable ({source_rpm:.0} tr/min)", spec.rpm)));
    }
    let d: Vec<f32> = pos.iter().map(|&p| cubic(&rec.mono, p)).collect();

    // Boucle sans couture : le début est fondu avec ce qui suit la fin (même phase,
    // puisque `len` est un nombre entier de cycles).
    let mut out: Vec<f32> = (0..len)
        .map(|i| {
            if i < fade {
                let a = std::f32::consts::FRAC_PI_2 * i as f32 / fade as f32;
                d[i] * a.sin() + d[len + i] * a.cos()
            } else {
                d[i]
            }
        })
        .collect();
    let mean = out.iter().sum::<f32>() / len as f32;
    out.iter_mut().for_each(|x| *x -= mean);
    let rms = (out.iter().map(|x| x * x).sum::<f32>() / len as f32).sqrt().max(1e-9);
    let peak = out.iter().fold(0f32, |m, x| m.max(x.abs())).max(1e-9);
    let gain = (spec.rms / rms).min(0.98 / peak);

    let ch = usize::from(spec.channels.max(1));
    let mut pcm = Vec::with_capacity(len * ch);
    for x in out {
        let v = (x * gain * 32767.).round().clamp(-32768., 32767.) as i16;
        pcm.extend(std::iter::repeat_n(v, ch));
    }
    Ok(Generated { pcm, rate: spec.rate, channels: spec.channels, source_rpm, source_time, shift })
}

/// Niveau RMS (-1..1) d'un son du jeu, tous canaux confondus.
pub fn sample_rms(codec: Codec, sample: &Sample) -> Result<f32> {
    let pcm = preview::pcm16(codec, sample)?;
    let sum: f64 = pcm.iter().map(|&v| (f64::from(v) / 32768.).powi(2)).sum();
    Ok((sum / pcm.len().max(1) as f64).sqrt() as f32)
}

/// Écrit une boucle générée en WAV 16 bits.
pub fn write_wav(path: &Path, g: &Generated) -> Result<()> {
    std::fs::write(path, preview::wav(&g.pcm, g.rate, g.channels)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Balayage synthétique d'un 6 cylindres : harmoniques 1,5 / 3 / 6 du régime.
    fn sweep(rate: u32, secs: f32, from: f32, to: f32) -> Recording {
        let n = (rate as f32 * secs) as usize;
        let mut phase = 0f64;
        let mono = (0..n)
            .map(|i| {
                let rpm = from + (to - from) * i as f32 / n as f32;
                phase += f64::from(rpm) / 60. / f64::from(rate) * std::f64::consts::TAU;
                (0.2 * (1.5 * phase).sin() + 0.6 * (3. * phase).sin() + 0.25 * (6. * phase).sin()) as f32
            })
            .collect();
        Recording { rate, mono }
    }

    #[test]
    fn tracks_a_synthetic_sweep() {
        let rec = sweep(48_000, 8., 1500., 6500.);
        let tr = track_rpm(&rec, 3.).unwrap();
        let (lo, hi) = tr.range().unwrap();
        assert!((lo - 1600.).abs() < 150. && (hi - 6400.).abs() < 150., "plage {lo}..{hi}");
        let mid = tr.rpm.len() / 2;
        let expected = 1500. + 5000. * tr.time(mid, rec.rate) / 8.;
        assert!((tr.rpm[mid] - expected).abs() / expected < 0.02, "{} vs {expected}", tr.rpm[mid]);
    }

    #[test]
    fn loop_has_the_requested_pitch_and_level() {
        let rec = sweep(48_000, 8., 1500., 6500.);
        let tr = track_rpm(&rec, 3.).unwrap();
        let spec = LoopSpec { rpm: 4000., rate: 48_000, channels: 2, seconds: 1., rms: 0.1 };
        let g = make_loop(&rec, &tr, spec).unwrap();
        assert_eq!(g.channels, 2);
        let mono: Vec<f32> = g.pcm.chunks(2).map(|f| f32::from(f[0]) / 32768.).collect();
        let rms = (mono.iter().map(|x| x * x).sum::<f32>() / mono.len() as f32).sqrt();
        assert!((rms - 0.1).abs() < 0.01, "rms {rms}");
        // Cycle moteur (2 tours) à 4000 tr/min : 30 ms = 1440 échantillons ; un signal
        // ramené à un régime constant est périodique à ce décalage.
        let lag = 1440;
        let r: f32 = mono.iter().zip(&mono[lag..]).map(|(a, b)| a * b).sum::<f32>()
            / mono.iter().map(|x| x * x).sum::<f32>();
        assert!(r > 0.9, "autocorrélation {r}");
        // Raccord de boucle : pas de saut entre la fin et le début.
        let jump = (mono[mono.len() - 1] - mono[0]).abs();
        assert!(jump < 0.05, "saut {jump}");
    }
}
