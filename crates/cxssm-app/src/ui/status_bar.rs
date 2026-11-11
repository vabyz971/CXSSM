//! Barre d'état : version et message en cours, projet ou bundle ouvert, sons remplacés et encodeur.

use cxssm_core::update::CURRENT_VERSION;
use gpui::{div, prelude::*, px, Entity};

use crate::state::AppState;
use crate::theme;
use crate::widgets::on;

pub fn render(s: &AppState, state: &Entity<AppState>) -> impl IntoElement {
    let source = match (&s.workspace, &s.project) {
        (Some(w), _) => w.file.display().to_string(),
        (None, Some(p)) => p.source.display().to_string(),
        _ => s.t("Aucun bundle chargé", "No bundle loaded").into(),
    };
    let replaced = s.replaced_count();
    let encoder = if s.encoder.fsbankcl.is_some() {
        s.t("Encodeur FMOD : OK", "FMOD encoder: OK")
    } else {
        s.t("Encodeur FMOD : introuvable (cliquez pour l'installer)", "FMOD encoder: not found (click to install it)")
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .gap(theme::GAP_LG)
        .px(theme::APPBAR_PAD_X)
        .py(theme::STATUSBAR_PAD_Y)
        .bg(theme::BAR)
        .border_t_1()
        .border_color(theme::DIVIDER)
        .text_size(theme::TEXT_SM)
        .text_color(theme::TEXT_DIM)
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .truncate()
                .child(format!("Version {CURRENT_VERSION}  ·  {}", s.status)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .truncate()
                .text_center()
                .font_family(theme::FONT_MONO)
                .text_size(theme::TEXT_XS)
                .child(source),
        )
        .child(div().id("status-fmod").flex_1().min_w(px(0.)).truncate().text_right()
            .when(s.fmod.is_none(), |d| d.text_color(theme::ACCENT).cursor_pointer().on_click(on(state, |s, cx| s.set_fmod_help(true, cx))))
            .child(s.tf(
            format!("{replaced} son(s) remplacé(s)  ·  {encoder}"),
            format!("{replaced} replaced sound(s)  ·  {encoder}"),
        )))
}
