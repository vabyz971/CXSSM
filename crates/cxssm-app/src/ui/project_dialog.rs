//! Fenêtre **Projet** : contenu du dossier `.cxssm`, état du fichier du jeu, et les
//! actions installer / restaurer / mettre à jour l'original.

use cxssm_core::workspace::{Folder, GameState};
use gpui::{div, prelude::*, px, Context, Entity, FontWeight, Rgba, SharedString, Window};

use crate::assets::{icon_sm, Icon};
use crate::state::AppState;
use crate::theme;
use crate::widgets::{button, disabled, on, path_field, primary};

pub struct ProjectDialog {
    state: Entity<AppState>,
}

impl ProjectDialog {
    pub fn new(state: Entity<AppState>) -> Self {
        Self { state }
    }
}

fn section(title: &'static str) -> gpui::Div {
    div().flex().flex_col().gap(px(10.)).child(div().font_weight(FontWeight::SEMIBOLD).child(title))
}

/// Ligne « libellé · valeur » du contenu du projet.
fn row(label: &'static str, value: impl Into<SharedString>, color: Rgba) -> impl IntoElement {
    div()
        .flex()
        .gap(theme::GAP_MD)
        .child(div().flex_none().w(px(150.)).text_color(theme::text_dim()).child(label))
        .child(div().flex_1().min_w(px(0.)).truncate().text_color(color).child(value.into()))
}

/// Description de l'état du fichier du jeu : (texte, couleur, explication).
fn game_text(s: &AppState) -> (&'static str, Rgba, Option<&'static str>) {
    match s.game_state {
        None => (s.t("Vérification…", "Checking…"), theme::text_dim(), None),
        Some(GameState::Unknown) => (
            s.t("Aucun fichier du jeu associé", "No game file set"),
            theme::text_dim(),
            Some(s.t("Choisissez fmod_assets_all.bundle dans le dossier du jeu.", "Choose fmod_assets_all.bundle in the game folder.")),
        ),
        Some(GameState::Missing) => (
            s.t("Fichier du jeu introuvable", "Game file not found"),
            theme::danger(),
            Some(s.t("Le jeu a été déplacé ou désinstallé : choisissez à nouveau son fichier.", "The game was moved or uninstalled: choose its file again.")),
        ),
        Some(GameState::Original) => (s.t("Fichier d'origine — mod non installé", "Original file — mod not installed"), theme::text_soft(), None),
        Some(GameState::Installed) if s.mod_stale() => (
            s.t("Mod installé, mais des changements ne le sont pas encore", "Mod installed, but some changes are not yet"),
            theme::accent(),
            Some(s.t("Cliquez sur « Installer dans le jeu » pour les appliquer.", "Click “Install in the game” to apply them.")),
        ),
        Some(GameState::Installed) => (s.t("Mod installé", "Mod installed"), theme::accent(), None),
        Some(GameState::Updated) => (
            s.t("Le jeu a été mis à jour", "The game was updated"),
            theme::danger(),
            Some(s.t(
                "Son fichier d'origine a changé : mettez à jour l'original du projet, puis réinstallez le mod (vos sons sont conservés).",
                "Its original file changed: update the project's original, then reinstall the mod (your sounds are kept).",
            )),
        ),
        Some(GameState::Modified) => (
            s.t("Fichier modifié (ancienne version du mod ou autre outil)", "Modified file (older mod build or another tool)"),
            theme::danger(),
            Some(s.t(
                "Installez le mod pour le remplacer, ou restaurez le fichier d'origine.",
                "Install the mod to replace it, or restore the original file.",
            )),
        ),
    }
}

impl Render for ProjectDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let state = &self.state;
        let Some(w) = &s.workspace else { return div().into_any_element() };
        let file_name = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();

        // ── contenu du dossier ──
        let imported = w.manifest.sounds.iter().filter(|e| e.file.starts_with(Folder::Imported.rel())).count();
        let edited = w.manifest.sounds.iter().filter(|e| e.file.starts_with(Folder::Edited.rel())).count();
        let mod_text = if !w.mod_bundle().is_file() {
            (s.t("pas encore construit", "not built yet"), theme::text_dim())
        } else if s.mod_stale() {
            (s.t("à reconstruire (sons modifiés depuis)", "needs rebuilding (sounds changed since)"), theme::accent())
        } else {
            (s.t("à jour", "up to date"), theme::text_soft())
        };
        let recordings = [&w.manifest.recordings.acc, &w.manifest.recordings.dec]
            .iter()
            .filter_map(|r| r.as_deref().map(file_name))
            .collect::<Vec<_>>()
            .join(", ");
        let content = section(s.t("Dossier du projet", "Project folder"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::GAP_SM)
                    .child(path_field(w.dir.display().to_string()).flex_1())
                    .child(
                        button("project-reveal")
                            .h(px(38.))
                            .child(icon_sm(Icon::Folder, theme::text()))
                            .child(s.t("Ouvrir", "Open"))
                            .on_click(on(state, |s, cx| s.reveal_project(cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .p(theme::GAP_MD)
                    .rounded(theme::RADIUS)
                    .bg(theme::inset())
                    .border_1()
                    .border_color(theme::border())
                    .child(row(s.t("Original", "Original"), "original/fmod_assets_all.bundle", theme::text_soft()))
                    .child(row(s.t("Mod (mod/)", "Mod (mod/)"), mod_text.0, mod_text.1))
                    .child(row(
                        s.t("Sons remplacés", "Replaced sounds"),
                        s.tf(
                            format!("{} · {imported} importé(s), {edited} édité(s)", w.manifest.sounds.len()),
                            format!("{} · {imported} imported, {edited} edited", w.manifest.sounds.len()),
                        ),
                        theme::text_soft(),
                    ))
                    .child(row(
                        s.t("Enregistrements", "Recordings"),
                        if recordings.is_empty() { s.t("aucun", "none").to_string() } else { recordings },
                        theme::text_soft(),
                    )),
            );

        // ── jeu ──
        let (text, color, hint) = game_text(s);
        let st = s.game_state;
        let busy = s.busy || st.is_none();
        let can_install = !busy && w.manifest.game_bundle.is_some() && st != Some(GameState::Updated) && s.replaced_count() > 0;
        let can_restore = !busy && matches!(st, Some(GameState::Installed | GameState::Modified | GameState::Missing));
        let game_path = w.manifest.game_bundle.as_ref().map_or_else(|| s.t("non défini", "not set").to_string(), |p| p.display().to_string());
        let game = section(s.t("Jeu", "Game"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::GAP_SM)
                    .child(path_field(game_path).flex_1())
                    .child(
                        button("project-game")
                            .h(px(38.))
                            .child(s.t("Changer…", "Change…"))
                            .on_click(on(state, |s, cx| s.choose_project_game_bundle(cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme::GAP_SM)
                            .child(div().size(px(8.)).rounded(px(4.)).bg(super::app_bar::game_dot(s)))
                            .child(div().font_weight(FontWeight::MEDIUM).text_color(color).child(text)),
                    )
                    .when_some(hint, |d, h| d.child(div().text_size(theme::TEXT_SM).text_color(theme::text_dim()).child(h))),
            )
            .child(
                div()
                    .flex()
                    .gap(theme::GAP_SM)
                    .when(st == Some(GameState::Updated), |d| {
                        d.child(
                            disabled(primary("project-update"), busy)
                                .h(theme::BUTTON_H)
                                .child(s.t("Mettre à jour l'original du projet", "Update the project's original"))
                                .when(!busy, |b| b.on_click(on(state, |s, cx| s.update_original(cx)))),
                        )
                    })
                    .when(st != Some(GameState::Updated), |d| {
                        d.child(
                            disabled(primary("project-install"), !can_install)
                                .h(theme::BUTTON_H)
                                .child(icon_sm(Icon::Download, theme::on_accent()))
                                .child(s.t("Installer dans le jeu", "Install in the game"))
                                .when(can_install, |b| b.on_click(on(state, |s, cx| s.install_into_game(cx)))),
                        )
                    })
                    .child(
                        disabled(button("project-restore"), !can_restore)
                            .h(theme::BUTTON_H)
                            .font_weight(FontWeight::MEDIUM)
                            .child(s.t("Restaurer le fichier d'origine", "Restore the original file"))
                            .when(can_restore, |b| b.on_click(on(state, |s, cx| s.restore_game(cx)))),
                    )
            )
            .child(
                div()
                    .flex()
                    .gap(theme::GAP_SM)
                    .child(
                        disabled(button("project-build"), busy || s.replaced_count() == 0)
                            .h(theme::BUTTON_H)
                            .font_weight(FontWeight::MEDIUM)
                            .child(s.t("Construire seulement", "Build only"))
                            .when(!busy, |b| b.on_click(on(state, |s, cx| s.build_mod(false, cx)))),
                    )
                    .child(
                        disabled(button("project-check"), busy)
                            .h(theme::BUTTON_H)
                            .font_weight(FontWeight::MEDIUM)
                            .child(s.t("Vérifier à nouveau", "Check again"))
                            .when(!busy, |b| b.on_click(on(state, |s, cx| s.check_game_state(cx)))),
                    ),
            );

        let note = div()
            .p(theme::GAP_MD)
            .rounded(theme::RADIUS)
            .bg(theme::accent_tint())
            .border_1()
            .border_color(theme::accent_edge())
            .text_size(theme::TEXT_SM)
            .child(s.t(
                "Le Steam Workshop ne peut pas remplacer les sons : le jeu charge fmod_assets_all.bundle sans passer par \
                 ses mods. CXSSM installe donc le mod à la place du fichier du jeu. Steam remet le fichier d'origine à \
                 chaque mise à jour ou vérification des fichiers : réinstallez le mod ensuite.",
                "The Steam Workshop cannot replace sounds: the game loads fmod_assets_all.bundle without going through \
                 its mods. CXSSM therefore installs the mod in place of the game file. Steam puts the original file back \
                 on every update or file verification: reinstall the mod afterwards.",
            ));

        let dialog = div()
            .id("project-dialog")
            .flex()
            .flex_col()
            .w(px(680.))
            .rounded(theme::RADIUS_XL)
            .bg(theme::panel())
            .border_1()
            .border_color(theme::border())
            .overflow_hidden()
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
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .min_w(px(0.))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(theme::TEXT_TITLE)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(s.tf(format!("Projet · {}", w.manifest.name), format!("Project · {}", w.manifest.name))),
                            )
                            .child(div().text_size(theme::TEXT_SM).text_color(theme::text_faint()).child(s.t(
                                "Enregistré automatiquement à chaque changement",
                                "Saved automatically on every change",
                            ))),
                    )
                    .child(
                        button("project-close")
                            .px(px(16.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(icon_sm(Icon::Close, theme::text()))
                            .child(s.t("Fermer", "Close"))
                            .on_click(on(state, |s, cx| s.set_project_dialog(false, cx))),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(24.))
                    .py(px(22.))
                    .child(div().flex().flex_col().gap(px(22.)).child(content).child(game).child(note)),
            );

        div()
            .id("project-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p(px(32.))
            .bg(theme::backdrop())
            .occlude()
            .on_click(on(state, |s, cx| s.set_project_dialog(false, cx)))
            .child(dialog)
            .into_any_element()
    }
}
