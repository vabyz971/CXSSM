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
    /// Longueur PCM de chaque son du FSB (identification des sons joués).
    lengths: Vec<u64>,
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
                lengths: b.fsb.samples.iter().map(|s| s.num_samples).collect(),
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

    /// Retrouve un son de la banque d'après sa longueur PCM (les noms sont supprimés
    /// dans les FSB du jeu). `None` si la longueur est ambiguë.
    pub fn identify(&self, bank: usize, length_pcm: u32) -> Option<SoundRef> {
        let b = self.bank(bank).ok()?;
        let mut hits = b.lengths.iter().enumerate().filter(|(_, &l)| l == u64::from(length_pcm));
        let (i, _) = hits.next()?;
        hits.next().is_none().then_some(SoundRef { bank, sample: i })
    }

    fn to_active(&self, bank: usize, layers: Vec<cxssm_fmod_studio::Layer>) -> Vec<ActiveLayer> {
        let mut v: Vec<ActiveLayer> = layers
            .into_iter()
            .filter(|l| l.volume > AUDIBLE)
            .map(|l| ActiveLayer { sound: self.identify(bank, l.length_pcm), volume: l.volume, pitch: l.pitch })
            .collect();
        v.sort_by(|a, b| b.volume.total_cmp(&a.volume));
        v
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
    data: EngineData,
    instance: Option<Instance>,
    bank_index: usize,
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
        let bytes = match modded_bank {
            Some(b) => b,
            None => &data.bank(event.bank)?.bytes,
        };
        let bank = system.load_bank(bytes)?;
        system.load_sample_data(bank);
        let instance = system.create_instance(&event.info.path)?;
        let (min, _) = event.rpm_range();
        system.set_param(&instance, RPM, min.max(800.));
        system.set_param(&instance, LOAD, 0.);
        system.start(&instance);
        system.update();
        Ok(Self { system, data: data.clone(), instance: Some(instance), bank_index: event.bank, banks: vec![bank], modded })
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
            .map(|i| self.data.to_active(self.bank_index, self.system.layers(i)))
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

/// Balaye tout le régime (pied levé puis accélération) sans sortie audio et
/// renvoie, pour chaque son entendu, sa plage de régime et son volume maximal.
pub fn sweep(lib: &Arc<FmodLib>, data: &EngineData, event: &EngineEvent, step: f32) -> Result<Vec<MapEntry>> {
    let sys = System::new(lib.clone(), Output::Analysis)?;
    data.load_base(&sys)?;
    let bank = sys.load_bank(&data.bank(event.bank)?.bytes)?;
    sys.load_sample_data(bank);
    for _ in 0..4 {
        sys.update();
    }
    let (min, max) = event.rpm_range();
    let mut acc: BTreeMap<(SoundRef, bool), MapEntry> = BTreeMap::new();

    for on_load in [true, false] {
        let inst = sys.create_instance(&event.info.path)?;
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
            for l in data.to_active(event.bank, sys.layers(&inst)) {
                let Some(sound) = l.sound else { continue };
                let e = acc.entry((sound, on_load)).or_insert(MapEntry {
                    sound,
                    on_load,
                    rpm_min: rpm,
                    rpm_max: rpm,
                    rpm_peak: rpm,
                    peak_volume: 0.,
                    curve: Vec::new(),
                });
                e.curve.push((rpm, l.volume));
                e.rpm_min = e.rpm_min.min(rpm);
                e.rpm_max = e.rpm_max.max(rpm);
                if l.volume > e.peak_volume {
                    e.peak_volume = l.volume;
                    e.rpm_peak = rpm;
                }
            }
            rpm += step;
        }
        sys.release(inst);
        sys.update();
    }
    sys.unload_bank(bank);

    let mut v: Vec<MapEntry> = acc.into_values().collect();
    v.sort_by(|a, b| (!a.on_load, a.rpm_peak).partial_cmp(&(!b.on_load, b.rpm_peak)).unwrap_or(std::cmp::Ordering::Equal));
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(curve: Vec<(f32, f32)>) -> MapEntry {
        MapEntry { sound: SoundRef { bank: 0, sample: 0 }, on_load: true, rpm_min: curve[0].0, rpm_max: curve.last().unwrap().0, rpm_peak: 2000., peak_volume: 1., curve }
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
