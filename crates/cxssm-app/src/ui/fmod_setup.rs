//! Tutoriel d'installation du **FMOD Engine** quand il est introuvable sur le poste :
//! lien de téléchargement, décompression / installation selon le système, puis choix
//! du dossier. Affiché en fenêtre au démarrage et dans Préférences › FMOD.
//!
//! CXSSM ne peut pas redistribuer FMOD (licence) : l'utilisateur l'installe lui-même.

use gpui::{div, prelude::*, px, Context, Entity, FontWeight, SharedString, Window};

use crate::assets::{icon_sm, Icon};
use crate::state::AppState;
use crate::theme;
use crate::widgets::{button, on, primary};

/// Page de téléchargement officielle (compte FMOD gratuit requis).
pub const DOWNLOAD_URL: &str = "https://www.fmod.com/download";

/// Étape numérotée du tutoriel.
fn step(n: u8, title: &'static str, body: impl IntoElement) -> impl IntoElement {
    div()
        .flex()
        .gap(theme::GAP_MD)
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(26.))
                .rounded(px(13.))
                .bg(theme::accent())
                .text_color(theme::on_accent())
                .font_weight(FontWeight::SEMIBOLD)
                .child(n.to_string()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.))
                .gap(px(6.))
                .child(div().pt(px(3.)).font_weight(FontWeight::SEMIBOLD).child(title))
                .child(body),
        )
}

fn text(t: impl Into<SharedString>) -> gpui::Div {
    div().text_color(theme::text_dim()).child(t.into())
}

/// Chemin ou commande en police à chasse fixe.
fn code(t: impl Into<SharedString>) -> gpui::Div {
    div()
        .px(theme::GAP_SM)
        .py(px(5.))
        .rounded(theme::RADIUS_SM)
        .bg(theme::field())
        .border_1()
        .border_color(theme::border())
        .font_family(theme::FONT_MONO)
        .text_size(theme::TEXT_SM)
        .child(t.into())
}

/// Instructions d'installation pour le système en cours.
fn install_steps(s: &AppState) -> gpui::Div {
    let d = div().flex().flex_col().gap(px(6.));
    if cfg!(windows) {
        d.child(text(s.t(
            "Lancez l'installateur téléchargé (.exe). Dossier d'installation par défaut :",
            "Run the downloaded installer (.exe). Default installation folder:",
        )))
        .child(code(r"C:\Program Files (x86)\FMOD SoundSystem\FMOD Studio API Windows"))
    } else if cfg!(target_os = "macos") {
        d.child(text(s.t(
            "Ouvrez le fichier .dmg et copiez le dossier « FMOD Programmers API », par exemple dans Applications :",
            "Open the .dmg file and copy the “FMOD Programmers API” folder, for example into Applications:",
        )))
        .child(code("/Applications/FMOD Programmers API"))
    } else {
        d.child(text(s.t(
            "Décompressez l'archive .tar.gz, par exemple dans votre dossier Téléchargements :",
            "Extract the .tar.gz archive, for example in your Downloads folder:",
        )))
        .child(code("tar -xzf fmodstudioapi*linux.tar.gz"))
        .child(text(s.t("Vous obtenez un dossier comme :", "You get a folder like:")))
        .child(code("~/Downloads/fmodstudioapi20400linux"))
    }
}

/// Contenu du tutoriel (sans cadre), réutilisé par la fenêtre et les préférences.
pub fn guide(s: &AppState, state: &Entity<AppState>) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(18.))
        .child(
            div()
                .p(theme::GAP_MD)
                .rounded(theme::RADIUS)
                .bg(theme::accent_tint())
                .border_1()
                .border_color(theme::accent_edge())
                .child(s.t(
                    "FMOD Engine est introuvable sur ce poste. CXSSM en a besoin pour encoder vos sons et faire \
                     tourner le simulateur moteur, mais sa licence interdit de l'inclure : installez-le en 3 étapes.",
                    "FMOD Engine was not found on this computer. CXSSM needs it to encode your sounds and run the \
                     engine simulator, but its license forbids bundling it: install it in 3 steps.",
                )),
        )
        .child(step(
            1,
            s.t("Téléchargez le FMOD Engine (gratuit)", "Download FMOD Engine (free)"),
            div()
                .flex()
                .flex_col()
                .gap(theme::GAP_SM)
                .child(text(s.t(
                    "Créez un compte gratuit sur fmod.com, puis dans la section « FMOD Engine » téléchargez \
                     la version pour votre système (2.02 ou plus récente, testé avec 2.04).",
                    "Create a free account on fmod.com, then in the “FMOD Engine” section download the \
                     version for your system (2.02 or newer, tested with 2.04).",
                )))
                .child(
                    div().flex().child(
                        button("fmod-download")
                            .h(theme::BUTTON_H)
                            .font_weight(FontWeight::MEDIUM)
                            .child(icon_sm(Icon::Download, theme::text()))
                            .child(s.t("Ouvrir la page de téléchargement", "Open the download page"))
                            .on_click(|_, _, cx| cx.open_url(DOWNLOAD_URL)),
                    ),
                )
                .child(div().font_family(theme::FONT_MONO).text_size(theme::TEXT_SM).text_color(theme::accent()).child(DOWNLOAD_URL)),
        ))
        .child(step(2, s.t("Décompressez-le ou installez-le", "Extract or install it"), install_steps(s)))
        .child(step(
            3,
            s.t("Indiquez son dossier à CXSSM", "Point CXSSM to its folder"),
            div()
                .flex()
                .flex_col()
                .gap(theme::GAP_SM)
                .child(text(s.t(
                    "Choisissez le dossier qui contient « api » et « bin ». S'il se trouve dans Téléchargements, \
                     Applications, /opt ou Program Files, « Rechercher à nouveau » le trouve tout seul.",
                    "Choose the folder that contains “api” and “bin”. If it is in Downloads, Applications, \
                     /opt or Program Files, “Search again” finds it on its own.",
                )))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(theme::GAP_SM)
                        .child(
                            primary("fmod-choose")
                                .h(theme::BUTTON_H)
                                .child(icon_sm(Icon::Folder, theme::on_accent()))
                                .child(s.t("Choisir le dossier…", "Choose the folder…"))
                                .on_click(on(state, |s, cx| s.choose_fmod_dir(cx))),
                        )
                        .child(
                            button("fmod-retry")
                                .h(theme::BUTTON_H)
                                .font_weight(FontWeight::MEDIUM)
                                .child(s.t("Rechercher à nouveau", "Search again"))
                                .on_click(on(state, |s, cx| s.retry_fmod_detection(cx))),
                        ),
                )
                .when_some(s.fmod_error.clone(), |d, e| d.child(div().text_size(theme::TEXT_SM).text_color(theme::danger()).child(e))),
        ))
}

/// Fenêtre du tutoriel, ouverte au démarrage quand FMOD est introuvable.
pub struct FmodSetup {
    state: Entity<AppState>,
}

impl FmodSetup {
    pub fn new(state: Entity<AppState>) -> Self {
        Self { state }
    }
}

impl Render for FmodSetup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let dialog = div()
            .id("fmod-dialog")
            .flex()
            .flex_col()
            .w_full()
            .max_w(px(680.))
            .max_h(gpui::relative(0.92))
            .rounded(theme::RADIUS_XL)
            .bg(theme::panel())
            .border_1()
            .border_color(theme::border())
            .overflow_hidden()
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .gap(theme::GAP_LG)
                    .px(px(20.))
                    .py(theme::GAP_LG)
                    .border_b_1()
                    .border_color(theme::divider())
                    .child(div().text_size(theme::TEXT_TITLE).font_weight(FontWeight::SEMIBOLD).child(s.t("Installer FMOD Engine", "Install FMOD Engine")))
                    .child(
                        button("fmod-later")
                            .px(px(16.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(icon_sm(Icon::Close, theme::text()))
                            .child(s.t("Plus tard", "Later"))
                            .on_click(on(&self.state, |s, cx| s.set_fmod_help(false, cx))),
                    ),
            )
            .child(div().id("fmod-guide").flex_shrink().min_h(px(0.)).overflow_y_scroll().px(px(24.)).py(px(22.)).child(guide(s, &self.state)))
            .child(
                div()
                    .flex_none()
                    .px(px(24.))
                    .py(theme::GAP_MD)
                    .border_t_1()
                    .border_color(theme::divider())
                    .text_size(theme::TEXT_SM)
                    .text_color(theme::text_faint())
                    .child(s.t(
                        "Sans FMOD, vous pouvez quand même parcourir et écouter les sons du bundle. Ce tutoriel reste \
                         accessible dans Préférences › FMOD.",
                        "Without FMOD you can still browse and listen to the bundle's sounds. This guide stays \
                         available in Preferences › FMOD.",
                    )),
            );

        div()
            .id("fmod-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p(px(32.))
            .bg(theme::backdrop())
            .occlude()
            .on_click(on(&self.state, |s, cx| s.set_fmod_help(false, cx)))
            .child(dialog)
    }
}
