//! Simulateur de régime : compte-tours, charge, rupteur, accélérateur et balayage.
//! Le moteur est joué par FMOD Studio avec les vrais évènements du jeu.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    canvas, div, prelude::*, px, relative, Bounds, Context, Entity, FontWeight, MouseButton, MouseDownEvent, Pixels,
    SharedString, Window,
};

use crate::assets::{icon, Icon};
use crate::state::engine::LIMITER_STEP;
use crate::state::{AppState, Drag, Shortcut};
use crate::theme;
use crate::widgets::{button, fmt_int, ghost, mono};

/// Largeur du bloc du régime (chiffres à chasse fixe + « tr/min »).
const READOUT_W: f32 = 225.;

pub struct RpmSimulator {
    state: Entity<AppState>,
}

impl RpmSimulator {
    pub fn new(state: Entity<AppState>) -> Self {
        Self { state }
    }

    fn card() -> gpui::Div {
        div()
            .flex()
            .flex_none()
            .gap(px(28.))
            .px(px(20.))
            .py(theme::GAP_LG)
            .rounded(theme::RADIUS_LG)
            .bg(theme::panel())
            .border_1()
            .border_color(theme::divider())
    }

    /// Jauge réglable à la souris : ses dimensions sont mémorisées pour le glisser.
    fn gauge(
        &self,
        id: &'static str,
        track: &Rc<Cell<Bounds<Pixels>>>,
        drag: Drag,
        height: Pixels,
        fill: gpui::Rgba,
        value: f32,
    ) -> gpui::Stateful<gpui::Div> {
        let track = track.clone();
        let st = self.state.clone();
        div()
            .id(id)
            .relative()
            .w_full()
            .h(height)
            .rounded(height / 2.)
            .bg(theme::track())
            .overflow_hidden()
            .cursor_pointer()
            .child(div().h_full().rounded(height / 2.).w(relative(value.clamp(0., 1.))).bg(fill))
            .child(canvas(move |b, _, _| track.set(b), |_, _, _, _| {}).absolute().size_full())
            .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, _, cx| {
                st.update(cx, |s, cx| {
                    s.drag = Some(drag.clone());
                    s.drag_to(e.position, cx);
                })
            })
    }
}

/// Graduation positionnée en fraction de la largeur, centrée sur sa position.
fn tick(left: f32, label: SharedString) -> impl IntoElement {
    div().absolute().top_0().left(relative(left)).w(px(0.)).child(
        div()
            .ml(px(-20.))
            .w(px(40.))
            .text_center()
            .font_family(theme::FONT_MONO)
            .text_size(theme::TEXT_XS)
            .text_color(theme::text_faint())
            .child(label),
    )
}

impl Render for RpmSimulator {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let e = &s.engine;
        let lang = s.lang();

        if let Some(err) = &e.error {
            let missing = s.fmod.is_none();
            return Self::card()
                .items_center()
                .justify_between()
                .child(div().flex_1().min_w(px(0.)).text_color(theme::text_dim()).child(err.clone()))
                .when(missing, |d| {
                    d.child(
                        crate::widgets::primary("fmod-howto")
                            .h(theme::BUTTON_H)
                            .child(s.t("Comment installer FMOD ?", "How to install FMOD?"))
                            .on_click(crate::widgets::on(&self.state, |s, cx| s.set_fmod_help(true, cx))),
                    )
                })
                .into_any_element();
        }

        let max = e.rpm_max().max(1.);
        let limiter_on = s.settings.rpm_limiter;
        let limiting = e.limiting(limiter_on);
        let running = e.running();
        let load_pct = (e.load * 100.).round();

        // ── Régime ──
        let readout = div()
            .flex()
            .flex_col()
            .justify_center()
            .flex_none()
            // Largeur fixe, prévue pour « 10 500 » : les colonnes voisines ne bougent pas
            // quand le nombre de chiffres du régime change.
            .w(px(READOUT_W))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(theme::GAP_SM)
                    .child(
                        mono(fmt_int(e.rpm, lang))
                            .text_size(theme::TEXT_RPM)
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_height(relative(1.))
                            .text_color(if limiting { theme::accent() } else { theme::text() }),
                    )
                    .child(div().text_color(theme::text_dim()).child(s.t("tr/min", "rpm"))),
            )
            .child(
                div()
                    .flex()
                    .gap(theme::GAP_XS)
                    .mt(theme::GAP_SM)
                    .text_size(theme::TEXT_SM)
                    .text_color(theme::text_dim())
                    .child(s.t("Charge", "Load"))
                    .child(mono(format!("{load_pct:.0} %")).text_color(theme::text())),
            );

        // ── Compte-tours, graduations, charge et rupteur ──
        let mut ticks = Vec::new();
        let mut r = 0.;
        while r <= max {
            let label: SharedString = if (r as u32).is_multiple_of(2000) {
                if r == 0. { "0".into() } else { format!("{}k", r as u32 / 1000).into() }
            } else {
                "·".into()
            };
            ticks.push(tick(r / max, label));
            r += 1000.;
        }
        let tach = self
            .gauge("tach", &e.rpm_track, Drag::Rpm, px(14.), theme::accent(), e.rpm / max)
            .when(limiter_on, |d| {
                d.child(
                    div()
                        .absolute()
                        .top_0()
                        .h_full()
                        .w(px(2.))
                        .left(relative((e.limiter / max).clamp(0., 1.)))
                        .bg(theme::text_bright()),
                )
            });
        let limiter = if limiter_on {
            div()
                .flex()
                .items_center()
                .gap(theme::GAP_XS)
                .child(s.t("Limiteur", "Limiter"))
                .child(
                    ghost("limiter-down")
                        .size(px(20.))
                        .text_color(theme::text_soft())
                        .child("−")
                        .on_click(crate::widgets::on(&self.state, |s, cx| s.adjust_limiter(-LIMITER_STEP, cx))),
                )
                .child(mono(fmt_int(e.limiter, lang)).text_color(theme::text()))
                .child(
                    ghost("limiter-up")
                        .size(px(20.))
                        .text_color(theme::text_soft())
                        .child("+")
                        .on_click(crate::widgets::on(&self.state, |s, cx| s.adjust_limiter(LIMITER_STEP, cx))),
                )
        } else {
            div().child(s.t("Limiteur désactivé", "Limiter off"))
        };
        let gauges = div()
            .flex()
            .flex_col()
            .justify_center()
            .gap(theme::GAP_SM)
            .flex_1()
            .min_w(px(0.))
            .overflow_hidden()
            .child(tach)
            .child(div().relative().h(px(16.)).mx(px(6.)).children(ticks))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .text_size(theme::TEXT_SM)
                    .text_color(theme::text_dim())
                    .child(div().w(px(56.)).child(s.t("Charge", "Load")))
                    .child(div().flex_1().child(self.gauge(
                        "load",
                        &e.load_track,
                        Drag::Load,
                        px(6.),
                        theme::text(),
                        e.load,
                    ))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .text_size(theme::TEXT_SM)
                    .text_color(theme::text_dim())
                    .child(limiter),
            );

        // ── Commandes ──
        let holding = e.throttle;
        let st = self.state.clone();
        let hold = div()
            .id("throttle")
            .flex()
            .items_center()
            .justify_center()
            .gap(px(10.))
            .h(px(52.))
            .rounded(px(10.))
            .border_1()
            .border_color(theme::accent())
            .bg(if holding { theme::accent() } else { theme::accent_tint() })
            .text_color(if holding { theme::on_accent() } else { theme::accent() })
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(14.))
            .cursor_pointer()
            .child(icon(Icon::ArrowUp, px(18.), if holding { theme::on_accent() } else { theme::accent() }))
            .child(s.t("Maintenir pour accélérer", "Hold to accelerate"))
            .tooltip(crate::widgets::tooltip(s.tf(
                format!("Raccourci : {} (maintenir)", s.shortcut_text(Shortcut::Throttle)),
                format!("Shortcut: {} (hold)", s.shortcut_text(Shortcut::Throttle)),
            )))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| st.update(cx, |s, cx| s.press_throttle(cx)));

        let engine_label = if e.starting {
            s.t("Démarrage…", "Starting…")
        } else if running {
            s.t("Arrêter le moteur", "Stop engine")
        } else {
            s.t("Démarrer le moteur", "Start engine")
        };
        let controls = div()
            .flex()
            .flex_col()
            .gap(theme::GAP_SM)
            .flex_none()
            .w(px(300.))
            .child(hold)
            .child(
                div()
                    .flex()
                    .gap(theme::GAP_SM)
                    .child(
                        button("engine-toggle")
                            .flex_1()
                            .px(px(8.))
                            .text_size(theme::TEXT_SM)
                            .h(theme::BUTTON_H)
                            .child(icon(if running { Icon::Stop } else { Icon::Play }, px(12.), theme::text()))
                            .child(engine_label)
                            .tooltip(crate::widgets::tooltip(s.tf(
                                format!("Raccourci : {}", s.shortcut_text(Shortcut::Engine)),
                                format!("Shortcut: {}", s.shortcut_text(Shortcut::Engine)),
                            )))
                            .on_click(crate::widgets::on(&self.state, |s, cx| s.toggle_engine(cx))),
                    )
                    .child(
                        button("sweep")
                            .flex_1()
                            .px(px(8.))
                            .text_size(theme::TEXT_SM)
                            .h(theme::BUTTON_H)
                            .when(e.sweeping, |b| b.border_color(theme::accent()).text_color(theme::accent()))
                            .child(if limiter_on {
                                s.t("Balayer 0 → limiteur", "Sweep 0 → limiter")
                            } else {
                                s.t("Balayer 0 → max", "Sweep 0 → max")
                            })
                            .tooltip(crate::widgets::tooltip(s.tf(
                                format!("Raccourci : {}", s.shortcut_text(Shortcut::Sweep)),
                                format!("Shortcut: {}", s.shortcut_text(Shortcut::Sweep)),
                            )))
                            .on_click(crate::widgets::on(&self.state, |s, cx| s.start_sweep(cx))),
                    ),
            );

        Self::card().child(readout).child(gauges).child(controls).into_any_element()
    }
}
