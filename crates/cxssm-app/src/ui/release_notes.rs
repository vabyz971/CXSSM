//! Notes de version publiées sur GitHub : fenêtre « Nouveautés » (ouverte au premier
//! lancement après une mise à jour, ou depuis Préférences › À propos) et rendu du texte,
//! réutilisé dans Préférences › Mises à jour.

use cxssm_core::update;
use gpui::{div, prelude::*, px, Context, Entity, FontWeight, SharedString, Window};

use crate::assets::{icon_sm, Icon};
use crate::state::AppState;
use crate::theme;
use crate::widgets::{button, on};

/// Retire le gras / l'italique / le code Markdown d'une ligne.
fn plain(line: &str) -> String {
    line.replace("**", "").replace("__", "").replace('`', "")
}

/// Texte des notes (Markdown simple ou message de tag) : titres, listes et paragraphes.
pub fn notes_view(text: &str) -> gpui::Div {
    let mut out = div().flex().flex_col().gap(px(4.));
    for raw in text.lines() {
        let line = raw.trim_end();
        let t = line.trim_start();
        if t.is_empty() {
            out = out.child(div().h(px(6.)));
            continue;
        }
        let indent = (line.len() - t.len()) as f32;
        out = out.child(if let Some(h) = t.strip_prefix('#') {
            div().pt(px(4.)).font_weight(FontWeight::SEMIBOLD).text_color(theme::text_bright()).child(plain(h.trim_start_matches('#').trim()))
        } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            div()
                .flex()
                .gap(px(8.))
                .pl(px(4. + indent * 4.))
                .child(div().flex_none().text_color(theme::accent()).child("•"))
                .child(div().flex_1().min_w(px(0.)).text_color(theme::text_soft()).child(plain(item)))
        } else if t.ends_with(':') && t.len() < 60 {
            div().pt(px(4.)).font_weight(FontWeight::SEMIBOLD).child(plain(t))
        } else {
            // Suite d'un élément de liste (ligne indentée) ou paragraphe.
            div().pl(px(if indent > 0. { 22. } else { 0. })).text_color(theme::text_soft()).child(plain(t))
        });
    }
    out
}

/// Fenêtre « Nouveautés de la version ».
pub struct ReleaseNotes {
    state: Entity<AppState>,
}

impl ReleaseNotes {
    pub fn new(state: Entity<AppState>) -> Self {
        Self { state }
    }
}

impl Render for ReleaseNotes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let tag = s.update.notes_open.clone().unwrap_or_default();
        let page = format!("{}/releases/tag/{tag}", update::repo_url());
        let body: gpui::AnyElement = match &s.update.notes {
            None => div().text_color(theme::text_dim()).child(s.t("Chargement des notes de version…", "Loading the release notes…")).into_any_element(),
            Some(Ok(text)) => notes_view(text).into_any_element(),
            Some(Err(e)) => div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(s.t("Impossible de charger les notes de version.", "Could not load the release notes."))
                .child(div().text_size(theme::TEXT_SM).text_color(theme::danger()).child(e.clone()))
                .into_any_element(),
        };
        let title: SharedString = s.tf(format!("Nouveautés de CXSSM {tag}"), format!("What's new in CXSSM {tag}")).into();

        let dialog = div()
            .id("notes-dialog")
            .flex()
            .flex_col()
            .w_full()
            .max_w(px(680.))
            .max_h(gpui::relative(0.9))
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
                    .child(div().text_size(theme::TEXT_TITLE).font_weight(FontWeight::SEMIBOLD).child(title))
                    .child(
                        button("notes-close")
                            .px(px(16.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(icon_sm(Icon::Close, theme::text()))
                            .child(s.t("Fermer", "Close"))
                            .on_click(on(&self.state, |s, cx| s.close_release_notes(cx))),
                    ),
            )
            .child(div().id("notes-body").flex_shrink().min_h(px(0.)).overflow_y_scroll().px(px(24.)).py(px(20.)).child(body))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .gap(theme::GAP_MD)
                    .px(px(24.))
                    .py(theme::GAP_MD)
                    .border_t_1()
                    .border_color(theme::divider())
                    .child(div().text_size(theme::TEXT_SM).text_color(theme::text_faint()).child(s.t(
                        "Ces notes restent accessibles dans Préférences › À propos.",
                        "These notes stay available in Preferences › About.",
                    )))
                    .child(
                        button("notes-github")
                            .font_weight(FontWeight::MEDIUM)
                            .child(s.t("Voir sur GitHub", "View on GitHub"))
                            .on_click(move |_, _, cx| cx.open_url(&page)),
                    ),
            );

        div()
            .id("notes-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p(px(32.))
            .bg(theme::backdrop())
            .occlude()
            .on_click(on(&self.state, |s, cx| s.close_release_notes(cx)))
            .child(dialog)
    }
}
