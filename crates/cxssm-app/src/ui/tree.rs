//! Arborescence des sons : Moteur ▸ architecture (V8, V6…) ▸ voitures, puis les
//! autres catégories ▸ banques, avec une recherche par nom.

use cxssm_core::SoundCategory;
use gpui::{
    div, prelude::*, px, radians, svg, Context, ElementId, Entity, FontWeight, KeyDownEvent, MouseButton, SharedString,
    Transformation, Window,
};

use crate::assets::{icon, Icon};
use crate::i18n;
use crate::state::{AppState, Leaf, TreeKey, CATEGORIES};
use crate::theme;
use crate::widgets::{fmt_int, ghost, on, scrollbar};

pub struct Tree {
    state: Entity<AppState>,
}

/// Action d'une ligne de l'arborescence.
#[derive(Clone, Copy)]
enum Action {
    Toggle(TreeKey),
    Select(Leaf),
    None,
}

#[derive(Clone, Copy, PartialEq)]
enum Level {
    Category,
    Group,
    Item,
}

enum Row {
    Node {
        label: SharedString,
        count: String,
        level: Level,
        /// `Some(déplié)` si la ligne a des enfants.
        expanded: Option<bool>,
        selected: bool,
        disabled: bool,
        /// Contient des sons remplacés : point de couleur et son explication.
        modified: Option<SharedString>,
        action: Action,
    },
    Note(SharedString),
}

impl Tree {
    pub fn new(state: Entity<AppState>) -> Self {
        Self { state }
    }

    fn rows(s: &AppState) -> Vec<Row> {
        let lang = s.lang();
        let q = s.query.trim().to_lowercase();
        let searching = !q.is_empty();
        let matches = |name: &str| !searching || name.to_lowercase().contains(&q);
        let mut rows = Vec::new();
        if s.project.is_none() {
            return rows;
        }

        let car_tip: SharedString = s.t("Voiture modifiée : au moins un de ses sons est remplacé", "Modified car: at least one of its sounds is replaced").into();
        let bank_tip: SharedString = s.t("Contient des sons remplacés", "Contains replaced sounds").into();
        for cat in CATEGORIES {
            if s.settings.is_hidden(cat) {
                continue;
            }
            let count = s.catalog.count(cat);
            let mut banks: Vec<usize> = s.catalog.groups.get(&cat).map(|g| g.keys().copied().collect()).unwrap_or_default();
            banks.sort_by_key(|b| natural_key(&s.bank_name(*b)));
            let open = s.open.contains(&TreeKey::Category(cat));

            // Enfants de la catégorie (calculés d'abord : la recherche masque les branches vides).
            let mut children = Vec::new();
            let cars_known = cat == SoundCategory::Engine && !s.engine.events.is_empty();
            if cars_known {
                for &bank in &banks {
                    let cars: Vec<usize> = (0..s.engine.events.len())
                        .filter(|&i| s.engine.events[i].bank == bank && matches(s.engine.events[i].name()))
                        .collect();
                    if cars.is_empty() {
                        continue;
                    }
                    let group_open = searching || s.open.contains(&TreeKey::Group(cat, bank));
                    children.push(Row::Node {
                        label: s.bank_name(bank).into(),
                        count: cars.len().to_string(),
                        level: Level::Group,
                        expanded: Some(group_open),
                        selected: false,
                        disabled: false,
                        modified: s.bank_modified(bank).then(|| bank_tip.clone()),
                        action: Action::Toggle(TreeKey::Group(cat, bank)),
                    });
                    if group_open {
                        for i in cars {
                            children.push(Row::Node {
                                label: s.engine.events[i].name().to_string().into(),
                                count: String::new(),
                                level: Level::Item,
                                expanded: None,
                                selected: s.leaf == Some(Leaf::Car(i)),
                                disabled: false,
                                modified: s.car_modified(i).then(|| car_tip.clone()),
                                action: Action::Select(Leaf::Car(i)),
                            });
                        }
                    }
                }
            } else {
                if cat == SoundCategory::Engine && !searching {
                    if s.engine.loading_events {
                        children.push(Row::Note(s.t("Chargement des voitures…", "Loading cars…").into()));
                    } else if s.engine.error.is_some() {
                        children.push(Row::Note(s.t("FMOD indisponible : sons par banque", "FMOD unavailable: sounds by bank").into()));
                    }
                }
                for &bank in &banks {
                    let name = s.bank_name(bank);
                    if !matches(&name) {
                        continue;
                    }
                    children.push(Row::Node {
                        label: name.into(),
                        count: fmt_int(s.catalog.sounds(cat, bank).len() as f32, lang),
                        level: Level::Group,
                        expanded: None,
                        selected: s.leaf == Some(Leaf::Bank(cat, bank)),
                        disabled: false,
                        modified: s.bank_modified(bank).then(|| bank_tip.clone()),
                        action: Action::Select(Leaf::Bank(cat, bank)),
                    });
                }
            }

            let has_children = children.iter().any(|r| matches!(r, Row::Node { .. }));
            if searching && !has_children {
                continue;
            }
            let expanded = count > 0 && (searching || open);
            rows.push(Row::Node {
                label: i18n::category(lang, cat).into(),
                count: if searching { String::new() } else { fmt_int(count as f32, lang) },
                level: Level::Category,
                expanded: (count > 0).then_some(expanded),
                selected: false,
                disabled: count == 0,
                modified: None,
                action: if count > 0 { Action::Toggle(TreeKey::Category(cat)) } else { Action::None },
            });
            if expanded {
                rows.extend(children);
            }
        }
        if searching && rows.is_empty() {
            rows.push(Row::Note(s.t("Aucune voiture trouvée", "No car found").into()));
        }
        rows
    }

    fn render_row(&self, i: usize, row: Row) -> gpui::AnyElement {
        match row {
            Row::Note(text) => div()
                .pl(px(40.))
                .pr(theme::GAP_MD)
                .pt(px(6.))
                .pb(theme::GAP_SM)
                .text_size(theme::TEXT_SM)
                .text_color(theme::text_faint())
                .child(text)
                .into_any_element(),
            Row::Node { label, count, level, expanded, selected, disabled, modified, action } => {
                let pad = match level {
                    Level::Category => 10.,
                    Level::Group => 28.,
                    Level::Item => 52.,
                };
                let (color, weight) = if disabled {
                    (theme::text_disabled(), FontWeight::MEDIUM)
                } else if selected {
                    (theme::text_bright(), FontWeight::SEMIBOLD)
                } else {
                    match level {
                        Level::Category => (theme::text_bright(), FontWeight::SEMIBOLD),
                        Level::Group => (theme::text_soft(), FontWeight::MEDIUM),
                        Level::Item => (theme::text_row(), FontWeight::NORMAL),
                    }
                };
                let chevron = expanded.map(|open| {
                    svg()
                        .path("icons/chevron.svg")
                        .flex_none()
                        .size(px(14.))
                        .text_color(color)
                        .with_transformation(Transformation::rotate(radians(if open { std::f32::consts::FRAC_PI_2 } else { 0. })))
                });
                let st = self.state.clone();
                div()
                    .id(ElementId::from(("tree-row", i)))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(theme::GAP_SM)
                    .w_full()
                    .h(theme::ROW_H)
                    .pl(px(pad))
                    .pr(theme::GAP_MD)
                    .rounded(theme::RADIUS_SM)
                    .text_color(color)
                    .font_weight(weight)
                    .when(selected, |d| d.bg(theme::accent_select()))
                    .when(!disabled && !selected, |d| d.cursor_pointer().hover(|s| s.bg(theme::hover())))
                    .map(|d| match chevron {
                        Some(c) => d.child(c),
                        // Les feuilles sans chevron restent alignées avec leurs voisines.
                        None if level != Level::Item => d.child(div().w(px(14.)).flex_none()),
                        None => d,
                    })
                    .child(div().flex_1().min_w(px(0.)).truncate().child(label))
                    .when_some(modified, |d, tip| {
                        d.child(
                            div()
                                .id(ElementId::from(("tree-mod", i)))
                                .flex_none()
                                .size(px(7.))
                                .rounded(px(4.))
                                .bg(theme::accent())
                                .tooltip(crate::widgets::tooltip(tip)),
                        )
                    })
                    .child(div().flex_none().font_family(theme::FONT_MONO).text_size(theme::TEXT_XS).text_color(theme::text_faint()).child(count))
                    .on_click(move |_, _, cx| {
                        st.update(cx, |s, cx| match action {
                            Action::Toggle(k) => s.toggle_open(k, cx),
                            Action::Select(l) => s.select_leaf(l, cx),
                            Action::None => {}
                        })
                    })
                    .into_any_element()
            }
        }
    }

    fn render_search(&self, s: &AppState, window: &Window) -> impl IntoElement {
        let focused = s.search_focus.is_focused(window);
        let focus = s.search_focus.clone();
        let st = self.state.clone();
        let has_query = !s.query.is_empty();
        div()
            .id("tree-search")
            .track_focus(&s.search_focus)
            .relative()
            .flex()
            .items_center()
            .h(px(36.))
            .pl(px(34.))
            .pr(px(34.))
            .rounded(theme::RADIUS)
            .bg(theme::field())
            .border_1()
            .border_color(if focused { theme::accent() } else { theme::border() })
            .cursor_text()
            .child(div().absolute().left(px(11.)).top(px(10.)).child(icon(Icon::Search, px(15.), theme::text_dim())))
            .child(
                div()
                    .flex()
                    .min_w(px(0.))
                    .truncate()
                    .map(|d| {
                        if has_query {
                            d.child(s.query.clone())
                        } else {
                            d.text_color(theme::text_faint()).child(s.t("Rechercher une voiture", "Search for a car"))
                        }
                    })
                    .when(focused, |d| d.child(div().w(px(1.5)).h(px(16.)).ml(px(1.)).bg(theme::accent()))),
            )
            .when(has_query, |d| {
                d.child(
                    ghost("tree-search-clear")
                        .absolute()
                        .right(px(4.))
                        .top(px(3.))
                        .size(px(28.))
                        .child(icon(Icon::Close, px(14.), theme::text_icon()))
                        .on_click(on(&self.state, |s, cx| s.set_query(String::new(), cx))),
                )
            })
            .on_mouse_down(MouseButton::Left, move |_, window, _| window.focus(&focus))
            .on_key_down(move |e: &KeyDownEvent, _, cx| {
                let k = &e.keystroke;
                let m = &k.modifiers;
                st.update(cx, |s, cx| {
                    let mut q = s.query.clone();
                    match k.key.as_str() {
                        "backspace" => {
                            q.pop();
                        }
                        "escape" => q.clear(),
                        _ if m.control || m.platform || m.alt => return,
                        _ => match &k.key_char {
                            Some(c) if !c.chars().any(char::is_control) => q.push_str(c),
                            _ => return,
                        },
                    }
                    s.set_query(q, cx);
                });
                cx.stop_propagation();
            })
    }
}

/// Clé de tri « naturelle » : V6 < V8 < V10 < V12.
fn natural_key(name: &str) -> (String, u32, String) {
    let prefix: String = name.chars().take_while(|c| !c.is_ascii_digit()).collect::<String>().to_lowercase();
    let rest = &name[prefix.len().min(name.len())..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    (prefix, digits.parse().unwrap_or(0), rest[digits.len()..].to_lowercase())
}

impl Render for Tree {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let rows = Self::rows(s);
        let summary = if !s.query.trim().is_empty() {
            s.t("Résultats de la recherche", "Search results").to_string()
        } else if s.project.is_none() {
            s.t("Aucun bundle chargé", "No bundle loaded").to_string()
        } else if !s.engine.events.is_empty() {
            let (cars, sounds) = (fmt_int(s.engine.events.len() as f32, s.lang()), fmt_int(s.catalog.total as f32, s.lang()));
            s.tf(format!("{cars} voitures · {sounds} sons"), format!("{cars} cars · {sounds} sounds"))
        } else {
            let banks = s.project.as_ref().map_or(0, |p| p.banks.len());
            let sounds = fmt_int(s.catalog.total as f32, s.lang());
            s.tf(format!("{banks} banques · {sounds} sons"), format!("{banks} banks · {sounds} sounds"))
        };
        let dragging = s.scroll_dragging("tree-scrollbar");

        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(theme::TREE_W)
            .min_h(px(0.))
            .bg(theme::sidebar())
            .border_r_1()
            .border_color(theme::divider())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme::GAP_SM)
                    .px(px(14.))
                    .pt(px(14.))
                    .pb(px(10.))
                    .child(self.render_search(s, window))
                    .child(div().text_size(theme::TEXT_SM).text_color(theme::text_dim()).child(summary)),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.))
                    .child(
                        div()
                            .id("tree-nav")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .gap(px(1.))
                            .pl(theme::GAP_SM)
                            .pb(theme::GAP_MD)
                            .overflow_y_scroll()
                            .track_scroll(&s.tree_scroll)
                            .children(rows.into_iter().enumerate().map(|(i, r)| self.render_row(i, r))),
                    )
                    .child(scrollbar("tree-scrollbar", &s.tree_scroll, &self.state, dragging)),
            )
    }
}
