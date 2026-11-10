//! Petits composants réutilisables.

use gpui::{div, prelude::*, px, Div, FontWeight, Rgba, SharedString, Stateful};

use crate::theme;

/// Bouton « jeu » : majuscules, fond accent, coin biseauté simulé par la bordure gauche.
pub fn primary_button(id: &'static str, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px_5()
        .py_2()
        .bg(theme::accent())
        .text_color(theme::accent_text())
        .font_weight(FontWeight::BOLD)
        .text_sm()
        .cursor_pointer()
        .hover(|s| s.opacity(0.85))
        .child(label.into())
}

pub fn ghost_button(id: &'static str, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px_4()
        .py_2()
        .border_1()
        .border_color(theme::border())
        .text_color(theme::text())
        .text_sm()
        .cursor_pointer()
        .hover(|s| s.bg(theme::panel_hi()))
        .child(label.into())
}

pub fn badge(label: impl Into<SharedString>, color: Rgba) -> Div {
    div()
        .px_2()
        .py(px(1.))
        .border_1()
        .border_color(color)
        .text_color(color)
        .text_xs()
        .child(label.into())
}

/// Titre de section en petites capitales.
pub fn section_title(label: impl Into<SharedString>) -> Div {
    div()
        .text_xs()
        .font_weight(FontWeight::BOLD)
        .text_color(theme::text_dim())
        .child(label.into())
}

/// Ligne « clé : valeur » du panneau de détails.
pub fn spec_row(key: impl Into<SharedString>, value: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .justify_between()
        .py_1()
        .border_b_1()
        .border_color(theme::border())
        .child(div().text_sm().text_color(theme::text_dim()).child(key.into()))
        .child(div().text_sm().text_color(theme::text()).child(value.into()))
}
