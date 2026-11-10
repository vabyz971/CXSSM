//! Mise à jour depuis les **Releases GitHub** du projet.
//!
//! 1. [`check`] interroge `api.github.com/repos/<REPO>/releases/latest` et compare la
//!    version publiée (tag `vX.Y.Z`) à la version de l'exécutable en cours.
//! 2. [`install`] télécharge l'archive de la plateforme (même nommage que le workflow
//!    `release.yml`), en extrait le binaire et remplace l'exécutable en cours
//!    (ainsi que l'outil `cxssm` s'il est installé à côté).
//!
//! Les installations Nix (`/nix/store`, en lecture seule) se mettent à jour avec Nix.

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use semver::Version;
use serde::Deserialize;

use crate::error::{Error, Result};

/// Dépôt GitHub des releases.
pub const REPO: &str = "vabyz971/CXSSM";
/// Version de cet exécutable.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Taille maximale acceptée pour une archive (protection contre un téléchargement aberrant).
const MAX_ARCHIVE: u64 = 300 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: Version,
    pub tag: String,
    /// Page de la release sur GitHub (notes de version).
    pub page: String,
    /// Archive correspondant à ce système, si elle a été publiée.
    pub asset: Option<Asset>,
}

/// Façon dont cet exécutable peut être mis à jour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallMethod {
    /// Remplacement automatique de l'exécutable.
    SelfReplace,
    /// Installé par Nix : `nix profile upgrade` / mise à jour du flake.
    Nix,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Fin du nom d'archive pour ce système (cf. `.github/workflows/release.yml`).
pub fn platform_suffix() -> Option<&'static str> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x86_64.tar.gz")
    } else if cfg!(all(windows, target_arch = "x86_64")) {
        Some("windows-x86_64.zip")
    } else if cfg!(target_os = "macos") {
        Some("macos-universal.zip")
    } else {
        None
    }
}

pub fn install_method() -> InstallMethod {
    match std::env::current_exe() {
        Ok(p) if p.starts_with("/nix/store") => InstallMethod::Nix,
        _ => InstallMethod::SelfReplace,
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .user_agent(format!("cxssm-updater/{CURRENT_VERSION}"))
        .build()
        .into()
}

fn err(e: impl std::fmt::Display) -> Error {
    Error::Update(e.to_string())
}

fn parse_version(tag: &str) -> Option<Version> {
    Version::parse(tag.trim_start_matches(['v', 'V'])).ok()
}

/// Dernière release publiée si elle est **plus récente** que cet exécutable.
/// `Ok(None)` : à jour, ou aucune release publiée pour l'instant.
pub fn check() -> Result<Option<Release>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut resp = match agent().get(&url).header("Accept", "application/vnd.github+json").call() {
        Ok(r) => r,
        // 404 : le dépôt n'a encore aucune release.
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(e) => return Err(err(e)),
    };
    let rel: GhRelease = serde_json::from_str(&resp.body_mut().read_to_string().map_err(err)?).map_err(err)?;
    if rel.draft || rel.prerelease {
        return Ok(None);
    }
    let Some(version) = parse_version(&rel.tag_name) else {
        return Err(err(format!("tag de release invalide : {}", rel.tag_name)));
    };
    let current = Version::parse(CURRENT_VERSION).map_err(err)?;
    if version <= current {
        return Ok(None);
    }
    let asset = platform_suffix().and_then(|suffix| {
        rel.assets
            .into_iter()
            .find(|a| a.name.ends_with(suffix))
            .map(|a| Asset { name: a.name, url: a.browser_download_url, size: a.size })
    });
    Ok(Some(Release { version, tag: rel.tag_name, page: rel.html_url, asset }))
}

/// Nom de l'exécutable graphique dans l'archive.
fn app_name() -> &'static str {
    if cfg!(windows) {
        "cxssm-app.exe"
    } else {
        "cxssm-app"
    }
}

fn cli_name() -> &'static str {
    if cfg!(windows) {
        "cxssm.exe"
    } else {
        "cxssm"
    }
}

/// Extrait les fichiers dont le nom (dernier segment du chemin) figure dans `wanted`.
fn extract(archive: &[u8], name: &str, wanted: &[&str]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    let keep = |path: &str| -> Option<String> {
        let file = path.rsplit('/').next()?;
        wanted.contains(&file).then(|| file.to_string())
    };
    if name.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(Cursor::new(archive)).map_err(err)?;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(err)?;
            let name = f.name().map_err(err)?.into_owned();
            if let Some(file) = keep(&name).filter(|_| f.is_file()) {
                let mut data = Vec::new();
                f.read_to_end(&mut data)?;
                out.push((file, data));
            }
        }
    } else {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(archive)));
        for entry in tar.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_string_lossy().into_owned();
            if let Some(file) = keep(&path).filter(|_| entry.header().entry_type().is_file()) {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                out.push((file, data));
            }
        }
    }
    Ok(out)
}

fn write_executable(path: &Path, data: &[u8]) -> Result<()> {
    std::fs::write(path, data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

/// Télécharge et installe la release. Il faut ensuite relancer l'application.
pub fn install(release: &Release) -> Result<PathBuf> {
    if install_method() == InstallMethod::Nix {
        return Err(err("installation Nix : mettez à jour avec `nix profile upgrade` ou votre flake"));
    }
    let asset = release
        .asset
        .as_ref()
        .ok_or_else(|| err("aucune archive publiée pour ce système"))?;
    if asset.size > MAX_ARCHIVE {
        return Err(err(format!("archive trop volumineuse ({} o)", asset.size)));
    }
    log::info!("téléchargement de {}", asset.url);
    let mut resp = agent().get(&asset.url).call().map_err(err)?;
    let bytes = resp.body_mut().with_config().limit(MAX_ARCHIVE).read_to_vec().map_err(err)?;

    let files = extract(&bytes, &asset.name, &[app_name(), cli_name()])?;
    let app = files
        .iter()
        .find(|(n, _)| n == app_name())
        .ok_or_else(|| err(format!("{} absent de l'archive", app_name())))?;

    let exe = std::env::current_exe()?;
    let dir = exe.parent().ok_or_else(|| err("dossier de l'exécutable introuvable"))?;

    // 1. Application graphique : remplacement sûr de l'exécutable en cours d'exécution.
    let staged = dir.join(format!(".{}.update", app_name()));
    write_executable(&staged, &app.1)?;
    let replaced = self_replace::self_replace(&staged);
    let _ = std::fs::remove_file(&staged);
    replaced?;

    // 2. Outil en ligne de commande, s'il est installé à côté.
    if let Some((_, cli)) = files.iter().find(|(n, _)| n == cli_name()) {
        let target = dir.join(cli_name());
        if target.is_file() {
            let tmp = dir.join(format!(".{}.update", cli_name()));
            write_executable(&tmp, cli)?;
            std::fs::rename(&tmp, &target)?;
        }
    }
    log::info!("mise à jour {} installée", release.tag);
    Ok(exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_suffix() {
        assert_eq!(parse_version("v1.2.3"), Some(Version::new(1, 2, 3)));
        assert_eq!(parse_version("0.4.0"), Some(Version::new(0, 4, 0)));
        assert!(parse_version("nightly").is_none());
        assert!(Version::parse(CURRENT_VERSION).is_ok());
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        assert_eq!(platform_suffix(), Some("linux-x86_64.tar.gz"));
    }

    #[test]
    fn extracts_binary_from_tar_gz() {
        let mut tar_data = Vec::new();
        {
            let gz = flate2::write::GzEncoder::new(&mut tar_data, flate2::Compression::fast());
            let mut b = tar::Builder::new(gz);
            for (path, body) in [("cxssm-v9.9.9-linux-x86_64/cxssm-app", b"APP".as_slice()), ("cxssm-v9.9.9-linux-x86_64/README.md", b"doc")] {
                let mut h = tar::Header::new_gnu();
                h.set_size(body.len() as u64);
                h.set_mode(0o755);
                h.set_cksum();
                b.append_data(&mut h, path, body).unwrap();
            }
            b.into_inner().unwrap().finish().unwrap();
        }
        let files = extract(&tar_data, "x.tar.gz", &["cxssm-app"]).unwrap();
        assert_eq!(files, vec![("cxssm-app".to_string(), b"APP".to_vec())]);
    }
}
