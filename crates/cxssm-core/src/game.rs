//! Détection automatique de l'installation de CarX Street (Steam).
//!
//! On cherche `fmod_assets_all.bundle` dans :
//! 1. les dossiers Steam par défaut de chaque système ;
//! 2. toutes les bibliothèques Steam déclarées dans `steamapps/libraryfolders.vdf`
//!    (jeux installés sur un autre disque).
//!
//! Disposition Unity : `<jeu>/CarX Street_Data/StreamingAssets/aa/<plateforme>/` (Windows, Linux)
//! ou `<jeu>/CarX Street.app/Contents/Resources/Data/StreamingAssets/aa/<plateforme>/` (macOS).

use std::path::{Path, PathBuf};

pub const BUNDLE_NAME: &str = "fmod_assets_all.bundle";
const GAME_DIR: &str = "CarX Street";

/// Dossiers Steam par défaut (racines contenant `steamapps/`).
pub fn default_steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(p) = std::env::var_os(var) {
                roots.push(PathBuf::from(p).join("Steam"));
            }
        }
        roots.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    } else if let Some(home) = dirs::home_dir() {
        if cfg!(target_os = "macos") {
            roots.push(home.join("Library/Application Support/Steam"));
        } else {
            roots.push(home.join(".local/share/Steam"));
            roots.push(home.join(".steam/steam"));
            roots.push(home.join(".steam/root"));
            // Steam en Flatpak
            roots.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
        }
    }
    roots
}

/// Chemins `"path"` déclarés dans un `libraryfolders.vdf` (format texte KeyValues de Valve).
pub fn parse_library_folders(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|line| {
            let mut parts = line.split('"').filter(|s| !s.trim().is_empty());
            (parts.next()? == "path").then(|| parts.next())?.map(|p| PathBuf::from(p.replace(r"\\", r"\")))
        })
        .collect()
}

/// Toutes les bibliothèques Steam connues (racines par défaut + bibliothèques déclarées).
pub fn steam_libraries() -> Vec<PathBuf> {
    let mut libs: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        let key = p.canonicalize().unwrap_or_else(|_| p.clone());
        if !libs.iter().any(|l| l.canonicalize().unwrap_or_else(|_| l.clone()) == key) {
            libs.push(p);
        }
    };
    for root in default_steam_roots() {
        let vdf = root.join("steamapps/libraryfolders.vdf");
        if let Ok(text) = std::fs::read_to_string(&vdf) {
            for lib in parse_library_folders(&text) {
                push(lib);
            }
        }
        push(root);
    }
    libs
}

/// `StreamingAssets` possibles pour un dossier d'installation du jeu.
fn streaming_assets(game_dir: &Path) -> [PathBuf; 2] {
    [
        game_dir.join("CarX Street_Data/StreamingAssets"),
        game_dir.join("CarX Street.app/Contents/Resources/Data/StreamingAssets"),
    ]
}

/// Bundles trouvés dans un dossier d'installation (toutes plateformes `aa/*`).
pub fn bundles_in_game_dir(game_dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for sa in streaming_assets(game_dir) {
        let Ok(entries) = std::fs::read_dir(sa.join("aa")) else { continue };
        for e in entries.flatten() {
            let candidate = e.path().join(BUNDLE_NAME);
            if candidate.is_file() {
                found.push(candidate);
            }
        }
    }
    found.sort();
    found
}

/// Chemins où le jeu pourrait être installé (existant ou non), pour information.
pub fn candidate_game_dirs() -> Vec<PathBuf> {
    steam_libraries().into_iter().map(|lib| lib.join("steamapps/common").join(GAME_DIR)).collect()
}

/// Tous les `fmod_assets_all.bundle` trouvés sur la machine.
pub fn find_bundles() -> Vec<PathBuf> {
    let mut all: Vec<PathBuf> = Vec::new();
    for dir in candidate_game_dirs() {
        for b in bundles_in_game_dir(&dir) {
            if !all.contains(&b) {
                all.push(b);
            }
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_library_folders() {
        let vdf = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"/home/user/.local/share/Steam"
		"label"		""
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps" { "1114150" "123" }
	}
}"#;
        let libs = parse_library_folders(vdf);
        assert_eq!(libs, vec![PathBuf::from("/home/user/.local/share/Steam"), PathBuf::from(r"D:\SteamLibrary")]);
    }
}
