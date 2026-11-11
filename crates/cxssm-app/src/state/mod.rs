//! État partagé de l'application : un **modèle GPUI** (`Entity<AppState>`) lu par tous
//! les composants de `ui/`. Il contient le bundle ouvert, la navigation (voiture et son
//! sélectionnés, panneau latéral), le simulateur (régime…), l'écoute et les préférences.
//!
//! Les composants ne gardent aucun état métier : ils lisent `AppState` pour s'afficher
//! et appellent ses méthodes (via `Entity::update`) quand l'utilisateur agit.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use cxssm_core::workspace::{GameState, Workspace};
use cxssm_core::{encoder::EncoderConfig, export, game, FmodInstall, Language, ModProject, Settings, SoundCategory, SoundRef};
use gpui::{AppContext as _, point, px, Context, FocusHandle, ScrollHandle, SharedString, UniformListScrollHandle};

pub mod editor;
pub mod engine;
pub mod playback;
pub mod updates;
pub mod workspace;

pub use editor::{Editor, Phase, Slider, Source as EditSource};
pub use engine::EngineState;
pub use playback::{Playback, Playing};
pub use updates::UpdateState;

use crate::i18n::tr;

/// Catégories, dans l'ordre de l'arborescence.
pub const CATEGORIES: [SoundCategory; 7] = [
    SoundCategory::Engine,
    SoundCategory::Turbo,
    SoundCategory::Exhaust,
    SoundCategory::Voice,
    SoundCategory::Music,
    SoundCategory::Ui,
    SoundCategory::Other,
];

/// Nœud dépliable de l'arborescence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreeKey {
    Category(SoundCategory),
    /// Banque d'une catégorie (pour le moteur : architecture V8, V6…).
    Group(SoundCategory, usize),
}

/// Élément sélectionnable de l'arborescence : ce que montre la zone centrale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leaf {
    /// Voiture (évènement moteur FMOD, index dans `engine.events`).
    Car(usize),
    /// Sons d'une banque dans une catégorie.
    Bank(SoundCategory, usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SideTab {
    #[default]
    Info,
    Edit,
    Io,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrefSection {
    #[default]
    General,
    Game,
    Fmod,
    Updates,
}

/// Glisser en cours. La vue racine reçoit tous les mouvements de la souris et
/// le transmet ici, même si le pointeur sort de l'élément saisi.
#[derive(Clone)]
pub enum Drag {
    /// Barre de défilement (identifiant, zone défilée).
    Scroll(&'static str, ScrollHandle),
    Rpm,
    Load,
    /// Curseur de l'éditeur de pistes.
    Slider(Slider),
}

/// Index du bundle : sons par catégorie, puis par banque.
#[derive(Default)]
pub struct Catalog {
    pub groups: BTreeMap<SoundCategory, BTreeMap<usize, Vec<SoundRef>>>,
    pub total: usize,
}

impl Catalog {
    fn build(p: &ModProject) -> Self {
        let mut c = Catalog::default();
        for (r, bank, s) in p.sounds() {
            let cat = SoundCategory::guess(&bank.name, &s.name);
            c.groups.entry(cat).or_default().entry(r.bank).or_default().push(r);
            c.total += 1;
        }
        c
    }

    pub fn count(&self, cat: SoundCategory) -> usize {
        self.groups.get(&cat).map_or(0, |g| g.values().map(Vec::len).sum())
    }

    pub fn sounds(&self, cat: SoundCategory, bank: usize) -> &[SoundRef] {
        self.groups.get(&cat).and_then(|g| g.get(&bank)).map_or(&[], Vec::as_slice)
    }
}

pub struct AppState {
    pub settings: Settings,
    pub project: Option<ModProject>,
    /// Projet `.cxssm` ouvert (dossier : original, mod, pistes) ; `None` = bundle seul.
    pub workspace: Option<Workspace>,
    /// État du fichier du jeu par rapport au projet (`None` : vérification en cours).
    pub game_state: Option<GameState>,
    /// Fenêtre « Projet » ouverte.
    pub project_dialog: bool,
    pub catalog: Catalog,
    /// Incrémenté à chaque ouverture : les tâches de fond d'un ancien bundle sont ignorées.
    pub generation: u64,
    pub status: SharedString,
    pub busy: bool,
    pub encoder: EncoderConfig,
    pub fmod: Option<FmodInstall>,

    // ── navigation ──
    pub leaf: Option<Leaf>,
    pub sound: Option<SoundRef>,
    pub open: HashSet<TreeKey>,
    pub query: String,
    pub search_focus: FocusHandle,
    pub tree_scroll: ScrollHandle,
    pub list_scroll: UniformListScrollHandle,
    pub side_open: bool,
    pub side_tab: SideTab,
    pub drag: Option<Drag>,

    pub engine: EngineState,
    pub playback: Playback,
    pub editor: Editor,

    // ── préférences ──
    pub prefs_open: bool,
    pub prefs_section: PrefSection,
    /// Bundles du jeu trouvés par la détection automatique.
    pub detected_bundles: Vec<PathBuf>,
    /// Installations FMOD trouvées par la détection automatique.
    pub fmod_detected: Vec<FmodInstall>,
    /// Tutoriel d'installation de FMOD ouvert (au démarrage si FMOD est introuvable).
    pub fmod_help_open: bool,
    /// Dernier échec de la recherche ou du choix du dossier FMOD.
    pub fmod_error: Option<SharedString>,
    pub update: UpdateState,
}

impl AppState {
    pub fn new(cx: &mut Context<Self>, initial: Option<PathBuf>) -> Self {
        let settings = Settings::load();
        let fmod = settings.fmod();
        match &fmod {
            Some(f) => log::info!("FMOD Engine : {}", f.root.display()),
            None => log::warn!("FMOD Engine introuvable"),
        }
        // Projet ou bundle passé en argument, sinon le dernier projet, sinon le bundle du
        // jeu (choisi ou détecté automatiquement).
        let initial = initial.or_else(|| settings.startup_path());
        let mut s = Self {
            status: tr(settings.language, "Ouvrez fmod_assets_all.bundle pour commencer", "Open fmod_assets_all.bundle to get started")
                .into(),
            encoder: EncoderConfig { fsbankcl: fmod.as_ref().and_then(|f| f.fsbank.clone()), quality: 80 },
            settings,
            project: None,
            workspace: None,
            game_state: None,
            project_dialog: false,
            catalog: Catalog::default(),
            generation: 0,
            busy: false,
            fmod,
            leaf: None,
            sound: None,
            open: HashSet::from([TreeKey::Category(SoundCategory::Engine)]),
            query: String::new(),
            search_focus: cx.focus_handle(),
            tree_scroll: ScrollHandle::new(),
            list_scroll: UniformListScrollHandle::new(),
            side_open: true,
            side_tab: SideTab::Info,
            drag: None,
            engine: EngineState::default(),
            playback: Playback::new(),
            editor: Editor::default(),
            prefs_open: false,
            prefs_section: PrefSection::General,
            detected_bundles: Vec::new(),
            fmod_detected: Vec::new(),
            fmod_help_open: false,
            fmod_error: None,
            update: UpdateState::default(),
        };
        s.fmod_help_open = s.fmod.is_none();
        if let Some(path) = initial {
            s.open_path(path, cx);
        }
        if s.settings.check_updates {
            s.check_updates(true, cx);
        }
        s
    }

    // ───────────────────────────── langue ─────────────────────────────

    /// Texte dans la langue de l'interface.
    pub fn t(&self, fr: &'static str, en: &'static str) -> &'static str {
        tr(self.settings.language, fr, en)
    }

    /// Variante pour les textes formatés (les deux versions sont fournies).
    pub fn tf(&self, fr: String, en: String) -> String {
        match self.settings.language {
            Language::Fr => fr,
            Language::En => en,
        }
    }

    pub fn lang(&self) -> Language {
        self.settings.language
    }

    /// Message de la barre d'état décrivant le bundle chargé.
    pub fn summary_status(&self) -> SharedString {
        match &self.project {
            Some(p) => self
                .tf(
                    format!("{} banques, {} sons trouvés", p.banks.len(), self.catalog.total),
                    format!("{} banks, {} sounds found", p.banks.len(), self.catalog.total),
                )
                .into(),
            None => self.t("Ouvrez fmod_assets_all.bundle pour commencer", "Open fmod_assets_all.bundle to get started").into(),
        }
    }

    pub fn set_status(&mut self, fr: String, en: String, cx: &mut Context<Self>) {
        self.status = self.tf(fr, en).into();
        cx.notify();
    }

    pub fn save_settings(&self) {
        if let Err(e) = self.settings.save() {
            log::error!("enregistrement des préférences : {e}");
        }
    }

    pub fn replaced_count(&self) -> usize {
        self.project.as_ref().map_or(0, |p| p.replacements.len())
    }

    pub fn bank_name(&self, bank: usize) -> String {
        self.project.as_ref().and_then(|p| p.banks.get(bank)).map(|b| b.name.clone()).unwrap_or_default()
    }

    pub fn sound_name(&self, r: SoundRef) -> String {
        self.project.as_ref().and_then(|p| p.sample(r)).map(|(_, s)| s.name.clone()).unwrap_or_default()
    }

    // ───────────────────────────── ouverture / export ─────────────────────────────

    /// Ouvre un projet `.cxssm` ou un bundle.
    pub fn open_file_dialog(&mut self, cx: &mut Context<Self>) {
        let title = self.t("Ouvrir un projet CXSSM ou fmod_assets_all.bundle", "Open a CXSSM project or fmod_assets_all.bundle");
        cx.spawn(async move |this, cx| {
            let file = rfd::AsyncFileDialog::new()
                .set_title(title)
                .add_filter("CXSSM / AssetBundle", &["cxssm", "bundle"])
                .add_filter("CXSSM", &["cxssm"])
                .add_filter("AssetBundle Unity", &["bundle"])
                .pick_file()
                .await;
            let _ = this.update(cx, |s, cx| match file {
                Some(f) => s.open_path(f.path().to_path_buf(), cx),
                None => s.set_status("Ouverture annulée".into(), "Opening cancelled".into(), cx),
            });
        })
        .detach();
    }

    /// Ouvre un projet (`.cxssm` ou son dossier) ou un bundle seul.
    pub fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let is_project = path.is_dir() || path.extension().is_some_and(|e| e == cxssm_core::workspace::EXTENSION);
        if is_project {
            self.open_project(path, cx);
        } else {
            if self.settings.last_project.take().is_some() {
                self.save_settings();
            }
            self.open_bundle(path, None, cx);
        }
    }

    /// Charge un bundle ; avec `ws`, c'est l'original du projet et ses sons sont appliqués.
    pub(super) fn open_bundle(&mut self, path: PathBuf, ws: Option<Workspace>, cx: &mut Context<Self>) {
        log::info!("ouverture de {}", path.display());
        self.busy = true;
        self.set_status(format!("Analyse de {}…", path.display()), format!("Reading {}…", path.display()), cx);
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { ModProject::open(path) }).await;
            let _ = this.update(cx, |s, cx| {
                s.busy = false;
                match result {
                    Ok(p) => {
                        s.stop(cx);
                        s.generation += 1;
                        s.catalog = Catalog::build(&p);
                        s.project = Some(p);
                        s.status = s.summary_status();
                        s.leaf = None;
                        s.sound = None;
                        s.playback.reset_spectra();
                        s.editor.edits.clear();
                        s.editor.takes = Default::default();
                        s.engine.reset();
                        s.workspace = ws;
                        s.game_state = None;
                        s.apply_workspace(cx);
                        s.init_engine(cx);
                    }
                    Err(e) => {
                        log::error!("ouverture : {e}");
                        s.status = s.tf(format!("Erreur : {e}"), format!("Error: {e}")).into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn export_mod(&mut self, cx: &mut Context<Self>) {
        if self.replaced_count() == 0 {
            self.set_status("Aucun son remplacé : rien à exporter".into(), "No replaced sound: nothing to export".into(), cx);
            return;
        }
        // Projet : le mod est construit dans son dossier `mod/`.
        if self.workspace.is_some() {
            self.build_mod(false, cx);
            return;
        }
        let title = self.t("Dossier de sortie du mod", "Mod output folder");
        cx.spawn(async move |this, cx| {
            let Some(dir) = rfd::AsyncFileDialog::new().set_title(title).pick_folder().await else { return };
            // Le projet est emprunté le temps de l'export puis rendu.
            let taken = this.update(cx, |s, cx| {
                s.busy = true;
                s.set_status("Export en cours…".into(), "Exporting…".into(), cx);
                (s.project.take(), s.encoder.clone())
            });
            let Ok((Some(project), enc)) = taken else { return };
            let out = dir.path().to_path_buf();
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
                    Ok(p) => s.set_status(format!("Mod exporté : {}", p.display()), format!("Mod exported: {}", p.display()), cx),
                    Err(e) => s.set_status(format!("Erreur d'export : {e}"), format!("Export error: {e}"), cx),
                }
            });
        })
        .detach();
    }

    /// Copie les fichiers de remplacement dans un dossier (`<banque>_<son>.<ext>`).
    pub fn export_replaced_files(&mut self, cx: &mut Context<Self>) {
        let Some(p) = &self.project else { return };
        let files: Vec<(String, PathBuf)> = p
            .replacements
            .iter()
            .filter_map(|(r, f)| {
                let (bank, s) = p.sample(*r)?;
                let ext = f.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "wav".into());
                Some((format!("{}_{}.{ext}", bank.name, s.name), f.clone()))
            })
            .collect();
        if files.is_empty() {
            return;
        }
        let title = self.t("Dossier où copier les sons remplacés", "Folder to copy the replaced sounds into");
        cx.spawn(async move |this, cx| {
            let Some(dir) = rfd::AsyncFileDialog::new().set_title(title).pick_folder().await else { return };
            let dir = dir.path().to_path_buf();
            let res: std::io::Result<usize> = files.iter().try_fold(0, |n, (name, src)| std::fs::copy(src, dir.join(name)).map(|_| n + 1));
            let _ = this.update(cx, |s, cx| match res {
                Ok(n) => s.set_status(format!("{n} son(s) copié(s) dans {}", dir.display()), format!("{n} sound(s) copied to {}", dir.display()), cx),
                Err(e) => s.set_status(format!("Erreur : {e}"), format!("Error: {e}"), cx),
            });
        })
        .detach();
    }

    // ───────────────────────────── remplacement ─────────────────────────────

    pub fn pick_replacement(&mut self, r: SoundRef, cx: &mut Context<Self>) {
        let title = self.t("Choisir le son de remplacement", "Choose the replacement sound");
        cx.spawn(async move |this, cx| {
            let Some(file) = rfd::AsyncFileDialog::new()
                .set_title(title)
                .add_filter("Audio", &["wav", "ogg", "flac", "mp3"])
                .pick_file()
                .await
            else {
                return;
            };
            let _ = this.update(cx, |s, cx| {
                s.on_file_replacement(r, file.path().to_path_buf(), cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn restore_original(&mut self, r: SoundRef, cx: &mut Context<Self>) {
        self.forget_edit(r);
        if let Some(p) = self.project.as_mut() {
            p.clear_replacement(r);
            self.refresh_modded_engine(cx);
            self.project_changed(true);
            cx.notify();
        }
    }

    // ───────────────────────────── navigation ─────────────────────────────

    pub fn select_leaf(&mut self, leaf: Leaf, cx: &mut Context<Self>) {
        if self.leaf == Some(leaf) {
            return;
        }
        self.leaf = Some(leaf);
        self.sound = None;
        self.list_scroll.0.borrow().base_handle.set_offset(point(px(0.), px(0.)));
        if let Leaf::Car(i) = leaf {
            // Déplie la branche de la voiture (utile quand elle est choisie par la recherche).
            if let Some(bank) = self.engine.events.get(i).map(|e| e.bank) {
                self.open.insert(TreeKey::Group(SoundCategory::Engine, bank));
            }
            self.select_car(i, cx);
        }
        cx.notify();
    }

    pub fn select_sound(&mut self, r: SoundRef, cx: &mut Context<Self>) {
        self.sound = Some(r);
        cx.notify();
    }

    pub fn toggle_open(&mut self, key: TreeKey, cx: &mut Context<Self>) {
        if !self.open.remove(&key) {
            self.open.insert(key);
        }
        cx.notify();
    }

    pub fn set_query(&mut self, q: String, cx: &mut Context<Self>) {
        self.query = q;
        self.tree_scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    pub fn set_side(&mut self, open: bool, cx: &mut Context<Self>) {
        self.side_open = open;
        cx.notify();
    }

    pub fn set_tab(&mut self, tab: SideTab, cx: &mut Context<Self>) {
        self.side_tab = tab;
        self.side_open = true;
        cx.notify();
    }

    /// Fait défiler la liste tenue par `handle` d'après l'ordonnée de la souris.
    pub fn scroll_to_mouse(handle: &ScrollHandle, mouse_y: gpui::Pixels) {
        let bounds = handle.bounds();
        let max = handle.max_offset().height;
        if bounds.size.height <= px(0.) || max <= px(0.) {
            return;
        }
        let ratio = ((mouse_y - bounds.origin.y) / bounds.size.height).clamp(0., 1.);
        handle.set_offset(point(px(0.), -(max * ratio)));
    }

    /// La barre de défilement `id` est en cours de glisser.
    pub fn scroll_dragging(&self, id: &str) -> bool {
        matches!(&self.drag, Some(Drag::Scroll(d, _)) if *d == id)
    }

    /// Mouvement de la souris pendant un glisser (barre de défilement, jauges).
    pub fn drag_to(&mut self, pos: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        match self.drag.clone() {
            Some(Drag::Scroll(_, h)) => Self::scroll_to_mouse(&h, pos.y),
            Some(Drag::Rpm) => self.engine.set_rpm_from_mouse(pos.x),
            Some(Drag::Load) => self.engine.set_load_from_mouse(pos.x),
            Some(Drag::Slider(sl)) => self.slide(sl, pos.x),
            None => return,
        }
        cx.notify();
    }

    /// Bouton de la souris relâché n'importe où dans la fenêtre.
    pub fn mouse_released(&mut self, cx: &mut Context<Self>) {
        // Fin du réglage d'un curseur : le son est rendu avec la nouvelle valeur.
        if let (Some(Drag::Slider(_)), Some(r)) = (&self.drag, self.sound) {
            self.render_edit(r, cx);
        }
        self.drag = None;
        self.engine.throttle = false;
        cx.notify();
    }

    // ───────────────────────────── préférences ─────────────────────────────

    pub fn open_preferences(&mut self, section: Option<PrefSection>, cx: &mut Context<Self>) {
        self.rescan(cx);
        if let Some(sec) = section {
            self.prefs_section = sec;
        }
        self.prefs_open = true;
        cx.notify();
    }

    /// Relance la détection du jeu et de FMOD.
    pub fn rescan(&mut self, cx: &mut Context<Self>) {
        self.detected_bundles = game::find_bundles();
        self.fmod_detected = FmodInstall::detect();
        cx.notify();
    }

    pub fn close_preferences(&mut self, cx: &mut Context<Self>) {
        self.prefs_open = false;
        cx.notify();
    }

    pub fn set_prefs_section(&mut self, sec: PrefSection, cx: &mut Context<Self>) {
        self.prefs_section = sec;
        cx.notify();
    }

    pub fn set_language(&mut self, lang: Language, cx: &mut Context<Self>) {
        self.settings.language = lang;
        self.save_settings();
        self.status = self.summary_status();
        cx.notify();
    }

    pub fn toggle_limiter(&mut self, cx: &mut Context<Self>) {
        self.settings.rpm_limiter = !self.settings.rpm_limiter;
        self.save_settings();
        cx.notify();
    }

    pub fn toggle_category(&mut self, c: SoundCategory, cx: &mut Context<Self>) {
        self.settings.toggle_category(c);
        self.save_settings();
        if self.settings.is_hidden(c) && matches!(self.leaf, Some(Leaf::Bank(cat, _)) if cat == c) {
            self.leaf = None;
            self.sound = None;
        }
        cx.notify();
    }

    pub fn toggle_auto_open(&mut self, cx: &mut Context<Self>) {
        self.settings.auto_open = !self.settings.auto_open;
        self.save_settings();
        cx.notify();
    }

    pub fn toggle_check_updates(&mut self, cx: &mut Context<Self>) {
        self.settings.check_updates = !self.settings.check_updates;
        self.save_settings();
        cx.notify();
    }

    /// Définit le bundle du jeu par défaut et l'ouvre.
    pub fn use_bundle(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.settings.bundle_path = Some(path.clone());
        self.save_settings();
        self.prefs_open = false;
        self.open_path(path, cx);
    }

    pub fn choose_bundle(&mut self, cx: &mut Context<Self>) {
        let title = self.t("Choisir fmod_assets_all.bundle", "Choose fmod_assets_all.bundle");
        cx.spawn(async move |this, cx| {
            let file = rfd::AsyncFileDialog::new().set_title(title).add_filter("AssetBundle Unity", &["bundle"]).pick_file().await;
            if let Some(f) = file {
                let _ = this.update(cx, |s, cx| s.use_bundle(f.path().to_path_buf(), cx));
            }
        })
        .detach();
    }

    /// Utilise cette installation FMOD (encodeur + simulateur) et la mémorise.
    pub fn use_fmod(&mut self, inst: FmodInstall, cx: &mut Context<Self>) {
        self.fmod_help_open = false;
        self.fmod_error = None;
        self.settings.fmod_dir = Some(inst.root.clone());
        self.save_settings();
        self.fmod = self.settings.fmod();
        self.encoder.fsbankcl = self.fmod.as_ref().and_then(|f| f.fsbank.clone());
        if self.engine.has_library() {
            // La bibliothèque déjà chargée reste utilisée jusqu'au prochain lancement.
            self.set_status(
                "Dossier FMOD enregistré : redémarrez CXSSM pour l'utiliser dans le simulateur.".into(),
                "FMOD folder saved: restart CXSSM to use it in the simulator.".into(),
                cx,
            );
        } else {
            self.engine.clear_error();
            self.set_status(format!("FMOD Engine : {}", inst.root.display()), format!("FMOD Engine: {}", inst.root.display()), cx);
            self.init_engine(cx);
        }
    }

    pub fn choose_fmod_dir(&mut self, cx: &mut Context<Self>) {
        let title = self.t("Dossier du FMOD Engine", "FMOD Engine folder");
        cx.spawn(async move |this, cx| {
            let Some(dir) = rfd::AsyncFileDialog::new().set_title(title).pick_folder().await else { return };
            let _ = this.update(cx, |s, cx| match FmodInstall::from_path(dir.path()) {
                Some(inst) => s.use_fmod(inst, cx),
                None => {
                    s.set_status(
                        format!("{} : ce n'est pas un dossier FMOD Engine (api/studio introuvable)", dir.path().display()),
                        format!("{}: not a FMOD Engine folder (api/studio not found)", dir.path().display()),
                        cx,
                    );
                    s.fmod_error = Some(s.status.clone());
                }
            });
        })
        .detach();
    }

    pub fn set_fmod_help(&mut self, open: bool, cx: &mut Context<Self>) {
        self.fmod_help_open = open;
        cx.notify();
    }

    /// « Rechercher à nouveau » : détection automatique de FMOD aux emplacements habituels.
    pub fn retry_fmod_detection(&mut self, cx: &mut Context<Self>) {
        self.fmod_detected = FmodInstall::detect();
        match FmodInstall::resolve(self.settings.fmod_dir.as_deref()) {
            Some(inst) => self.use_fmod(inst, cx),
            None => {
                self.fmod_error = Some(
                    self.t(
                        "Toujours introuvable : indiquez le dossier avec « Choisir le dossier… ».",
                        "Still not found: point to the folder with “Choose the folder…”.",
                    )
                    .into(),
                );
                cx.notify();
            }
        }
    }

    /// Une tâche de fond ou une animation demande de redessiner à chaque image.
    pub fn is_animating(&self) -> bool {
        self.busy || self.playback.is_active() || self.engine.is_active() || self.editor_busy()
    }
}
