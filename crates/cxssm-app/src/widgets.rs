//! Petits composants réutilisables, fidèles à la maquette (`docs/design/`).

use gpui::{
    canvas, div, fill, point, prelude::*, px, size, App, PathBuilder, Bounds, Context, Div, ElementId, Entity, FontWeight, MouseButton,
    MouseDownEvent, Rgba, ScrollHandle, SharedString, Stateful, Window,
};

use std::sync::Arc;

use crate::state::{AppState, Drag};
use crate::theme;

/// Gestionnaire d'évènement qui appelle une action de l'état partagé.
pub fn on<E: 'static>(
    state: &Entity<AppState>,
    f: impl Fn(&mut AppState, &mut Context<AppState>) + 'static,
) -> impl Fn(&E, &mut Window, &mut App) + 'static {
    let state = state.clone();
    move |_, _, cx| state.update(cx, |s, cx| f(s, cx))
}

/// Bouton secondaire (`.btn2`) : fond sombre, bordure, survol plus clair.
pub fn button(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id.into())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(theme::GAP_SM)
        .h(theme::CONTROL_H)
        .px(px(14.))
        .rounded(theme::RADIUS)
        .bg(theme::BUTTON)
        .border_1()
        .border_color(theme::BORDER)
        .cursor_pointer()
        .hover(|s| s.bg(theme::BUTTON_HOVER).border_color(theme::BORDER_HOVER))
}

/// Bouton principal : fond accent, texte foncé, demi-gras.
pub fn primary(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id.into())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(theme::GAP_SM)
        .h(theme::CONTROL_H)
        .px(px(16.))
        .rounded(theme::RADIUS)
        .bg(theme::ACCENT)
        .text_color(theme::ON_ACCENT)
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(|s| s.opacity(0.9))
}

/// Bouton sans fond (icônes, onglets) : seul le survol est marqué.
pub fn ghost(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id.into())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(theme::RADIUS_SM)
        .cursor_pointer()
        .hover(|s| s.bg(theme::HOVER))
}

/// Désactive visuellement un bouton (les clics sont ignorés par l'appelant).
pub fn disabled(b: Stateful<Div>, off: bool) -> Stateful<Div> {
    if off {
        b.opacity(0.45).cursor_default()
    } else {
        b
    }
}

/// Interrupteur 42×24.
pub fn switch(id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
    div()
        .id(id.into())
        .relative()
        .flex_none()
        .w(px(42.))
        .h(px(24.))
        .rounded(px(12.))
        .cursor_pointer()
        .bg(if on { theme::ACCENT } else { theme::SWITCH_OFF })
        .child(
            div()
                .absolute()
                .top(px(3.))
                .left(px(if on { 21. } else { 3. }))
                .size(px(18.))
                .rounded(px(9.))
                .bg(if on { theme::ON_ACCENT } else { theme::TEXT }),
        )
}

/// Groupe de boutons segmentés (choix exclusif).
pub fn segmented() -> Div {
    div()
        .flex()
        .flex_none()
        .gap(px(3.))
        .p(px(3.))
        .rounded(theme::RADIUS_MD)
        .bg(theme::INSET)
        .border_1()
        .border_color(theme::BORDER)
}

pub fn segment(id: impl Into<ElementId>, label: impl Into<SharedString>, active: bool) -> Stateful<Div> {
    div()
        .id(id.into())
        .flex()
        .items_center()
        .h(px(30.))
        .px(px(14.))
        .rounded(theme::RADIUS_SM)
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .bg(if active { theme::ACCENT } else { gpui::transparent_black().into() })
        .text_color(if active { theme::ON_ACCENT } else { theme::TEXT_SOFT })
        .when(!active, |d| d.hover(|s| s.bg(theme::HOVER)))
        .child(label.into())
}

/// Texte en police à chasse fixe (nombres, chemins).
pub fn mono(text: impl Into<SharedString>) -> Div {
    div().font_family(theme::FONT_MONO).child(text.into())
}

/// Champ en lecture seule affichant un chemin.
pub fn path_field(text: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .items_center()
        .min_w(px(0.))
        .h(px(38.))
        .px(theme::GAP_MD)
        .rounded(theme::RADIUS)
        .bg(theme::FIELD)
        .border_1()
        .border_color(theme::BORDER)
        .font_family(theme::FONT_MONO)
        .text_size(theme::TEXT_SM)
        .child(div().min_w(px(0.)).truncate().child(text.into()))
}

/// Libellé + description (lignes des préférences).
pub fn label_block(title: impl Into<SharedString>, hint: Option<SharedString>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .min_w(px(0.))
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title.into()))
        .when_some(hint, |d, h| d.child(div().text_color(theme::TEXT_DIM).child(h)))
}

/// Barre de défilement verticale pour une zone suivie par `handle`
/// (clic pour sauter, glisser pour faire défiler).
/// `dragging` : vrai pendant le glisser de cette barre (cf. `AppState::scroll_dragging`).
pub fn scrollbar(id: &'static str, handle: &ScrollHandle, state: &Entity<AppState>, dragging: bool) -> impl IntoElement {
    let viewport = handle.bounds().size.height;
    let max = handle.max_offset().height;
    let (thumb, pos) = if viewport > px(0.) && max > px(0.) {
        let size = (viewport / (viewport + max)).clamp(0.06, 1.);
        let pos = ((-handle.offset().y) / max).clamp(0., 1.) * (1. - size);
        (size, pos)
    } else {
        (1., 0.)
    };
    let visible = thumb < 1.;
    let st = state.clone();
    let h = handle.clone();
    div()
        .id(id)
        .flex_none()
        .w(px(10.))
        .h_full()
        .relative()
        .when(visible, |d| {
            d.child(
                div()
                    .absolute()
                    .left(px(2.))
                    .w(px(6.))
                    .top(gpui::relative(pos))
                    .h(gpui::relative(thumb))
                    .rounded(px(3.))
                    .bg(if dragging { theme::ACCENT } else { theme::BORDER_STRONG })
                    .hover(|s| s.bg(theme::BORDER_HOVER)),
            )
            .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                st.update(cx, |s, cx| {
                    AppState::scroll_to_mouse(&h, e.position.y);
                    s.drag = Some(Drag::Scroll(id, h.clone()));
                    cx.notify();
                });
            })
        })
}

/// Plage de régime d'un son, dessinée sous son spectre (axe = régime, comme la règle
/// de l'en-tête de la liste).
#[derive(Clone)]
pub struct RpmRange {
    /// Plage où le son est entendu, en fraction du régime maximal.
    pub band: (f32, f32),
    /// Régime actuel, en fraction du régime maximal.
    pub head: Option<f32>,
}

/// Piste d'un son : son **spectre de fréquences** en miroir et fixe (fréquences en échelle
/// logarithmique de 30 Hz à 16 kHz) et, pour un son moteur, sa plage de régime.
#[derive(Clone, Default)]
pub struct Strip {
    /// Spectre 0..1 (`None` tant qu'il est en calcul).
    pub spectrum: Option<Arc<Vec<f32>>>,
    /// Son mis en évidence (entendu dans le simulateur, sélectionné, en lecture).
    pub lit: bool,
    pub range: Option<RpmRange>,
}

/// Octaves couvertes par l'axe des fréquences (30 Hz → 16 kHz).
const SPECTRUM_OCTAVES: f32 = 9.06;

/// Dessine une [`Strip`] dans une piste arrondie de hauteur `height`.
pub fn strip(s: Strip, height: gpui::Pixels) -> Div {
    div()
        .relative()
        .w_full()
        .h(height)
        .rounded(theme::RADIUS_SM)
        .bg(theme::STRIP)
        .border_1()
        .border_color(if s.lit { theme::ACCENT_EDGE } else { theme::DIVIDER })
        .overflow_hidden()
        .child(canvas(|_, _, _| (), move |b, _, window, _| paint_strip(&s, b, window)).absolute().size_full())
}

fn paint_strip(s: &Strip, b: Bounds<gpui::Pixels>, window: &mut Window) {
    let range_h = if s.range.is_some() { px(8.) } else { px(0.) };
    let (x0, w) = (b.origin.x, b.size.width);
    let (top, bottom) = (b.origin.y + px(3.), b.origin.y + b.size.height - range_h - px(1.));
    let quad = |window: &mut Window, x: gpui::Pixels, y: gpui::Pixels, bw: gpui::Pixels, bh: gpui::Pixels, c: Rgba| {
        window.paint_quad(fill(Bounds::new(point(x, y), size(bw, bh)), c));
    };

    // Repères 100 Hz, 1 kHz et 10 kHz.
    for hz in [100f32, 1_000., 10_000.] {
        let f = (hz / 30.).log2() / SPECTRUM_OCTAVES;
        quad(window, x0 + w * f, top, px(1.), bottom - top, theme::DIVIDER);
    }

    // Ligne centrale du miroir.
    let (cy, half) = ((top + bottom) / 2., (bottom - top) / 2.);
    quad(window, x0, cy, w, px(1.), theme::DIVIDER);

    if let Some(spec) = s.spectrum.as_ref().filter(|v| v.len() > 1) {
        let n = spec.len();
        // Spectre en miroir, fixe : la même courbe vers le haut et vers le bas.
        let x = |i: usize| x0 + w * (i as f32 / (n - 1) as f32);
        let up: Vec<_> = (0..n).map(|i| point(x(i), cy - half * spec[i].clamp(0., 1.) * 0.95)).collect();
        let down: Vec<_> = up.iter().map(|p| point(p.x, cy + (cy - p.y))).collect();
        // Courbe lissée : segments de Bézier passant par le milieu de chaque paire de points.
        let trace = |pb: &mut PathBuilder, pts: &[gpui::Point<gpui::Pixels>]| {
            for k in 1..pts.len() - 1 {
                let mid = point((pts[k].x + pts[k + 1].x) / 2., (pts[k].y + pts[k + 1].y) / 2.);
                pb.curve_to(mid, pts[k]);
            }
            pb.line_to(pts[pts.len() - 1]);
        };
        let mut area = PathBuilder::fill();
        area.move_to(point(up[0].x, cy));
        area.line_to(up[0]);
        trace(&mut area, &up);
        area.line_to(point(up[n - 1].x, cy));
        let rev: Vec<_> = down.iter().rev().copied().collect();
        area.line_to(rev[0]);
        trace(&mut area, &rev);
        area.close();
        let (fill_c, line_c) = if s.lit { (theme::SPECTRUM_LIT, theme::ACCENT) } else { (theme::SPECTRUM_IDLE, theme::SPECTRUM_LINE) };
        if let Ok(p) = area.build() {
            window.paint_path(p, fill_c);
        }
        for pts in [&up, &down] {
            let mut line = PathBuilder::stroke(px(1.2));
            line.move_to(pts[0]);
            trace(&mut line, pts);
            if let Ok(p) = line.build() {
                window.paint_path(p, line_c);
            }
        }
    }

    if let Some(r) = &s.range {
        let y = b.origin.y + b.size.height - range_h;
        quad(window, x0, y, w, range_h, theme::TRACK);
        let (a, z) = (r.band.0.clamp(0., 1.), r.band.1.clamp(0., 1.));
        quad(window, x0 + w * a, y, w * (z - a), range_h, if s.lit { theme::ACCENT } else { theme::ACCENT_DIM });
        if let Some(head) = r.head {
            quad(window, x0 + w * head.clamp(0., 1.) - px(1.), y - px(3.), px(2.), range_h + px(3.), theme::TEXT_BRIGHT);
        }
    }
}

/// Jauge horizontale pleine (volume…).
pub fn meter(value: f32, height: gpui::Pixels, track: Rgba, color: Rgba) -> Div {
    div()
        .flex_1()
        .h(height)
        .rounded(height / 2.)
        .bg(track)
        .child(div().h_full().rounded(height / 2.).w(gpui::relative(value.clamp(0., 1.))).bg(color))
}

/// Nombre entier avec séparateur de milliers (espace fine en français, virgule en anglais).
pub fn fmt_int(n: f32, lang: cxssm_core::Language) -> String {
    let digits = format!("{}", n.round().max(0.) as u64);
    let sep = match lang {
        cxssm_core::Language::Fr => '\u{202F}',
        cxssm_core::Language::En => ',',
    };
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// Curseur horizontal réglé au glisser (voir [`Drag::Slider`]) : `value` dans 0..1.
/// `centered` : le remplissage part du milieu (réglage ± autour de zéro).
pub fn slider(
    id: impl Into<ElementId>,
    track: &std::rc::Rc<std::cell::Cell<Bounds<gpui::Pixels>>>,
    value: f32,
    centered: bool,
    on_down: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let v = value.clamp(0., 1.);
    let (from, to) = if centered { (v.min(0.5), v.max(0.5)) } else { (0., v) };
    let track = track.clone();
    div()
        .id(id.into())
        .relative()
        .w_full()
        .h(px(18.))
        .cursor_pointer()
        .child(div().absolute().left_0().right_0().top(px(6.)).h(px(6.)).rounded(px(3.)).bg(theme::TRACK))
        .child(
            div()
                .absolute()
                .top(px(6.))
                .h(px(6.))
                .left(gpui::relative(from))
                .w(gpui::relative(to - from))
                .rounded(px(3.))
                .bg(theme::ACCENT),
        )
        .when(centered, |d| d.child(div().absolute().top(px(3.)).h(px(12.)).w(px(1.)).left(gpui::relative(0.5)).bg(theme::BORDER_HOVER)))
        .child(
            div().absolute().top(px(2.)).left(gpui::relative(v)).w(px(0.)).child(
                div().ml(px(-7.)).size(px(14.)).rounded(px(7.)).bg(theme::TEXT_BRIGHT).border_2().border_color(theme::ACCENT),
            ),
        )
        .child(canvas(move |b, _, _| track.set(b), |_, _, _, _| {}).absolute().size_full())
        .on_mouse_down(MouseButton::Left, on_down)
}
