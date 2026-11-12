//! Barre d'application : logo et projet ouvert, préférences, nouveau projet / ouverture,
//! puis l'action principale : installer le mod dans le jeu (projet) ou l'exporter (bundle seul).

use cxssm_core::workspace::GameState;
use gpui::{div, prelude::*, px, Entity, FontWeight, Rgba};

use crate::assets::{icon, icon_sm, Icon};
use crate::state::{AppState, Shortcut};
use crate::theme;
use crate::widgets::{button, disabled, ghost, on, primary, tooltip};

/// Pastille : nombre de sons remplacés.
fn badge(n: usize) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .min_w(px(18.))
        .h(px(18.))
        .px(px(5.))
        .rounded(px(9.))
        .bg(theme::on_accent())
        .text_color(theme::accent())
        .font_family(theme::FONT_MONO)
        .text_size(theme::TEXT_XS)
        .child(n.to_string())
}

/// Couleur du point d'état du jeu (installé, à installer, à surveiller).
pub fn game_dot(s: &AppState) -> Rgba {
    match s.game_state {
        Some(GameState::Installed) if !s.mod_stale() => theme::accent(),
        Some(GameState::Updated | GameState::Modified | GameState::Missing) => theme::danger(),
        _ => theme::text_disabled(),
    }
}

pub fn render(s: &AppState, state: &Entity<AppState>) -> impl IntoElement {
    let replaced = s.replaced_count();
    let (undo_key, redo_key) = (s.shortcut_text(Shortcut::Undo), s.shortcut_text(Shortcut::Redo));
    let undo_tip = match s.history.undo_label() {
        Some((fr, en)) => s.tf(format!("Annuler : {fr} ({undo_key})"), format!("Undo: {en} ({undo_key})")),
        None => s.t("Rien à annuler", "Nothing to undo").to_string(),
    };
    let redo_tip = match s.history.redo_label() {
        Some((fr, en)) => s.tf(format!("Rétablir : {fr} ({redo_key})"), format!("Redo: {en} ({redo_key})")),
        None => s.t("Rien à rétablir", "Nothing to redo").to_string(),
    };
    let brand = div()
        .flex()
        .items_center()
        .gap(theme::GAP_MD)
        .min_w(px(0.))
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(28.))
                .rounded(px(7.))
                .bg(theme::accent())
                .child(icon(Icon::Logo, px(18.), theme::on_accent())),
        )
        .child(div().flex_none().text_size(theme::TEXT_LG).font_weight(FontWeight::SEMIBOLD).child("CXSSM"))
        .when_some(s.project_name(), |d, name| {
            d.child(div().text_color(theme::text_disabled()).child("/")).child(
                div()
                    .id("project-name")
                    .min_w(px(0.))
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|d| d.text_color(theme::accent()))
                    .child(name.to_string())
                    .on_click(on(state, |s, cx| s.set_project_dialog(true, cx))),
            )
        });

    let main_action = if s.workspace.is_some() {
        primary("install")
            .child(icon_sm(Icon::Download, theme::on_accent()))
            .child(s.t("Installer dans le jeu", "Install in the game"))
            .child(badge(replaced))
            .on_click(on(state, |s, cx| s.install_into_game(cx)))
    } else {
        primary("export")
            .child(icon_sm(Icon::Download, theme::on_accent()))
            .child(s.t("Exporter le mod", "Export mod"))
            .child(badge(replaced))
            .on_click(on(state, |s, cx| s.export_mod(cx)))
    };

    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .gap(theme::GAP_LG)
        .px(theme::APPBAR_PAD_X)
        .py(theme::APPBAR_PAD_Y)
        .bg(theme::bar())
        .border_b_1()
        .border_color(theme::divider())
        .child(brand)
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(theme::GAP_SM)
                .child(
                    disabled(ghost("undo").size(px(34.)), !s.history.can_undo())
                        .child(icon(Icon::Undo, px(17.), theme::text_icon()))
                        .tooltip(tooltip(undo_tip))
                        .when(s.history.can_undo(), |b| b.on_click(on(state, |s, cx| s.undo(cx)))),
                )
                .child(
                    disabled(ghost("redo").size(px(34.)), !s.history.can_redo())
                        .child(icon(Icon::Redo, px(17.), theme::text_icon()))
                        .tooltip(tooltip(redo_tip))
                        .when(s.history.can_redo(), |b| b.on_click(on(state, |s, cx| s.redo(cx)))),
                )
                .child(div().w(px(1.)).h(px(20.)).mx(px(4.)).bg(theme::divider()))
                .child(
                    button("prefs")
                        .child(icon_sm(Icon::Gear, theme::text()))
                        .child(s.t("Préférences", "Preferences"))
                        .on_click(on(state, |s, cx| s.open_preferences(None, cx))),
                )
                .child(
                    button("new-project")
                        .child(s.t("Nouveau projet", "New project"))
                        .on_click(on(state, |s, cx| s.new_project(cx))),
                )
                .child(
                    button("open")
                        .child(icon_sm(Icon::Folder, theme::text()))
                        .child(s.t("Ouvrir", "Open"))
                        .on_click(on(state, |s, cx| s.open_file_dialog(cx))),
                )
                .when(s.workspace.is_some(), |d| {
                    d.child(
                        button("project")
                            .child(div().size(px(8.)).rounded(px(4.)).bg(game_dot(s)))
                            .child(s.t("Projet", "Project"))
                            .on_click(on(state, |s, cx| s.set_project_dialog(true, cx))),
                    )
                })
                .child(main_action),
        )
}
