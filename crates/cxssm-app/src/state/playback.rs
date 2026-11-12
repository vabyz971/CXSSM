//! Écoute des sons (originaux et remplacements), export d'un son original et
//! calcul des spectres de fréquences affichés dans la liste.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use cxssm_core::{preview, SoundRef};
use gpui::{AppContext as _, Context};

use super::AppState;
use crate::player::Player;

/// Nombre de crêtes calculées par son.
pub const SPECTRUM_BINS: usize = 96;

/// Ce qui est en cours de lecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playing {
    Original(SoundRef),
    Replacement(SoundRef),
}

/// Spectre de fréquences d'un son (FFT moyenne, échelle logarithmique).
#[derive(Clone)]
pub enum Spectrum {
    Pending,
    Ready(Arc<Vec<f32>>),
    Failed,
}

pub struct Playback {
    player: Option<Player>,
    pub playing: Option<Playing>,
    /// Son en cours de décodage pour l'écoute.
    pub decoding: Option<SoundRef>,
    pub spectra: HashMap<SoundRef, Spectrum>,
    queue: VecDeque<SoundRef>,
    worker: bool,
}

impl Playback {
    pub fn new() -> Self {
        Self {
            player: Player::new().map_err(|e| log::warn!("pas de sortie audio : {e}")).ok(),
            playing: None,
            decoding: None,
            spectra: HashMap::new(),
            queue: VecDeque::new(),
            worker: false,
        }
    }

    pub fn is_active(&self) -> bool {
        self.playing.is_some() || self.decoding.is_some() || self.worker
    }

    pub fn reset_spectra(&mut self) {
        self.spectra.clear();
        self.queue.clear();
    }

    pub fn spectrum(&self, r: SoundRef) -> Option<Arc<Vec<f32>>> {
        match self.spectra.get(&r) {
            Some(Spectrum::Ready(p)) => Some(p.clone()),
            _ => None,
        }
    }
}

impl AppState {
    /// ▶ / ■ sur un son d'origine du jeu.
    pub fn toggle_original(&mut self, r: SoundRef, cx: &mut Context<Self>) {
        if !self.is_selected(r) {
            self.sound = Some(r);
            self.selection = vec![r];
        }
        if self.playback.playing == Some(Playing::Original(r)) {
            self.stop(cx);
            return;
        }
        let Some((codec, sample)) = self.project.as_ref().and_then(|p| {
            let (bank, s) = p.sample(r)?;
            Some((bank.fsb.codec, s.clone()))
        }) else {
            return;
        };
        self.playback.decoding = Some(r);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let decoded = cx.background_spawn(async move { preview::decode(codec, &sample) }).await;
            let _ = this.update(cx, |s, cx| {
                s.playback.decoding = None;
                let res = decoded.map_err(|e| e.to_string()).and_then(|d| s.player_mut()?.play_bytes(d.bytes));
                s.after_play(res, Playing::Original(r), cx);
            });
        })
        .detach();
    }

    pub fn toggle_replacement(&mut self, r: SoundRef, cx: &mut Context<Self>) {
        if self.playback.playing == Some(Playing::Replacement(r)) {
            self.stop(cx);
            return;
        }
        let Some(path) = self.project.as_ref().and_then(|p| p.replacements.get(&r).cloned()) else { return };
        // Les sons de l'éditeur sont des boucles : on les écoute en boucle.
        let looped = self.editor.edits.contains_key(&r);
        let res = self.player_mut().and_then(|p| if looped { p.play_file_looped(&path) } else { p.play_file(&path) });
        self.after_play(res, Playing::Replacement(r), cx);
    }

    fn player_mut(&mut self) -> Result<&mut Player, String> {
        self.playback.player.as_mut().ok_or_else(|| "aucune sortie audio disponible".to_string())
    }

    fn after_play(&mut self, res: Result<(), String>, what: Playing, cx: &mut Context<Self>) {
        match res {
            Ok(()) => {
                self.playback.playing = Some(what);
                self.watch_playback(cx);
            }
            Err(e) => self.status = self.tf(format!("Lecture impossible : {e}"), format!("Cannot play: {e}")).into(),
        }
        cx.notify();
    }

    /// Remet le bouton ▶ quand le son est terminé.
    fn watch_playback(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(200)).await;
            let done = this
                .update(cx, |s, cx| {
                    let finished = !s.playback.player.as_ref().is_some_and(Player::is_playing);
                    if finished && s.playback.playing.is_some() {
                        s.playback.playing = None;
                        cx.notify();
                    }
                    finished || s.playback.playing.is_none()
                })
                .unwrap_or(true);
            if done {
                break;
            }
        })
        .detach();
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.playback.player.as_mut() {
            p.stop();
        }
        self.playback.playing = None;
        cx.notify();
    }

    /// Enregistre le son d'origine en WAV (pour le retravailler dans un éditeur audio).
    pub fn save_original(&mut self, r: SoundRef, cx: &mut Context<Self>) {
        let Some((bank, sample)) = self.project.as_ref().and_then(|p| p.sample(r)) else { return };
        let default_name = format!("{}_{}", bank.name, sample.name);
        let (codec, sample) = (bank.fsb.codec, sample.clone());
        let title = self.t("Enregistrer le son d'origine", "Save the original sound");
        // Projet : proposé dans `audio/exported/`.
        let dir = self.exported_dir();
        cx.spawn(async move |this, cx| {
            let decoded = match cx.background_spawn(async move { preview::decode(codec, &sample) }).await {
                Ok(d) => d,
                Err(e) => {
                    let _ = this.update(cx, |s, cx| s.set_status(format!("Décodage impossible : {e}"), format!("Cannot decode: {e}"), cx));
                    return;
                }
            };
            let mut dialog = rfd::AsyncFileDialog::new().set_title(title).set_file_name(format!("{default_name}.{}", decoded.extension));
            if let Some(d) = dir {
                dialog = dialog.set_directory(d);
            }
            let Some(file) = dialog.save_file().await else {
                return;
            };
            let res = std::fs::write(file.path(), &decoded.bytes);
            let _ = this.update(cx, |s, cx| match res {
                Ok(()) => s.set_status(format!("Enregistré : {}", file.path().display()), format!("Saved: {}", file.path().display()), cx),
                Err(e) => s.set_status(format!("Erreur : {e}"), format!("Error: {e}"), cx),
            });
        })
        .detach();
    }

    /// Demande le calcul des spectres manquants (une tâche de fond à la fois).
    pub fn ensure_spectra(&mut self, refs: impl IntoIterator<Item = SoundRef>, cx: &mut Context<Self>) {
        for r in refs {
            if let std::collections::hash_map::Entry::Vacant(e) = self.playback.spectra.entry(r) {
                e.insert(Spectrum::Pending);
                self.playback.queue.push_back(r);
            }
        }
        if self.playback.worker || self.playback.queue.is_empty() {
            return;
        }
        self.playback.worker = true;
        let generation = self.generation;
        cx.spawn(async move |this, cx| loop {
            // Prochain son de la file (copié pour le décodage en arrière-plan).
            let next = this
                .update(cx, |s, _| {
                    if s.generation != generation {
                        return None;
                    }
                    while let Some(r) = s.playback.queue.pop_front() {
                        if let Some((bank, sample)) = s.project.as_ref().and_then(|p| p.sample(r)) {
                            return Some((r, bank.fsb.codec, sample.clone()));
                        }
                    }
                    None
                })
                .ok()
                .flatten();
            let Some((r, codec, sample)) = next else {
                let _ = this.update(cx, |s, cx| {
                    s.playback.worker = false;
                    // Des demandes ont pu arriver pour un nouveau bundle entre-temps.
                    if !s.playback.queue.is_empty() {
                        s.ensure_spectra([], cx);
                    }
                });
                break;
            };
            let res = cx.background_spawn(async move { preview::spectrum(codec, &sample, SPECTRUM_BINS) }).await;
            let _ = this.update(cx, |s, cx| {
                if s.generation == generation {
                    s.playback.spectra.insert(r, res.map_or(Spectrum::Failed, |p| Spectrum::Ready(Arc::new(p))));
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
