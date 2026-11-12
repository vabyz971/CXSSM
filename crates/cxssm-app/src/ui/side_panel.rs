//! Panneau de détails rétractable : onglets Infos, Édition et Import / Export
//! pour le son sélectionné.

use cxssm_core::engine::MapEntry;
use cxssm_core::{SoundRef, SoundStatus};
use gpui::{div, prelude::*, px, Context, Entity, FontWeight, SharedString, Window};

use super::sound_list::{bank_strip, rpm_strip};
use crate::assets::{icon, Icon};
use crate::state::{AppState, Leaf, SideTab};
use crate::theme;
use crate::widgets::{button, disabled, fmt_int, ghost, on, primary, strip};

pub struct SidePanel {
    state: Entity<AppState>,
}

impl SidePanel {
    pub fn new(state: Entity<AppState>) -> Self {
        Self { state }
    }

    fn rail(&self) -> gpui::AnyElement {
        div()
            .flex()
            .flex_col()
            .flex_none()
            .items_center()
            .w(theme::RAIL_W)
            .py(px(7.))
            .bg(theme::sidebar())
            .border_l_1()
            .border_color(theme::divider())
            .child(
                ghost("side-open")
                    .size(px(36.))
                    .child(icon(Icon::PanelExpand, px(16.), theme::text_icon()))
                    .on_click(on(&self.state, |s, cx| s.set_side(true, cx))),
            )
            .into_any_element()
    }

    fn tabs(&self, s: &AppState) -> impl IntoElement {
        let tab = |id: &'static str, label: &'static str, t: SideTab| {
            let active = s.side_tab == t;
            div()
                .id(id)
                .flex()
                .items_center()
                .h(theme::TAB_H)
                .px(px(10.))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .text_color(if active { theme::text_bright() } else { theme::text_dim() })
                .border_b_2()
                .border_color(if active { theme::accent() } else { gpui::transparent_black().into() })
                .hover(|d| d.text_color(theme::text_bright()))
                .child(label)
                .on_click(on(&self.state, move |s, cx| s.set_tab(t, cx)))
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .pl(theme::GAP_MD)
            .pr(theme::GAP_SM)
            .border_b_1()
            .border_color(theme::divider())
            .child(
                div()
                    .flex()
                    .gap(px(2.))
                    .child(tab("tab-info", s.t("Infos", "Info"), SideTab::Info))
                    .child(tab("tab-edit", s.t("Édition", "Edit"), SideTab::Edit))
                    .child(tab("tab-io", s.t("Import / Export", "Import / Export"), SideTab::Io)),
            )
            .child(
                ghost("side-close")
                    .size(px(32.))
                    .child(icon(Icon::PanelCollapse, px(16.), theme::text_icon()))
                    .on_click(on(&self.state, |s, cx| s.set_side(false, cx))),
            )
    }

    /// Entrées de la carte de la voiture affichée pour ce son (accélération, décélération).
    fn map_entries(s: &AppState, r: SoundRef) -> Vec<&MapEntry> {
        let Some(Leaf::Car(i)) = s.leaf else { return Vec::new() };
        s.engine.maps.get(&i).and_then(|m| m.as_ref().ok()).map_or(Vec::new(), |m| m.iter().filter(|e| e.sound == r).collect())
    }

    fn info_tab(&self, s: &AppState, r: SoundRef) -> gpui::Div {
        let lang = s.lang();
        let Some((bank, smp)) = s.project.as_ref().and_then(|p| p.sample(r)) else { return div() };
        let entries = Self::map_entries(s, r);
        let max = s.engine.rpm_max().max(1.);
        let rpm = |v: f32| format!("{} {}", fmt_int(v, lang), s.t("tr/min", "rpm"));

        let (caption, preview) = match entries.first() {
            Some(m) => (s.t("Spectre · plage de régime utilisée", "Spectrum · RPM range used"), rpm_strip(s, m, max)),
            None => (s.t("Spectre (30 Hz → 16 kHz)", "Spectrum (30 Hz → 16 kHz)"), bank_strip(s, r, true)),
        };
        let mut rows: Vec<(&'static str, SharedString, bool)> = vec![(s.t("Statut", "Status"), status_text(s, r).into(), false)];
        if let Some(m) = entries.first() {
            let (fade_in, fade_out) = m.fades();
            let fade = |v: f32| if v < 1. { s.t("aucun", "none").to_string() } else { fmt_int(v, lang) };
            let trigger = match (entries.iter().any(|e| e.on_load), entries.iter().any(|e| !e.on_load)) {
                (true, true) => s.t("Charge et relâché", "On and off throttle"),
                (true, false) => s.t("Charge (pied sur l'accélérateur)", "Load (foot on the throttle)"),
                _ => s.t("Relâché (décélération)", "Released (deceleration)"),
            };
            let layer = s.engine.layer(r);
            rows.push((s.t("Plage de régime", "RPM range"), format!("{} – {}", fmt_int(m.rpm_min, lang), rpm(m.rpm_max)).into(), true));
            rows.push((s.t("Fondu", "Fade"), format!("{} / {} {}", fade(fade_in), fade(fade_out), s.t("tr/min", "rpm")).into(), true));
            rows.push((s.t("Déclenchement", "Trigger"), trigger.into(), false));
            rows.push((s.t("Volume actuel", "Current volume"), format!("{:.2}", layer.map_or(0., |l| l.volume)).into(), true));
            rows.push((s.t("Hauteur actuelle", "Current pitch"), layer.map_or("—".into(), |l| format!("{:.2}", l.pitch)).into(), true));
        }
        let channels = if smp.channels == 1 { "mono".to_string() } else { format!("{} ch", smp.channels) };
        rows.push((
            "Format",
            format!("{} · {} Hz · {channels} · {:.2} s", bank.fsb.codec, fmt_int(smp.frequency as f32, lang), smp.duration_secs()).into(),
            true,
        ));
        match s.leaf {
            Some(Leaf::Car(i)) => rows.push((s.t("Voiture", "Car"), format!("{} ({})", s.engine.events[i].name(), bank.name).into(), false)),
            _ => rows.push((s.t("Banque", "Bank"), bank.name.clone().into(), false)),
        }

        div()
            .flex()
            .flex_col()
            .gap(theme::GAP_LG)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(div().text_size(theme::TEXT_SM).text_color(theme::text_dim()).child(caption))
                    .child(strip(preview, px(76.))),
            )
            .child(div().flex().flex_col().gap(px(10.)).children(rows.into_iter().map(|(k, v, is_mono)| {
                div()
                    .flex()
                    .gap(theme::GAP_MD)
                    .child(div().flex_none().w(px(110.)).text_color(theme::text_dim()).child(k))
                    .child(div().flex_1().min_w(px(0.)).when(is_mono, |d| d.font_family(theme::FONT_MONO)).child(v))
            })))
    }

    fn io_tab(&self, s: &AppState, r: SoundRef) -> gpui::Div {
        let replaced = s.replaced_count();
        let wide = |b: gpui::Stateful<gpui::Div>| b.w_full().h(px(40.)).justify_start().px(px(14.));
        div()
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(s.t("Importer", "Import")))
                    .child(
                        button("import")
                            .w_full()
                            .h(px(56.))
                            .flex_col()
                            .items_start()
                            .gap(px(2.))
                            .child(s.t("Importer un fichier audio pour ce son", "Import an audio file for this sound"))
                            .child(div().text_size(theme::TEXT_SM).text_color(theme::text_dim()).child("WAV, OGG, FLAC ou MP3"))
                            .on_click(on(&self.state, move |s, cx| s.pick_replacement(r, cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(s.t("Exporter", "Export")))
                    .child(
                        wide(button("save-original"))
                            .child(s.t("Exporter le son original (.wav)", "Export the original sound (.wav)"))
                            .on_click(on(&self.state, move |s, cx| s.save_original(r, cx))),
                    )
                    .child(
                        disabled(wide(button("export-replaced")), replaced == 0)
                            .child(s.tf(format!("Exporter les sons remplacés ({replaced})"), format!("Export the replaced sounds ({replaced})")))
                            .when(replaced > 0, |b| b.on_click(on(&self.state, |s, cx| s.export_replaced_files(cx)))),
                    )
                    .child(
                        primary("export-mod")
                            .w_full()
                            .h(px(40.))
                            .child(if s.workspace.is_some() {
                                s.t("Construire le mod (dossier du projet)", "Build the mod (project folder)")
                            } else {
                                s.t("Exporter le mod complet", "Export the full mod")
                            })
                            .on_click(on(&self.state, |s, cx| s.export_mod(cx))),
                    ),
            )
    }
}

pub(super) fn status_text(s: &AppState, r: SoundRef) -> String {
    let file = |f: &std::path::Path| f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let adjusted_original = s.editor.edits.get(&r).is_some_and(|e| e.source == crate::state::EditSource::Original);
    match s.project.as_ref().map(|p| p.status(r)) {
        Some(SoundStatus::Replaced(_)) if adjusted_original => {
            s.t("Son original modifié (volume, hauteur)", "Original sound, adjusted (volume, pitch)").to_string()
        }
        Some(SoundStatus::Replaced(f)) => s.tf(format!("Remplacé par {}", file(&f)), format!("Replaced by {}", file(&f))),
        Some(SoundStatus::Mismatch(f, _)) => s.tf(format!("{} : incompatible", file(&f)), format!("{}: incompatible", file(&f))),
        _ => s.t("Son original du jeu", "Original game sound").to_string(),
    }
}

impl Render for SidePanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        if !s.side_open {
            return self.rail();
        }
        let body = match s.sound {
            None => div().text_color(theme::text_dim()).child(s.t("Sélectionnez un son dans la liste.", "Select a sound in the list.")),
            Some(r) => {
                let context = match s.leaf {
                    Some(Leaf::Car(i)) => s.engine.events.get(i).map(|e| e.name().to_string()).unwrap_or_default(),
                    _ => s.bank_name(r.bank),
                };
                let role = s.project.as_ref().and_then(|p| p.sample(r)).map(|(_, smp)| format!("{:.2} s", smp.duration_secs())).unwrap_or_default();
                div()
                    .flex()
                    .flex_col()
                    .gap(theme::GAP_LG)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(div().font_family(theme::FONT_MONO).text_size(theme::TEXT_LG).font_weight(FontWeight::SEMIBOLD).child(s.sound_name(r)))
                            .child(div().text_size(theme::TEXT_SM).text_color(theme::text_dim()).child(format!("{role} · {context}"))),
                    )
                    .child(match s.side_tab {
                        SideTab::Info => self.info_tab(s, r),
                        SideTab::Edit => super::track_editor::render(s, &self.state, r),
                        SideTab::Io => self.io_tab(s, r),
                    })
            }
        };
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(theme::SIDE_W)
            .min_h(px(0.))
            .bg(theme::sidebar())
            .border_l_1()
            .border_color(theme::divider())
            .child(self.tabs(s))
            .child(div().id("side-body").flex_1().min_h(px(0.)).overflow_y_scroll().p(theme::GAP_LG).child(body))
            .into_any_element()
    }
}
