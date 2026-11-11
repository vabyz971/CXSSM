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

/// Taille de `fmod_assets_all.bundle` déclarée par le catalogue Addressables du jeu
/// (`aa/catalog.json`, à côté du dossier de plateforme). C'est la taille du fichier
/// **d'origine** : un bundle de taille différente a été modifié.
///
/// `None` si le catalogue est absent (bundle copié hors du jeu) ou illisible.
pub fn catalog_bundle_size(bundle: &Path) -> Option<u64> {
    let catalog = bundle.parent()?.parent()?.join("catalog.json");
    let text = std::fs::read_to_string(catalog).ok()?;
    let name = bundle.file_name()?.to_str()?;
    bundle_size_in_catalog(&text, name)
}

/// Lit `m_BundleSize` du bundle `name` dans le texte d'un `catalog.json`.
///
/// Disposition (Addressables 1.x) : `m_EntryDataString` (base64) = i32 n, puis n entrées
/// de 7 i32 (`internal_id, provider, dep_key, dep_hash, data_index, primary_key, type`) ;
/// `data_index` est un décalage dans `m_ExtraDataString` (base64) où se trouve l'objet
/// sérialisé : u8 type, nom d'assembly et de classe (u8 longueur + ASCII), i32 longueur
/// puis le JSON des options (UTF-16).
pub fn bundle_size_in_catalog(catalog: &str, name: &str) -> Option<u64> {
    let c: serde_json::Value = serde_json::from_str(catalog).ok()?;
    let ids = c.get("m_InternalIds")?.as_array()?;
    let suffix = format!("/{name}");
    let id = ids.iter().position(|v| v.as_str().is_some_and(|s| s.ends_with(&suffix)))?;
    let entries = base64_decode(c.get("m_EntryDataString")?.as_str()?)?;
    let extra = base64_decode(c.get("m_ExtraDataString")?.as_str()?)?;
    let i32_at = |d: &[u8], p: usize| d.get(p..p + 4).map(|b| i32::from_le_bytes(b.try_into().expect("4")));
    let count = i32_at(&entries, 0)?.max(0) as usize;
    let data_index = (0..count).find_map(|k| {
        let base = 4 + k * 28;
        (i32_at(&entries, base)? == id as i32).then(|| i32_at(&entries, base + 16)).flatten()
    })?;
    // Objet sérialisé à `data_index`.
    let mut p = usize::try_from(data_index).ok()?;
    p += 1; // type (7 = objet JSON)
    for _ in 0..2 {
        let len = *extra.get(p)? as usize;
        p += 1 + len;
    }
    let len = i32_at(&extra, p)?.max(0) as usize;
    p += 4;
    let raw = extra.get(p..p + len)?;
    let units: Vec<u16> = (0..len / 2).map(|i| u16::from_le_bytes([raw[2 * i], raw[2 * i + 1]])).collect();
    let json: serde_json::Value = serde_json::from_str(&String::from_utf16(&units).ok()?).ok()?;
    json.get("m_BundleSize")?.as_u64()
}

/// Décodage base64 standard (avec ou sans `=` final).
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let bytes: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut acc = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= val(c)? << (18 - 6 * i);
        }
        let n = chunk.len().saturating_sub(1);
        out.extend_from_slice(&acc.to_be_bytes()[1..1 + n]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_base64() {
        assert_eq!(base64_decode("TWFu").unwrap(), b"Man");
        assert_eq!(base64_decode("TWE=").unwrap(), b"Ma");
        assert_eq!(base64_decode("TQ==").unwrap(), b"M");
    }

    /// Catalogue minimal au format Addressables : deux bundles, options en UTF-16.
    #[test]
    fn reads_bundle_size_from_catalog() {
        fn b64(d: &[u8]) -> String {
            const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            d.chunks(3)
                .flat_map(|c| {
                    let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
                    (0..4).map(move |i| if i <= c.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' })
                })
                .collect()
        }
        let object = |size: u64| {
            let mut o = vec![7u8, 3];
            o.extend(b"Asm");
            o.push(3);
            o.extend(b"Cls");
            let json: Vec<u8> = format!("{{\"m_Crc\":0,\"m_BundleSize\":{size}}}").encode_utf16().flat_map(u16::to_le_bytes).collect();
            o.extend((json.len() as i32).to_le_bytes());
            o.extend(json);
            o
        };
        let (a, b) = (object(111), object(209_703_970));
        let mut extra = a.clone();
        extra.extend(&b);
        let mut entries = 2i32.to_le_bytes().to_vec();
        for (id, data) in [(0, 0), (1, a.len() as i32)] {
            for v in [id, 2, -1, 0, data, id, 0] {
                entries.extend(v.to_le_bytes());
            }
        }
        let catalog = format!(
            r#"{{"m_InternalIds":["{{R}}/x/other.bundle","{{R}}/x/fmod_assets_all.bundle"],"m_EntryDataString":"{}","m_ExtraDataString":"{}"}}"#,
            b64(&entries),
            b64(&extra)
        );
        assert_eq!(bundle_size_in_catalog(&catalog, "fmod_assets_all.bundle"), Some(209_703_970));
        assert_eq!(bundle_size_in_catalog(&catalog, "missing.bundle"), None);
    }

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
