//! Préférences : fenêtre modale avec navigation par sections (Général, Apparence,
//! Raccourcis clavier, Jeu, FMOD, Mises à jour).

use cxssm_core::update::{self, InstallMethod};
use cxssm_core::{Language, ThemeMode};
use gpui::{div, prelude::*, px, relative, BoxShadow, Context, ElementId, Entity, FontWeight, Window};

use crate::assets::{icon, icon_sm, Icon};
use crate::i18n;
use crate::state::{AppState, PrefSection, Shortcut, CATEGORIES};
use crate::theme;
use crate::widgets::{button, label_block, on, path_field, primary, segment, segmented, switch};

pub struct Preferences {
    state: Entity<AppState>,
}

impl Preferences {
    pub fn new(state: Entity<AppState>) -> Self {
        Self { state }
    }

    /// Ligne « titre + description » avec un interrupteur à droite.
    fn switch_row(
        &self,
        id: &'static str,
        title: &'static str,
        hint: &'static str,
        value: bool,
        toggle: impl Fn(&mut AppState, &mut gpui::Context<AppState>) + 'static,
    ) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(theme::GAP_LG)
            .child(label_block(title, Some(hint.into())))
            .child(switch(id, value).on_click(on(&self.state, toggle)))
    }

    fn general(&self, s: &AppState) -> gpui::Div {
        let lang = s.lang();
        let categories = div().flex().flex_wrap().gap(theme::GAP_SM).children(CATEGORIES.iter().enumerate().map(|(i, &c)| {
            let shown = !s.settings.is_hidden(c);
            div()
                .id(ElementId::from(("pref-cat", i)))
                .flex()
                .items_center()
                .gap(px(7.))
                .h(theme::CONTROL_H)
                .px(px(14.))
                .rounded(theme::RADIUS)
                .border_1()
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .bg(if shown { theme::accent_tint() } else { theme::inset() })
                .border_color(if shown { theme::accent() } else { theme::border() })
                .text_color(if shown { theme::accent() } else { theme::text_soft() })
                .when(!shown, |d| d.hover(|d| d.bg(theme::hover())))
                .when(shown, |d| d.child(icon(Icon::Check, px(13.), theme::accent())))
                .child(i18n::category(lang, c))
                .on_click(on(&self.state, move |s, cx| s.toggle_category(c, cx)))
        }));

        div()
            .flex()
            .flex_col()
            .gap(theme::GAP_XL)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(s.t("Langue", "Language")))
                    .child(div().flex().child(
                        segmented()
                            .child(
                                segment("lang-fr", "Français", lang == Language::Fr)
                                    .px(px(16.))
                                    .on_click(on(&self.state, |s, cx| s.set_language(Language::Fr, cx))),
                            )
                            .child(
                                segment("lang-en", "English", lang == Language::En)
                                    .px(px(16.))
                                    .on_click(on(&self.state, |s, cx| s.set_language(Language::En, cx))),
                            ),
                    )),
            )
            .child(self.switch_row(
                "pref-limiter",
                s.t("Limiteur de régime", "Rev limiter"),
                s.t(
                    "Le régime rebondit sur le limiteur dans le simulateur (réglable depuis le simulateur).",
                    "The RPM bounces off the limiter in the simulator (adjustable from the simulator).",
                ),
                s.settings.rpm_limiter,
                |s, cx| s.toggle_limiter(cx),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(label_block(
                        s.t("Catégories visibles", "Visible categories"),
                        Some(s.t("Seules les catégories cochées apparaissent dans l'arborescence.", "Only checked categories appear in the tree.").into()),
                    ))
                    .child(categories),
            )
    }

    /// Carte de choix du thème, avec un aperçu miniature de l'interface.
    fn theme_card(&self, s: &AppState, mode: ThemeMode, label: &'static str) -> impl IntoElement {
        let active = s.settings.theme == mode;
        let p = theme::Palette::new(mode, s.settings.accent_rgb());
        let bar = |w: f32, c: gpui::Rgba| div().h(px(6.)).w(px(w)).rounded(px(3.)).bg(c);
        let preview = div()
            .flex()
            .h(px(86.))
            .rounded(theme::RADIUS_SM)
            .overflow_hidden()
            .bg(p.bg)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .w(px(46.))
                    .p(px(8.))
                    .bg(p.sidebar)
                    .border_r_1()
                    .border_color(p.divider)
                    .child(bar(28., p.text_dim))
                    .child(bar(22., p.accent))
                    .child(bar(26., p.text_dim)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap(px(7.))
                    .p(px(8.))
                    .child(bar(70., p.text))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .p(px(6.))
                            .rounded(px(5.))
                            .bg(p.panel)
                            .border_1()
                            .border_color(p.border)
                            .child(bar(60., p.text_soft))
                            .child(div().h(px(5.)).w_full().rounded(px(3.)).bg(p.track).child(div().h_full().w(px(46.)).rounded(px(3.)).bg(p.accent))),
                    )
                    .child(div().flex().justify_end().child(div().h(px(12.)).w(px(34.)).rounded(px(4.)).bg(p.accent))),
            );
        div()
            .id(match mode {
                ThemeMode::Dark => "theme-dark",
                ThemeMode::Light => "theme-light",
            })
            .flex()
            .flex_col()
            .flex_1()
            .gap(px(10.))
            .p(px(10.))
            .rounded(theme::RADIUS)
            .border_2()
            .border_color(if active { theme::accent() } else { theme::border() })
            .bg(theme::inset())
            .cursor_pointer()
            .when(!active, |d| d.hover(|d| d.border_color(theme::border_hover())))
            .child(preview)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .font_weight(FontWeight::MEDIUM)
                    .when(active, |d| d.child(icon(Icon::Check, px(14.), theme::accent())))
                    .child(label),
            )
            .on_click(on(&self.state, move |s, cx| s.set_theme(mode, cx)))
    }

    fn appearance(&self, s: &AppState) -> gpui::Div {
        let lang = s.lang();
        let current = s.settings.accent_rgb();
        let swatches = div().flex().flex_wrap().gap(px(10.)).children(theme::ACCENTS.iter().enumerate().map(|(i, &(rgb, fr, en))| {
            let active = rgb == current;
            div()
                .id(ElementId::from(("accent", i)))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.))
                .w(px(58.))
                .cursor_pointer()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(36.))
                        .rounded(px(18.))
                        .bg(theme::hex(rgb))
                        .border_2()
                        .border_color(if active { theme::text() } else { gpui::transparent_black().into() })
                        .when(active, |d| d.child(icon(Icon::Check, px(16.), theme::on(rgb)))),
                )
                .child(
                    div()
                        .text_size(theme::TEXT_SM)
                        .text_color(if active { theme::text() } else { theme::text_dim() })
                        .child(if lang == Language::Fr { fr } else { en }),
                )
                .on_click(on(&self.state, move |s, cx| s.set_accent(rgb, cx)))
        }));

        div()
            .flex()
            .flex_col()
            .gap(theme::GAP_XL)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(s.t("Thème", "Theme")))
                    .child(
                        div()
                            .flex()
                            .gap(theme::GAP_MD)
                            .child(self.theme_card(s, ThemeMode::Dark, s.t("Sombre", "Dark")))
                            .child(self.theme_card(s, ThemeMode::Light, s.t("Clair", "Light"))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(label_block(
                        s.t("Couleur d'accentuation", "Accent colour"),
                        Some(s.t("Boutons principaux, sélection, sons entendus dans le simulateur.", "Main buttons, selection, sounds heard in the simulator.").into()),
                    ))
                    .child(swatches),
            )
    }

    fn shortcuts(&self, s: &AppState) -> gpui::Div {
        let row = |i: usize, sc: Shortcut| {
            let (fr, en) = sc.label();
            let capturing = s.capturing == Some(sc);
            let unset = sc.get(&s.settings.shortcuts).is_empty();
            div()
                .flex()
                .items_center()
                .gap(theme::GAP_MD)
                .py(px(6.))
                .child(div().flex_1().min_w(px(0.)).child(if s.lang() == Language::Fr { fr } else { en }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .min_w(px(96.))
                        .h(px(30.))
                        .px(px(10.))
                        .rounded(theme::RADIUS_SM)
                        .border_1()
                        .border_color(if capturing { theme::accent() } else { theme::border_strong() })
                        .bg(if capturing { theme::accent_tint() } else { theme::inset() })
                        .font_family(theme::FONT_MONO)
                        .text_size(theme::TEXT_SM)
                        .text_color(if capturing {
                            theme::accent()
                        } else if unset {
                            theme::text_faint()
                        } else {
                            theme::text()
                        })
                        .child(if capturing { s.t("Appuyez sur une touche…", "Press a key…").to_string() } else { s.shortcut_text(sc) }),
                )
                .child(
                    button(ElementId::from(("shortcut-edit", i)))
                        .h(px(30.))
                        .px(px(12.))
                        .text_size(theme::TEXT_SM)
                        .child(if capturing { s.t("Annuler", "Cancel") } else { s.t("Modifier", "Change") })
                        .on_click(on(&self.state, move |s, cx| s.capture_shortcut((!capturing).then_some(sc), cx))),
                )
        };
        let group = |title: &'static str, items: &[Shortcut]| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().pb(px(4.)).font_weight(FontWeight::SEMIBOLD).child(title))
                .children(items.iter().map(|&sc| row(Shortcut::ALL.iter().position(|&x| x == sc).unwrap_or(0), sc)))
        };
        div()
            .flex()
            .flex_col()
            .gap(theme::GAP_XL)
            .child(div().text_color(theme::text_dim()).child(s.t(
                "Cliquez sur « Modifier », puis appuyez sur la touche ou la combinaison voulue (Échap pour annuler). \
                 Les touches sans Ctrl ni Alt ne fonctionnent pas pendant la saisie dans la recherche.",
                "Click “Change”, then press the key or combination you want (Esc to cancel). \
                 Keys without Ctrl or Alt don't work while typing in the search field.",
            )))
            .child(group(s.t("Simulateur moteur", "Engine simulator"), &[Shortcut::Engine, Shortcut::Sweep, Shortcut::Throttle]))
            .child(group(s.t("Édition", "Editing"), &[Shortcut::Undo, Shortcut::Redo, Shortcut::SelectAll]))
            .child(
                div().flex().child(
                    button("shortcuts-reset")
                        .h(theme::BUTTON_H)
                        .px(px(16.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(s.t("Rétablir les raccourcis par défaut", "Restore default shortcuts"))
                        .on_click(on(&self.state, |s, cx| s.reset_shortcuts(cx))),
                ),
            )
    }

    fn game(&self, s: &AppState) -> gpui::Div {
        let chosen = s.settings.bundle_path.clone();
        let detected = if s.detected_bundles.is_empty() {
            vec![div()
                .text_color(theme::text_dim())
                .child(s.t(
                    "Jeu introuvable dans les bibliothèques Steam. Choisissez le fichier manuellement.",
                    "Game not found in the Steam libraries. Choose the file manually.",
                ))
                .into_any_element()]
        } else {
            s.detected_bundles
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let path = p.clone();
                    div()
                        .flex()
                        .items_center()
                        .gap(theme::GAP_SM)
                        .child(path_field(p.display().to_string()).flex_1())
                        .child(
                            button(ElementId::from(("use-bundle", i)))
                                .h(px(38.))
                                .px(px(16.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(s.t("Ouvrir", "Open"))
                                .on_click(on(&self.state, move |s, cx| s.use_bundle(path.clone(), cx))),
                        )
                        .into_any_element()
                })
                .collect()
        };

        div()
            .flex()
            .flex_col()
            .gap(theme::GAP_XL)
            .child(self.switch_row(
                "pref-auto-open",
                s.t("Ouvrir le bundle au démarrage", "Open the bundle on startup"),
                s.t("Charge automatiquement le fichier choisi à l'ouverture de CXSSM.", "Automatically loads the chosen file when CXSSM starts."),
                s.settings.auto_open,
                |s, cx| s.toggle_auto_open(cx),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .gap(theme::GAP_XS)
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(s.t("Installation détectée", "Detected installation")))
                            .child(div().text_color(theme::text_dim()).child(s.t("(bibliothèques Steam)", "(Steam libraries)"))),
                    )
                    .children(detected),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(s.t("Fichier choisi", "Chosen file")))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .p(theme::GAP_MD)
                            .rounded(theme::RADIUS)
                            .bg(theme::inset())
                            .border_1()
                            .border_color(theme::border())
                            .map(|d| match &chosen {
                                Some(p) => d
                                    .child(icon(Icon::Check, px(16.), theme::accent()))
                                    .child(div().min_w(px(0.)).truncate().font_family(theme::FONT_MONO).text_size(theme::TEXT_SM).child(p.display().to_string())),
                                None => d.text_color(theme::text_dim()).child(s.t(
                                    "Aucun : le premier bundle détecté est utilisé.",
                                    "None: the first detected bundle is used.",
                                )),
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(theme::GAP_SM)
                            .child(
                                button("rescan")
                                    .h(theme::BUTTON_H)
                                    .px(px(16.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(s.t("Chercher à nouveau", "Search again"))
                                    .on_click(on(&self.state, |s, cx| s.rescan(cx))),
                            )
                            .child(
                                button("choose-bundle")
                                    .h(theme::BUTTON_H)
                                    .px(px(16.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(s.t("Choisir un fichier…", "Choose a file…"))
                                    .on_click(on(&self.state, |s, cx| s.choose_bundle(cx))),
                            ),
                    ),
            )
    }

    fn fmod(&self, s: &AppState) -> gpui::Div {
        let current = match &s.fmod {
            Some(f) => div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(path_field(f.root.display().to_string()))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(theme::GAP_SM)
                        .text_color(theme::text_soft())
                        .map(|d| match &f.fsbank {
                            Some(_) => d
                                .child(icon(Icon::Check, px(15.), theme::accent()))
                                .child(s.t("Encodeur (fsbank) et simulateur disponibles", "Encoder (fsbank) and simulator available")),
                            None => d
                                .text_color(theme::danger())
                                .child(s.t("Encodeur fsbank absent de ce dossier", "fsbank encoder missing from this folder")),
                        }),
                ),
            // Introuvable : le tutoriel d'installation complet.
            None => super::fmod_setup::guide(s, &self.state),
        };
        let others: Vec<_> = s
            .fmod_detected
            .iter()
            .enumerate()
            .filter(|(_, f)| s.fmod.as_ref().is_none_or(|cur| cur.root != f.root))
            .map(|(i, f)| {
                let inst = f.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(theme::GAP_SM)
                    .child(path_field(f.root.display().to_string()).flex_1())
                    .child(
                        button(ElementId::from(("use-fmod", i)))
                            .h(px(38.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(s.t("Utiliser", "Use"))
                            .on_click(on(&self.state, move |s, cx| s.use_fmod(inst.clone(), cx))),
                    )
            })
            .collect();
        let has_others = !others.is_empty();

        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(label_block(
                s.t("Moteur FMOD", "FMOD Engine"),
                Some(
                    s.t(
                        "Dossier de l'API FMOD Studio, utilisé pour encoder les sons remplacés et faire tourner le simulateur.",
                        "FMOD Studio API folder, used to encode replaced sounds and run the simulator.",
                    )
                    .into(),
                ),
            ))
            .child(current)
            .when(has_others, |d| {
                d.child(div().pt(theme::GAP_SM).font_weight(FontWeight::SEMIBOLD).child(s.t("Autres installations détectées", "Other detected installations")))
                    .children(others)
            })
            // (Sans FMOD, le tutoriel ci-dessus a déjà ses propres boutons.)
            .when(s.fmod.is_some(), |d| d.child(
                div().child(
                    button("choose-fmod")
                        .h(theme::BUTTON_H)
                        .px(px(16.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(s.t("Choisir un dossier…", "Choose a folder…"))
                        .on_click(on(&self.state, |s, cx| s.choose_fmod_dir(cx))),
                ),
            ))
    }

    fn updates(&self, s: &AppState) -> gpui::Div {
        let u = &s.update;
        let nix = update::install_method() == InstallMethod::Nix;
        let can_install = !nix && !u.installing && !u.installed && u.latest.as_ref().is_some_and(|r| r.asset.is_some());

        let state = if u.checking {
            Some(div().text_color(theme::text_dim()).child(s.t("Vérification…", "Checking…")))
        } else if u.installed {
            Some(div().text_color(theme::accent()).child(s.t("Mise à jour installée. Redémarrez pour l'utiliser.", "Update installed. Restart to use it.")))
        } else if let Some(r) = &u.latest {
            let page = r.page.clone();
            Some(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(14.))
                    .rounded(px(10.))
                    .bg(theme::accent_tint())
                    .border_1()
                    .border_color(theme::accent())
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(s.tf(
                        format!("Nouvelle version disponible : {}", r.tag),
                        format!("New version available: {}", r.tag),
                    )))
                    .child(
                        div()
                            .id("release-page")
                            .truncate()
                            .font_family(theme::FONT_MONO)
                            .text_size(theme::TEXT_SM)
                            .text_color(theme::accent())
                            .underline()
                            .cursor_pointer()
                            .child(r.page.trim_start_matches("https://").to_string())
                            .on_click(move |_, _, cx| cx.open_url(&page)),
                    )
                    .when(nix, |d| {
                        d.child(div().text_size(theme::TEXT_SM).text_color(theme::text_dim()).child(s.t(
                            "Installation Nix : mettez à jour avec `nix profile upgrade` ou votre flake.",
                            "Nix install: update with `nix profile upgrade` or your flake.",
                        )))
                    })
                    .when(!nix && r.asset.is_none(), |d| {
                        d.child(div().text_size(theme::TEXT_SM).text_color(theme::danger()).child(s.t(
                            "Pas encore d'archive pour ce système dans cette release.",
                            "No archive for this system in this release yet.",
                        )))
                    }),
            )
        } else if u.checked {
            Some(div().text_color(theme::text_dim()).child(s.t("CXSSM est à jour.", "CXSSM is up to date.")))
        } else {
            None
        };

        div()
            .flex()
            .flex_col()
            .gap(theme::GAP_XL)
            .child(self.switch_row(
                "pref-check-updates",
                s.t("Vérifier les mises à jour au démarrage", "Check for updates on startup"),
                s.t("Interroge GitHub à chaque lancement.", "Asks GitHub on every launch."),
                s.settings.check_updates,
                |s, cx| s.toggle_check_updates(cx),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme::GAP_MD)
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap(theme::GAP_LG)
                            .child(div().text_color(theme::text_dim()).child(s.t("Version installée", "Installed version")))
                            .child(div().font_family(theme::FONT_MONO).child(update::CURRENT_VERSION)),
                    )
                    .children(state)
                    .when_some(u.error.clone(), |d, e| d.child(div().text_size(theme::TEXT_SM).text_color(theme::danger()).child(e)))
                    .child(
                        div()
                            .flex()
                            .gap(theme::GAP_SM)
                            .child(
                                button("check-update")
                                    .h(theme::BUTTON_H)
                                    .px(px(16.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(s.t("Vérifier", "Check"))
                                    .on_click(on(&self.state, |s, cx| s.check_updates(false, cx))),
                            )
                            .when(can_install, |d| {
                                d.child(
                                    primary("install-update")
                                        .h(theme::BUTTON_H)
                                        .px(px(18.))
                                        .child(s.t("Installer", "Install"))
                                        .on_click(on(&self.state, |s, cx| s.install_update(cx))),
                                )
                            })
                            .when(u.installing, |d| d.child(div().flex().items_center().text_color(theme::text_dim()).child(s.t("Installation…", "Installing…"))))
                            .when(u.installed, |d| {
                                d.child(
                                    primary("restart")
                                        .h(theme::BUTTON_H)
                                        .px(px(18.))
                                        .child(s.t("Redémarrer", "Restart"))
                                        .on_click(on(&self.state, |s, cx| s.restart(cx))),
                                )
                            }),
                    ),
            )
    }

    fn nav(&self, s: &AppState) -> impl IntoElement {
        let has_update = s.update.latest.is_some();
        let items = [
            (PrefSection::General, s.t("Général", "General")),
            (PrefSection::Appearance, s.t("Apparence", "Appearance")),
            (PrefSection::Shortcuts, s.t("Raccourcis clavier", "Keyboard shortcuts")),
            (PrefSection::Game, s.t("Jeu", "Game")),
            (PrefSection::Fmod, "FMOD"),
            (PrefSection::Updates, s.t("Mises à jour", "Updates")),
        ];
        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(2.))
            .w(px(210.))
            .p(theme::GAP_MD)
            .bg(theme::sidebar())
            .border_r_1()
            .border_color(theme::divider())
            .children(items.into_iter().enumerate().map(|(i, (sec, label))| {
                let active = s.prefs_section == sec;
                div()
                    .id(ElementId::from(("pref-nav", i)))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(theme::GAP_SM)
                    .h(px(38.))
                    .px(theme::GAP_MD)
                    .rounded(px(7.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .bg(if active { theme::accent_band() } else { gpui::transparent_black().into() })
                    .text_color(if active { theme::text_bright() } else { theme::text_icon() })
                    .when(!active, |d| d.hover(|d| d.bg(theme::hover())))
                    .child(label)
                    .when(sec == PrefSection::Updates && has_update, |d| d.child(div().size(px(8.)).rounded(px(4.)).bg(theme::accent())))
                    .on_click(on(&self.state, move |s, cx| s.set_prefs_section(sec, cx)))
            }))
    }
}

impl Render for Preferences {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let content = match s.prefs_section {
            PrefSection::General => self.general(s),
            PrefSection::Appearance => self.appearance(s),
            PrefSection::Shortcuts => self.shortcuts(s),
            PrefSection::Game => self.game(s),
            PrefSection::Fmod => self.fmod(s),
            PrefSection::Updates => self.updates(s),
        };

        let dialog = div()
            .id("prefs-dialog")
            .flex()
            .flex_col()
            .w_full()
            .max_w(px(900.))
            .max_h(relative(0.9))
            .rounded(theme::RADIUS_XL)
            .bg(theme::panel())
            .border_1()
            .border_color(theme::border())
            .shadow(vec![BoxShadow {
                color: gpui::hsla(0., 0., 0., 0.6),
                offset: gpui::point(px(0.), px(24.)),
                blur_radius: px(80.),
                spread_radius: px(0.),
            }])
            .overflow_hidden()
            // Les clics dans la fenêtre ne doivent pas la fermer.
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .gap(theme::GAP_LG)
                    .px(px(20.))
                    .py(theme::GAP_LG)
                    .border_b_1()
                    .border_color(theme::divider())
                    .child(div().text_size(theme::TEXT_TITLE).font_weight(FontWeight::SEMIBOLD).child(s.t("Préférences", "Preferences")))
                    .child(
                        button("prefs-close")
                            .px(px(16.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(icon_sm(Icon::Close, theme::text()))
                            .child(s.t("Fermer", "Close"))
                            .on_click(on(&self.state, |s, cx| s.close_preferences(cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(520.))
                    .child(self.nav(s))
                    .child(
                        div()
                            .id("prefs-content")
                            .flex_1()
                            .min_w(px(0.))
                            .px(px(24.))
                            .py(px(22.))
                            .overflow_y_scroll()
                            .child(content),
                    ),
            );

        // Fond assombri : un clic en dehors de la fenêtre la ferme.
        div()
            .id("prefs-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p(px(32.))
            .bg(theme::backdrop())
            .occlude()
            .on_click(on(&self.state, |s, cx| s.close_preferences(cx)))
            .child(dialog)
    }
}
