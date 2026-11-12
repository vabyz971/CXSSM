//! Liste des sons de la sélection.
//!
//! - **Voiture** : les sons utilisés par son évènement moteur (carte du balayage),
//!   en accélération puis en décélération, avec la plage de régime où chacun est
//!   entendu, son volume et sa hauteur en direct.
//! - **Banque** : tous ses sons, avec leur spectre, leur durée et leur format.

use cxssm_core::engine::MapEntry;
use cxssm_core::{SoundRef, SoundStatus};
use gpui::{div, prelude::*, px, relative, uniform_list, App, Context, ElementId, Entity, FontWeight, SharedString, Window};

use crate::assets::{icon, Icon};
use crate::state::{AppState, Leaf, Playing};
use crate::theme;
use crate::widgets::{fmt_int, meter, mono, scrollbar, strip, RpmRange, Strip};

/// Largeurs des colonnes (bouton, nom, spectre extensible, valeurs).
const COL_PLAY: f32 = 36.;
const COL_NAME: f32 = 188.;
const COL_VALUES: f32 = 140.;

pub struct SoundList {
    state: Entity<AppState>,
}

/// Colonne de droite d'une ligne.
enum Values {
    /// Voiture : volume et hauteur en direct.
    Live { volume: f32, pitch: Option<f32> },
    /// Banque : durée et format.
    Info { duration: String, format: String },
}

struct Item {
    r: SoundRef,
    name: SharedString,
    role: SharedString,
    status: SoundStatus,
    strip: Strip,
    values: Values,
    /// Son audible en ce moment dans le simulateur.
    active: bool,
}

/// Rôle d'un son d'après le régime où il est le plus fort.
fn role(s: &AppState, m: &MapEntry, max: f32) -> &'static str {
    match m.rpm_peak / max {
        f if f < 0.15 => s.t("Ralenti", "Idle"),
        f if f < 0.4 => s.t("Bas régime", "Low RPM"),
        f if f < 0.7 => s.t("Mi-régime", "Mid RPM"),
        _ => s.t("Haut régime", "High RPM"),
    }
}

/// Son de voiture : son spectre et sa plage de régime. Quand il est entendu, le spectre
/// s'allume (couleur d'accent) sans bouger.
pub fn rpm_strip(s: &AppState, m: &MapEntry, max: f32) -> Strip {
    let layer = live_layer(s, m);
    Strip {
        spectrum: s.playback.spectrum(m.sound),
        lit: layer.is_some(),
        range: Some(RpmRange {
            band: (m.rpm_min / max, (m.rpm_max + 1.) / max),
            head: (s.engine.rpm > 0.).then(|| s.engine.rpm / max),
        }),
    }
}

/// Son de banque : son spectre, allumé s'il est sélectionné ou en lecture.
pub fn bank_strip(s: &AppState, r: SoundRef, lit: bool) -> Strip {
    Strip { spectrum: s.playback.spectrum(r), lit, range: None }
}

/// Couche FMOD de ce son **dans cette phase** : un son d'accélération n'est mis en
/// évidence que pendant l'accélération, un son de décélération quand on relâche.
fn live_layer<'a>(s: &'a AppState, m: &MapEntry) -> Option<&'a cxssm_core::engine::ActiveLayer> {
    let accelerating = s.engine.load >= 0.5;
    (accelerating == m.on_load).then(|| s.engine.layer(m.sound)).flatten()
}

impl SoundList {
    pub fn new(state: Entity<AppState>) -> Self {
        Self { state }
    }

    fn car_items(s: &AppState, entries: &[MapEntry], on_load: bool) -> Vec<Item> {
        let max = s.engine.rpm_max().max(1.);
        let Some(p) = &s.project else { return Vec::new() };
        entries
            .iter()
            .filter(|m| m.on_load == on_load)
            .map(|m| {
                let layer = live_layer(s, m);
                let looping = p.sample(m.sound).is_some_and(|(_, smp)| smp.loop_range.is_some());
                Item {
                    r: m.sound,
                    name: s.sound_name(m.sound).into(),
                    role: if looping {
                        format!("{} · {}", role(s, m, max), s.t("boucle", "loop")).into()
                    } else {
                        role(s, m, max).into()
                    },
                    status: p.status(m.sound),
                    strip: rpm_strip(s, m, max),
                    values: Values::Live { volume: layer.map_or(0., |l| l.volume), pitch: layer.map(|l| l.pitch) },
                    active: layer.is_some(),
                }
            })
            .collect()
    }

    fn bank_item(s: &AppState, r: SoundRef) -> Option<Item> {
        let p = s.project.as_ref()?;
        let (bank, smp) = p.sample(r)?;
        let channels = if smp.channels == 1 { "mono".to_string() } else { format!("{} ch", smp.channels) };
        let selected = s.is_selected(r) || matches!(s.playback.playing, Some(Playing::Original(x)) if x == r);
        Some(Item {
            r,
            name: smp.name.clone().into(),
            role: bank.name.clone().into(),
            status: p.status(r),
            strip: bank_strip(s, r, selected),
            values: Values::Info {
                duration: format!("{:.2} s", smp.duration_secs()),
                format: format!("{} · {} Hz · {channels}", bank.fsb.codec, fmt_int(smp.frequency as f32, s.lang())),
            },
            active: false,
        })
    }
}

/// Ligne d'un son (partagée par les deux modes).
fn render_item(s: &AppState, state: &Entity<AppState>, i: usize, it: Item) -> gpui::AnyElement {
    let r = it.r;
    let selected = s.is_selected(r);
    // Plusieurs sons sélectionnés : le son affiché dans le panneau garde un trait plein.
    let primary = s.sound == Some(r);
    let playing = s.playback.playing == Some(Playing::Original(r));
    let decoding = s.playback.decoding == Some(r);
    let num_color = if it.active { theme::text() } else { theme::text_faint() };

    let play = div()
        .id(ElementId::from(("sound-play", i)))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(34.))
        .rounded(px(17.))
        .border_1()
        .border_color(theme::border_strong())
        .bg(theme::button())
        .cursor_pointer()
        .hover(|d| d.bg(theme::button_hover()))
        .map(|d| {
            if decoding {
                d.text_color(theme::accent()).child("…")
            } else if playing {
                d.child(icon(Icon::Stop, px(12.), theme::accent()))
            } else {
                d.child(icon(Icon::Play, px(13.), theme::text_soft()))
            }
        })
        .on_click({
            let st = state.clone();
            move |_, _, cx| {
                cx.stop_propagation();
                st.update(cx, |s, cx| s.toggle_original(r, cx));
            }
        });

    let badge = match &it.status {
        SoundStatus::Original => None,
        SoundStatus::Replaced(_) => Some((s.t("Remplacé", "Replaced"), theme::accent())),
        SoundStatus::Mismatch(..) => Some((s.t("Incompatible", "Incompatible"), theme::danger())),
    };
    let info = div()
        .flex()
        .flex_col()
        .flex_none()
        .gap(px(2.))
        .w(px(COL_NAME))
        .min_w(px(0.))
        .child(
            mono(it.name)
                .text_size(theme::TEXT_MD)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text_bright())
                .truncate(),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(theme::TEXT_SM)
                .text_color(theme::text_dim())
                .child(div().truncate().child(it.role))
                .when_some(badge, |d, (label, color)| {
                    d.child(
                        div()
                            .flex_none()
                            .px(px(6.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(color)
                            .text_color(color)
                            .text_size(theme::TEXT_XS)
                            .child(label),
                    )
                }),
        );

    let values = match it.values {
        Values::Live { volume, pitch } => div()
            .flex()
            .flex_col()
            .gap(theme::GAP_XS)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::GAP_SM)
                    .child(meter(volume, px(5.), theme::divider(), theme::accent()))
                    .child(mono(format!("{volume:.2}")).w(px(32.)).text_right().text_size(theme::TEXT_XS).text_color(num_color)),
            )
            .child(
                mono(pitch.map_or("pitch —".to_string(), |p| format!("pitch {p:.2}")))
                    .text_size(theme::TEXT_XS)
                    .text_color(num_color),
            ),
        Values::Info { duration, format } => div()
            .flex()
            .flex_col()
            .gap(theme::GAP_XS)
            .child(mono(duration).text_size(theme::TEXT_XS).text_color(theme::text()))
            .child(div().truncate().text_size(theme::TEXT_XS).text_color(theme::text_faint()).child(format)),
    };

    let st = state.clone();
    div()
        .id(ElementId::from(("sound-row", i)))
        .flex()
        .flex_none()
        .items_center()
        .gap(theme::GAP_LG)
        .w_full()
        .h(theme::SOUND_ROW_H)
        .p(theme::GAP_SM)
        .rounded(theme::RADIUS_MD)
        .border_1()
        .border_color(if primary {
            theme::accent()
        } else if selected || it.active {
            theme::accent_edge()
        } else {
            gpui::transparent_black().into()
        })
        .bg(if selected {
            theme::accent_tint()
        } else if it.active {
            theme::row_live()
        } else {
            gpui::transparent_black().into()
        })
        // Moteur en marche : les sons qui ne jouent pas s'effacent, ceux entendus ressortent.
        .when(s.engine.running() && !it.active && !selected, |d| d.opacity(0.5))
        .cursor_pointer()
        .when(!selected, |d| d.hover(|d| d.bg(theme::hover())))
        .child(div().flex_none().w(px(COL_PLAY)).child(play))
        .child(info)
        .child(div().flex_1().min_w(px(0.)).child(strip(it.strip, theme::STRIP_H)))
        .child(div().flex_none().w(px(COL_VALUES)).child(values))
        // Clic : ce son seul ; Ctrl (Cmd) + clic : ajouter / retirer ; Maj + clic : plage.
        .on_click(move |e, _, cx| st.update(cx, |s, cx| s.click_sound(r, e.modifiers(), cx)))
        .into_any_element()
}

/// Titre d'un groupe ; `live` : la phase (accélération ou décélération) est en cours.
fn group_header(s: &AppState, title: &'static str, hint: &'static str, live: bool) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_baseline()
        .gap(px(10.))
        .px(theme::GAP_SM)
        .pt(px(10.))
        .pb(px(2.))
        .child(
            div()
                .text_size(theme::TEXT_SM)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(if live { theme::accent() } else { theme::text_soft() })
                .child(title.to_uppercase()),
        )
        .child(div().text_size(theme::TEXT_SM).text_color(theme::text_faint()).child(hint))
        .when(live, |d| {
            d.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .px(px(7.))
                    .rounded(px(9.))
                    .bg(theme::accent())
                    .text_color(theme::on_accent())
                    .text_size(theme::TEXT_XS)
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(div().size(px(6.)).rounded(px(3.)).bg(theme::on_accent()))
                    .child(s.t("EN COURS", "LIVE")),
            )
        })
}

fn message(text: impl Into<SharedString>) -> gpui::AnyElement {
    div().flex_1().flex().items_center().justify_center().p(px(24.)).text_color(theme::text_dim()).child(text.into()).into_any_element()
}

impl Render for SoundList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Spectres à calculer pour une voiture (peu de sons : toutes d'un coup).
        let wanted: Vec<SoundRef> = {
            let s = self.state.read(cx);
            match s.leaf {
                Some(Leaf::Car(i)) => s.engine.maps.get(&i).and_then(|m| m.as_ref().ok()).map_or(Vec::new(), |m| m.iter().map(|e| e.sound).collect()),
                _ => Vec::new(),
            }
        };
        if !wanted.is_empty() {
            self.state.update(cx, |s, cx| s.ensure_spectra(wanted, cx));
        }

        let state = self.state.clone();
        let s = self.state.read(cx);
        let is_car = matches!(s.leaf, Some(Leaf::Car(_)));

        // ── En-tête des colonnes ──
        let ruler = if is_car {
            let max = s.engine.rpm_max().max(1.);
            let mut ticks = Vec::new();
            let mut r = 0.;
            while r < max {
                let label = if r == 0. { "0".to_string() } else { format!("{}k", r as u32 / 1000) };
                ticks.push(
                    div().absolute().top_0().left(relative(r / max)).w(px(0.)).child(
                        div().ml(px(-15.)).w(px(30.)).text_center().font_family(theme::FONT_MONO).text_size(theme::TEXT_XS).text_color(theme::text_faint()).child(label),
                    ),
                );
                r += 2000.;
            }
            div().relative().h(px(14.)).children(ticks).into_any_element()
        } else {
            div().child(s.t("Spectre  ·  30 Hz → 16 kHz", "Spectrum  ·  30 Hz → 16 kHz")).into_any_element()
        };
        let header = div()
            .flex()
            .flex_none()
            .items_end()
            .gap(theme::GAP_LG)
            .px(theme::GAP_LG)
            .pt(theme::GAP_MD)
            .pb(theme::GAP_SM)
            .border_b_1()
            .border_color(theme::divider())
            .text_size(theme::TEXT_SM)
            .text_color(theme::text_dim())
            .child(div().flex_none().w(px(COL_PLAY)))
            .child(div().flex_none().w(px(COL_NAME)).child(s.t("Son", "Sound")))
            .child(div().flex_1().min_w(px(0.)).child(ruler))
            .child(div().flex_none().w(px(COL_VALUES)).child(if is_car {
                s.t("Volume · hauteur", "Volume · pitch")
            } else {
                s.t("Durée · format", "Length · format")
            }));

        // ── Corps ──
        let body = match s.leaf {
            Some(Leaf::Car(i)) => match s.engine.maps.get(&i) {
                None if s.engine.mapping.is_some() || s.engine.has_library() => {
                    message(s.t("Analyse des sons de cette voiture sur toute la plage de régime…", "Analysing this car's sounds over the whole RPM range…"))
                }
                None => message(s.t("FMOD indisponible : impossible d'analyser cette voiture.", "FMOD unavailable: cannot analyse this car.")),
                Some(Err(e)) => message(s.tf(format!("Analyse impossible : {e}"), format!("Analysis failed: {e}"))),
                Some(Ok(entries)) if entries.is_empty() => message(s.t("Aucun son identifié pour cette voiture.", "No sound identified for this car.")),
                Some(Ok(entries)) => {
                    let mut children = Vec::new();
                    let mut n = 0;
                    for (on_load, title, hint) in [
                        (true, s.t("Accélération", "On throttle"), s.t("actifs quand la charge monte", "heard while the load rises")),
                        (false, s.t("Décélération", "Off throttle"), s.t("actifs quand on relâche", "heard when you lift off")),
                    ] {
                        let items = SoundList::car_items(s, entries, on_load);
                        if items.is_empty() {
                            continue;
                        }
                        let live = s.engine.running() && (s.engine.load >= 0.5) == on_load;
                        children.push(group_header(s, title, hint, live).into_any_element());
                        for it in items {
                            children.push(render_item(s, &state, n, it));
                            n += 1;
                        }
                    }
                    div()
                        .id("car-sounds")
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h(px(0.))
                        .gap(px(6.))
                        .p(theme::GAP_SM)
                        .overflow_y_scroll()
                        .children(children)
                        .into_any_element()
                }
            },
            Some(Leaf::Bank(cat, bank)) => {
                let count = s.catalog.sounds(cat, bank).len();
                let list_state = state.clone();
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.))
                    .child(
                        uniform_list("bank-sounds", count, move |range, _: &mut Window, cx: &mut App| {
                            let refs: Vec<SoundRef> = list_state.read(cx).catalog.sounds(cat, bank)[range.clone()].to_vec();
                            list_state.update(cx, |s, cx| s.ensure_spectra(refs.iter().copied(), cx));
                            let s = list_state.read(cx);
                            refs.iter()
                                .zip(range)
                                .filter_map(|(&r, i)| {
                                    let it = SoundList::bank_item(s, r)?;
                                    Some(div().pb(px(6.)).child(render_item(s, &list_state, i, it)))
                                })
                                .collect()
                        })
                        .track_scroll(s.list_scroll.clone())
                        .flex_1()
                        .h_full()
                        .p(theme::GAP_SM),
                    )
                    .child(scrollbar("list-scrollbar", &s.list_scroll.0.borrow().base_handle, &state, s.scroll_dragging("list-scrollbar")))
                    .into_any_element()
            }
            None => message(""),
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .rounded(theme::RADIUS_LG)
            .bg(theme::panel())
            .border_1()
            .border_color(theme::divider())
            .overflow_hidden()
            .child(header)
            .child(body)
    }
}
