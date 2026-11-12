//! Onglet **Édition** du panneau latéral : source du son (fichier ou passage d'un
//! enregistrement de la voiture), volume, hauteur, et génération de toutes les pistes
//! d'une voiture depuis ses enregistrements (expérimental).

use cxssm_core::{SoundRef, SoundStatus};
use gpui::{div, prelude::*, px, Entity, FontWeight, MouseDownEvent, SharedString};

use crate::assets::{icon, Icon};
use crate::state::editor::{GAIN_RANGE, PITCH_RANGE};
use crate::state::{AppState, Drag, EditSource, Leaf, Phase, Playing, Slider, SourceKind};
use crate::theme;
use crate::widgets::{button, disabled, fmt_int, ghost, mono, on, primary, segment, segmented, slider};

fn title(t: &'static str) -> gpui::Div {
    div().font_weight(FontWeight::SEMIBOLD).child(t)
}

fn hint(t: impl Into<SharedString>) -> gpui::Div {
    div().text_size(theme::TEXT_SM).text_color(theme::text_dim()).child(t.into())
}

fn section() -> gpui::Div {
    div().flex().flex_col().gap(px(10.))
}

/// Valeur affichée d'un curseur : texte, position 0..1, remplissage depuis le milieu.
struct Value(String, f32, bool);

/// Ligne « libellé … valeur [↺] » au-dessus d'un curseur.
fn slider_row(s: &AppState, state: &Entity<AppState>, r: SoundRef, which: Slider, label: &'static str, v: Value) -> gpui::Div {
    let Value(value_text, value, centered) = v;
    let id = match which {
        Slider::Gain => "sl-gain",
        Slider::Pitch => "sl-pitch",
        Slider::Passage => "sl-passage",
    };
    let st = state.clone();
    let down = move |e: &MouseDownEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
        st.update(cx, |s, cx| {
            s.begin_slide(r, which);
            s.drag = Some(Drag::Slider(which));
            s.slide(which, e.position.x);
            cx.notify();
        })
    };
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(theme::GAP_SM)
                .child(div().text_color(theme::text_dim()).child(label))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(theme::GAP_XS)
                        .child(mono(value_text))
                        .child(
                            ghost(SharedString::from(format!("{id}-reset")))
                                .size(px(22.))
                                .text_color(theme::text_dim())
                                .child("↺")
                                .on_click(on(state, move |s, cx| s.reset_slider(r, which, cx))),
                        ),
                ),
        )
        .child(slider(id, s.editor.track(which), value, centered, down))
}

/// Enregistrements de la voiture et génération de toutes ses pistes.
fn recordings(s: &AppState, state: &Entity<AppState>) -> gpui::Div {
    let lang = s.lang();
    let row = |phase: Phase, label: &'static str, id: &'static str| {
        let take = s.editor.take(phase);
        let loading = s.editor.loading[phase as usize];
        let detail: SharedString = match take {
            _ if loading => s.t("Analyse du régime…", "Tracking the RPM…").into(),
            Some(t) => format!(
                "{} · {} → {} {}",
                t.path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned()),
                fmt_int(t.range.0, lang),
                fmt_int(t.range.1, lang),
                s.t("tr/min", "rpm")
            )
            .into(),
            None => s.t("Aucun enregistrement", "No recording").into(),
        };
        div()
            .flex()
            .items_center()
            .gap(theme::GAP_SM)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.))
                    .child(label)
                    .child(div().text_size(theme::TEXT_SM).text_color(theme::text_dim()).truncate().child(detail)),
            )
            .child(
                button(id)
                    .h(px(30.))
                    .px(px(10.))
                    .text_size(theme::TEXT_SM)
                    .child(s.t("Choisir…", "Choose…"))
                    .on_click(on(state, move |s, cx| s.pick_take(phase, cx))),
            )
    };
    let can_generate = s.car_map().is_some() && s.editor.takes.iter().any(Option::is_some);
    section()
        .p(theme::GAP_MD)
        .rounded(theme::RADIUS)
        .bg(theme::inset())
        .border_1()
        .border_color(theme::border())
        .child(
            div()
                .flex()
                .items_center()
                .gap(theme::GAP_SM)
                .child(title(s.t("Enregistrements de la voiture", "Car recordings")))
                .child(
                    div()
                        .px(px(6.))
                        .rounded(theme::RADIUS_SM)
                        .bg(theme::accent_tint())
                        .text_size(theme::TEXT_XS)
                        .text_color(theme::accent())
                        .child(s.t("EXPÉRIMENTAL", "EXPERIMENTAL")),
                ),
        )
        .child(hint(s.t(
            "Un balayage continu du régime par phase (WAV). Chaque piste du jeu est recréée au régime où FMOD la joue.",
            "One continuous RPM sweep per phase (WAV). Each game track is rebuilt at the RPM FMOD plays it at.",
        )))
        .child(row(Phase::Acc, s.t("Accélération", "Acceleration"), "take-acc"))
        .child(row(Phase::Dec, s.t("Pied levé", "Deceleration"), "take-dec"))
        .child(
            disabled(primary("generate-car").h(theme::BUTTON_H), !can_generate)
                .child(s.t("Générer toutes les pistes de la voiture", "Generate all of the car's tracks"))
                .when(can_generate, |b| b.on_click(on(state, |s, cx| s.generate_car(cx)))),
        )
}

/// Volume et hauteur du son affiché ; avec `all`, appliqués à toute la sélection.
fn adjustments(s: &AppState, state: &Entity<AppState>, r: SoundRef, all: bool) -> gpui::Div {
    let edit = s.editor.edits.get(&r);
    let (gain, semis) = edit.map_or((0., 0.), |e| (e.gain_db, e.semitones));
    let norm = |v: f32, (lo, hi): (f32, f32)| (v - lo) / (hi - lo);
    section()
        .child(title(s.t("Réglages", "Adjustments")))
        .child(slider_row(s, state, r, Slider::Gain, s.t("Volume", "Volume"), Value(format!("{gain:+.1} dB"), norm(gain, GAIN_RANGE), true)))
        .child(slider_row(
            s,
            state,
            r,
            Slider::Pitch,
            s.t("Hauteur", "Pitch"),
            Value(format!("{semis:+.1} {} (×{:.2})", s.t("demi-ton", "semitone"), (semis / 12.).exp2()), norm(semis, PITCH_RANGE), true),
        ))
        .child(hint(if all {
            s.t(
                "Les valeurs réglées ici remplacent celles de chaque son sélectionné. Chaque son garde sa source (original, fichier ou enregistrement).",
                "The values set here replace those of every selected sound. Each sound keeps its source (original, file or recording).",
            )
        } else {
            s.t(
                "Appliqués au son lui-même : FMOD garde sa propre variation de hauteur selon le régime.",
                "Applied to the sound itself: FMOD keeps its own RPM-driven pitch changes.",
            )
        }))
}

/// Plusieurs sons sélectionnés : leur liste, volume et hauteur communs, restauration.
fn render_many(s: &AppState, state: &Entity<AppState>, r: SoundRef, sel: &[SoundRef]) -> gpui::Div {
    const SHOWN: usize = 24;
    let replaced = sel.iter().filter(|&&x| s.project.as_ref().is_some_and(|p| p.replacements.contains_key(&x))).count();
    let rendering = sel.iter().filter(|x| s.editor.edits.get(x).is_some_and(|e| e.rendering())).count();
    let chips = div().flex().flex_wrap().gap(px(6.)).children(sel.iter().take(SHOWN).map(|&x| {
        let primary = x == r;
        div()
            .px(px(7.))
            .py(px(2.))
            .rounded(theme::RADIUS_SM)
            .border_1()
            .border_color(if primary { theme::accent() } else { theme::border() })
            .bg(if primary { theme::accent_tint() } else { theme::inset() })
            .font_family(theme::FONT_MONO)
            .text_size(theme::TEXT_XS)
            .child(s.sound_name(x))
    }));
    let more = sel.len().saturating_sub(SHOWN);
    div()
        .flex()
        .flex_col()
        .gap(px(20.))
        .child(
            section()
                .p(theme::GAP_MD)
                .rounded(theme::RADIUS)
                .bg(theme::inset())
                .border_1()
                .border_color(theme::border())
                .child(title(s.t("Sélection", "Selection")))
                .child(hint(s.tf(
                    format!("{} sons, dont {replaced} déjà remplacé(s). Ctrl + clic pour en ajouter ou en retirer, Maj + clic pour une plage.", sel.len()),
                    format!("{} sounds, {replaced} already replaced. Ctrl + click to add or remove, Shift + click for a range.", sel.len()),
                )))
                .child(chips)
                .when(more > 0, |d| d.child(hint(s.tf(format!("et {more} autre(s)"), format!("and {more} more"))))),
        )
        .child(adjustments(s, state, r, true))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(theme::GAP_SM)
                .when(rendering > 0, |d| {
                    d.child(hint(s.tf(format!("Rendu de {rendering} son(s)…"), format!("Rendering {rendering} sound(s)…"))))
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme::GAP_SM)
                        .child(
                            disabled(button("restore-all").w_full().h(theme::BUTTON_H), replaced == 0)
                                .child(s.tf(format!("Restaurer les originaux ({replaced})"), format!("Restore originals ({replaced})")))
                                .when(replaced > 0, |b| b.on_click(on(state, |s, cx| s.restore_selected(cx)))),
                        )
                        .child(
                            button("select-one")
                                .w_full()
                                .h(theme::BUTTON_H)
                                .child(s.t("Garder ce son seul", "Keep this sound only"))
                                .on_click(on(state, move |s, cx| s.select_sound(r, cx))),
                        ),
                ),
        )
}

pub fn render(s: &AppState, state: &Entity<AppState>, r: SoundRef) -> gpui::Div {
    let sel = s.selected();
    if sel.len() > 1 {
        return render_many(s, state, r, &sel);
    }
    let Some(p) = &s.project else { return div() };
    let Some((bank, smp)) = p.sample(r) else { return div() };
    let lang = s.lang();
    let status = p.status(r);
    let replaced = !matches!(status, SoundStatus::Original);
    let edit = s.editor.edits.get(&r);
    let is_car = matches!(s.leaf, Some(Leaf::Car(_)));
    let kind = s.source_kind(r);
    let can_record = kind == SourceKind::Recording || s.can_use_recording(r);
    let playing_new = s.playback.playing == Some(Playing::Replacement(r));
    let rendering = edit.is_some_and(|e| e.rendering());

    // ── Source ──
    let src = |id: &'static str, label: &'static str, k: SourceKind| {
        segment(id, label, kind == k).on_click(on(state, move |s, cx| s.set_edit_source(r, k, cx)))
    };
    let source = section().child(title(s.t("Source du son", "Sound source"))).child(
        div().flex().child(
            segmented()
                .child(src("src-orig", s.t("Original", "Original"), SourceKind::Original))
                .child(src("src-file", s.t("Fichier", "File"), SourceKind::File))
                .when(is_car, |d| {
                    d.child(
                        disabled(segment("src-rec", s.t("Enregistrement", "Recording"), kind == SourceKind::Recording), !can_record).when(
                            can_record,
                            |b| b.on_click(on(state, move |s, cx| s.set_edit_source(r, SourceKind::Recording, cx))),
                        ),
                    )
                }),
        ),
    );
    let source = match edit.map(|e| &e.source) {
        Some(EditSource::Recording { phase, rpm }) => {
            let (phase, rpm) = (*phase, *rpm);
            let native = s.native_rpm(r);
            let semis = edit.map_or(0., |e| e.semitones);
            let range = s.passage_range(r, phase, semis).unwrap_or((0., 1.));
            let phase_btn = |id: &'static str, label: &'static str, ph: Phase| {
                let ok = s.editor.take(ph).is_some();
                disabled(segment(id, label, phase == ph), !ok).when(ok, |b| b.on_click(on(state, move |s, cx| s.set_edit_phase(r, ph, cx))))
            };
            source
                .child(
                    div().flex().child(
                        segmented()
                            .child(phase_btn("ph-acc", s.t("Accélération", "Acceleration"), Phase::Acc))
                            .child(phase_btn("ph-dec", s.t("Pied levé", "Deceleration"), Phase::Dec)),
                    ),
                )
                .when_some(native, |d, n| {
                    d.child(
                        div()
                            .flex()
                            .justify_between()
                            .child(div().text_color(theme::text_dim()).child(s.t("Régime naturel du son", "Sound's natural RPM")))
                            .child(mono(format!("{} {}", fmt_int(n, lang), s.t("tr/min", "rpm")))),
                    )
                })
                .child(slider_row(
                    s,
                    state,
                    r,
                    Slider::Passage,
                    s.t("Passage utilisé", "Passage used"),
                    Value(format!("{} {}", fmt_int(rpm, lang), s.t("tr/min", "rpm")), (rpm - range.0) / (range.1 - range.0).max(1.), false),
                ))
                .child(hint(s.t(
                    "Régime de l'enregistrement dont la boucle est tirée ; sa hauteur est ramenée au régime naturel.",
                    "RPM of the recording the loop is taken from; its pitch is brought to the natural RPM.",
                )))
        }
        _ if kind == SourceKind::Original => source.child(hint(s.t(
            "Le son du jeu, avec le volume et la hauteur réglés ci-dessous.",
            "The game's own sound, with the volume and pitch set below.",
        ))),
        _ => {
            source
                .child(
                    div()
                        .p(theme::GAP_MD)
                        .rounded(theme::RADIUS)
                        .bg(theme::inset())
                        .border_1()
                        .border_color(if matches!(status, SoundStatus::Mismatch(..)) { theme::danger() } else { theme::border_hover() })
                        .text_color(theme::text_icon())
                        .child(super::side_panel::status_text(s, r))
                        .when_some(
                            match &status {
                                SoundStatus::Mismatch(_, why) => Some(why.clone()),
                                _ => None,
                            },
                            |d, why| d.child(div().mt(px(6.)).text_size(theme::TEXT_SM).text_color(theme::danger()).child(why)),
                        ),
                )
                .child(
                    primary("replace")
                        .h(theme::BUTTON_H)
                        .child(s.t("Choisir un fichier…", "Choose a file…"))
                        .on_click(on(state, move |s, cx| s.pick_replacement(r, cx))),
                )
        }
    };

    // ── Réglages ──
    let settings = adjustments(s, state, r, false);

    // ── Résultat ──
    let note = edit.and_then(|e| e.note.clone());
    let result = div()
        .flex()
        .flex_col()
        .gap(theme::GAP_SM)
        .when(rendering, |d| d.child(hint(s.t("Rendu du son…", "Rendering the sound…"))))
        .when_some(note, |d, n| match n {
            Ok(t) => d.child(hint(t)),
            Err(e) => d.child(div().text_size(theme::TEXT_SM).text_color(theme::danger()).child(e)),
        })
        .child(
            div()
                .flex()
                .gap(theme::GAP_SM)
                .child(
                    disabled(button("play-new").flex_1().h(theme::BUTTON_H), !replaced)
                        .child(icon(if playing_new { Icon::Stop } else { Icon::Play }, px(12.), theme::text()))
                        .child(if playing_new { s.t("Arrêter", "Stop") } else { s.t("Écouter", "Play") })
                        .when(replaced, |b| b.on_click(on(state, move |s, cx| s.toggle_replacement(r, cx)))),
                )
                .child(
                    disabled(button("restore").h(theme::BUTTON_H), !replaced && edit.is_none())
                        .child(s.t("Restaurer l'original", "Restore original"))
                        .when(replaced || edit.is_some(), |b| b.on_click(on(state, move |s, cx| s.restore_original(r, cx)))),
                ),
        );

    // ── Format ──
    let spec = |k: &'static str, v: String| {
        div()
            .flex()
            .justify_between()
            .gap(theme::GAP_MD)
            .child(div().text_color(theme::text_dim()).child(k))
            .child(div().font_family(theme::FONT_MONO).child(v))
    };
    let format = section()
        .child(title(s.t("Caractéristiques à respecter", "Required format")))
        .child(hint(s.t(
            "Le son de remplacement est encodé par FMOD avec ces réglages.",
            "The replacement sound is encoded by FMOD with these settings.",
        )))
        .child(spec("Codec", bank.fsb.codec.to_string()))
        .child(spec(s.t("Fréquence", "Sample rate"), format!("{} Hz", fmt_int(smp.frequency as f32, lang))))
        .child(spec(s.t("Canaux", "Channels"), smp.channels.to_string()))
        .child(spec(s.t("Durée", "Length"), format!("{:.2} s", smp.duration_secs())));

    div()
        .flex()
        .flex_col()
        .gap(px(20.))
        .when(is_car, |d| d.child(recordings(s, state)))
        .child(source)
        .child(settings)
        .child(result)
        .child(format)
}
