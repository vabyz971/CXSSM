//! Interface : un module par composant, tous branchés sur l'état partagé
//! [`AppState`](crate::state::AppState).
//!
//! ```text
//! ┌ app_bar ─────────────────────────────────────────────────────────┐
//! │ tree │ en-tête · rpm_simulator · sound_list            │ side_panel │
//! └ status_bar ──────────────────────────────────────────────────────┘
//!                       (preferences : fenêtre modale)
//! ```

use gpui::{div, prelude::*, px, Context, Entity, FontWeight, MouseButton, MouseMoveEvent, Window};

mod app_bar;
mod fmod_setup;
mod preferences;
mod rpm_simulator;
mod side_panel;
mod sound_list;
mod status_bar;
mod tree;

use crate::assets::{icon_sm, Icon};
use crate::i18n;
use crate::state::{AppState, Leaf};
use crate::theme;
use crate::widgets::{on, primary, segment, segmented};

/// Vue racine : assemble les composants et relaie les glisser à l'état partagé.
pub struct Workspace {
    state: Entity<AppState>,
    tree: Entity<tree::Tree>,
    simulator: Entity<rpm_simulator::RpmSimulator>,
    sounds: Entity<sound_list::SoundList>,
    side: Entity<side_panel::SidePanel>,
    prefs: Entity<preferences::Preferences>,
    fmod_setup: Entity<fmod_setup::FmodSetup>,
}

impl Workspace {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        // Tout changement de l'état redessine la fenêtre.
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            tree: cx.new(|_| tree::Tree::new(state.clone())),
            simulator: cx.new(|_| rpm_simulator::RpmSimulator::new(state.clone())),
            sounds: cx.new(|_| sound_list::SoundList::new(state.clone())),
            side: cx.new(|_| side_panel::SidePanel::new(state.clone())),
            prefs: cx.new(|_| preferences::Preferences::new(state.clone())),
            fmod_setup: cx.new(|_| fmod_setup::FmodSetup::new(state.clone())),
            state,
        }
    }

    /// Zone centrale : en-tête de la sélection, simulateur, liste des sons.
    fn render_main(&self, s: &AppState) -> impl IntoElement {
        let main = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .gap(theme::GAP_LG)
            .px(theme::MAIN_PAD_X)
            .py(theme::MAIN_PAD_Y);

        if s.project.is_none() {
            return main.child(self.render_empty(s));
        }
        let Some(leaf) = s.leaf else {
            return main.child(
                div().flex_1().flex().items_center().justify_center().text_color(theme::TEXT_DIM).child(s.t(
                    "Choisissez une voiture (ou une banque de sons) dans l'arborescence.",
                    "Pick a car (or a sound bank) in the tree.",
                )),
            );
        };

        let (breadcrumb, title) = match leaf {
            Leaf::Car(i) => {
                let ev = &s.engine.events[i];
                (format!("{}  ›  {}", i18n::category(s.lang(), cxssm_core::SoundCategory::Engine), s.bank_name(ev.bank)), ev.name().to_string())
            }
            Leaf::Bank(cat, bank) => (i18n::category(s.lang(), cat).to_string(), s.bank_name(bank)),
        };
        let is_car = matches!(leaf, Leaf::Car(_));
        let use_mod = s.engine.use_mod;
        let header = div()
            .flex()
            .flex_none()
            .items_end()
            .justify_between()
            .gap(theme::GAP_LG)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme::GAP_XS)
                    .min_w(px(0.))
                    .child(div().text_size(theme::TEXT_SM).text_color(theme::TEXT_DIM).child(breadcrumb))
                    .child(div().text_size(theme::TEXT_H1).font_weight(FontWeight::SEMIBOLD).truncate().child(title)),
            )
            .when(is_car, |d| {
                d.child(
                    segmented()
                        .child(
                            segment("sounds-original", s.t("Sons originaux", "Original sounds"), !use_mod)
                                .on_click(on(&self.state, |s, cx| s.set_use_mod(false, cx))),
                        )
                        .child(
                            segment("sounds-modified", s.t("Sons remplacés", "Replaced sounds"), use_mod)
                                .on_click(on(&self.state, |s, cx| s.set_use_mod(true, cx))),
                        ),
                )
            });

        main.child(header)
            .when(is_car, |d| d.child(self.simulator.clone()))
            .child(self.sounds.clone())
    }

    fn render_empty(&self, s: &AppState) -> impl IntoElement {
        div().flex_1().flex().items_center().justify_center().child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(theme::GAP_MD)
                .p(px(32.))
                .rounded(theme::RADIUS_LG)
                .bg(theme::PANEL)
                .border_1()
                .border_color(theme::DIVIDER)
                .child(div().text_size(theme::TEXT_TITLE).font_weight(FontWeight::SEMIBOLD).child(s.t("Aucun bundle chargé", "No bundle loaded")))
                .child(
                    div()
                        .font_family(theme::FONT_MONO)
                        .text_size(theme::TEXT_SM)
                        .text_color(theme::TEXT_DIM)
                        .child("CarX Street_Data/StreamingAssets/aa/<platform>/fmod_assets_all.bundle"),
                )
                .child(
                    primary("open-empty")
                        .child(icon_sm(Icon::Folder, theme::ON_ACCENT))
                        .child(s.t("Ouvrir un bundle", "Open a bundle"))
                        .on_click(on(&self.state, |s, cx| s.open_file_dialog(cx))),
                ),
        )
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Simulateur : accélérateur, paramètres FMOD et sons entendus, à chaque image.
        self.state.update(cx, |s, _| s.tick());
        let state = self.state.clone();
        let s = self.state.read(cx);
        // Garde la boucle de rendu active pendant les tâches de fond et les animations
        // (sous Wayland, GPUI ne redessine sinon que sur évènement).
        if s.is_animating() {
            window.request_animation_frame();
        }
        let dragging = s.drag.is_some();

        let body = div()
            .flex()
            .flex_1()
            .min_h(px(0.))
            .child(self.tree.clone())
            .child(self.render_main(s))
            .child(self.side.clone());

        div()
            .id("root")
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::BG)
            .text_color(theme::TEXT)
            .font_family(theme::FONT_UI)
            .text_size(theme::TEXT_MD)
            .when(dragging, |d| {
                let st = state.clone();
                d.on_mouse_move(move |e: &MouseMoveEvent, _, cx| {
                    st.update(cx, |s, cx| {
                        if e.pressed_button == Some(MouseButton::Left) {
                            s.drag_to(e.position, cx);
                        } else {
                            s.mouse_released(cx);
                        }
                    })
                })
            })
            .on_mouse_up(MouseButton::Left, on(&state, |s, cx| s.mouse_released(cx)))
            .child(app_bar::render(s, &state))
            .child(body)
            .child(status_bar::render(s, &state))
            .when(s.prefs_open, |d| d.child(self.prefs.clone()))
            .when(s.fmod_help_open && !s.prefs_open, |d| d.child(self.fmod_setup.clone()))
    }
}
