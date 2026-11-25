//! Simulateur moteur : rejoue les évènements moteur du jeu avec l'API FMOD Studio
//! et retrouve quels sons du bundle sont mélangés pour un régime donné.
//!
//! Chaque voiture a un évènement `event:/GameEvents/Engines/<voiture>` piloté par
//! les paramètres `RPM` (0..10500) et `Load` (0 = pied levé, 1 = accélération).
//! FMOD superpose 1 à 3 boucles dont il règle le volume et la hauteur selon le régime.

use std::collections::BTreeMap;
use std::sync::Arc;

use cxssm_fmod_studio::{Bank, EventInfo, FmodLib, Instance, Output, System};

use crate::error::{Error, Result};
use crate::project::{ModProject, SoundCategory, SoundRef};

pub use cxssm_fmod_studio::{load_library, ParamInfo};

/// Bibliothèque FMOD chargée, partageable entre threads.
pub type FmodLibHandle = Arc<FmodLib>;

/// Paramètre de régime moteur.
pub const RPM: &str = "RPM";
/// Paramètre de charge (0 = pied levé, 1 = accélération).
pub const LOAD: &str = "Load";

/// Volume en dessous duquel une couche est considérée comme inaudible.
const AUDIBLE: f32 = 0.02;

/// Taille des sons d'une banque, telle que FMOD la rapporte pour un son joué :
/// durée (échantillons PCM) et taille des données compressées (octets).
#[derive(Debug, Clone, Default)]
struct Sizes {
    pcm: Vec<u64>,
    raw: Vec<u64>,
}

impl Sizes {
    fn of(fsb: &cxssm_fmod::Fsb5) -> Self {
        Self { pcm: fsb.samples.iter().map(|s| s.num_samples).collect(), raw: fsb.samples.iter().map(|s| s.data.len() as u64).collect() }
    }

    /// Son de la banque `bank` joué avec cette durée et cette taille compressée (les FSB du
    /// jeu n'ont pas de noms). Plusieurs sons ont souvent la même durée : la taille compressée
    /// les départage. `None` si l'ambiguïté demeure.
    fn identify(&self, bank: usize, length_pcm: u32, length_raw: u32) -> Option<SoundRef> {
        let same_pcm: Vec<usize> = (0..self.pcm.len()).filter(|&i| self.pcm[i] == u64::from(length_pcm)).collect();
        let one = |v: &[usize]| (v.len() == 1).then(|| SoundRef { bank, sample: v[0] });
        one(&same_pcm).or_else(|| {
            let same_raw: Vec<usize> = same_pcm.into_iter().filter(|&i| self.raw.get(i) == Some(&u64::from(length_raw))).collect();
            one(&same_raw)
        })
    }
}

/// Copie de la banque dont chaque son a une durée unique, pour l'analyse : les sons de même
/// durée (et parfois de même taille) sont raccourcis de quelques échantillons, ce qui ne
/// change rien à ce que l'évènement joue mais permet de reconnaître chaque son à coup sûr.
fn distinct_bank(bytes: &[u8]) -> Result<(Vec<u8>, Sizes)> {
    let bank = cxssm_fmod::Bank::parse(bytes.to_vec())?;
    let mut fsb = bank.fsb()?;
    let mut used: std::collections::HashSet<u64> = fsb.samples.iter().map(|s| s.num_samples).collect();
    let mut seen = std::collections::HashSet::new();
    let mut changed = false;
    for s in &mut fsb.samples {
        if seen.insert(s.num_samples) {
            continue; // premier son de cette durée : inchangé
        }
        let mut len = s.num_samples;
        while len > 1 && used.contains(&len) {
            len -= 1;
        }
        used.insert(len);
        s.num_samples = len;
        if let Some((start, end)) = s.loop_range.as_mut() {
            let last = len.saturating_sub(1) as u32;
            *end = (*end).min(last);
            *start = (*start).min(*end);
        }
        changed = true;
    }
    let sizes = Sizes::of(&fsb);
    Ok((if changed { bank.with_fsb(&fsb.to_bytes()) } else { bytes.to_vec() }, sizes))
}

/// Couches audibles, identifiées avec les tailles `sizes`, de la plus forte à la plus faible.
fn to_active_in(sizes: &Sizes, bank: usize, layers: Vec<cxssm_fmod_studio::Layer>) -> Vec<ActiveLayer> {
    let mut v: Vec<ActiveLayer> = layers
        .into_iter()
        .filter(|l| l.volume > AUDIBLE)
        .map(|l| ActiveLayer {
            sound: sizes.identify(bank, l.length_pcm, l.length_raw),
            volume: l.volume,
            pitch: l.pitch,
            position_pcm: l.position_pcm,
            length_pcm: l.length_pcm,
            length_raw: l.length_raw,
        })
        .collect();
    v.sort_by(|a, b| b.volume.total_cmp(&a.volume));
    v
}

/// Évènement moteur d'une voiture.
#[derive(Debug, Clone)]
pub struct EngineEvent {
    /// Index de la banque dans [`ModProject::banks`].
    pub bank: usize,
    pub info: EventInfo,
}

impl EngineEvent {
    pub fn name(&self) -> &str {
        self.info.short_name()
    }

    pub fn rpm_range(&self) -> (f32, f32) {
        self.info.param(RPM).map_or((0., 10_500.), |p| (p.min, p.max))
    }
}

/// Son entendu à un instant donné.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveLayer {
    /// `None` si le son n'a pas pu être identifié de façon certaine.
    pub sound: Option<SoundRef>,
    pub volume: f32,
    pub pitch: f32,
    /// Tête de lecture dans le son (échantillons PCM).
    pub position_pcm: u32,
    /// Longueur du son joué (échantillons PCM), qui sert à l'identifier.
    pub length_pcm: u32,
    /// Taille compressée du son joué (octets).
    pub length_raw: u32,
}

/// Plage d'utilisation d'un son dans un évènement (résultat du balayage).
#[derive(Debug, Clone, PartialEq)]
pub struct MapEntry {
    pub sound: SoundRef,
    /// `true` : entendu en accélération (Load = 1) ; `false` : pied levé (Load = 0).
    pub on_load: bool,
    pub rpm_min: f32,
    pub rpm_max: f32,
    /// Régime où le son est le plus fort.
    pub rpm_peak: f32,
    pub peak_volume: f32,
    /// Volume mesuré à chaque pas du balayage où le son est audible : `(régime, volume)`.
    pub curve: Vec<(f32, f32)>,
    /// Hauteur de lecture appliquée par FMOD à chaque point de `curve` (1.0 = vitesse d'origine).
    pub pitches: Vec<f32>,
    /// `None` : entendu dans le balayage de base ; sinon, seulement dans cette situation.
    pub condition: Option<Condition>,
}

impl MapEntry {
    /// Volume (interpolé) à un régime donné, d'après la courbe du balayage.
    pub fn volume_at(&self, rpm: f32) -> f32 {
        let c = &self.curve;
        // Pas du balayage : écart minimal entre deux mesures.
        let step = c.windows(2).map(|w| w[1].0 - w[0].0).fold(f32::MAX, f32::min);
        match c.iter().position(|&(r, _)| r >= rpm) {
            None => 0.,
            Some(0) => if c[0].0 - rpm < 1. { c[0].1 } else { 0. },
            Some(i) => {
                let ((r0, v0), (r1, v1)) = (c[i - 1], c[i]);
                // Trou dans la courbe : le son est inaudible entre ces deux mesures.
                if r1 - r0 > 1.5 * step {
                    return 0.;
                }
                v0 + (v1 - v0) * ((rpm - r0) / (r1 - r0)).clamp(0., 1.)
            }
        }
    }

    /// Régime « naturel » du son : celui du moteur enregistré tel que FMOD le restitue,
    /// soit `régime / hauteur`, moyenné sur la plage où le son est audible (pondéré par
    /// son volume). `None` si FMOD ne fait pas varier la hauteur de ce son.
    pub fn native_rpm(&self) -> Option<f32> {
        let (mut sum, mut weight) = (0f64, 0f64);
        for (&(rpm, vol), &pitch) in self.curve.iter().zip(&self.pitches) {
            if pitch > 0.02 && rpm > 0. {
                sum += f64::from(rpm / pitch * vol);
                weight += f64::from(vol);
            }
        }
        (weight > 0.).then(|| (sum / weight) as f32)
    }

    /// Plages de fondu (entrée, sortie) en tr/min : régime parcouru entre l'apparition
    /// du son et 90 % de son volume maximal, puis entre ce plateau et sa disparition.
    pub fn fades(&self) -> (f32, f32) {
        let top = 0.9 * self.peak_volume;
        let first = self.curve.iter().find(|&&(_, v)| v >= top).map_or(self.rpm_peak, |&(r, _)| r);
        let last = self.curve.iter().rev().find(|&&(_, v)| v >= top).map_or(self.rpm_peak, |&(r, _)| r);
        (first - self.rpm_min, self.rpm_max - last)
    }
}

/// Banque moteur copiée hors du projet (pour travailler en arrière-plan).
#[derive(Debug)]
struct EngineBank {
    index: usize,
    bytes: Vec<u8>,
    /// Taille de chaque son du FSB (identification des sons joués).
    sizes: Sizes,
}

/// Instantané des banques nécessaires au simulateur : peu coûteux à cloner
/// (partagé par `Arc`) et indépendant de [`ModProject`].
#[derive(Debug, Clone)]
pub struct EngineData {
    /// Banques communes : `Master Bank` et `Master Bank.strings` (noms des évènements).
    base: Arc<Vec<Vec<u8>>>,
    banks: Arc<Vec<EngineBank>>,
}

impl EngineData {
    pub fn from_project(p: &ModProject) -> Self {
        let mut base: Vec<Vec<u8>> = p.aux_banks.iter().map(|(_, b)| b.clone()).collect();
        if let Some(master) = p.banks.iter().find(|b| b.name.eq_ignore_ascii_case("Master Bank")) {
            base.push(master.bank.raw().to_vec());
        }
        let banks = p
            .banks
            .iter()
            .enumerate()
            .filter(|(_, b)| SoundCategory::guess(&b.name, "") == SoundCategory::Engine)
            .map(|(index, b)| EngineBank {
                index,
                bytes: b.bank.raw().to_vec(),
                sizes: Sizes::of(&b.fsb),
            })
            .collect();
        Self { base: Arc::new(base), banks: Arc::new(banks) }
    }

    fn bank(&self, index: usize) -> Result<&EngineBank> {
        self.banks
            .iter()
            .find(|b| b.index == index)
            .ok_or_else(|| Error::Audio(format!("banque #{index} : pas une banque moteur")))
    }

    fn load_base(&self, sys: &System) -> Result<()> {
        for bytes in self.base.iter() {
            sys.load_bank(bytes)?;
        }
        Ok(())
    }

    /// Banque de l'évènement préparée pour l'analyse (voir [`distinct_bank`]).
    fn analysis_bank(&self, bank: usize) -> Result<(Vec<u8>, Sizes)> {
        distinct_bank(&self.bank(bank)?.bytes)
    }
}

/// Liste les évènements moteur de toutes les banques moteur (ceux qui ont un paramètre `RPM`).
pub fn list_events(lib: &Arc<FmodLib>, data: &EngineData) -> Result<Vec<EngineEvent>> {
    let sys = System::new(lib.clone(), Output::Analysis)?;
    data.load_base(&sys)?;
    let mut out = Vec::new();
    for b in data.banks.iter() {
        let bank = sys.load_bank(&b.bytes)?;
        for info in sys.events(bank)? {
            if info.param(RPM).is_some() {
                out.push(EngineEvent { bank: b.index, info });
            }
        }
        sys.unload_bank(bank);
    }
    out.sort_by_key(|e| (e.bank, e.name().to_lowercase()));
    Ok(out)
}

/// Lecture en direct d'un évènement moteur sur la carte son.
pub struct Simulator {
    system: System,
    instance: Option<Instance>,
    bank_index: usize,
    /// Taille de chaque son de la banque **jouée** : celle du mod quand elle contient
    /// des remplacements (leur durée diffère de l'original), sinon celle du jeu.
    sizes: Sizes,
    banks: Vec<Bank>,
    /// La banque jouée contient les sons de remplacement de l'utilisateur.
    pub modded: bool,
}

impl Simulator {
    /// `modded_bank` : banque reconstruite avec les remplacements (voir
    /// [`crate::export::build_bank`]) pour écouter le mod tel qu'il sonnera en jeu.
    pub fn new(
        lib: &Arc<FmodLib>,
        data: &EngineData,
        event: &EngineEvent,
        modded_bank: Option<&[u8]>,
    ) -> Result<Self> {
        let system = System::new(lib.clone(), Output::Speakers)?;
        data.load_base(&system)?;
        let modded = modded_bank.is_some();
        let original = data.bank(event.bank)?;
        let (bytes, sizes) = match modded_bank {
            Some(b) => {
                let fsb = cxssm_fmod::Bank::parse(b.to_vec())?.fsb()?;
                (b, Sizes::of(&fsb))
            }
            None => (original.bytes.as_slice(), original.sizes.clone()),
        };
        let bank = system.load_bank(bytes)?;
        system.load_sample_data(bank);
        let instance = system.create_instance(&event.info.path)?;
        let (min, _) = event.rpm_range();
        system.set_param(&instance, RPM, min.max(800.));
        system.set_param(&instance, LOAD, 0.);
        system.start(&instance);
        system.update();
        Ok(Self { system, instance: Some(instance), bank_index: event.bank, sizes, banks: vec![bank], modded })
    }

    pub fn set_param(&self, name: &str, value: f32) {
        if let Some(i) = &self.instance {
            self.system.set_param(i, name, value);
        }
    }

    /// À appeler régulièrement (≈ 60 fois par seconde) pendant la lecture.
    pub fn update(&self) {
        self.system.update();
    }

    pub fn layers(&self) -> Vec<ActiveLayer> {
        self.instance
            .as_ref()
            .map(|i| to_active_in(&self.sizes, self.bank_index, self.system.layers(i)))
            .unwrap_or_default()
    }
}

impl Drop for Simulator {
    fn drop(&mut self) {
        if let Some(i) = self.instance.take() {
            self.system.release(i);
        }
        for b in self.banks.drain(..) {
            self.system.unload_bank(b);
        }
        self.system.update();
    }
}

/// Situation dans laquelle un son n'est entendu qu'en dehors du balayage de base
/// (régime et charge, autres paramètres à leur valeur par défaut).
#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    /// Un autre paramètre de l'évènement à cette valeur (`Turbo`, `CameraView`, `Exhaust`…).
    Param { name: String, value: f32 },
    /// Juste après avoir relâché l'accélérateur (pétarades, décharges…).
    LiftOff,
}

impl std::fmt::Display for Condition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Condition::Param { name, value } => write!(f, "{name} = {value}"),
            Condition::LiftOff => f.write_str("lift-off"),
        }
    }
}

/// Paramètres qui ne font qu'atténuer ou spatialiser le son (distance, orientation) :
/// inutile de les balayer pour trouver d'autres sons.
fn spatial(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("distance") || n.contains("orientation")
}

/// Accumule les couches identifiées dans la carte.
fn record(acc: &mut BTreeMap<(SoundRef, bool), MapEntry>, layers: Vec<ActiveLayer>, on_load: bool, rpm: f32, condition: Option<&Condition>) {
    for l in layers {
        let Some(sound) = l.sound else { continue };
        // Un son déjà trouvé dans le balayage de base (quelle que soit la charge) y reste.
        if condition.is_some() && acc.iter().any(|((s, _), e)| *s == sound && e.condition.is_none()) {
            continue;
        }
        let e = acc.entry((sound, on_load)).or_insert(MapEntry {
            sound,
            on_load,
            rpm_min: rpm,
            rpm_max: rpm,
            rpm_peak: rpm,
            peak_volume: 0.,
            curve: Vec::new(),
            pitches: Vec::new(),
            condition: condition.cloned(),
        });
        if e.condition.as_ref() != condition {
            continue; // trouvé d'abord dans une autre condition
        }
        match e.curve.last_mut() {
            // Plusieurs relevés au même régime (relâchement) : le plus fort.
            Some(last) if last.0 == rpm => last.1 = last.1.max(l.volume),
            _ => {
                e.curve.push((rpm, l.volume));
                e.pitches.push(l.pitch);
            }
        }
        e.rpm_min = e.rpm_min.min(rpm);
        e.rpm_max = e.rpm_max.max(rpm);
        if l.volume > e.peak_volume {
            e.peak_volume = l.volume;
            e.rpm_peak = rpm;
        }
    }
}

/// Balaye tout le régime (accélération puis pied levé) sans sortie audio et renvoie,
/// pour chaque son entendu, sa plage de régime et son volume maximal.
///
/// Puis cherche les sons que ce balayage n'entend pas, pour qu'aucun son de la voiture
/// n'échappe à la liste (et qu'on puisse tous les remplacer ou les couper) :
/// - chaque autre paramètre de l'évènement (`Turbo`, `CameraView`, `Exhaust`, `HP`…) à son
///   minimum puis à son maximum ;
/// - le relâchement de l'accélérateur à différents régimes (sons ponctuels).
///
/// Ces sons portent leur [`Condition`].
pub fn sweep(lib: &Arc<FmodLib>, data: &EngineData, event: &EngineEvent, step: f32) -> Result<Vec<MapEntry>> {
    let sys = System::new(lib.clone(), Output::Analysis)?;
    data.load_base(&sys)?;
    let (bytes, sizes) = data.analysis_bank(event.bank)?;
    let bank = sys.load_bank(&bytes)?;
    sys.load_sample_data(bank);
    sys.wait_sample_data(bank, 2000);
    let (min, max) = event.rpm_range();
    let mut acc: BTreeMap<(SoundRef, bool), MapEntry> = BTreeMap::new();

    // Balayage du régime, avec éventuellement un autre paramètre fixé.
    let pass = |acc: &mut BTreeMap<(SoundRef, bool), MapEntry>, on_load: bool, step: f32, condition: Option<&Condition>| -> Result<()> {
        let inst = sys.create_instance(&event.info.path)?;
        if let Some(Condition::Param { name, value }) = condition {
            sys.set_param(&inst, name, *value);
        }
        sys.set_param(&inst, LOAD, if on_load { 1. } else { 0. });
        sys.set_param(&inst, RPM, min);
        sys.start(&inst);
        let mut rpm = min;
        while rpm <= max {
            sys.set_param(&inst, RPM, rpm);
            // Mixage non temps réel : chaque update avance d'un bloc ; on laisse
            // le temps aux fondus de volume de se stabiliser.
            for _ in 0..8 {
                sys.update();
            }
            record(acc, to_active_in(&sizes, event.bank, sys.layers(&inst)), on_load, rpm, condition);
            rpm += step;
        }
        sys.release(inst);
        sys.update();
        Ok(())
    };

    for on_load in [true, false] {
        pass(&mut acc, on_load, step, None)?;
    }
    let coarse = step.max(500.);
    for prm in event.info.params.iter().filter(|p| p.name != RPM && p.name != LOAD && !spatial(&p.name)) {
        for value in [prm.min, prm.max] {
            if (value - prm.default).abs() < 1e-3 {
                continue;
            }
            let cond = Condition::Param { name: prm.name.clone(), value };
            for on_load in [true, false] {
                pass(&mut acc, on_load, coarse, Some(&cond))?;
            }
        }
    }
    // Relâchement de l'accélérateur : sons ponctuels joués juste après.
    let mut rpm = (min + 1000.).max(2000.);
    while rpm <= max {
        let inst = sys.create_instance(&event.info.path)?;
        sys.set_param(&inst, RPM, rpm);
        sys.set_param(&inst, LOAD, 1.);
        sys.start(&inst);
        for _ in 0..16 {
            sys.update();
        }
        sys.set_param(&inst, LOAD, 0.);
        for _ in 0..24 {
            sys.update();
            record(&mut acc, to_active_in(&sizes, event.bank, sys.layers(&inst)), false, rpm, Some(&Condition::LiftOff));
        }
        sys.release(inst);
        sys.update();
        rpm += 1500.;
    }
    sys.unload_bank(bank);

    let mut v: Vec<MapEntry> = acc.into_values().collect();
    v.sort_by(|a, b| {
        (a.condition.is_some(), !a.on_load, a.rpm_peak).partial_cmp(&(b.condition.is_some(), !b.on_load, b.rpm_peak)).unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(v)
}

/// Relevé détaillé (diagnostic) : pour chaque régime de `rpms`, les couches audibles
/// après `updates` mises à jour du mixage (≈ 21 ms chacune en mode analyse).
pub fn probe(
    lib: &Arc<FmodLib>,
    data: &EngineData,
    event: &EngineEvent,
    on_load: bool,
    rpms: &[f32],
    updates: u32,
) -> Result<Vec<(f32, Vec<ActiveLayer>)>> {
    probe_with(lib, data, event, &[], on_load, rpms, updates)
}

/// [`probe`] avec d'autres paramètres de l'évènement fixés (`Turbo`, `CameraView`…).
pub fn probe_with(
    lib: &Arc<FmodLib>,
    data: &EngineData,
    event: &EngineEvent,
    params: &[(&str, f32)],
    on_load: bool,
    rpms: &[f32],
    updates: u32,
) -> Result<Vec<(f32, Vec<ActiveLayer>)>> {
    let sys = System::new(lib.clone(), Output::Analysis)?;
    data.load_base(&sys)?;
    let (bytes, sizes) = data.analysis_bank(event.bank)?;
    let bank = sys.load_bank(&bytes)?;
    sys.load_sample_data(bank);
    sys.wait_sample_data(bank, 2000);
    let inst = sys.create_instance(&event.info.path)?;
    for (name, v) in params {
        sys.set_param(&inst, name, *v);
    }
    sys.set_param(&inst, LOAD, if on_load { 1. } else { 0. });
    sys.set_param(&inst, RPM, rpms.first().copied().unwrap_or(0.));
    sys.start(&inst);
    let mut out = Vec::new();
    for &rpm in rpms {
        sys.set_param(&inst, RPM, rpm);
        for _ in 0..updates {
            sys.update();
        }
        out.push((rpm, to_active_in(&sizes, event.bank, sys.layers(&inst))));
    }
    sys.release(inst);
    sys.update();
    sys.unload_bank(bank);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(curve: Vec<(f32, f32)>) -> MapEntry {
        MapEntry { sound: SoundRef { bank: 0, sample: 0 }, on_load: true, rpm_min: curve[0].0, rpm_max: curve.last().unwrap().0, rpm_peak: 2000., peak_volume: 1., pitches: vec![1.; curve.len()], curve, condition: None }
    }

    #[test]
    fn volume_curve_interpolates_and_respects_gaps() {
        let e = entry(vec![(1000., 0.2), (1500., 0.6), (2000., 1.0), (2500., 0.6), (4000., 0.5)]);
        assert_eq!(e.volume_at(500.), 0.);
        assert_eq!(e.volume_at(1000.), 0.2);
        assert!((e.volume_at(1250.) - 0.4).abs() < 1e-6);
        assert_eq!(e.volume_at(3000.), 0., "trou entre 2500 et 4000");
        assert_eq!(e.volume_at(4500.), 0.);
        assert_eq!(e.fades(), (1000., 2000.));
    }
}
