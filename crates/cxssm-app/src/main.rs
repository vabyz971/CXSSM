//! CXSSM – CarX Street Sound Mod : interface graphique GPUI.

mod player;
mod theme;
mod view;
mod widgets;

use gpui::{px, size, App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,blade_graphics=error")).init();

    // `cxssm-app [chemin/vers/fmod_assets_all.bundle]` : ouvre directement le bundle.
    let initial = std::env::args_os().nth(1).map(std::path::PathBuf::from);

    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1360.), px(820.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("cxssm".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some("CXSSM — CarX Street Sound Mod".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|cx| view::MainView::new(cx, initial)),
        )
        .expect("ouverture de la fenêtre");
        cx.activate(true);
    });
}
