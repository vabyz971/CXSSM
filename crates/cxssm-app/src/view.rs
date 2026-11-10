//! Vue principale : barre supérieure, liste des sons du bundle, panneau de détails.

use std::path::PathBuf;
use std::time::Duration;

use cxssm_core::{encoder::EncoderConfig, export, preview, ModProject, SoundCategory, SoundRef, SoundStatus};
use gpui::{
    div, point, prelude::*, px, uniform_list, ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent,
    MouseMoveEvent, Pixels, SharedString, UniformListScrollHandle, Window,
};

use crate::player::Player;
use crate::theme;
use crate::widgets::{badge, ghost_button, primary_button, section_title, spec_row};

const CATEGORIES: [SoundCategory; 7] = [
    SoundCategory::Engine,
    SoundCategory::Turbo,
    SoundCategory::Exhaust,
    SoundCategory::Music,
    SoundCategory::Voice,
    SoundCategory::Ui,
    SoundCategory::Other,
];

/// Hauteur d'une ligne de la liste (doit correspondre au rendu de `render_row`).
const ROW_HEIGHT: f32 = 34.;

/// Ligne pré-calculée de la liste (évite de reparcourir le projet à chaque frame).
#[derive(Clone)]
struct Row {
    r: SoundRef,
    bank: SharedString,
    name: SharedString,
    spec: SharedString,
    duration: SharedString,
}

/// Ce qui est en cours de lecture.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Playing {
    Original(SoundRef),
    Replacement(SoundRef),
}

pub struct MainView {
    project: Option<ModProject>,
    rows: Vec<Row>,
    /// Nombre de sons : [tous, puis une case par entrée de CATEGORIES].
    counts: [usize; CATEGORIES.len() + 1],
    filter: Option<SoundCategory>,
    scroll: UniformListScrollHandle,
    /// Glisser-déposer du curseur de la barre de défilement en cours.
    dragging_scrollbar: bool,
    selected: Option<SoundRef>,
    status: SharedString,
    busy: bool,
    encoder: EncoderConfig,
    player: Option<Player>,
    playing: Option<Playing>,
    /// Son en cours de décodage (affiche « … » sur la ligne).
    decoding: Option<SoundRef>,
}

impl MainView {
    pub fn new(cx: &mut Context<Self>, initial: Option<PathBuf>) -> Self {
        let player = Player::new().map_err(|e| log::warn!("pas de sortie audio : {e}")).ok();
        let mut view = Self {
            project: None,
            rows: Vec::new(),
            counts: [0; CATEGORIES.len() + 1],
            filter: None,
            scroll: UniformListScrollHandle::new(),
            dragging_scrollbar: false,
            selected: None,
            status: "Ouvrez fmod_assets_all.bundle pour commencer".into(),
            busy: false,
            encoder: EncoderConfig {
                fsbankcl: std::env::var_os("CXSSM_FSBANKCL").map(PathBuf::from),
                quality: 80,
            },
            player,
            playing: None,
            decoding: None,
        };
        if let Some(path) = initial {
            view.open_path(path, cx);
        }
        view
    }

    // ───────────────────────────── ouverture / export ─────────────────────────────

    fn open_dialog(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let file = rfd::AsyncFileDialog::new()
                .set_title("Ouvrir fmod_assets_all.bundle")
                .add_filter("AssetBundle Unity", &["bundle"])
                .pick_file()
                .await;
            let _ = this.update(cx, |v, cx| match file {
                Some(f) => v.open_path(f.path().to_path_buf(), cx),
                None => {
                    v.status = "Ouverture annulée".into();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        log::info!("ouverture de {}", path.display());
        self.busy = true;
        self.status = format!("Analyse de {}…", path.display()).into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { ModProject::open(path) }).await;
            let _ = this.update(cx, |v, cx| {
                v.busy = false;
                match result {
                    Ok(p) => {
                        v.status =
                            format!("{} banques, {} sons trouvés", p.banks.len(), p.sounds().count()).into();
                        v.project = Some(p);
                        v.selected = None;
                        v.rebuild_rows();
                    }
                    Err(e) => {
                        log::error!("ouverture : {e}");
                        v.status = format!("Erreur : {e}").into();
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn export_mod(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.project.as_ref().is_none_or(|p| p.replacements.is_empty()) {
            self.status = "Aucun son remplacé : rien à exporter".into();
            cx.notify();
            return;
        }
        cx.spawn(async move |this, cx| {
            let Some(dir) = rfd::AsyncFileDialog::new()
                .set_title("Dossier de sortie du mod (Steam Workshop)")
                .pick_folder()
                .await
            else {
                return;
            };
            // Le projet est emprunté le temps de l'export puis rendu à la vue.
            let taken = this.update(cx, |v, cx| {
                v.busy = true;
                v.status = "Export en cours…".into();
                cx.notify();
                (v.project.take(), v.encoder.clone())
            });
            let Ok((Some(project), enc)) = taken else { return };
            let out = dir.path().to_path_buf();
            let (project, result) = cx
                .background_spawn(async move {
                    let r = export::export(&project, &enc, &out);
                    (project, r)
                })
                .await;
            let _ = this.update(cx, |v, cx| {
                v.project = Some(project);
                v.busy = false;
                v.status = match result {
                    Ok(p) => format!("Mod exporté : {}", p.display()).into(),
                    Err(e) => format!("Erreur d'export : {e}").into(),
                };
                cx.notify();
            });
        })
        .detach();
    }

    // ───────────────────────────── remplacement ─────────────────────────────

    fn pick_replacement(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.selected else { return };
        cx.spawn(async move |this, cx| {
            let Some(file) = rfd::AsyncFileDialog::new()
                .set_title("Choisir le son de remplacement")
                .add_filter("Audio", &["wav", "ogg", "flac", "mp3"])
                .pick_file()
                .await
            else {
                return;
            };
            let _ = this.update(cx, |v, cx| {
                if let Some(p) = v.project.as_mut() {
                    p.set_replacement(r, file.path().to_path_buf());
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn restore_original(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let (Some(p), Some(r)) = (self.project.as_mut(), self.selected) {
            p.clear_replacement(r);
            cx.notify();
        }
    }

    // ───────────────────────────── écoute ─────────────────────────────

    /// ▶ / ■ sur un son d'origine du jeu.
    fn toggle_original(&mut self, r: SoundRef, cx: &mut Context<Self>) {
        if self.playing == Some(Playing::Original(r)) {
            self.stop(cx);
            return;
        }
        let Some((codec, sample)) = self.project.as_ref().and_then(|p| {
            let (bank, s) = p.sample(r)?;
            Some((bank.fsb.codec, s.clone()))
        }) else {
            return;
        };
        self.decoding = Some(r);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let decoded = cx.background_spawn(async move { preview::decode(codec, &sample) }).await;
            let _ = this.update(cx, |v, cx| {
                v.decoding = None;
                let res = decoded
                    .map_err(|e| e.to_string())
                    .and_then(|d| v.player_mut()?.play_bytes(d.bytes));
                v.after_play(res, Playing::Original(r), cx);
            });
        })
        .detach();
    }

    fn toggle_replacement(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.selected else { return };
        if self.playing == Some(Playing::Replacement(r)) {
            self.stop(cx);
            return;
        }
        let Some(path) = self.project.as_ref().and_then(|p| p.replacements.get(&r).cloned()) else {
            return;
        };
        let res = self.player_mut().and_then(|p| p.play_file(&path));
        self.after_play(res, Playing::Replacement(r), cx);
    }

    fn player_mut(&mut self) -> Result<&mut Player, String> {
        self.player.as_mut().ok_or_else(|| "aucune sortie audio disponible".to_string())
    }

    fn after_play(&mut self, res: Result<(), String>, what: Playing, cx: &mut Context<Self>) {
        match res {
            Ok(()) => {
                self.playing = Some(what);
                self.watch_playback(cx);
            }
            Err(e) => self.status = format!("Lecture impossible : {e}").into(),
        }
        cx.notify();
    }

    /// Remet le bouton ▶ quand le son est terminé.
    fn watch_playback(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(200)).await;
            let done = this
                .update(cx, |v, cx| {
                    let finished = !v.player.as_ref().is_some_and(Player::is_playing);
                    if finished && v.playing.is_some() {
                        v.playing = None;
                        cx.notify();
                    }
                    finished || v.playing.is_none()
                })
                .unwrap_or(true);
            if done {
                break;
            }
        })
        .detach();
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.player.as_mut() {
            p.stop();
        }
        self.playing = None;
        cx.notify();
    }

    /// Enregistre le son d'origine en .ogg/.wav (pour le retravailler dans Audacity).
    fn save_original(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.selected else { return };
        let Some((bank, sample)) = self.project.as_ref().and_then(|p| p.sample(r)) else { return };
        let default_name = format!("{}_{}", bank.name, sample.name);
        let (codec, sample) = (bank.fsb.codec, sample.clone());
        cx.spawn(async move |this, cx| {
            let decoded = cx.background_spawn(async move { preview::decode(codec, &sample) }).await;
            let decoded = match decoded {
                Ok(d) => d,
                Err(e) => {
                    let _ = this.update(cx, |v, cx| {
                        v.status = format!("Décodage impossible : {e}").into();
                        cx.notify();
                    });
                    return;
                }
            };
            let Some(file) = rfd::AsyncFileDialog::new()
                .set_title("Enregistrer le son d'origine")
                .set_file_name(format!("{default_name}.{}", decoded.extension))
                .save_file()
                .await
            else {
                return;
            };
            let res = std::fs::write(file.path(), &decoded.bytes);
            let _ = this.update(cx, |v, cx| {
                v.status = match res {
                    Ok(()) => format!("Enregistré : {}", file.path().display()).into(),
                    Err(e) => format!("Erreur : {e}").into(),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn rebuild_rows(&mut self) {
        self.counts = [0; CATEGORIES.len() + 1];
        let Some(p) = &self.project else {
            self.rows.clear();
            return;
        };
        let mut rows = Vec::new();
        for (r, bank, s) in p.sounds() {
            let category = SoundCategory::guess(&bank.name, &s.name);
            self.counts[0] += 1;
            if let Some(i) = CATEGORIES.iter().position(|c| *c == category) {
                self.counts[i + 1] += 1;
            }
            if self.filter.is_some_and(|f| f != category) {
                continue;
            }
            let channels = if s.channels == 1 { "mono".to_string() } else { format!("{} can.", s.channels) };
            rows.push(Row {
                r,
                bank: bank.name.clone().into(),
                name: s.name.clone().into(),
                spec: format!("{} Hz · {channels}", s.frequency).into(),
                duration: format!("{:.2} s", s.duration_secs()).into(),
            });
        }
        self.rows = rows;
    }

    fn set_filter(&mut self, filter: Option<SoundCategory>, cx: &mut Context<Self>) {
        self.filter = filter;
        self.rebuild_rows();
        self.scroll.0.borrow().base_handle.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    /// Positionne la liste d'après l'ordonnée de la souris sur la barre de défilement.
    fn scroll_to_mouse(&mut self, mouse_y: Pixels, cx: &mut Context<Self>) {
        let handle = self.scroll.0.borrow().base_handle.clone();
        let bounds = handle.bounds();
        let max = handle.max_offset().height;
        if bounds.size.height <= px(0.) || max <= px(0.) {
            return;
        }
        let ratio = ((mouse_y - bounds.origin.y) / bounds.size.height).clamp(0., 1.);
        handle.set_offset(point(px(0.), -(max * ratio)));
        cx.notify();
    }

    // ───────────────────────────── rendu ─────────────────────────────

    fn render_topbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let source = self
            .project
            .as_ref()
            .map(|p| p.source.display().to_string())
            .unwrap_or_else(|| "Aucun bundle chargé".into());
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(64.))
            .px_6()
            .bg(theme::panel())
            .border_b_1()
            .border_color(theme::border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .overflow_hidden()
                    .child(div().text_xl().font_weight(FontWeight::EXTRA_BOLD).child("CXSSM"))
                    .child(div().ml_6().text_sm().text_color(theme::text_dim()).child(source)),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .gap_2()
                    .child(ghost_button("open", "OUVRIR LE BUNDLE").on_click(cx.listener(Self::open_dialog)))
                    .child(primary_button("export", "EXPORTER LE MOD").on_click(cx.listener(Self::export_mod))),
            )
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let item = |id: usize, label: &'static str, cat: Option<SoundCategory>, active: bool| {
            div()
                .id(("cat", id))
                .flex()
                .justify_between()
                .px_4()
                .py_2()
                .text_sm()
                .cursor_pointer()
                .border_l_4()
                .border_color(if active { theme::accent() } else { gpui::transparent_black().into() })
                .when(active, |d| d.bg(theme::panel_hi()))
                .hover(|s| s.bg(theme::panel_hi()))
                .text_color(if active { theme::text() } else { theme::text_dim() })
                .child(label)
                .child(self.counts[id].to_string())
                .on_click(cx.listener(move |v, _, _, cx| v.set_filter(cat, cx)))
        };
        let mut side = div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(210.))
            .h_full()
            .py_4()
            .bg(theme::panel())
            .border_r_1()
            .border_color(theme::border())
            .child(div().px_4().pb_2().child(section_title("CATÉGORIES")))
            .child(item(0, "Tous les sons", None, self.filter.is_none()));
        for (i, c) in CATEGORIES.iter().enumerate() {
            side = side.child(item(i + 1, c.label(), Some(*c), self.filter == Some(*c)));
        }
        side
    }

    /// Barre de défilement verticale : affichage du curseur + clic / glisser.
    fn render_scrollbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let handle = self.scroll.0.borrow().base_handle.clone();
        let viewport = handle.bounds().size.height;
        let content = px(ROW_HEIGHT * self.rows.len() as f32);
        let max = handle.max_offset().height;
        // Taille et position du curseur, en fraction de la hauteur de la piste.
        let (thumb_size, thumb_pos) = if content > viewport && viewport > px(0.) && max > px(0.) {
            let size = (viewport / content).clamp(0.04, 1.);
            let pos = ((-handle.offset().y) / max).clamp(0., 1.) * (1. - size);
            (size, pos)
        } else {
            (1., 0.)
        };
        let visible = thumb_size < 1.;
        let active = self.dragging_scrollbar;

        div()
            .id("scrollbar")
            .flex_none()
            .w(px(12.))
            .h_full()
            .relative()
            .border_l_1()
            .border_color(theme::border())
            .when(visible, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(px(2.))
                        .w(px(7.))
                        .top(gpui::relative(thumb_pos))
                        .h(gpui::relative(thumb_size))
                        .rounded_sm()
                        .bg(if active { theme::accent() } else { theme::border() })
                        .hover(|s| s.bg(theme::text_dim())),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|v, e: &MouseDownEvent, _, cx| {
                        v.dragging_scrollbar = true;
                        v.scroll_to_mouse(e.position.y, cx);
                    }),
                )
            })
    }

    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let header = div()
            .flex()
            .flex_none()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(theme::border())
            .text_xs()
            .text_color(theme::text_dim())
            .child(div().w(px(36.)))
            .child(div().w(px(200.)).child("BANQUE"))
            .child(div().flex_1().child("SON"))
            .child(div().w(px(150.)).child("FORMAT"))
            .child(div().w(px(80.)).child("DURÉE"))
            .child(div().w(px(30.)));

        let body = if self.project.is_none() {
            self.render_empty(cx).into_any_element()
        } else {
            div()
                .flex()
                .size_full()
                .child(
                    uniform_list(
                        "sounds",
                        self.rows.len(),
                        cx.processor(|this, range: std::ops::Range<usize>, _window, cx| {
                            range.map(|i| this.render_row(i, cx)).collect()
                        }),
                    )
                    .track_scroll(self.scroll.clone())
                    .flex_1()
                    .h_full(),
                )
                .child(self.render_scrollbar(cx))
                .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .overflow_hidden()
            .child(header)
            .child(div().flex_1().overflow_hidden().child(body))
    }

    fn render_row(&self, i: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let row = self.rows[i].clone();
        let r = row.r;
        let selected = self.selected == Some(r);
        let play_label = if self.decoding == Some(r) {
            "…"
        } else if self.playing == Some(Playing::Original(r)) {
            "■"
        } else {
            "▶"
        };
        let dot = match self.project.as_ref().map(|p| p.status(r)) {
            Some(SoundStatus::Replaced(_)) => theme::ok(),
            Some(SoundStatus::Mismatch(..)) => theme::warn(),
            _ => gpui::transparent_black().into(),
        };
        div()
            .id(("row", i))
            .flex()
            .w_full()
            .items_center()
            .h(px(ROW_HEIGHT))
            .px_4()
            .text_sm()
            .cursor_pointer()
            .when(selected, |d| d.bg(theme::panel_hi()))
            .hover(|s| s.bg(theme::panel_hi()))
            .on_click(cx.listener(move |v, _, _, cx| {
                v.selected = Some(r);
                cx.notify();
            }))
            .child(
                div().w(px(36.)).child(
                    div()
                        .id(("play", i))
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(theme::accent())
                        .border_1()
                        .border_color(theme::border())
                        .hover(|s| s.bg(theme::accent()).text_color(theme::accent_text()))
                        .child(play_label)
                        .on_click(cx.listener(move |v, _, _, cx| {
                            cx.stop_propagation();
                            v.selected = Some(r);
                            v.toggle_original(r, cx);
                        })),
                ),
            )
            .child(div().w(px(200.)).text_color(theme::text_dim()).child(row.bank))
            .child(div().flex_1().text_color(theme::text()).child(row.name))
            .child(div().w(px(150.)).text_color(theme::text_dim()).child(row.spec))
            .child(div().w(px(80.)).text_color(theme::text_dim()).child(row.duration))
            .child(div().w(px(18.)).child(div().size(px(8.)).rounded_full().bg(dot)))
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().items_center().justify_center().child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .p_8()
                .border_1()
                .border_color(theme::border())
                .bg(theme::panel())
                .child(div().text_lg().font_weight(FontWeight::BOLD).child("AUCUN BUNDLE CHARGÉ"))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme::text_dim())
                        .child("CarX Street_Data/StreamingAssets/aa/<plateforme>/fmod_assets_all.bundle"),
                )
                .child(primary_button("open-empty", "OUVRIR LE BUNDLE").on_click(cx.listener(Self::open_dialog))),
        )
    }

    fn render_details(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = div()
            .flex()
            .flex_col()
            .flex_none()
            .gap_4()
            .w(px(340.))
            .h_full()
            .p_5()
            .bg(theme::panel())
            .border_l_1()
            .border_color(theme::border());

        let (Some(p), Some(r)) = (self.project.as_ref(), self.selected) else {
            return panel
                .child(section_title("DÉTAILS"))
                .child(div().text_sm().text_color(theme::text_dim()).child("Sélectionnez un son dans la liste."));
        };
        let Some((bank, s)) = p.sample(r) else { return panel };
        let codec = p.codec(r).map(|c| c.to_string()).unwrap_or_default();
        let status = p.status(r);
        let playing_orig = self.playing == Some(Playing::Original(r));
        let playing_repl = self.playing == Some(Playing::Replacement(r));

        let replacement = match &status {
            SoundStatus::Original => {
                div().text_sm().text_color(theme::text_dim()).child("Son d'origine (non modifié)")
            }
            SoundStatus::Replaced(f) => div()
                .flex()
                .flex_col()
                .gap_1()
                .child(badge("COMPATIBLE", theme::ok()))
                .child(div().text_sm().child(file_name(f))),
            SoundStatus::Mismatch(f, why) => div()
                .flex()
                .flex_col()
                .gap_1()
                .child(badge("INCOMPATIBLE", theme::warn()))
                .child(div().text_sm().child(file_name(f)))
                .child(div().text_xs().text_color(theme::warn()).child(why.clone())),
        };
        let has_replacement = !matches!(status, SoundStatus::Original);

        panel
            .child(section_title("SON SÉLECTIONNÉ"))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_lg().font_weight(FontWeight::BOLD).child(s.name.clone()))
                    .child(div().text_sm().text_color(theme::text_dim()).child(bank.name.clone())),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        primary_button("play-orig", if playing_orig { "■ STOP" } else { "▶ ÉCOUTER" })
                            .on_click(cx.listener(move |v, _, _, cx| v.toggle_original(r, cx))),
                    )
                    .child(ghost_button("save-orig", "ENREGISTRER…").on_click(cx.listener(Self::save_original))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(section_title("CARACTÉRISTIQUES À RESPECTER"))
                    .child(spec_row("Codec", codec))
                    .child(spec_row("Fréquence", format!("{} Hz", s.frequency)))
                    .child(spec_row("Canaux", s.channels.to_string()))
                    .child(spec_row("Durée", format!("{:.2} s", s.duration_secs())))
                    .child(spec_row("Boucle", s.loop_range.map_or("non".into(), |(a, b)| format!("{a} → {b}")))),
            )
            .child(div().flex().flex_col().gap_2().child(section_title("REMPLACEMENT")).child(replacement))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(primary_button("replace", "REMPLACER…").on_click(cx.listener(Self::pick_replacement)))
                    .when(has_replacement, |d| {
                        d.child(
                            ghost_button("play-repl", if playing_repl { "■ STOP" } else { "▶ NOUVEAU" })
                                .on_click(cx.listener(Self::toggle_replacement)),
                        )
                        .child(ghost_button("restore", "RÉTABLIR").on_click(cx.listener(Self::restore_original)))
                    }),
            )
    }

    fn render_statusbar(&self) -> impl IntoElement {
        let replaced = self.project.as_ref().map_or(0, |p| p.replacements.len());
        let fsbank = if self.encoder.fsbankcl.is_some() {
            "encodeur FMOD : OK"
        } else {
            "encodeur FMOD : non configuré (CXSSM_FSBANKCL)"
        };
        div()
            .flex()
            .flex_none()
            .justify_between()
            .h(px(28.))
            .px_4()
            .items_center()
            .bg(theme::panel())
            .border_t_1()
            .border_color(theme::border())
            .text_xs()
            .text_color(theme::text_dim())
            .child(format!("{}{}", if self.busy { "⏳ " } else { "" }, self.status))
            .child(format!("{replaced} son(s) remplacé(s)  ·  {fsbank}"))
    }
}

fn file_name(p: &std::path::Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Sous Wayland (niri…), GPUI ne redessine que sur « frame callback » du compositeur ;
        // sans changement à l'écran la boucle s'arrête et le résultat d'une tâche de fond
        // (chargement, décodage, fin de lecture) ne s'afficherait qu'au prochain
        // redimensionnement. On garde donc la boucle active tant qu'une tâche est en cours.
        if self.busy || self.decoding.is_some() || self.playing.is_some() {
            window.request_animation_frame();
        }
        div()
            .id("root")
            .flex()
            .flex_col()
            .size_full()
            .when(self.dragging_scrollbar, |d| {
                d.on_mouse_move(cx.listener(|v, e: &MouseMoveEvent, _, cx| {
                    if e.pressed_button == Some(MouseButton::Left) {
                        v.scroll_to_mouse(e.position.y, cx);
                    } else {
                        v.dragging_scrollbar = false;
                        cx.notify();
                    }
                }))
            })
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|v, _, _, cx| {
                    if v.dragging_scrollbar {
                        v.dragging_scrollbar = false;
                        cx.notify();
                    }
                }),
            )
            .bg(theme::bg())
            .text_color(theme::text())
            .child(self.render_topbar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(self.render_sidebar(cx))
                    .child(self.render_list(cx))
                    .child(self.render_details(cx)),
            )
            .child(self.render_statusbar())
    }
}
