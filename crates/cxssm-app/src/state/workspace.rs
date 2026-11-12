//! **Projet `.cxssm`** : création, ouverture, enregistrement automatique du manifeste,
//! construction du mod dans le dossier du projet, installation dans le jeu et
//! restauration du fichier d'origine (voir [`cxssm_core::workspace`]).

use std::path::PathBuf;

use cxssm_core::workspace::{self as ws, EditEntry, SoundEntry, Workspace};
use cxssm_core::{export, game};
use gpui::{AppContext as _, Context};

use super::editor::{Phase, SoundEdit, Source};
use super::AppState;

impl AppState {
    /// Nom du projet ouvert.
    pub fn project_name(&self) -> Option<&str> {
        self.workspace.as_ref().map(|w| w.manifest.name.as_str())
    }

    /// Le mod du dossier du projet doit être reconstruit.
    pub fn mod_stale(&self) -> bool {
        self.workspace.as_ref().is_some_and(|w| w.manifest.mod_stale || !w.mod_bundle().is_file())
    }

    pub fn set_project_dialog(&mut self, open: bool, cx: &mut Context<Self>) {
        self.project_dialog = open;
        if open {
            self.check_game_state(cx);
        }
        cx.notify();
    }

    // ───────────────────────────── création ─────────────────────────────

    /// Bundle du jeu à associer au projet : celui ouvert s'il est dans le jeu, sinon celui
    /// du projet ouvert, sinon celui choisi dans les préférences, sinon le premier détecté.
    fn game_bundle_for(&self, source: Option<&std::path::Path>) -> Option<PathBuf> {
        source
            .filter(|p| game::catalog_bundle_size(p).is_some())
            .map(PathBuf::from)
            .or_else(|| self.workspace.as_ref().and_then(|w| w.manifest.game_bundle.clone()))
            .or_else(|| self.settings.bundle_path.clone().filter(|p| p.is_file()))
            .or_else(|| game::find_bundles().into_iter().next())
    }

    /// « Nouveau projet » : le bundle d'origine (celui ouvert, sinon celui du jeu) est
    /// copié dans un nouveau dossier, avec les sons déjà remplacés.
    pub fn new_project(&mut self, cx: &mut Context<Self>) {
        let loaded = self.project.as_ref().map(|p| p.source.clone());
        let game = self.game_bundle_for(loaded.as_deref());
        let Some(source) = loaded.clone().or_else(|| game.clone()) else {
            self.set_status(
                "Ouvrez d'abord fmod_assets_all.bundle du jeu : il sera copié dans le projet.".into(),
                "Open the game's fmod_assets_all.bundle first: it will be copied into the project.".into(),
                cx,
            );
            return;
        };
        // Un bundle déjà modifié ne peut pas servir d'original.
        if ws::is_pristine(&source, game.as_deref()) == Some(false) {
            self.set_status(
                format!(
                    "{} est déjà modifié (sa taille diffère de celle du catalogue du jeu). Restaurez l'original avec « Vérifier l'intégrité des fichiers » dans Steam.",
                    source.display()
                ),
                format!(
                    "{} is already modified (its size differs from the game catalog's). Restore the original with “Verify integrity of game files” in Steam.",
                    source.display()
                ),
                cx,
            );
            return;
        }
        let title = self.t("Créer un projet CXSSM", "Create a CXSSM project");
        let default = self.t("Mon mod.cxssm", "My mod.cxssm");
        cx.spawn(async move |this, cx| {
            let Some(file) = rfd::AsyncFileDialog::new().set_title(title).set_file_name(default).add_filter("CXSSM", &["cxssm"]).save_file().await
            else {
                return;
            };
            let chosen = file.path().to_path_buf();
            let _ = this.update(cx, |s, cx| s.create_project(chosen, source, game, loaded.is_some(), cx));
        })
        .detach();
    }

    fn create_project(&mut self, chosen: PathBuf, source: PathBuf, game: Option<PathBuf>, keep_loaded: bool, cx: &mut Context<Self>) {
        self.busy = true;
        let dir = Workspace::dir_for(&chosen);
        self.set_status(
            format!("Création du projet dans {} (copie du bundle d'origine)…", dir.display()),
            format!("Creating the project in {} (copying the original bundle)…", dir.display()),
            cx,
        );
        let takes: Vec<(Phase, PathBuf)> =
            [Phase::Acc, Phase::Dec].into_iter().filter_map(|p| self.editor.take(p).map(|t| (p, t.path.clone()))).collect();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    let w = Workspace::create(&chosen, &source, game)?;
                    let takes = takes.into_iter().map(|(p, path)| Ok((p, w.import_recording(&path)?))).collect::<cxssm_core::Result<Vec<_>>>()?;
                    Ok::<_, cxssm_core::Error>((w, takes))
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.busy = false;
                let (w, takes) = match res {
                    Ok(v) => v,
                    Err(e) => return s.set_status(format!("Création du projet impossible : {e}"), format!("Cannot create the project: {e}"), cx),
                };
                s.remember_project(&w);
                if !keep_loaded {
                    let original = w.original_bundle();
                    return s.open_bundle(original, Some(w), cx);
                }
                // Le bundle ouvert devient le projet : ses sons remplacés y sont copiés.
                for (p, path) in takes {
                    if let Some(t) = s.editor.takes[p as usize].as_mut() {
                        t.path = path;
                    }
                }
                if let Some(p) = s.project.as_mut() {
                    p.source = w.original_bundle();
                }
                let errors = s.adopt_files(&w);
                s.workspace = Some(w);
                s.project_changed(true);
                s.check_game_state(cx);
                let name = s.project_name().unwrap_or_default().to_string();
                match errors {
                    0 => s.set_status(format!("Projet « {name} » créé"), format!("Project “{name}” created"), cx),
                    n => s.set_status(
                        format!("Projet « {name} » créé, mais {n} fichier(s) n'ont pas pu être copiés"),
                        format!("Project “{name}” created, but {n} file(s) could not be copied"),
                        cx,
                    ),
                }
            });
        })
        .detach();
    }

    /// Copie dans le projet les fichiers des sons déjà remplacés. Renvoie le nombre d'échecs.
    fn adopt_files(&mut self, w: &Workspace) -> usize {
        let Some(p) = self.project.as_mut() else { return 0 };
        let mut errors = 0;
        let repl: Vec<_> = p.replacements.iter().map(|(r, f)| (*r, f.clone())).collect();
        for (r, file) in repl {
            let Some((bank, smp)) = p.sample(r).map(|(b, s)| (b.name.clone(), s.name.clone())) else { continue };
            let edit = self.editor.edits.get_mut(&r);
            // Piste rendue par l'éditeur → `edited/` ; fichier choisi tel quel → `imported/`.
            let rendered = edit.as_ref().is_some_and(|e| !matches!(&e.source, Source::File(f) if *f == file));
            let copied = if rendered {
                let dst = w.edited_path(&bank, &smp);
                std::fs::copy(&file, &dst).map(|_| dst).map_err(cxssm_core::Error::from)
            } else {
                w.import(&file, &bank, &smp)
            };
            if let Some(Source::File(f)) = edit.map(|e| &mut e.source) {
                match w.import(f, &bank, &smp) {
                    Ok(c) => *f = c,
                    Err(_) => errors += 1,
                }
            }
            match copied {
                Ok(c) => p.set_replacement(r, c),
                Err(e) => {
                    log::error!("copie de {} : {e}", file.display());
                    errors += 1;
                }
            }
        }
        errors
    }

    // ───────────────────────────── ouverture ─────────────────────────────

    pub fn open_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match Workspace::open(&path) {
            Ok(w) => {
                self.remember_project(&w);
                let original = w.original_bundle();
                self.open_bundle(original, Some(w), cx);
            }
            Err(e) => self.set_status(format!("Projet illisible : {e}"), format!("Unreadable project: {e}"), cx),
        }
    }

    fn remember_project(&mut self, w: &Workspace) {
        if self.settings.last_project.as_ref() != Some(&w.file) {
            self.settings.last_project = Some(w.file.clone());
            self.save_settings();
        }
    }

    /// Après le chargement de l'original : remplacements, réglages et enregistrements du projet.
    pub(super) fn apply_workspace(&mut self, cx: &mut Context<Self>) {
        let (Some(w), Some(p)) = (self.workspace.clone(), self.project.as_mut()) else { return };
        let (found, missing) = w.resolve(p);
        let mut absent = Vec::new();
        for (r, e) in &found {
            let file = w.abs(&e.file);
            if !file.is_file() {
                absent.push(e.file.clone());
                continue;
            }
            p.set_replacement(*r, file);
            if let Some(ed) = &e.edit {
                let source = match &ed.source {
                    ws::EditSource::Original => Source::Original,
                    ws::EditSource::File { path } => Source::File(w.abs(path)),
                    ws::EditSource::Recording { phase, rpm } => Source::Recording { phase: (*phase).into(), rpm: *rpm },
                };
                self.editor.edits.insert(*r, SoundEdit::restored(source, ed.gain_db, ed.semitones));
            }
        }
        for (phase, rel) in [(Phase::Acc, &w.manifest.recordings.acc), (Phase::Dec, &w.manifest.recordings.dec)] {
            if let Some(path) = rel.as_ref().map(|r| w.abs(r)).filter(|p| p.is_file()) {
                self.load_take(phase, path, cx);
            }
        }
        let n = found.len() - absent.len();
        let name = &w.manifest.name;
        let mut fr = format!("Projet « {name} » : {n} son(s) remplacé(s)");
        let mut en = format!("Project “{name}”: {n} replaced sound(s)");
        if !missing.is_empty() {
            let list = missing.iter().map(|e| format!("{}/{}", e.bank, e.sample)).collect::<Vec<_>>().join(", ");
            fr += &format!(" · introuvables dans ce bundle : {list}");
            en += &format!(" · not found in this bundle: {list}");
        }
        if !absent.is_empty() {
            fr += &format!(" · fichiers manquants : {}", absent.join(", "));
            en += &format!(" · missing files: {}", absent.join(", "));
        }
        self.set_status(fr, en, cx);
        self.check_game_state(cx);
    }

    // ───────────────────────────── enregistrement ─────────────────────────────

    /// Le projet a changé : manifeste mis à jour. `sounds_changed` : un son a été
    /// remplacé, rendu ou restauré, le mod est à reconstruire (charger un enregistrement
    /// ne change pas le mod).
    pub(super) fn project_changed(&mut self, sounds_changed: bool) {
        let Some(sounds) = self.manifest_sounds() else { return };
        let recordings = self.manifest_recordings();
        let Some(w) = self.workspace.as_mut() else { return };
        let stale = w.manifest.mod_stale || sounds_changed || w.manifest.sounds != sounds;
        if w.manifest.sounds == sounds && w.manifest.recordings == recordings && w.manifest.mod_stale == stale {
            return;
        }
        w.manifest.sounds = sounds;
        w.manifest.recordings = recordings;
        w.manifest.mod_stale = stale;
        if let Err(e) = w.save() {
            log::error!("enregistrement du projet : {e}");
        }
    }

    fn manifest_sounds(&self) -> Option<Vec<SoundEntry>> {
        let (w, p) = (self.workspace.as_ref()?, self.project.as_ref()?);
        Some(
            p.replacements
                .iter()
                .filter_map(|(r, file)| {
                    let (bank, smp) = p.sample(*r)?;
                    let edit = self.editor.edits.get(r).map(|e| EditEntry {
                        source: match &e.source {
                            Source::Original => ws::EditSource::Original,
                            Source::File(f) => ws::EditSource::File { path: w.rel(f) },
                            Source::Recording { phase, rpm } => ws::EditSource::Recording { phase: (*phase).into(), rpm: *rpm },
                        },
                        gain_db: e.gain_db,
                        semitones: e.semitones,
                    });
                    Some(SoundEntry { bank: bank.name.clone(), sample: smp.name.clone(), file: w.rel(file), edit })
                })
                .collect(),
        )
    }

    fn manifest_recordings(&self) -> ws::Recordings {
        let rel = |p: Phase| Some(self.workspace.as_ref()?.rel(&self.editor.take(p)?.path));
        ws::Recordings { acc: rel(Phase::Acc), dec: rel(Phase::Dec) }
    }

    // ───────────────────────────── mod et jeu ─────────────────────────────

    /// Construit le mod dans `mod/` du projet, puis l'installe dans le jeu si demandé.
    pub fn build_mod(&mut self, install: bool, cx: &mut Context<Self>) {
        let Some(w) = self.workspace.clone() else { return };
        if install && w.manifest.game_bundle.is_none() {
            self.project_dialog = true;
            self.set_status(
                "Indiquez d'abord le fichier du jeu dans la fenêtre Projet.".into(),
                "First set the game file in the Project window.".into(),
                cx,
            );
            return;
        }
        if self.replaced_count() == 0 {
            self.set_status("Aucun son remplacé : rien à construire".into(), "No replaced sound: nothing to build".into(), cx);
            return;
        }
        if !self.mod_stale() {
            if install {
                self.install_mod(cx);
            } else {
                self.set_status(
                    format!("Le mod est à jour : {}", w.mod_bundle().display()),
                    format!("The mod is up to date: {}", w.mod_bundle().display()),
                    cx,
                );
            }
            return;
        }
        let Some(project) = self.project.take() else { return };
        self.busy = true;
        self.set_status("Construction du mod…".into(), "Building the mod…".into(), cx);
        let enc = self.encoder.clone();
        cx.spawn(async move |this, cx| {
            let out = w.folder(ws::Folder::Mod);
            let (project, result) = cx
                .background_spawn(async move {
                    let r = export::export(&project, &enc, &out);
                    (project, r)
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.project = Some(project);
                s.busy = false;
                match result {
                    Ok(path) => {
                        if let Some(w) = s.workspace.as_mut() {
                            w.manifest.mod_stale = false;
                            if let Err(e) = w.save() {
                                log::error!("enregistrement du projet : {e}");
                            }
                        }
                        if install {
                            s.install_mod(cx);
                        } else {
                            s.set_status(format!("Mod construit : {}", path.display()), format!("Mod built: {}", path.display()), cx);
                            s.check_game_state(cx);
                        }
                    }
                    Err(e) => s.set_status(format!("Erreur de construction : {e}"), format!("Build error: {e}"), cx),
                }
            });
        })
        .detach();
    }

    /// « Installer dans le jeu » : construit le mod si besoin, puis remplace le fichier du jeu.
    pub fn install_into_game(&mut self, cx: &mut Context<Self>) {
        self.build_mod(true, cx);
    }

    fn install_mod(&mut self, cx: &mut Context<Self>) {
        self.game_job(
            |w| w.install(),
            |p| (format!("Mod installé dans le jeu : {}", p.display()), format!("Mod installed in the game: {}", p.display())),
            cx,
        );
    }

    /// Remet le fichier d'origine du projet dans le jeu.
    pub fn restore_game(&mut self, cx: &mut Context<Self>) {
        self.game_job(
            |w| w.restore(),
            |p| (format!("Fichier d'origine restauré : {}", p.display()), format!("Original file restored: {}", p.display())),
            cx,
        );
    }

    /// Le jeu a été mis à jour : son nouveau fichier d'origine remplace celui du projet,
    /// puis le projet est rouvert (les sons sont retrouvés par leur nom).
    pub fn update_original(&mut self, cx: &mut Context<Self>) {
        let Some(w) = self.workspace.clone() else { return };
        self.busy = true;
        self.set_status("Copie du nouveau fichier d'origine du jeu…".into(), "Copying the game's new original file…".into(), cx);
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { w.update_original().map(|_| w) }).await;
            let _ = this.update(cx, |s, cx| {
                s.busy = false;
                match res {
                    Ok(mut w) => {
                        w.manifest.mod_stale = true;
                        let _ = w.save();
                        let original = w.original_bundle();
                        s.open_bundle(original, Some(w), cx);
                    }
                    Err(e) => s.set_status(format!("Mise à jour impossible : {e}"), format!("Cannot update: {e}"), cx),
                }
            });
        })
        .detach();
    }

    /// Opération sur le fichier du jeu en arrière-plan, puis nouvelle vérification de son état.
    fn game_job(
        &mut self,
        job: impl FnOnce(&Workspace) -> cxssm_core::Result<PathBuf> + Send + 'static,
        done: impl FnOnce(&std::path::Path) -> (String, String) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(w) = self.workspace.clone() else { return };
        self.busy = true;
        self.game_state = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { job(&w) }).await;
            let _ = this.update(cx, |s, cx| {
                s.busy = false;
                match res {
                    Ok(p) => {
                        let (fr, en) = done(&p);
                        s.set_status(fr, en, cx);
                    }
                    Err(e) => s.set_status(format!("Erreur : {e}"), format!("Error: {e}"), cx),
                }
                s.check_game_state(cx);
            });
        })
        .detach();
    }

    /// Compare le fichier du jeu à l'original et au mod du projet (en arrière-plan).
    pub fn check_game_state(&mut self, cx: &mut Context<Self>) {
        let Some(w) = self.workspace.clone() else { return };
        self.game_state = None;
        cx.notify();
        let file = w.file.clone();
        cx.spawn(async move |this, cx| {
            let state = cx.background_spawn(async move { w.game_state() }).await;
            let _ = this.update(cx, |s, cx| {
                // Résultat d'un projet fermé entre-temps : ignoré.
                if s.workspace.as_ref().is_some_and(|w| w.file == file) {
                    s.game_state = Some(state);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Associe un autre `fmod_assets_all.bundle` du jeu au projet.
    pub fn choose_project_game_bundle(&mut self, cx: &mut Context<Self>) {
        let title = self.t("fmod_assets_all.bundle du jeu", "The game's fmod_assets_all.bundle");
        cx.spawn(async move |this, cx| {
            let Some(f) = rfd::AsyncFileDialog::new().set_title(title).add_filter("AssetBundle Unity", &["bundle"]).pick_file().await else {
                return;
            };
            let _ = this.update(cx, |s, cx| {
                if let Some(w) = s.workspace.as_mut() {
                    w.manifest.game_bundle = Some(f.path().to_path_buf());
                    if let Err(e) = w.save() {
                        log::error!("enregistrement du projet : {e}");
                    }
                }
                s.check_game_state(cx);
            });
        })
        .detach();
    }

    pub fn reveal_project(&self, cx: &mut Context<Self>) {
        if let Some(w) = &self.workspace {
            cx.open_with_system(&w.dir);
        }
    }

    /// Dossier proposé pour exporter un son d'origine.
    pub fn exported_dir(&self) -> Option<PathBuf> {
        self.workspace.as_ref().map(|w| w.folder(ws::Folder::Exported))
    }
}
