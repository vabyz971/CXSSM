//! Panneau des préférences : langue, rupteur du simulateur, catégories affichées,
//! détection et ouverture automatique du jeu.

use std::path::PathBuf;

use cxssm_core::{game, FmodInstall, Language};
use gpui::{div, prelude::*, px, Context, ElementId, FontWeight, SharedString};

use super::{MainView, CATEGORIES};
use crate::i18n;
use crate::theme;
use crate::widgets::{ghost_button, primary_button, section_title};

impl MainView {
    // ───────────────────────────── actions ─────────────────────────────

    pub(super) fn open_preferences(&mut self, cx: &mut Context<Self>) {
        self.detected = game::find_bundles();
        self.fmod_detected = FmodInstall::detect();
        self.prefs_open = true;
        cx.notify();
    }

    fn set_language(&mut self, lang: Language, cx: &mut Context<Self>) {
        self.settings.language = lang;
        self.save_settings();
        // La barre d'état est réécrite dans la nouvelle langue.
        self.status = self.summary_status();
        cx.notify();
    }

    fn toggle_limiter(&mut self, cx: &mut Context<Self>) {
        self.settings.rpm_limiter = !self.settings.rpm_limiter;
        self.save_settings();
        cx.notify();
    }

    fn toggle_category(&mut self, i: usize, cx: &mut Context<Self>) {
        let c = CATEGORIES[i];
        self.settings.toggle_category(c);
        self.save_settings();
        // Le filtre actif ne peut pas rester sur une catégorie masquée.
        if self.settings.is_hidden(c) && self.filter == Some(c) {
            self.set_filter(None, cx);
        }
        cx.notify();
    }

    fn toggle_auto_open(&mut self, cx: &mut Context<Self>) {
        self.settings.auto_open = !self.settings.auto_open;
        self.save_settings();
        cx.notify();
    }

    /// Définit le bundle du jeu par défaut et l'ouvre.
    fn use_bundle(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.settings.bundle_path = Some(path.clone());
        self.save_settings();
        self.prefs_open = false;
        self.open_path(path, cx);
    }

    /// Utilise cette installation FMOD (encodeur + simulateur) et la mémorise.
    fn use_fmod(&mut self, inst: FmodInstall, cx: &mut Context<Self>) {
        self.settings.fmod_dir = Some(inst.root.clone());
        self.save_settings();
        self.fmod = self.settings.fmod();
        self.encoder.fsbankcl = self.fmod.as_ref().and_then(|f| f.fsbank.clone());
        if self.engine.has_library() {
            // La bibliothèque déjà chargée reste utilisée jusqu'au prochain lancement.
            self.status = self
                .t(
                    "Dossier FMOD enregistré : redémarrez CXSSM pour l'utiliser dans le simulateur.",
                    "FMOD folder saved: restart CXSSM to use it in the simulator.",
                )
                .into();
        } else {
            self.engine.clear_error();
            self.status = self.tf(
                format!("FMOD Engine : {}", inst.root.display()),
                format!("FMOD Engine: {}", inst.root.display()),
            )
            .into();
        }
        cx.notify();
    }

    fn choose_fmod_dir(&mut self, cx: &mut Context<Self>) {
        let title = self.t("Dossier du FMOD Engine", "FMOD Engine folder");
        cx.spawn(async move |this, cx| {
            let Some(dir) = rfd::AsyncFileDialog::new().set_title(title).pick_folder().await else { return };
            let _ = this.update(cx, |v, cx| match FmodInstall::from_path(dir.path()) {
                Some(inst) => v.use_fmod(inst, cx),
                None => {
                    v.status = v
                        .tf(
                            format!("{} : ce n'est pas un dossier FMOD Engine (api/studio introuvable)", dir.path().display()),
                            format!("{}: not a FMOD Engine folder (api/studio not found)", dir.path().display()),
                        )
                        .into();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn toggle_check_updates(&mut self, cx: &mut Context<Self>) {
        self.settings.check_updates = !self.settings.check_updates;
        self.save_settings();
        cx.notify();
    }

    fn choose_bundle(&mut self, cx: &mut Context<Self>) {
        let title = self.t("Choisir fmod_assets_all.bundle", "Choose fmod_assets_all.bundle");
        cx.spawn(async move |this, cx| {
            let file = rfd::AsyncFileDialog::new()
                .set_title(title)
                .add_filter("AssetBundle Unity", &["bundle"])
                .pick_file()
                .await;
            if let Some(f) = file {
                let _ = this.update(cx, |v, cx| v.use_bundle(f.path().to_path_buf(), cx));
            }
        })
        .detach();
    }

    // ───────────────────────────── rendu ─────────────────────────────

    pub(super) fn render_preferences(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = self.settings.language;

        let language = div()
            .flex()
            .gap_2()
            .child(
                chip("lang-fr", "Français", lang == Language::Fr)
                    .on_click(cx.listener(|v, _, _, cx| v.set_language(Language::Fr, cx))),
            )
            .child(
                chip("lang-en", "English", lang == Language::En)
                    .on_click(cx.listener(|v, _, _, cx| v.set_language(Language::En, cx))),
            );

        let limiter = div()
            .flex()
            .items_center()
            .gap_3()
            .child(
                chip(
                    "limiter",
                    if self.settings.rpm_limiter { self.t("✔ ACTIVÉ", "✔ ON") } else { self.t("DÉSACTIVÉ", "OFF") },
                    self.settings.rpm_limiter,
                )
                .on_click(cx.listener(|v, _, _, cx| v.toggle_limiter(cx))),
            )
            .child(div().text_sm().text_color(theme::text_dim()).child(if self.settings.rpm_limiter {
                self.t(
                    "Le régime rebondit sur le rupteur (réglable dans le simulateur).",
                    "RPM bounces on the rev limiter (adjustable in the simulator).",
                )
            } else {
                self.t("Pas de limite : le régime monte jusqu'au maximum.", "No limit: RPM climbs to the maximum.")
            }));

        let categories = div().flex().flex_wrap().gap_2().children(CATEGORIES.iter().enumerate().map(|(i, c)| {
            let shown = !self.settings.is_hidden(*c);
            let label = i18n::category(lang, *c);
            chip(("cat-pref", i), if shown { format!("✔ {label}") } else { label.to_string() }, shown)
                .on_click(cx.listener(move |v, _, _, cx| v.toggle_category(i, cx)))
        }));

        let current = self.settings.bundle_path.clone();
        let detected = if self.detected.is_empty() {
            vec![div()
                .text_sm()
                .text_color(theme::text_dim())
                .child(self.t(
                    "Jeu introuvable dans les bibliothèques Steam. Choisissez le fichier manuellement.",
                    "Game not found in the Steam libraries. Choose the file manually.",
                ))
                .into_any_element()]
        } else {
            self.detected
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let is_default = current.as_ref() == Some(p);
                    let path = p.clone();
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .truncate()
                                .text_xs()
                                .text_color(if is_default { theme::ok() } else { theme::text() })
                                .child(p.display().to_string()),
                        )
                        .child(
                            ghost_button_id(("use-bundle", i), self.t("OUVRIR", "OPEN"))
                                .on_click(cx.listener(move |v, _, _, cx| v.use_bundle(path.clone(), cx))),
                        )
                        .into_any_element()
                })
                .collect()
        };
        let custom = current
            .as_ref()
            .filter(|p| !self.detected.contains(p))
            .map(|p| div().truncate().text_xs().text_color(theme::ok()).child(p.display().to_string()));

        let game_section = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        chip(
                            "auto-open",
                            if self.settings.auto_open { self.t("✔ ACTIVÉE", "✔ ON") } else { self.t("DÉSACTIVÉE", "OFF") },
                            self.settings.auto_open,
                        )
                        .on_click(cx.listener(|v, _, _, cx| v.toggle_auto_open(cx))),
                    )
                    .child(div().text_sm().text_color(theme::text_dim()).child(self.t(
                        "Ouvrir le bundle du jeu au démarrage",
                        "Open the game bundle on startup",
                    ))),
            )
            .child(div().pt_2().text_xs().text_color(theme::text_dim()).child(self.t(
                "INSTALLATIONS DÉTECTÉES (bibliothèques Steam)",
                "DETECTED INSTALLATIONS (Steam libraries)",
            )))
            .children(detected)
            .when_some(custom, |d, c| {
                d.child(div().pt_2().text_xs().text_color(theme::text_dim()).child(self.t("FICHIER CHOISI", "CHOSEN FILE")))
                    .child(c)
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .pt_2()
                    .child(
                        ghost_button("rescan", self.t("RECHERCHER À NOUVEAU", "SEARCH AGAIN"))
                            .on_click(cx.listener(|v, _, _, cx| v.open_preferences(cx))),
                    )
                    .child(
                        ghost_button("choose-bundle", self.t("CHOISIR UN FICHIER…", "CHOOSE A FILE…"))
                            .on_click(cx.listener(|v, _, _, cx| v.choose_bundle(cx))),
                    ),
            );

        // ── FMOD Engine ──
        let fmod_current = match &self.fmod {
            Some(f) => div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().truncate().text_xs().text_color(theme::ok()).child(f.root.display().to_string()))
                .child(div().text_xs().text_color(theme::text_dim()).child(match &f.fsbank {
                    Some(_) => self.t("✔ encodeur (fsbank) et simulateur disponibles", "✔ encoder (fsbank) and simulator available"),
                    None => self.t("⚠ encodeur fsbank absent de ce dossier", "⚠ fsbank encoder missing from this folder"),
                })),
            None => div().text_sm().text_color(theme::warn()).child(self.t(
                "FMOD Engine introuvable. Téléchargez-le (gratuit) sur fmod.com/download puis indiquez son dossier.",
                "FMOD Engine not found. Download it (free) from fmod.com/download, then choose its folder.",
            )),
        };
        let fmod_rows = self
            .fmod_detected
            .iter()
            .enumerate()
            .filter(|(_, f)| self.fmod.as_ref().is_none_or(|cur| cur.root != f.root))
            .map(|(i, f)| {
                let inst = f.clone();
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().flex_1().min_w(px(0.)).truncate().text_xs().child(f.root.display().to_string()))
                    .child(
                        ghost_button_id(("use-fmod", i), self.t("UTILISER", "USE"))
                            .on_click(cx.listener(move |v, _, _, cx| v.use_fmod(inst.clone(), cx))),
                    )
            })
            .collect::<Vec<_>>();
        let has_other_fmod = !fmod_rows.is_empty();
        let fmod_section = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(fmod_current)
            .when(has_other_fmod, |d| {
                d.child(div().pt_2().text_xs().text_color(theme::text_dim()).child(self.t(
                    "AUTRES INSTALLATIONS DÉTECTÉES",
                    "OTHER DETECTED INSTALLATIONS",
                )))
                .children(fmod_rows)
            })
            .child(
                div().flex().gap_2().pt_2().child(
                    ghost_button("choose-fmod", self.t("CHOISIR LE DOSSIER…", "CHOOSE FOLDER…"))
                        .on_click(cx.listener(|v, _, _, cx| v.choose_fmod_dir(cx))),
                ),
            );

        // ── Mises à jour ──
        let update_section = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        chip(
                            "check-updates",
                            if self.settings.check_updates { self.t("✔ ACTIVÉE", "✔ ON") } else { self.t("DÉSACTIVÉE", "OFF") },
                            self.settings.check_updates,
                        )
                        .on_click(cx.listener(|v, _, _, cx| v.toggle_check_updates(cx))),
                    )
                    .child(div().text_sm().text_color(theme::text_dim()).child(self.t(
                        "Vérifier les nouvelles versions au démarrage (GitHub)",
                        "Check for new versions on startup (GitHub)",
                    ))),
            )
            .child(self.render_update_section(cx));

        let panel = div()
            .id("prefs-panel")
            .w(px(640.))
            .max_h(gpui::relative(0.9))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_5()
            .p_6()
            .bg(theme::panel())
            .border_1()
            .border_color(theme::border())
            // Les clics dans le panneau ne doivent pas le fermer.
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_xl().font_weight(FontWeight::BOLD).child(self.t("PRÉFÉRENCES", "PREFERENCES")))
                    .child(
                        primary_button("prefs-close", self.t("FERMER", "CLOSE")).on_click(cx.listener(|v, _, _, cx| {
                            v.prefs_open = false;
                            cx.notify();
                        })),
                    ),
            )
            .child(section(self.t("LANGUE", "LANGUAGE"), language))
            .child(section(self.t("SIMULATEUR : RUPTEUR", "SIMULATOR: REV LIMITER"), limiter))
            .child(section(self.t("CATÉGORIES AFFICHÉES", "VISIBLE CATEGORIES"), categories))
            .child(section(self.t("JEU", "GAME"), game_section))
            .child(section("FMOD ENGINE", fmod_section))
            .child(section(self.t("MISE À JOUR", "UPDATES"), update_section));

        // Fond assombri : un clic en dehors du panneau le ferme.
        div()
            .id("prefs-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x000000aa))
            .occlude()
            .on_click(cx.listener(|v, _, _, cx| {
                v.prefs_open = false;
                cx.notify();
            }))
            .child(panel)
    }
}

fn section(title: &'static str, body: impl IntoElement) -> impl IntoElement {
    div().flex().flex_col().gap_2().child(section_title(title)).child(body)
}

/// Bouton bascule : surligné quand l'option est active.
fn chip(id: impl Into<ElementId>, label: impl Into<SharedString>, active: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id.into())
        .px_3()
        .py_1()
        .border_1()
        .text_sm()
        .cursor_pointer()
        .border_color(if active { theme::accent() } else { theme::border() })
        .text_color(if active { theme::accent() } else { theme::text_dim() })
        .hover(|s| s.bg(theme::panel_hi()))
        .child(label.into())
}

/// `ghost_button` avec un identifiant dynamique (une ligne par installation détectée).
fn ghost_button_id(id: impl Into<ElementId>, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id.into())
        .px_3()
        .py_1()
        .border_1()
        .border_color(theme::border())
        .text_sm()
        .cursor_pointer()
        .hover(|s| s.bg(theme::panel_hi()))
        .child(label)
}
