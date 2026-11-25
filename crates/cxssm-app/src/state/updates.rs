//! Mises à jour depuis les Releases GitHub : vérification (au démarrage et à la demande),
//! installation et redémarrage, notes de version (après une mise à jour), et contributeurs
//! affichés dans Préférences › À propos.

use cxssm_core::update::{self, Release};
use gpui::{AppContext as _, Context};

use super::AppState;

#[derive(Default)]
pub struct UpdateState {
    pub checking: bool,
    /// Une vérification a abouti (même sans nouvelle version).
    pub checked: bool,
    pub latest: Option<Release>,
    pub error: Option<String>,
    pub installing: bool,
    /// La nouvelle version est installée : il reste à redémarrer.
    pub installed: bool,
    /// Fenêtre « Nouveautés » ouverte, pour ce tag.
    pub notes_open: Option<String>,
    /// Notes de version de `notes_open` : `None` pendant le chargement.
    pub notes: Option<Result<String, String>>,
    /// Contributeurs GitHub : `None` tant qu'ils n'ont pas été demandés ou reçus.
    pub contributors: Option<Result<Vec<String>, String>>,
    pub contributors_loading: bool,
}

/// Tag de la version en cours (`v0.4.3`).
pub fn current_tag() -> String {
    format!("v{}", update::CURRENT_VERSION)
}

impl AppState {
    /// Interroge GitHub en arrière-plan. `silent` : vérification de démarrage, qui ne
    /// signale rien s'il n'y a pas de nouvelle version (ni en cas d'erreur réseau).
    pub fn check_updates(&mut self, silent: bool, cx: &mut Context<Self>) {
        if self.update.checking {
            return;
        }
        self.update.checking = true;
        self.update.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { update::check() }).await;
            let _ = this.update(cx, |s, cx| {
                s.update.checking = false;
                match res {
                    Ok(latest) => {
                        s.update.checked = true;
                        if let Some(r) = &latest {
                            s.status = s
                                .tf(
                                    format!("Nouvelle version {} disponible : Préférences › Mises à jour", r.tag),
                                    format!("New version {} available: Preferences › Updates", r.tag),
                                )
                                .into();
                        }
                        s.update.latest = latest;
                    }
                    Err(e) => {
                        log::warn!("vérification des mises à jour : {e}");
                        if !silent {
                            s.update.error = Some(e.to_string());
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn install_update(&mut self, cx: &mut Context<Self>) {
        let Some(release) = self.update.latest.clone() else { return };
        self.update.installing = true;
        self.update.error = None;
        self.set_status(format!("Téléchargement de {}…", release.tag), format!("Downloading {}…", release.tag), cx);
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { update::install(&release).map(|_| release.tag) }).await;
            let _ = this.update(cx, |s, cx| {
                s.update.installing = false;
                match res {
                    Ok(tag) => {
                        s.update.installed = true;
                        s.set_status(format!("{tag} installée : redémarrez CXSSM"), format!("{tag} installed: restart CXSSM"), cx);
                    }
                    Err(e) => {
                        log::error!("installation de la mise à jour : {e}");
                        s.update.error = Some(e.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Premier lancement d'une nouvelle version : enregistre la version, puis affiche ses notes
    /// si CXSSM a été mis à jour (préférences déjà présentes, vérification en ligne autorisée).
    pub fn note_version_change(&mut self, had_settings: bool, cx: &mut Context<Self>) {
        let current = update::CURRENT_VERSION;
        let previous = self.settings.last_version.clone();
        if previous.as_deref() == Some(current) {
            return;
        }
        self.settings.last_version = Some(current.to_string());
        self.save_settings();
        let updated = match previous.as_deref().and_then(|p| semver::Version::parse(p).ok()) {
            Some(prev) => semver::Version::parse(current).is_ok_and(|c| c > prev),
            // Préférences d'une version antérieure à cet enregistrement : c'est une mise à jour.
            None => had_settings && previous.is_none(),
        };
        if updated && self.settings.check_updates {
            self.show_release_notes(current_tag(), cx);
        }
    }

    /// Ouvre la fenêtre des notes de version de `tag` et les télécharge.
    pub fn show_release_notes(&mut self, tag: String, cx: &mut Context<Self>) {
        let same = self.update.notes_open.as_deref() == Some(tag.as_str());
        self.update.notes_open = Some(tag.clone());
        if same && matches!(self.update.notes, Some(Ok(_))) {
            cx.notify();
            return;
        }
        self.update.notes = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let t = tag.clone();
            let res = cx.background_spawn(async move { update::release_notes(&t).map_err(|e| e.to_string()) }).await;
            let _ = this.update(cx, |s, cx| {
                if s.update.notes_open.as_deref() == Some(tag.as_str()) {
                    s.update.notes = Some(res);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn close_release_notes(&mut self, cx: &mut Context<Self>) {
        self.update.notes_open = None;
        cx.notify();
    }

    /// Télécharge la liste des contributeurs (une fois, ou à nouveau après une erreur).
    pub fn load_contributors(&mut self, cx: &mut Context<Self>) {
        if self.update.contributors_loading || matches!(self.update.contributors, Some(Ok(_))) {
            return;
        }
        self.update.contributors_loading = true;
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { update::contributors().map_err(|e| e.to_string()) }).await;
            let _ = this.update(cx, |s, cx| {
                s.update.contributors_loading = false;
                s.update.contributors = Some(res);
                cx.notify();
            });
        })
        .detach();
    }

    /// Relance l'exécutable (désormais à jour) puis quitte.
    pub fn restart(&mut self, cx: &mut Context<Self>) {
        match std::env::current_exe().and_then(|exe| std::process::Command::new(exe).spawn()) {
            Ok(_) => cx.quit(),
            Err(e) => {
                self.update.error = Some(e.to_string());
                cx.notify();
            }
        }
    }
}
