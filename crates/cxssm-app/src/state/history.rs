//! **Annuler / rétablir** : l'historique garde des instantanés des sons remplacés et des
//! réglages de l'éditeur, pris juste avant chaque action de l'utilisateur.
//!
//! Revenir à un instantané remet les remplacements tels quels et rend de nouveau les
//! sons dont les réglages changent (dans un projet, une piste éditée est réécrite au
//! même endroit : son ancien contenu n'existe plus).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use cxssm_core::SoundRef;
use gpui::Context;

use super::editor::{SoundEdit, Source};
use super::AppState;

/// Nombre d'actions gardées.
const LIMIT: usize = 100;

/// Réglages d'un son, sans l'état du rendu.
#[derive(Debug, Clone, PartialEq)]
struct EditState {
    source: Source,
    gain_db: f32,
    semitones: f32,
}

impl EditState {
    /// Le son doit être rendu (pas seulement un fichier ou l'original tels quels).
    fn needs_render(&self) -> bool {
        !(self.gain_db == 0. && self.semitones == 0. && matches!(self.source, Source::Original | Source::File(_)))
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Snapshot {
    replacements: BTreeMap<SoundRef, PathBuf>,
    edits: BTreeMap<SoundRef, EditState>,
}

/// Une action annulable : l'état d'avant et son nom (français, anglais).
#[derive(Debug, Clone)]
struct Step {
    before: Snapshot,
    label: (String, String),
}

#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
}

impl History {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Nom de la prochaine action à annuler / rétablir (français, anglais).
    pub fn undo_label(&self) -> Option<&(String, String)> {
        self.undo.last().map(|s| &s.label)
    }

    pub fn redo_label(&self) -> Option<&(String, String)> {
        self.redo.last().map(|s| &s.label)
    }
}

impl AppState {
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            replacements: self.project.as_ref().map(|p| p.replacements.clone()).unwrap_or_default(),
            edits: self
                .editor
                .edits
                .iter()
                .map(|(r, e)| (*r, EditState { source: e.source.clone(), gain_db: e.gain_db, semitones: e.semitones }))
                // L'original sans réglage équivaut à aucune édition.
                .filter(|(_, e)| !(e.source == Source::Original && !e.needs_render()))
                .collect(),
        }
    }

    /// À appeler juste avant une action de l'utilisateur qui change les sons.
    pub fn checkpoint(&mut self, fr: impl Into<String>, en: impl Into<String>) {
        if self.project.is_none() {
            return;
        }
        let before = self.snapshot();
        let h = &mut self.history;
        h.redo.clear();
        h.undo.push(Step { before, label: (fr.into(), en.into()) });
        if h.undo.len() > LIMIT {
            h.undo.remove(0);
        }
    }

    /// Nom d'une action portant sur `n` sons.
    pub fn action_label(&self, fr: &str, en: &str, n: usize) -> (String, String) {
        if n > 1 {
            (format!("{fr} ({n} sons)"), format!("{en} ({n} sounds)"))
        } else {
            (fr.to_string(), en.to_string())
        }
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) {
        self.travel(true, cx);
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) {
        self.travel(false, cx);
    }

    fn travel(&mut self, back: bool, cx: &mut Context<Self>) {
        let current = self.snapshot();
        // Les actions sans effet (clic sur un curseur sans le bouger…) sont sautées.
        let step = loop {
            let stack = if back { &mut self.history.undo } else { &mut self.history.redo };
            match stack.pop() {
                Some(s) if s.before == current => continue,
                Some(s) => break s,
                None => {
                    cx.notify();
                    return;
                }
            }
        };
        let label = step.label.clone();
        let other = Step { before: current, label: label.clone() };
        if back {
            self.history.redo.push(other);
        } else {
            self.history.undo.push(other);
        }
        self.restore_snapshot(step.before, cx);
        let (fr, en) = label;
        if back {
            self.set_status(format!("Annulé : {fr}"), format!("Undone: {en}"), cx);
        } else {
            self.set_status(format!("Rétabli : {fr}"), format!("Redone: {en}"), cx);
        }
    }

    fn restore_snapshot(&mut self, target: Snapshot, cx: &mut Context<Self>) {
        let current = self.snapshot();
        let Some(p) = self.project.as_mut() else { return };

        let keys: BTreeSet<SoundRef> = current.replacements.keys().chain(target.replacements.keys()).copied().collect();
        for r in keys {
            match target.replacements.get(&r) {
                Some(f) if current.replacements.get(&r) != Some(f) => p.set_replacement(r, f.clone()),
                Some(_) => {}
                None => p.clear_replacement(r),
            }
        }

        let mut to_render = Vec::new();
        let keys: BTreeSet<SoundRef> = current.edits.keys().chain(target.edits.keys()).copied().collect();
        for r in keys {
            match target.edits.get(&r) {
                None => {
                    self.editor.edits.remove(&r);
                }
                Some(e) if current.edits.get(&r) != Some(e) => {
                    self.editor.edits.insert(r, SoundEdit::restored(e.source.clone(), e.gain_db, e.semitones));
                    if e.needs_render() {
                        to_render.push(r);
                    }
                }
                Some(_) => {}
            }
        }
        // Les éditions en cours de rendu sont remplacées : leurs résultats seront ignorés.
        for r in to_render {
            self.render_edit(r, cx);
        }
        self.refresh_modded_engine(cx);
        self.project_changed(true);
        cx.notify();
    }
}
