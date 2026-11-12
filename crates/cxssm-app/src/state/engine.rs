//! Simulateur moteur : bibliothèque FMOD, voitures (évènements moteur), régime et
//! charge, rupteur, balayage, sons entendus et carte des sons par régime.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use cxssm_core::engine::{self, ActiveLayer, EngineData, EngineEvent, FmodLibHandle, MapEntry, Simulator};
use cxssm_core::{export, SoundRef};
use gpui::{AppContext as _, px, Bounds, Context, Pixels, SharedString};

use super::{AppState, Leaf};

/// Régime de ralenti.
pub const IDLE_RPM: f32 = 900.;
/// Montée du régime avec « maintenir pour accélérer » (tr/min par seconde).
const RPM_RISE: f32 = 3_500.;
/// Montée plus lente du balayage « 0 → limiteur », pour bien entendre chaque son.
const SWEEP_RISE: f32 = 1_200.;
/// Retour au ralenti, et chute à l'arrêt du moteur.
const RPM_FALL: f32 = 2_800.;
/// Mise en route : montée jusqu'au ralenti.
const IDLE_RISE: f32 = 2_400.;
/// Rupteur : durée de la coupure d'injection et chute du régime pendant la coupure
/// (avec `RPM_RISE`, environ 7 coupures par seconde).
const LIMITER_CUT: f32 = 0.045;
const LIMITER_DROP: f32 = 7_000.;
/// Bornes et pas de réglage du rupteur.
pub const LIMITER_MIN: f32 = 2_000.;
pub const LIMITER_STEP: f32 = 250.;
/// Pas du balayage pour la carte des sons.
const MAP_STEP: f32 = 125.;

/// Carte des sons d'une voiture : résultat du balayage, ou erreur.
pub type SoundMap = Result<Vec<MapEntry>, SharedString>;

pub struct EngineState {
    lib: Option<FmodLibHandle>,
    pub error: Option<SharedString>,
    data: Option<EngineData>,
    pub events: Vec<EngineEvent>,
    pub loading_events: bool,
    /// Voiture du simulateur (index dans `events`).
    pub car: Option<usize>,
    sim: Option<Simulator>,
    pub starting: bool,
    pub rpm: f32,
    pub load: f32,
    /// « Maintenir pour accélérer » enfoncé.
    pub throttle: bool,
    /// Balayage automatique 0 → limiteur en cours.
    pub sweeping: bool,
    /// Le régime suit l'accélérateur (sinon il est fixé à la souris sur la jauge).
    auto: bool,
    /// Régime du rupteur (par défaut : le maximum de l'évènement).
    pub limiter: f32,
    cut: f32,
    last_tick: Option<Instant>,
    pub layers: Vec<ActiveLayer>,
    pub maps: HashMap<usize, SoundMap>,
    /// Voiture en cours de cartographie.
    pub mapping: Option<usize>,
    /// Écouter la banque avec les sons de remplacement de l'utilisateur.
    pub use_mod: bool,
    /// Dimensions des jauges (pour les régler à la souris).
    pub rpm_track: Rc<Cell<Bounds<Pixels>>>,
    pub load_track: Rc<Cell<Bounds<Pixels>>>,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            lib: None,
            error: None,
            data: None,
            events: Vec::new(),
            loading_events: false,
            car: None,
            sim: None,
            starting: false,
            rpm: 0.,
            load: 0.,
            throttle: false,
            sweeping: false,
            auto: true,
            limiter: 10_500.,
            cut: 0.,
            last_tick: None,
            layers: Vec::new(),
            maps: HashMap::new(),
            mapping: None,
            use_mod: false,
            rpm_track: Rc::default(),
            load_track: Rc::default(),
        }
    }
}

impl EngineState {
    pub fn is_active(&self) -> bool {
        self.sim.is_some() || self.loading_events || self.starting || self.mapping.is_some() || self.rpm > 0.5
    }

    pub fn running(&self) -> bool {
        self.sim.is_some()
    }

    pub fn has_library(&self) -> bool {
        self.lib.is_some()
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    pub fn reset(&mut self) {
        // La bibliothèque FMOD chargée reste valable pour le prochain bundle.
        let lib = self.lib.take();
        *self = Self::default();
        self.lib = lib;
    }

    pub fn event(&self) -> Option<&EngineEvent> {
        self.events.get(self.car?)
    }

    pub fn rpm_max(&self) -> f32 {
        self.event().map_or(10_500., |e| e.rpm_range().1)
    }

    /// Plafond effectif du régime : rupteur s'il est activé, sinon le maximum.
    pub fn ceiling(&self, limiter_enabled: bool) -> f32 {
        if limiter_enabled {
            self.limiter.min(self.rpm_max())
        } else {
            self.rpm_max()
        }
    }

    /// Le rupteur est en train de couper (ou le régime rebondit dessus).
    pub fn limiting(&self, limiter_enabled: bool) -> bool {
        limiter_enabled && self.sim.is_some() && self.throttle && (self.cut > 0. || self.rpm >= self.limiter - 400.)
    }

    /// Couche audible correspondant à un son, s'il est entendu en ce moment.
    pub fn layer(&self, r: SoundRef) -> Option<&ActiveLayer> {
        self.layers.iter().find(|l| l.sound == Some(r))
    }

    /// Avance la simulation d'une image : accélérateur, paramètres FMOD, sons entendus.
    pub fn tick(&mut self, limiter_enabled: bool) {
        let now = Instant::now();
        let dt = self.last_tick.map_or(0., |t| (now - t).as_secs_f32()).min(0.1);
        self.last_tick = Some(now);
        let Some(sim) = &self.sim else {
            // Moteur coupé : le régime retombe à zéro.
            self.rpm = (self.rpm - RPM_FALL * dt).max(0.);
            self.load = 0.;
            self.layers.clear();
            return;
        };
        if self.auto {
            let ceiling = self.ceiling(limiter_enabled);
            if self.throttle || self.sweeping {
                if self.cut > 0. {
                    // Rupteur : coupure, le régime chute brièvement sans charge.
                    self.cut -= dt;
                    self.rpm = (self.rpm - LIMITER_DROP * dt).max(IDLE_RPM);
                    self.load = 0.;
                } else {
                    self.rpm += if self.throttle { RPM_RISE } else { SWEEP_RISE } * dt;
                    self.load = 1.;
                    if self.rpm >= ceiling {
                        self.rpm = ceiling;
                        if !self.throttle {
                            self.sweeping = false;
                        } else if limiter_enabled {
                            self.cut = LIMITER_CUT;
                        }
                    }
                }
            } else {
                self.cut = 0.;
                self.load = 0.;
                self.rpm = if self.rpm > IDLE_RPM {
                    (self.rpm - RPM_FALL * dt).max(IDLE_RPM)
                } else {
                    (self.rpm + IDLE_RISE * dt).min(IDLE_RPM)
                };
            }
        }
        sim.set_param(engine::RPM, self.rpm);
        sim.set_param(engine::LOAD, self.load);
        sim.update();
        self.layers = sim.layers();
    }

    fn ratio(track: &Rc<Cell<Bounds<Pixels>>>, x: Pixels) -> Option<f32> {
        let b = track.get();
        (b.size.width > px(0.)).then(|| ((x - b.origin.x) / b.size.width).clamp(0., 1.))
    }

    pub fn set_rpm_from_mouse(&mut self, x: Pixels) {
        if let Some(r) = Self::ratio(&self.rpm_track, x) {
            self.auto = false;
            self.sweeping = false;
            self.rpm = r * self.rpm_max();
        }
    }

    pub fn set_load_from_mouse(&mut self, x: Pixels) {
        if let Some(r) = Self::ratio(&self.load_track, x) {
            self.auto = false;
            self.sweeping = false;
            self.load = r;
        }
    }

    pub fn adjust_limiter(&mut self, delta: f32) {
        self.limiter = (self.limiter + delta).clamp(LIMITER_MIN, self.rpm_max());
    }
}

impl AppState {
    /// Charge FMOD (une fois) puis la liste des voitures du bundle ouvert.
    pub fn init_engine(&mut self, cx: &mut Context<Self>) {
        let not_found = self.t(
            "FMOD Engine introuvable : installez-le (fmod.com/download) ou indiquez son dossier dans les préférences.",
            "FMOD Engine not found: install it (fmod.com/download) or set its folder in Preferences.",
        );
        let api = self.fmod.as_ref().map(|f| f.api.clone());
        let e = &mut self.engine;
        if e.lib.is_none() {
            e.error = None;
            match api {
                None => e.error = Some(not_found.into()),
                Some(dir) => match engine::load_library(&dir) {
                    Ok(lib) => e.lib = Some(lib),
                    Err(err) => e.error = Some(format!("FMOD : {err}").into()),
                },
            }
        }
        let (Some(lib), Some(p)) = (e.lib.clone(), self.project.as_ref()) else {
            cx.notify();
            return;
        };
        if e.data.is_some() || e.loading_events {
            return;
        }
        let data = EngineData::from_project(p);
        e.data = Some(data.clone());
        e.loading_events = true;
        let generation = self.generation;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { engine::list_events(&lib, &data) }).await;
            let _ = this.update(cx, |s, cx| {
                if s.generation != generation {
                    return;
                }
                s.engine.loading_events = false;
                match res {
                    Ok(mut list) => {
                        list.sort_by_key(|e| e.name().to_lowercase());
                        s.engine.events = list;
                        // Projet rouvert : voitures utilisant des sons déjà remplacés.
                        s.ensure_map(cx);
                    }
                    Err(err) => s.engine.error = Some(format!("FMOD : {err}").into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Nouvelle voiture : moteur arrêté, rupteur au maximum, carte des sons calculée.
    pub(super) fn select_car(&mut self, i: usize, cx: &mut Context<Self>) {
        let e = &mut self.engine;
        if e.car == Some(i) {
            return;
        }
        e.sim = None;
        e.car = Some(i);
        e.layers.clear();
        e.throttle = false;
        e.sweeping = false;
        e.auto = true;
        e.cut = 0.;
        e.limiter = e.rpm_max();
        self.ensure_map(cx);
    }

    /// Voiture dont un son est remplacé (d'après sa carte : sons partagés compris).
    pub fn car_modified(&self, i: usize) -> bool {
        let (Some(p), Some(Ok(map))) = (self.project.as_ref(), self.engine.maps.get(&i)) else { return false };
        map.iter().any(|m| p.replacements.contains_key(&m.sound))
    }

    /// Banque dont un son est remplacé.
    pub fn bank_modified(&self, bank: usize) -> bool {
        self.project.as_ref().is_some_and(|p| p.replacements.keys().any(|r| r.bank == bank))
    }

    /// Cartographie une voiture à la fois : d'abord celle affichée, puis, en arrière-plan,
    /// celles des banques modifiées (pour savoir quelles voitures utilisent les sons
    /// remplacés, même celles jamais ouvertes).
    pub(super) fn ensure_map(&mut self, cx: &mut Context<Self>) {
        let e = &self.engine;
        if e.mapping.is_some() {
            return;
        }
        let shown = match self.leaf {
            Some(Leaf::Car(i)) if !e.maps.contains_key(&i) => Some(i),
            _ => None,
        };
        let Some(i) = shown.or_else(|| (0..e.events.len()).find(|&i| !e.maps.contains_key(&i) && self.bank_modified(e.events[i].bank)))
        else {
            return;
        };
        let e = &self.engine;
        let (Some(lib), Some(data), Some(event)) = (e.lib.clone(), e.data.clone(), e.events.get(i).cloned()) else {
            return;
        };
        self.engine.mapping = Some(i);
        let generation = self.generation;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { engine::sweep(&lib, &data, &event, MAP_STEP) }).await;
            let _ = this.update(cx, |s, cx| {
                if s.generation != generation {
                    return;
                }
                s.engine.mapping = None;
                if let Err(err) = &res {
                    log::error!("cartographie : {err}");
                }
                s.engine.maps.insert(i, res.map_err(|e| e.to_string().into()));
                // Une autre voiture a pu être choisie pendant l'analyse.
                s.ensure_map(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn toggle_engine(&mut self, cx: &mut Context<Self>) {
        if self.engine.sim.take().is_some() {
            self.engine.layers.clear();
            self.engine.throttle = false;
            self.engine.sweeping = false;
            cx.notify();
        } else {
            self.start_engine(cx);
        }
    }

    fn start_engine(&mut self, cx: &mut Context<Self>) {
        if self.engine.starting || self.engine.sim.is_some() {
            return;
        }
        let (Some(lib), Some(data), Some(event), Some(p)) =
            (self.engine.lib.clone(), self.engine.data.clone(), self.engine.event().cloned(), self.project.as_ref())
        else {
            return;
        };
        // Écoute du mod : la banque est reconstruite avec les sons de remplacement.
        let modded = if self.engine.use_mod {
            let repl = export::bank_replacements(p, event.bank);
            (!repl.is_empty()).then(|| (p.banks[event.bank].clone(), repl, self.encoder.clone()))
        } else {
            None
        };
        // Coupe une éventuelle pré-écoute en cours.
        self.stop(cx);
        self.engine.starting = true;
        if modded.is_some() {
            self.set_status(
                "Encodage de vos sons et démarrage du moteur…".into(),
                "Encoding your sounds and starting the engine…".into(),
                cx,
            );
        } else {
            self.set_status("Démarrage du moteur…".into(), "Starting the engine…".into(), cx);
        }
        let car = self.engine.car;
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    let bytes = match modded {
                        Some((entry, repl, enc)) => Some(export::build_bank_entry(&entry, &repl, &enc)?),
                        None => None,
                    };
                    Simulator::new(&lib, &data, &event, bytes.as_deref())
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.engine.starting = false;
                // Voiture changée pendant le démarrage : on abandonne.
                if s.engine.car != car {
                    return;
                }
                match res {
                    Ok(sim) => {
                        if sim.modded {
                            s.set_status("Moteur démarré avec VOS sons".into(), "Engine started with YOUR sounds".into(), cx);
                        } else {
                            s.set_status("Moteur démarré (sons originaux)".into(), "Engine started (original sounds)".into(), cx);
                        }
                        s.engine.sim = Some(sim);
                        s.engine.last_tick = None;
                    }
                    Err(err) => {
                        log::error!("simulateur : {err}");
                        s.set_status(format!("Simulateur : {err}"), format!("Simulator: {err}"), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// « Maintenir pour accélérer » : démarre le moteur au besoin.
    pub fn press_throttle(&mut self, cx: &mut Context<Self>) {
        self.start_engine(cx);
        let e = &mut self.engine;
        e.throttle = true;
        e.sweeping = false;
        e.auto = true;
        cx.notify();
    }

    /// Balayage automatique du ralenti jusqu'au rupteur.
    pub fn start_sweep(&mut self, cx: &mut Context<Self>) {
        self.start_engine(cx);
        let e = &mut self.engine;
        e.rpm = e.rpm.min(IDLE_RPM);
        e.sweeping = true;
        e.throttle = false;
        e.auto = true;
        cx.notify();
    }

    /// Sons originaux ou sons remplacés : redémarre le moteur avec l'autre banque.
    /// Le simulateur joue le mod : il redémarre pour entendre une modification.
    /// Appelé à chaque changement des sons remplacés : cartographie aussi les voitures
    /// des banques touchées (indicateur « modifiée » dans l'arborescence).
    pub(super) fn refresh_modded_engine(&mut self, cx: &mut Context<Self>) {
        if self.engine.use_mod && self.engine.sim.take().is_some() {
            self.start_engine(cx);
        }
        self.ensure_map(cx);
    }

    pub fn set_use_mod(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.engine.use_mod == on {
            return;
        }
        self.engine.use_mod = on;
        if self.engine.sim.take().is_some() {
            self.start_engine(cx);
        }
        cx.notify();
    }

    pub fn adjust_limiter(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.engine.adjust_limiter(delta);
        cx.notify();
    }

    /// Image suivante du simulateur (appelée par la vue racine à chaque rendu).
    pub fn tick(&mut self) {
        self.tick_keys();
        self.engine.tick(self.settings.rpm_limiter);
    }
}
