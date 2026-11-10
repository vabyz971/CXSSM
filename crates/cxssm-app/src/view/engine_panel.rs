//! Simulateur moteur : choix d'une voiture, régime (RPM) et charge, sons entendus
//! en direct et carte des sons selon le régime.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use cxssm_core::engine::{self, ActiveLayer, EngineData, EngineEvent, FmodLibHandle, MapEntry, Simulator};
use cxssm_core::{export, SoundRef};
use gpui::{
    canvas, div, prelude::*, px, relative, uniform_list, Bounds, Context, FontWeight, MouseButton, MouseDownEvent,
    Pixels, SharedString, UniformListScrollHandle,
};

use super::MainView;
use crate::theme;
use crate::widgets::{badge, ghost_button, primary_button, section_title};

/// Régime de ralenti utilisé par l'accélérateur automatique.
const IDLE_RPM: f32 = 900.;
/// Vitesse de montée / descente du régime avec le bouton « accélérer » (tr/min par seconde).
const RPM_RISE: f32 = 3_500.;
const RPM_FALL: f32 = 2_800.;
/// Rupteur : durée de la coupure d'injection et vitesse de chute du régime pendant la coupure.
/// Avec la remontée à `RPM_RISE`, cela donne un rebond d'environ 7 coupures par seconde.
const LIMITER_CUT: f32 = 0.045;
const LIMITER_DROP: f32 = 7_000.;
/// Régime minimal réglable pour le rupteur.
const LIMITER_MIN: f32 = 2_000.;
/// Pas du balayage pour la carte des sons.
const MAP_STEP: f32 = 125.;

/// Curseur en cours de glissement.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Slider {
    Rpm,
    Load,
    Limiter,
}

pub(super) struct EngineState {
    lib: Option<FmodLibHandle>,
    error: Option<SharedString>,
    data: Option<EngineData>,
    events: Vec<EngineEvent>,
    loading_events: bool,
    selected: Option<usize>,
    sim: Option<Simulator>,
    starting: bool,
    pub(super) rpm: f32,
    pub(super) load: f32,
    /// Bouton « accélérer » maintenu.
    pub(super) throttle: bool,
    /// Le régime est piloté par l'accélérateur (sinon par le curseur).
    auto: bool,
    /// Rupteur activé (préférences). Désactivé : le régime monte jusqu'au maximum.
    pub(super) limiter_enabled: bool,
    /// Régime du rupteur (par défaut : le maximum de l'évènement).
    limiter: f32,
    /// Temps restant de la coupure du rupteur en cours (secondes).
    cut: f32,
    last_tick: Option<Instant>,
    layers: Vec<ActiveLayer>,
    map: Option<(usize, Vec<MapEntry>)>,
    mapping: bool,
    /// Jouer la banque avec les sons de remplacement de l'utilisateur.
    use_mod: bool,
    list_scroll: UniformListScrollHandle,
    rpm_track: Rc<Cell<Bounds<Pixels>>>,
    load_track: Rc<Cell<Bounds<Pixels>>>,
    limiter_track: Rc<Cell<Bounds<Pixels>>>,
    pub(super) dragging: Option<Slider>,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            lib: None,
            error: None,
            data: None,
            events: Vec::new(),
            loading_events: false,
            selected: None,
            sim: None,
            starting: false,
            rpm: IDLE_RPM,
            load: 0.,
            throttle: false,
            auto: false,
            limiter_enabled: true,
            limiter: 10_500.,
            cut: 0.,
            last_tick: None,
            layers: Vec::new(),
            map: None,
            mapping: false,
            use_mod: false,
            list_scroll: UniformListScrollHandle::new(),
            rpm_track: Rc::default(),
            load_track: Rc::default(),
            limiter_track: Rc::default(),
            dragging: None,
        }
    }
}

impl EngineState {
    /// Une tâche ou une lecture demande de continuer à redessiner.
    pub(super) fn is_active(&self) -> bool {
        self.sim.is_some() || self.loading_events || self.starting || self.mapping
    }

    fn event(&self) -> Option<&EngineEvent> {
        self.events.get(self.selected?)
    }

    fn rpm_max(&self) -> f32 {
        self.event().map_or(10_500., |e| e.rpm_range().1)
    }

    /// Avance la simulation d'une image : accélérateur, paramètres FMOD, sons entendus.
    pub(super) fn tick(&mut self) {
        let now = Instant::now();
        let dt = self.last_tick.map_or(0., |t| (now - t).as_secs_f32()).min(0.1);
        self.last_tick = Some(now);
        let Some(sim) = &self.sim else { return };
        if self.auto {
            if self.throttle && self.cut > 0. {
                // Rupteur : coupure, le régime chute brièvement sans charge.
                self.cut -= dt;
                self.rpm = (self.rpm - LIMITER_DROP * dt).max(IDLE_RPM);
                self.load = 0.;
            } else if self.throttle {
                self.rpm += RPM_RISE * dt;
                self.load = 1.;
                if !self.limiter_enabled {
                    self.rpm = self.rpm.min(self.rpm_max());
                } else if self.rpm >= self.limiter {
                    self.rpm = self.limiter;
                    self.cut = LIMITER_CUT;
                }
            } else {
                self.cut = 0.;
                self.rpm = (self.rpm - RPM_FALL * dt).max(IDLE_RPM);
                self.load = 0.;
            }
        }
        sim.set_param(engine::RPM, self.rpm);
        sim.set_param(engine::LOAD, self.load);
        sim.update();
        self.layers = sim.layers();
    }

    pub(super) fn set_from_mouse(&mut self, slider: Slider, x: Pixels) {
        let b = match slider {
            Slider::Rpm => self.rpm_track.get(),
            Slider::Load => self.load_track.get(),
            Slider::Limiter => self.limiter_track.get(),
        };
        if b.size.width <= px(0.) {
            return;
        }
        let ratio = ((x - b.origin.x) / b.size.width).clamp(0., 1.);
        match slider {
            Slider::Rpm => {
                self.auto = false;
                self.rpm = ratio * self.rpm_max();
            }
            Slider::Load => {
                self.auto = false;
                self.load = ratio;
            }
            // Le rupteur se règle sans quitter le mode accélérateur.
            Slider::Limiter => self.limiter = LIMITER_MIN + ratio * (self.rpm_max() - LIMITER_MIN),
        }
    }

    /// Le rupteur est en train de couper (ou le régime rebondit dessus).
    fn limiting(&self) -> bool {
        self.limiter_enabled && self.auto && self.throttle && (self.cut > 0. || self.rpm >= self.limiter - 400.)
    }

    pub(super) fn has_library(&self) -> bool {
        self.lib.is_some()
    }

    /// Permet de réessayer de charger FMOD après un changement de dossier.
    pub(super) fn clear_error(&mut self) {
        self.error = None;
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

impl MainView {
    // ───────────────────────────── actions ─────────────────────────────

    /// Ouvre le simulateur : charge FMOD et la liste des voitures (une seule fois).
    pub(super) fn enter_engine_mode(&mut self, cx: &mut Context<Self>) {
        self.engine_mode = true;
        let not_found = self.t(
            "FMOD Engine introuvable : installez-le (fmod.com/download) ou indiquez son dossier \
             dans ⚙ Préférences.",
            "FMOD Engine not found: install it (fmod.com/download) or set its folder \
             in ⚙ Preferences.",
        );
        let api = self.fmod.as_ref().map(|f| f.api.clone());
        let e = &mut self.engine;
        if e.lib.is_none() {
            e.error = None;
            match api {
                None => e.error = Some(not_found.into()),
                Some(dir) => match engine::load_library(&dir) {
                    Ok(lib) => e.lib = Some(lib),
                    Err(err) => e.error = Some(format!("FMOD : {err}").into()),
                },
            }
        }
        let (Some(lib), Some(p)) = (e.lib.clone(), self.project.as_ref()) else {
            cx.notify();
            return;
        };
        if e.data.is_some() || e.loading_events {
            cx.notify();
            return;
        }
        let data = EngineData::from_project(p);
        e.data = Some(data.clone());
        e.loading_events = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { engine::list_events(&lib, &data) }).await;
            let _ = this.update(cx, |v, cx| {
                v.engine.loading_events = false;
                match res {
                    Ok(list) => {
                        v.status = v.tf(format!("{} voitures (évènements moteur) trouvées", list.len()), format!("{} cars (engine events) found", list.len())).into();
                        v.engine.events = list;
                    }
                    Err(err) => v.engine.error = Some(format!("FMOD : {err}").into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn select_event(&mut self, i: usize, cx: &mut Context<Self>) {
        let e = &mut self.engine;
        if e.selected == Some(i) {
            return;
        }
        e.sim = None;
        e.selected = Some(i);
        e.layers.clear();
        e.map = None;
        e.rpm = IDLE_RPM;
        e.load = 0.;
        e.auto = true;
        e.cut = 0.;
        e.limiter = e.rpm_max();
        cx.notify();
    }

    fn toggle_sim(&mut self, cx: &mut Context<Self>) {
        if self.engine.sim.take().is_some() {
            self.engine.layers.clear();
            cx.notify();
            return;
        }
        let (Some(lib), Some(data), Some(event), Some(p)) = (
            self.engine.lib.clone(),
            self.engine.data.clone(),
            self.engine.event().cloned(),
            self.project.as_ref(),
        ) else {
            return;
        };
        // Écoute du mod : la banque est reconstruite avec les sons de remplacement.
        let modded = if self.engine.use_mod {
            let repl = export::bank_replacements(p, event.bank);
            (!repl.is_empty()).then(|| (p.banks[event.bank].clone(), repl, self.encoder.clone()))
        } else {
            None
        };
        // Coupe une éventuelle pré-écoute en cours.
        self.stop(cx);
        self.engine.starting = true;
        self.status = if modded.is_some() {
            self.t("Encodage de vos sons et démarrage du moteur…", "Encoding your sounds and starting the engine…").into()
        } else {
            self.t("Démarrage du moteur…", "Starting the engine…").into()
        };
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    let bytes = match modded {
                        Some((entry, repl, enc)) => Some(export::build_bank_entry(&entry, &repl, &enc)?),
                        None => None,
                    };
                    Simulator::new(&lib, &data, &event, bytes.as_deref())
                })
                .await;
            let _ = this.update(cx, |v, cx| {
                v.engine.starting = false;
                match res {
                    Ok(sim) => {
                        v.status = if sim.modded {
                            v.t("Moteur démarré avec VOS sons", "Engine started with YOUR sounds").into()
                        } else {
                            v.t("Moteur démarré (sons d'origine)", "Engine started (original sounds)").into()
                        };
                        v.engine.sim = Some(sim);
                        v.engine.last_tick = None;
                    }
                    Err(err) => {
                        log::error!("simulateur : {err}");
                        v.status = v.tf(format!("Simulateur : {err}"), format!("Simulator: {err}")).into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn build_map(&mut self, cx: &mut Context<Self>) {
        let (Some(lib), Some(data), Some(i)) = (self.engine.lib.clone(), self.engine.data.clone(), self.engine.selected)
        else {
            return;
        };
        let event = self.engine.events[i].clone();
        self.engine.mapping = true;
        self.status = self.tf(format!("Cartographie de {}…", event.name()), format!("Mapping {}…", event.name())).into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { engine::sweep(&lib, &data, &event, MAP_STEP) }).await;
            let _ = this.update(cx, |v, cx| {
                v.engine.mapping = false;
                match res {
                    Ok(map) => {
                        v.status = v.tf(format!("{} sons utilisés par cette voiture", map.len()), format!("{} sounds used by this car", map.len())).into();
                        v.engine.map = Some((i, map));
                    }
                    Err(err) => {
                        log::error!("cartographie : {err}");
                        v.status = v.tf(format!("Cartographie : {err}"), format!("Mapping: {err}")).into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ───────────────────────────── rendu ─────────────────────────────

    pub(super) fn render_engine(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let e = &self.engine;
        let body = if let Some(err) = &e.error {
            message(err.clone()).into_any_element()
        } else if self.project.is_none() {
            message(self.t("Ouvrez d'abord fmod_assets_all.bundle.", "Open fmod_assets_all.bundle first.").into()).into_any_element()
        } else if e.loading_events {
            message(self.t("Lecture des évènements moteur avec FMOD…", "Reading engine events with FMOD…").into()).into_any_element()
        } else {
            div()
                .flex()
                .size_full()
                .child(self.render_event_list(cx))
                .child(self.render_controls(cx))
                .into_any_element()
        };
        div().flex_1().h_full().overflow_hidden().child(body)
    }

    fn render_event_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let e = &self.engine;
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(250.))
            .h_full()
            .border_r_1()
            .border_color(theme::border())
            .child(div().px_4().py_2().child(section_title(self.tf(format!("VOITURES ({})", e.events.len()), format!("CARS ({})", e.events.len())))))
            .child(
                uniform_list(
                    "engine-events",
                    e.events.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _w, cx| {
                        range
                            .map(|i| {
                                let ev = &this.engine.events[i];
                                let active = this.engine.selected == Some(i);
                                let bank = this.project.as_ref().map(|p| p.banks[ev.bank].name.clone()).unwrap_or_default();
                                div()
                                    .id(("ev", i))
                                    .flex()
                                    .w_full()
                                    .gap_2()
                                    .h(px(30.))
                                    .px_4()
                                    .items_center()
                                    .text_sm()
                                    .cursor_pointer()
                                    .border_l_4()
                                    .border_color(if active { theme::accent() } else { gpui::transparent_black().into() })
                                    .when(active, |d| d.bg(theme::panel_hi()))
                                    .hover(|s| s.bg(theme::panel_hi()))
                                    .child(div().w(px(52.)).text_xs().text_color(theme::text_dim()).child(bank))
                                    .child(div().flex_1().truncate().child(ev.name().to_string()))
                                    .on_click(cx.listener(move |v, _, _, cx| v.select_event(i, cx)))
                            })
                            .collect()
                    }),
                )
                .track_scroll(e.list_scroll.clone())
                .flex_1(),
            )
    }

    fn render_controls(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let e = &self.engine;
        let Some(ev) = e.event() else {
            return div()
                .flex_1()
                .p_6()
                .child(div().text_sm().text_color(theme::text_dim()).child(self.t("Choisissez une voiture à gauche.", "Pick a car on the left.")))
                .into_any_element();
        };
        let p = self.project.as_ref();
        let bank_name = p.map(|p| p.banks[ev.bank].name.clone()).unwrap_or_default();
        let replaced = p.map_or(0, |p| export::bank_replacements(p, ev.bank).len());
        let running = e.sim.is_some();
        let max = e.rpm_max();
        let limiting = running && e.limiting();

        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().text_xl().font_weight(FontWeight::BOLD).child(ev.name().to_string()))
                    .child(badge(bank_name, theme::accent())),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        ghost_button("map", if e.mapping { self.t("ANALYSE…", "ANALYSING…") } else { self.t("CARTOGRAPHIER", "MAP SOUNDS") })
                            .on_click(cx.listener(|v, _, _, cx| v.build_map(cx))),
                    )
                    .child(
                        ghost_button("use-mod", if e.use_mod { self.t("✔ MES SONS", "✔ MY SOUNDS") } else { self.t("SONS D'ORIGINE", "ORIGINAL SOUNDS") })
                            .when(e.use_mod, |b| b.border_color(theme::ok()).text_color(theme::ok()))
                            .on_click(cx.listener(|v, _, _, cx| {
                                v.engine.use_mod = !v.engine.use_mod;
                                // Redémarre pour charger l'autre version de la banque.
                                if v.engine.sim.take().is_some() {
                                    v.toggle_sim(cx);
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        primary_button(
                            "sim",
                            if e.starting {
                                "…"
                            } else if running {
                                self.t("■ ARRÊTER", "■ STOP")
                            } else {
                                self.t("▶ DÉMARRER", "▶ START")
                            },
                        )
                        .on_click(cx.listener(|v, _, _, cx| v.toggle_sim(cx))),
                    ),
            );

        let rpm_block = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap_2()
                    .child(
                        div()
                            .text_3xl()
                            .font_weight(FontWeight::EXTRA_BOLD)
                            .text_color(if limiting { theme::warn() } else { theme::text() })
                            .child(format!("{:.0}", e.rpm)),
                    )
                    .child(div().pb_1().text_sm().text_color(theme::text_dim()).child(self.t("tr/min", "rpm")))
                    .when(limiting, |d| d.child(div().pb_1().ml_2().child(badge(self.t("RUPTEUR", "LIMITER"), theme::warn())))),
            )
            .child(slider("rpm-track", e.rpm / max, e.limiter_enabled.then_some(e.limiter / max), e.rpm_track.clone(), Slider::Rpm, cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(div().w(px(110.)).text_sm().text_color(theme::text_dim()).child(self.tf(format!("Charge {:.0} %", e.load * 100.), format!("Load {:.0} %", e.load * 100.))))
                    .child(div().flex_1().child(slider("load-track", e.load, None, e.load_track.clone(), Slider::Load, cx))),
            )
            .when(e.limiter_enabled, |d| d.child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .w(px(110.))
                            .text_sm()
                            .text_color(theme::text_dim())
                            .child(self.tf(format!("Rupteur {:.0}", e.limiter), format!("Limiter {:.0}", e.limiter))),
                    )
                    .child(div().flex_1().child(slider(
                        "limiter-track",
                        (e.limiter - LIMITER_MIN) / (max - LIMITER_MIN),
                        None,
                        e.limiter_track.clone(),
                        Slider::Limiter,
                        cx,
                    ))),
            ))
            .child(
                div()
                    .id("throttle")
                    .mt_2()
                    .py_3()
                    .flex()
                    .justify_center()
                    .border_1()
                    .border_color(if e.throttle { theme::accent() } else { theme::border() })
                    .bg(if e.throttle { theme::accent() } else { theme::panel() })
                    .text_color(if e.throttle { theme::accent_text() } else { theme::text() })
                    .font_weight(FontWeight::BOLD)
                    .text_sm()
                    .cursor_pointer()
                    .child(if running {
                        self.t("MAINTENIR POUR ACCÉLÉRER", "HOLD TO ACCELERATE")
                    } else {
                        self.t("DÉMARREZ LE MOTEUR POUR ACCÉLÉRER", "START THE ENGINE TO ACCELERATE")
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|v, _: &MouseDownEvent, _, cx| {
                            v.engine.throttle = true;
                            v.engine.auto = true;
                            cx.notify();
                        }),
                    ),
            );

        // Hauteur réservée pour 3 couches : la carte en dessous ne bouge pas quand
        // une boucle apparaît ou disparaît.
        let layers = div()
            .flex()
            .flex_col()
            .gap_1()
            .min_h(px(130.))
            .child(section_title(self.t("SONS ENTENDUS", "ACTIVE SOUNDS")))
            .when(e.layers.is_empty(), |d| {
                d.child(div().text_sm().text_color(theme::text_dim()).child(if running {
                    self.t("Aucun son audible à ce régime.", "No audible sound at this RPM.")
                } else {
                    self.t("Démarrez le moteur pour voir le mélange en direct.", "Start the engine to see the live mix.")
                }))
            })
            .children(e.layers.iter().enumerate().map(|(i, l)| self.render_layer(i, l, cx)));

        let map = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(section_title(self.t("CARTE DES SONS SELON LE RÉGIME", "SOUND MAP BY RPM")))
            .children(self.render_map(max, cx));

        div()
            .id("engine-controls")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .p_6()
            .flex()
            .flex_col()
            .gap_6()
            .child(header)
            .when(replaced > 0, |d| {
                d.child(div().text_xs().text_color(theme::ok()).child(self.tf(
                    format!("{replaced} son(s) remplacé(s) dans cette banque : activez « MES SONS » pour les entendre."),
                    format!("{replaced} replaced sound(s) in this bank: enable « MY SOUNDS » to hear them."),
                )))
            })
            .child(rpm_block)
            .child(layers)
            .child(map)
            .into_any_element()
    }

    fn render_layer(&self, i: usize, l: &ActiveLayer, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self.sound_name(l.sound).unwrap_or_else(|| self.t("son non identifié", "unidentified sound").into());
        let selected = l.sound.is_some() && self.selected == l.sound;
        let sound = l.sound;
        div()
            .id(("layer", i))
            .flex()
            .items_center()
            .gap_3()
            .h(px(30.))
            .px_2()
            .text_sm()
            .cursor_pointer()
            .when(selected, |d| d.bg(theme::panel_hi()))
            .hover(|s| s.bg(theme::panel_hi()))
            .child(div().w(px(120.)).child(name))
            .child(
                div()
                    .flex_1()
                    .h(px(8.))
                    .bg(theme::panel_hi())
                    .child(div().h_full().w(relative((l.volume / 3.).clamp(0.02, 1.))).bg(theme::accent())),
            )
            .child(
                div()
                    .w(px(150.))
                    .text_xs()
                    .text_color(theme::text_dim())
                    .child(format!("vol {:.2} · pitch {:.2}", l.volume, l.pitch)),
            )
            .on_click(cx.listener(move |v, _, _, cx| {
                if sound.is_some() {
                    v.selected = sound;
                    cx.notify();
                }
            }))
    }

    fn render_map(&self, max: f32, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let e = &self.engine;
        let Some((ev, map)) = &e.map else {
            return vec![div()
                .text_sm()
                .text_color(theme::text_dim())
                .child(self.t("Analysez toute la plage de régime pour savoir quels sons remplacer.", "Sweep the whole RPM range to find which sounds to replace."))
                .into_any_element()];
        };
        if Some(*ev) != e.selected {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (group, on_load) in [
            (self.t("ACCÉLÉRATION (Load = 1)", "ON THROTTLE (Load = 1)"), true),
            (self.t("PIED LEVÉ (Load = 0)", "OFF THROTTLE (Load = 0)"), false),
        ] {
            out.push(div().mt_2().text_xs().text_color(theme::text_dim()).child(group).into_any_element());
            for (i, m) in map.iter().enumerate().filter(|(_, m)| m.on_load == on_load) {
                let r = m.sound;
                let current = e.sim.is_some() && e.rpm >= m.rpm_min && e.rpm <= m.rpm_max && (e.load >= 0.5) == on_load;
                let start = (m.rpm_min / max).clamp(0., 1.);
                let width = ((m.rpm_max - m.rpm_min) / max).clamp(0.01, 1.);
                let replaced = self.project.as_ref().is_some_and(|p| p.replacements.contains_key(&r));
                out.push(
                    div()
                        .id(("map", i))
                        .flex()
                        .items_center()
                        .gap_3()
                        .h(px(26.))
                        .px_2()
                        .text_sm()
                        .cursor_pointer()
                        .when(self.selected == Some(r), |d| d.bg(theme::panel_hi()))
                        .hover(|s| s.bg(theme::panel_hi()))
                        .child(
                            div()
                                .w(px(120.))
                                .text_color(if replaced { theme::ok() } else { theme::text() })
                                .child(self.sound_name(Some(r)).unwrap_or_default()),
                        )
                        .child(
                            div()
                                .w(px(130.))
                                .text_xs()
                                .text_color(theme::text_dim())
                                .child(format!("{:.0} → {:.0}", m.rpm_min, m.rpm_max)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .h(px(10.))
                                .relative()
                                .bg(theme::panel_hi())
                                .child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .h_full()
                                        .left(relative(start))
                                        .w(relative(width))
                                        .bg(if current { theme::accent() } else { theme::text_dim() }),
                                )
                                .when(e.sim.is_some(), |d| {
                                    d.child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .h_full()
                                            .w(px(2.))
                                            .left(relative((e.rpm / max).clamp(0., 1.)))
                                            .bg(theme::warn()),
                                    )
                                }),
                        )
                        .on_click(cx.listener(move |v, _, _, cx| {
                            v.selected = Some(r);
                            cx.notify();
                        }))
                        .into_any_element(),
                );
            }
        }
        out
    }

    fn sound_name(&self, r: Option<SoundRef>) -> Option<String> {
        let (_, s) = self.project.as_ref()?.sample(r?)?;
        Some(s.name.clone())
    }
}

fn message(text: SharedString) -> impl IntoElement {
    div().p_6().text_sm().text_color(theme::text_dim()).child(text)
}

/// Curseur horizontal : piste + remplissage ; ses dimensions sont mémorisées pour le glisser.
fn slider(
    id: &'static str,
    value: f32,
    marker: Option<f32>,
    track: Rc<Cell<Bounds<Pixels>>>,
    which: Slider,
    cx: &mut Context<MainView>,
) -> impl IntoElement {
    div()
        .id(id)
        .relative()
        .h(px(14.))
        .w_full()
        .bg(theme::panel_hi())
        .cursor_pointer()
        .child(div().h_full().w(relative(value.clamp(0., 1.))).bg(theme::accent()))
        // Repère (ex. régime du rupteur), dessiné dans la piste pour ne pas gêner le clic.
        .when_some(marker, |d, m| {
            d.child(div().absolute().top_0().h_full().w(px(3.)).left(relative(m.clamp(0., 1.))).bg(theme::warn()))
        })
        .child(
            canvas(move |bounds, _, _| track.set(bounds), |_, _, _, _| {})
                .absolute()
                .size_full(),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |v, ev: &MouseDownEvent, _, cx| {
                v.engine.dragging = Some(which);
                v.engine.set_from_mouse(which, ev.position.x);
                cx.notify();
            }),
        )
}
