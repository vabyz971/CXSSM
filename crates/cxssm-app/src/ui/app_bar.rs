//! Barre d'application : logo, préférences, ouverture du bundle, export du mod.

use gpui::{div, prelude::*, px, Entity, FontWeight};

use crate::assets::{icon, icon_sm, Icon};
use crate::state::AppState;
use crate::theme;
use crate::widgets::{button, on, primary};

pub fn render(s: &AppState, state: &Entity<AppState>) -> impl IntoElement {
    let replaced = s.replaced_count();
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .gap(theme::GAP_LG)
        .px(theme::APPBAR_PAD_X)
        .py(theme::APPBAR_PAD_Y)
        .bg(theme::BAR)
        .border_b_1()
        .border_color(theme::DIVIDER)
        .child(
            div()
                .flex()
                .items_center()
                .gap(theme::GAP_MD)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(28.))
                        .rounded(px(7.))
                        .bg(theme::ACCENT)
                        .child(icon(Icon::Logo, px(18.), theme::ON_ACCENT)),
                )
                .child(div().text_size(theme::TEXT_LG).font_weight(FontWeight::SEMIBOLD).child("CXSSM")),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(theme::GAP_SM)
                .child(
                    button("prefs")
                        .child(icon_sm(Icon::Gear, theme::TEXT))
                        .child(s.t("Préférences", "Preferences"))
                        .on_click(on(state, |s, cx| s.open_preferences(None, cx))),
                )
                .child(
                    button("open")
                        .child(icon_sm(Icon::Folder, theme::TEXT))
                        .child(s.t("Ouvrir un bundle", "Open a bundle"))
                        .on_click(on(state, |s, cx| s.open_file_dialog(cx))),
                )
                .child(
                    primary("export")
                        .child(icon_sm(Icon::Download, theme::ON_ACCENT))
                        .child(s.t("Exporter le mod", "Export mod"))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .min_w(px(18.))
                                .h(px(18.))
                                .px(px(5.))
                                .rounded(px(9.))
                                .bg(theme::ON_ACCENT)
                                .text_color(theme::ACCENT)
                                .font_family(theme::FONT_MONO)
                                .text_size(theme::TEXT_XS)
                                .child(replaced.to_string()),
                        )
                        .on_click(on(state, |s, cx| s.export_mod(cx))),
                ),
        )
}
