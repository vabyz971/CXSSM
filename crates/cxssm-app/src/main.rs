//! CXSSM – CarX Street Sound Mod : interface graphique GPUI.

// Windows : application graphique, sans fenêtre de console.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod assets;
mod i18n;
mod player;
mod state;
mod theme;
mod ui;
mod widgets;

use gpui::{px, size, App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions};

fn main() {
    prefer_x11();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,blade_graphics=error")).init();

    // `cxssm-app [chemin/vers/fmod_assets_all.bundle]` : ouvre directement le bundle.
    let initial = std::env::args_os().nth(1).map(std::path::PathBuf::from);

    Application::new().with_assets(assets::Assets).run(move |cx: &mut App| {
        assets::load_fonts(cx);
        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
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
            |_, cx| {
                // État partagé, puis la vue racine qui assemble les composants.
                let state = cx.new(|cx| state::AppState::new(cx, initial));
                cx.new(|cx| ui::Workspace::new(state, cx))
            },
        )
        .expect("ouverture de la fenêtre");
        cx.activate(true);
    });
}

/// GPUI 0.2.2 sous Wayland ne redessine que sur signal du compositeur : avec niri
/// (et d'autres), la fenêtre peut rester figée après une tâche de fond (chargement
/// du bundle…) jusqu'à un redimensionnement. Le backend X11 (XWayland) rafraîchit
/// sur sa propre minuterie et n'a pas ce problème : on le préfère quand il est
/// disponible. `CXSSM_WAYLAND=1` force Wayland.
fn prefer_x11() {
    #[cfg(target_os = "linux")]
    if std::env::var_os("CXSSM_WAYLAND").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var_os("DISPLAY").is_some()
    {
        // Appelé tout au début de `main`, avant la création de tout thread.
        std::env::remove_var("WAYLAND_DISPLAY");
    }
}
