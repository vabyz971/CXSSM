//! Mises à jour depuis les Releases GitHub : vérification (au démarrage et à la demande),
//! installation et redémarrage.

use cxssm_core::update::{self, InstallMethod, Release};
use gpui::{div, prelude::*, Context};

use super::MainView;
use crate::theme;
use crate::widgets::{ghost_button, primary_button};

#[derive(Default)]
pub(super) struct UpdateState {
    checking: bool,
    /// Une vérification a abouti (même sans nouvelle version).
    checked: bool,
    latest: Option<Release>,
    error: Option<String>,
    installing: bool,
    /// La nouvelle version est installée : il reste à redémarrer.
    installed: bool,
}

impl MainView {
    /// Interroge GitHub en arrière-plan. `silent` : vérification de démarrage, qui ne
    /// signale rien s'il n'y a pas de nouvelle version (ni en cas d'erreur réseau).
    pub(super) fn check_updates(&mut self, silent: bool, cx: &mut Context<Self>) {
        if self.update.checking {
            return;
        }
        self.update.checking = true;
        self.update.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { update::check() }).await;
            let _ = this.update(cx, |v, cx| {
                v.update.checking = false;
                match res {
                    Ok(latest) => {
                        v.update.checked = true;
                        if let Some(r) = &latest {
                            v.status = v
                                .tf(
                                    format!("Nouvelle version {} disponible : ⚙ Préférences pour l'installer", r.tag),
                                    format!("New version {} available: open ⚙ Preferences to install it", r.tag),
                                )
                                .into();
                        }
                        v.update.latest = latest;
                    }
                    Err(e) => {
                        log::warn!("vérification des mises à jour : {e}");
                        if !silent {
                            v.update.error = Some(e.to_string());
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn install_update(&mut self, cx: &mut Context<Self>) {
        let Some(release) = self.update.latest.clone() else { return };
        self.update.installing = true;
        self.update.error = None;
        self.status = self.tf(format!("Téléchargement de {}…", release.tag), format!("Downloading {}…", release.tag)).into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { update::install(&release).map(|_| release.tag) }).await;
            let _ = this.update(cx, |v, cx| {
                v.update.installing = false;
                match res {
                    Ok(tag) => {
                        v.update.installed = true;
                        v.status = v
                            .tf(format!("{tag} installée : redémarrez CXSSM"), format!("{tag} installed: restart CXSSM"))
                            .into();
                    }
                    Err(e) => {
                        log::error!("installation de la mise à jour : {e}");
                        v.update.error = Some(e.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Relance l'exécutable (désormais à jour) puis quitte.
    fn restart(&mut self, cx: &mut Context<Self>) {
        match std::env::current_exe().and_then(|exe| std::process::Command::new(exe).spawn()) {
            Ok(_) => cx.quit(),
            Err(e) => self.update.error = Some(e.to_string()),
        }
    }

    pub(super) fn render_update_section(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let u = &self.update;
        let current = self.tf(
            format!("Version installée : {}", update::CURRENT_VERSION),
            format!("Installed version: {}", update::CURRENT_VERSION),
        );
        let nix = update::install_method() == InstallMethod::Nix;

        let state = if u.checking {
            div().text_sm().text_color(theme::text_dim()).child(self.t("Vérification…", "Checking…"))
        } else if u.installed {
            div().text_sm().text_color(theme::ok()).child(self.t(
                "Mise à jour installée. Redémarrez pour l'utiliser.",
                "Update installed. Restart to use it.",
            ))
        } else if let Some(r) = &u.latest {
            let mut d = div().flex().flex_col().gap_1().child(
                div().text_sm().text_color(theme::ok()).child(self.tf(
                    format!("Nouvelle version disponible : {}", r.tag),
                    format!("New version available: {}", r.tag),
                )),
            );
            d = d.child(div().text_xs().text_color(theme::text_dim()).child(r.page.clone()));
            if nix {
                d = d.child(div().text_xs().text_color(theme::text_dim()).child(self.t(
                    "Installation Nix : mettez à jour avec `nix profile upgrade` ou votre flake.",
                    "Nix install: update with `nix profile upgrade` or your flake.",
                )));
            } else if r.asset.is_none() {
                d = d.child(div().text_xs().text_color(theme::warn()).child(self.t(
                    "Pas encore d'archive pour ce système dans cette release.",
                    "No archive for this system in this release yet.",
                )));
            }
            d
        } else if u.checked {
            div().text_sm().text_color(theme::text_dim()).child(self.t("CXSSM est à jour.", "CXSSM is up to date."))
        } else {
            div()
        };

        let can_install = !nix && !u.installing && !u.installed && u.latest.as_ref().is_some_and(|r| r.asset.is_some());

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().child(current))
            .child(state)
            .when_some(u.error.clone(), |d, e| d.child(div().text_xs().text_color(theme::warn()).child(e)))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        ghost_button("check-update", self.t("VÉRIFIER", "CHECK"))
                            .on_click(cx.listener(|v, _, _, cx| v.check_updates(false, cx))),
                    )
                    .when(can_install, |d| {
                        d.child(
                            primary_button("install-update", self.t("INSTALLER", "INSTALL"))
                                .on_click(cx.listener(|v, _, _, cx| v.install_update(cx))),
                        )
                    })
                    .when(u.installing, |d| {
                        d.child(div().text_sm().text_color(theme::text_dim()).child(self.t("Installation…", "Installing…")))
                    })
                    .when(u.installed, |d| {
                        d.child(
                            primary_button("restart", self.t("REDÉMARRER", "RESTART"))
                                .on_click(cx.listener(|v, _, _, cx| v.restart(cx))),
                        )
                    }),
            )
    }
}
