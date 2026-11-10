//! Mises à jour depuis les Releases GitHub : vérification (au démarrage et à la demande),
//! installation et redémarrage.

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
