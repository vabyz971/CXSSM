//! **Éditeur de pistes (expérimental)** : réglages d'un son remplacé (volume, hauteur)
//! et génération des boucles moteur depuis des enregistrements continus de la voiture
//! (voir [`cxssm_core::generator`]).
//!
//! Chaque modification est rendue en WAV dans un dossier temporaire, puis devient le
//! remplacement du son dans le projet : l'export, la pré-écoute et le simulateur
//! (« Sons remplacés ») l'utilisent comme n'importe quel fichier choisi à la main.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use cxssm_core::engine::MapEntry;
use cxssm_core::generator::{self as gen, LoopSpec, Recording, RpmTrack};
use cxssm_core::{Language, SoundRef};
use gpui::{AppContext as _, Bounds, Context, Pixels};

use super::{AppState, Leaf};

/// Plages des curseurs.
pub const GAIN_RANGE: (f32, f32) = (-12., 12.);
pub const PITCH_RANGE: (f32, f32) = (-6., 6.);

/// Phase moteur d'un enregistrement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Accélération (pied sur l'accélérateur).
    Acc = 0,
    /// Pied levé (décélération).
    Dec = 1,
}

impl Phase {
    pub fn of(on_load: bool) -> Self {
        if on_load { Phase::Acc } else { Phase::Dec }
    }
}

impl From<Phase> for cxssm_core::workspace::Phase {
    fn from(p: Phase) -> Self {
        match p {
            Phase::Acc => Self::Acc,
            Phase::Dec => Self::Dec,
        }
    }
}

impl From<cxssm_core::workspace::Phase> for Phase {
    fn from(p: cxssm_core::workspace::Phase) -> Self {
        match p {
            cxssm_core::workspace::Phase::Acc => Self::Acc,
            cxssm_core::workspace::Phase::Dec => Self::Dec,
        }
    }
}

/// Enregistrement chargé et son suivi du régime.
pub struct Take {
    pub path: PathBuf,
    pub rec: Arc<Recording>,
    pub track: Arc<RpmTrack>,
    /// Régimes couverts (min, max).
    pub range: (f32, f32),
}

/// Origine du son de remplacement.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// Fichier choisi par l'utilisateur.
    File(PathBuf),
    /// Passage d'un enregistrement : phase et régime du passage utilisé.
    Recording { phase: Phase, rpm: f32 },
}

/// Réglages d'un son.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundEdit {
    pub source: Source,
    pub gain_db: f32,
    /// Hauteur en demi-tons.
    pub semitones: f32,
    /// Incrémenté à chaque changement : un rendu périmé est ignoré.
    pub version: u64,
    /// Version actuellement rendue (et utilisée comme remplacement).
    pub rendered: Option<u64>,
    /// Résumé du dernier rendu, ou erreur.
    pub note: Option<Result<String, String>>,
}

impl SoundEdit {
    fn new(source: Source) -> Self {
        Self { source, gain_db: 0., semitones: 0., version: 0, rendered: None, note: None }
    }

    /// Réglages relus d'un projet : la piste rendue existe déjà.
    pub(super) fn restored(source: Source, gain_db: f32, semitones: f32) -> Self {
        Self { gain_db, semitones, rendered: Some(0), ..Self::new(source) }
    }

    pub fn pitch_ratio(&self) -> f32 {
        (self.semitones / 12.).exp2()
    }

    pub fn rendering(&self) -> bool {
        self.rendered != Some(self.version) && !matches!(self.note, Some(Err(_)))
    }
}

/// Curseur en cours de glisser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slider {
    Gain,
    Pitch,
    /// Régime du passage de l'enregistrement.
    Passage,
}

#[derive(Default)]
pub struct Editor {
    /// Enregistrements de la voiture : `[accélération, pied levé]`.
    pub takes: [Option<Take>; 2],
    pub loading: [bool; 2],
    pub edits: HashMap<SoundRef, SoundEdit>,
    /// Pistes des curseurs (mémorisées au rendu pour le glisser) : gain, hauteur, passage.
    pub tracks: [Rc<Cell<Bounds<Pixels>>>; 3],
    counter: u64,
}

impl Editor {
    pub fn take(&self, phase: Phase) -> Option<&Take> {
        self.takes[phase as usize].as_ref()
    }

    pub fn track(&self, slider: Slider) -> &Rc<Cell<Bounds<Pixels>>> {
        &self.tracks[slider as usize]
    }
}

/// Entrée de carte à suivre pour un son : celle où il est le plus fort (l'accélération
/// à égalité).
pub fn main_entry<'a>(entries: impl IntoIterator<Item = &'a MapEntry>, r: SoundRef) -> Option<&'a MapEntry> {
    entries
        .into_iter()
        .filter(|m| m.sound == r)
        .max_by(|a, b| a.peak_volume.total_cmp(&b.peak_volume).then(a.on_load.cmp(&b.on_load)))
}

impl AppState {
    /// Carte de la voiture affichée (si elle est calculée).
    pub fn car_map(&self) -> Option<&Vec<MapEntry>> {
        let Some(Leaf::Car(i)) = self.leaf else { return None };
        self.engine.maps.get(&i).and_then(|m| m.as_ref().ok())
    }

    /// Régime naturel du son dans la voiture affichée.
    pub fn native_rpm(&self, r: SoundRef) -> Option<f32> {
        main_entry(self.car_map()?, r)?.native_rpm()
    }

    /// Régimes de l'enregistrement `phase` utilisables pour ce son : ceux dont la hauteur
    /// peut être ramenée au régime visé (naturel × hauteur réglée) à ±35 % près.
    pub fn passage_range(&self, r: SoundRef, phase: Phase, semitones: f32) -> Option<(f32, f32)> {
        let (lo, hi) = self.editor.take(phase)?.range;
        let Some(target) = self.native_rpm(r).map(|n| n * (semitones / 12.).exp2()) else { return Some((lo, hi)) };
        let (a, b) = ((target / gen::MAX_SHIFT).max(lo), (target * gen::MAX_SHIFT).min(hi));
        Some(if a < b { (a, b) } else { (lo, hi) })
    }

    /// Choisit l'enregistrement d'une phase et en suit le régime en arrière-plan.
    pub fn pick_take(&mut self, phase: Phase, cx: &mut Context<Self>) {
        let title = match phase {
            Phase::Acc => self.t("Enregistrement en accélération (WAV)", "Acceleration recording (WAV)"),
            Phase::Dec => self.t("Enregistrement pied levé (WAV)", "Deceleration recording (WAV)"),
        };
        cx.spawn(async move |this, cx| {
            let Some(file) = rfd::AsyncFileDialog::new().set_title(title).add_filter("WAV", &["wav"]).pick_file().await else {
                return;
            };
            let path = file.path().to_path_buf();
            let _ = this.update(cx, |s, cx| s.load_take(phase, path, cx));
        })
        .detach();
    }

    /// Charge l'enregistrement `path` d'une phase et en suit le régime en arrière-plan.
    pub fn load_take(&mut self, phase: Phase, path: PathBuf, cx: &mut Context<Self>) {
        self.editor.loading[phase as usize] = true;
        cx.notify();
        let ws = self.workspace.clone();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    // Projet : l'enregistrement est copié dans `audio/recordings/`.
                    let p = match &ws {
                        Some(ws) => ws.import_recording(&path).map_err(|e| e.to_string())?,
                        None => path,
                    };
                    let rec = Recording::load(&p).map_err(|e| e.to_string())?;
                    let track = gen::track_rpm(&rec, gen::DEFAULT_ORDER).map_err(|e| e.to_string())?;
                    let range = track.range().ok_or("régime introuvable dans l'enregistrement")?;
                    Ok::<_, String>(Take { path: p, rec: Arc::new(rec), track: Arc::new(track), range })
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.editor.loading[phase as usize] = false;
                match res {
                    Ok(take) => {
                        let (lo, hi) = take.range;
                        s.set_status(
                            format!("Enregistrement chargé : {lo:.0} → {hi:.0} tr/min"),
                            format!("Recording loaded: {lo:.0} → {hi:.0} rpm"),
                            cx,
                        );
                        s.editor.takes[phase as usize] = Some(take);
                        s.project_changed(false);
                    }
                    Err(e) => s.set_status(format!("Enregistrement illisible : {e}"), format!("Unreadable recording: {e}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Source par défaut d'un son tiré d'un enregistrement : sa phase (ou l'autre si
    /// elle manque) et son régime naturel, ramené dans la plage enregistrée.
    fn default_recording_source(&self, r: SoundRef) -> Option<Source> {
        let entry = main_entry(self.car_map()?, r)?;
        let native = entry.native_rpm()?;
        let preferred = Phase::of(entry.on_load);
        let other = if preferred == Phase::Acc { Phase::Dec } else { Phase::Acc };
        let covers = |p: Phase| self.editor.take(p).map(|t| native / native.clamp(t.range.0, t.range.1));
        // Phase de même nature si elle couvre ce régime à ±35 % près, sinon l'autre.
        let phase = match (covers(preferred), covers(other)) {
            (Some(k), _) if k <= gen::MAX_SHIFT => preferred,
            (_, Some(k)) if k <= gen::MAX_SHIFT => other,
            (Some(_), _) => preferred,
            (None, Some(_)) => other,
            (None, None) => return None,
        };
        let (lo, hi) = self.passage_range(r, phase, 0.)?;
        Some(Source::Recording { phase, rpm: native.clamp(lo, hi) })
    }

    /// Le son peut être tiré d'un enregistrement (voiture cartographiée, enregistrement chargé).
    pub fn can_use_recording(&self, r: SoundRef) -> bool {
        self.default_recording_source(r).is_some()
    }

    /// Change la source du son (fichier actuel ou enregistrement) et le rend.
    pub fn set_edit_source(&mut self, r: SoundRef, from_recording: bool, cx: &mut Context<Self>) {
        let source = if from_recording {
            match self.default_recording_source(r) {
                Some(s) => s,
                None => return,
            }
        } else {
            match self.editor.edits.get(&r).map(|e| &e.source) {
                Some(Source::File(_)) => return,
                _ => {
                    // Revenir au fichier : il faut en choisir un.
                    self.editor.edits.remove(&r);
                    if let Some(p) = self.project.as_mut() {
                        p.clear_replacement(r);
                    }
                    self.refresh_modded_engine(cx);
                    self.project_changed(true);
                    self.pick_replacement(r, cx);
                    return;
                }
            }
        };
        let edit = self.editor.edits.entry(r).or_insert_with(|| SoundEdit::new(source.clone()));
        edit.source = source;
        self.render_edit(r, cx);
    }

    /// Choisit la phase de l'enregistrement utilisé pour ce son.
    pub fn set_edit_phase(&mut self, r: SoundRef, phase: Phase, cx: &mut Context<Self>) {
        let semis = self.editor.edits.get(&r).map_or(0., |e| e.semitones);
        let Some((lo, hi)) = self.passage_range(r, phase, semis) else { return };
        let native = self.native_rpm(r);
        let Some(edit) = self.editor.edits.get_mut(&r) else { return };
        let Source::Recording { phase: p, rpm } = &mut edit.source else { return };
        if *p == phase {
            return;
        }
        *p = phase;
        *rpm = native.unwrap_or(*rpm).clamp(lo, hi);
        self.render_edit(r, cx);
    }

    /// Le fichier choisi à la main devient la source de l'édition (réglages conservés).
    pub(super) fn on_file_replacement(&mut self, r: SoundRef, path: PathBuf, cx: &mut Context<Self>) {
        // Projet : le fichier est copié dans `audio/imported/`.
        let path = match (&self.workspace, self.project.as_ref().and_then(|p| p.sample(r))) {
            (Some(ws), Some((bank, smp))) => match ws.import(&path, &bank.name, &smp.name) {
                Ok(p) => p,
                Err(e) => {
                    self.set_status(format!("Copie dans le projet impossible : {e}"), format!("Cannot copy into the project: {e}"), cx);
                    return;
                }
            },
            _ => path,
        };
        match self.editor.edits.get_mut(&r) {
            Some(edit) => {
                edit.source = Source::File(path);
                self.render_edit(r, cx);
            }
            None => {
                if let Some(p) = self.project.as_mut() {
                    p.set_replacement(r, path);
                }
                self.refresh_modded_engine(cx);
                self.project_changed(true);
            }
        }
    }

    pub(super) fn forget_edit(&mut self, r: SoundRef) {
        self.editor.edits.remove(&r);
    }

    /// Curseur sous la souris : met à jour la valeur (le rendu se fait au relâchement).
    pub fn slide(&mut self, slider: Slider, x: Pixels) {
        let Some(r) = self.sound else { return };
        let b = self.editor.track(slider).get();
        if b.size.width <= gpui::px(0.) {
            return;
        }
        let t = ((x - b.origin.x) / b.size.width).clamp(0., 1.);
        let passage = match self.editor.edits.get(&r) {
            Some(e @ SoundEdit { source: Source::Recording { phase, .. }, .. }) => self.passage_range(r, *phase, e.semitones),
            _ => None,
        };
        let Some(edit) = self.editor.edits.get_mut(&r) else { return };
        let lerp = |(lo, hi): (f32, f32)| lo + (hi - lo) * t;
        match slider {
            Slider::Gain => edit.gain_db = (lerp(GAIN_RANGE) * 2.).round() / 2.,
            Slider::Pitch => edit.semitones = (lerp(PITCH_RANGE) * 10.).round() / 10.,
            Slider::Passage => {
                if let (Some(range), Source::Recording { rpm, .. }) = (passage, &mut edit.source) {
                    *rpm = (lerp(range) / 10.).round() * 10.;
                }
            }
        }
    }

    /// Crée l'édition d'un son remplacé par un fichier (pour régler volume et hauteur).
    pub fn ensure_edit(&mut self, r: SoundRef) {
        if self.editor.edits.contains_key(&r) {
            return;
        }
        if let Some(path) = self.project.as_ref().and_then(|p| p.replacements.get(&r).cloned()) {
            let mut e = SoundEdit::new(Source::File(path));
            e.rendered = Some(0);
            self.editor.edits.insert(r, e);
        }
    }

    /// Remet un réglage à zéro.
    pub fn reset_slider(&mut self, r: SoundRef, slider: Slider, cx: &mut Context<Self>) {
        let passage = match self.editor.edits.get(&r) {
            Some(e @ SoundEdit { source: Source::Recording { phase, .. }, .. }) => {
                self.passage_range(r, *phase, e.semitones).zip(self.native_rpm(r)).map(|((lo, hi), n)| n.clamp(lo, hi))
            }
            _ => None,
        };
        let Some(edit) = self.editor.edits.get_mut(&r) else { return };
        match slider {
            Slider::Gain => edit.gain_db = 0.,
            Slider::Pitch => edit.semitones = 0.,
            Slider::Passage => {
                if let (Some(v), Source::Recording { rpm, .. }) = (passage, &mut edit.source) {
                    *rpm = v;
                }
            }
        }
        self.render_edit(r, cx);
    }

    /// Rend l'édition du son en WAV (en arrière-plan) et en fait son remplacement.
    pub fn render_edit(&mut self, r: SoundRef, cx: &mut Context<Self>) {
        let native = self.native_rpm(r);
        // Le passage reste dans la plage utilisable (elle dépend de la hauteur réglée).
        let clamp = match self.editor.edits.get(&r) {
            Some(e @ SoundEdit { source: Source::Recording { phase, .. }, .. }) => self.passage_range(r, *phase, e.semitones),
            _ => None,
        };
        if let (Some((lo, hi)), Some(SoundEdit { source: Source::Recording { rpm, .. }, .. })) = (clamp, self.editor.edits.get_mut(&r)) {
            *rpm = rpm.clamp(lo, hi);
        }
        let Some(p) = self.project.as_ref() else { return };
        let Some((bank, sample)) = p.sample(r) else { return };
        let (codec, sample, bank_name) = (bank.fsb.codec, sample.clone(), bank.name.clone());
        let Some(edit) = self.editor.edits.get_mut(&r) else { return };
        edit.version += 1;
        edit.note = None;
        let (version, source, gain_db, pitch) = (edit.version, edit.source.clone(), edit.gain_db, edit.pitch_ratio());

        // Fichier sans réglage : utilisé tel quel.
        if let Source::File(path) = &source {
            if gain_db == 0. && pitch == 1. {
                edit.rendered = Some(version);
                let path = path.clone();
                if let Some(p) = self.project.as_mut() {
                    p.set_replacement(r, path);
                }
                self.refresh_modded_engine(cx);
                self.project_changed(true);
                cx.notify();
                return;
            }
        }
        let take = match &source {
            Source::Recording { phase, .. } => match self.editor.take(*phase) {
                Some(t) => Some((t.rec.clone(), t.track.clone())),
                None => return,
            },
            Source::File(_) => None,
        };
        self.editor.counter += 1;
        // Rendu dans un fichier à part, puis renommé en piste du son s'il est toujours à jour.
        // Projet : `audio/edited/<banque>_<son>.wav` ; sinon un dossier temporaire.
        let final_path = match &self.workspace {
            Some(ws) => Some(ws.edited_path(&bank_name, &sample.name)),
            None => None,
        };
        let out = match &final_path {
            Some(f) => f.with_extension(format!("part{}.wav", self.editor.counter)),
            None => std::env::temp_dir()
                .join(format!("cxssm-edits-{}", std::process::id()))
                .join(format!("{bank_name}_{}_{}.wav", sample.name, self.editor.counter)),
        };
        let dir = out.parent().map(PathBuf::from).unwrap_or_default();
        let lang = self.lang();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    let (g, note) = match (&source, take) {
                        (Source::Recording { rpm, .. }, Some((rec, track))) => {
                            let native = native.ok_or("régime naturel inconnu (carte de la voiture absente)")?;
                            let rms = gen::sample_rms(codec, &sample).map_err(|e| e.to_string())?;
                            let spec = LoopSpec {
                                rpm: native * pitch,
                                rate: sample.frequency,
                                channels: sample.channels,
                                seconds: (sample.duration_secs() as f32).clamp(1., 2.5),
                                rms: rms * 10f32.powf(gain_db / 20.),
                                source_rpm: Some(*rpm),
                            };
                            let g = gen::make_loop(&rec, &track, spec).map_err(|e| e.to_string())?;
                            let secs = g.pcm.len() as f32 / f32::from(g.channels.max(1)) / g.rate as f32;
                            let note = match lang {
                                Language::Fr => format!(
                                    "Passage à {:.0} tr/min ({:.1} s) · hauteur ×{:.2} · boucle de {secs:.2} s",
                                    g.source_rpm, g.source_time, g.shift
                                ),
                                Language::En => format!(
                                    "Passage at {:.0} rpm ({:.1} s) · pitch ×{:.2} · {secs:.2} s loop",
                                    g.source_rpm, g.source_time, g.shift
                                ),
                            };
                            (g, note)
                        }
                        (Source::File(path), _) => {
                            let g = gen::process_file(path, &sample, gain_db, pitch).map_err(|e| e.to_string())?;
                            (g, String::new())
                        }
                        _ => return Err("enregistrement absent".to_string()),
                    };
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    gen::write_wav(&out, &g).map_err(|e| e.to_string())?;
                    Ok::<_, String>((out, note))
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                let current = s.editor.edits.get(&r).is_some_and(|e| e.version == version);
                if !current {
                    // Rendu périmé : un plus récent est en cours.
                    if let (Ok((path, _)), Some(_)) = (&res, &final_path) {
                        let _ = std::fs::remove_file(path);
                    }
                    return;
                }
                // Piste définitive du son (projet).
                let res = res.and_then(|(path, note)| match &final_path {
                    Some(f) => std::fs::rename(&path, f).map(|_| (f.clone(), note)).map_err(|e| e.to_string()),
                    None => Ok((path, note)),
                });
                let Some(edit) = s.editor.edits.get_mut(&r) else { return };
                match res {
                    Ok((path, note)) => {
                        edit.rendered = Some(version);
                        edit.note = (!note.is_empty()).then_some(Ok(note));
                        if let Some(p) = s.project.as_mut() {
                            p.set_replacement(r, path);
                        }
                        s.refresh_modded_engine(cx);
                        s.project_changed(true);
                    }
                    Err(e) => edit.note = Some(Err(e)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Génère toutes les pistes de la voiture affichée depuis ses enregistrements.
    pub fn generate_car(&mut self, cx: &mut Context<Self>) {
        let Some(map) = self.car_map() else { return };
        let mut sounds: Vec<SoundRef> = map.iter().map(|m| m.sound).collect();
        sounds.dedup();
        let mut done = 0;
        for r in sounds {
            if self.editor.edits.get(&r).is_some_and(|e| matches!(e.source, Source::Recording { .. })) {
                continue;
            }
            if let Some(source) = self.default_recording_source(r) {
                self.editor.edits.insert(r, SoundEdit::new(source));
                self.render_edit(r, cx);
                done += 1;
            }
        }
        self.set_status(
            format!("Génération de {done} piste(s) depuis les enregistrements (détail dans l'onglet Édition de chaque son)"),
            format!("Generating {done} track(s) from the recordings (details in each sound's Edit tab)"),
            cx,
        );
    }

    /// Un rendu est en cours.
    pub fn editor_busy(&self) -> bool {
        self.editor.loading.iter().any(|&l| l) || self.editor.edits.values().any(SoundEdit::rendering)
    }
}
