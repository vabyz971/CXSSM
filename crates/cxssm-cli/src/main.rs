//! `cxssm` : outil en ligne de commande, pratique pour tester sur le vrai
//! `fmod_assets_all.bundle` sans lancer l'interface.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use cxssm_core::{encoder::EncoderConfig, export, ModProject, SoundCategory, SoundRef};

#[derive(Parser)]
#[command(name = "cxssm", version, about = "CarX Street Sound Mod – outil CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Vérifie si une nouvelle version de CXSSM est publiée sur GitHub.
    CheckUpdate,
    /// Cherche l'installation de CarX Street (bibliothèques Steam) et le FMOD Engine.
    FindGame,
    /// Affiche la structure brute du bundle (nœuds, tailles).
    Bundle { bundle: PathBuf },
    /// Extrait toutes les banques FMOD (.bank brutes, y compris Master) dans un dossier.
    ExtractBanks {
        bundle: PathBuf,
        #[arg(long, default_value = "output/banks")]
        out: PathBuf,
    },
    /// Liste les évènements moteur (une voiture = un évènement) ou, avec `--event`,
    /// la carte des sons utilisés selon le régime. Nécessite l'API FMOD Studio
    /// (détectée automatiquement, ou `CXSSM_FMOD_API`).
    EngineMap {
        bundle: PathBuf,
        /// Nom de l'évènement (ex. `eng_V8_MGTC`).
        #[arg(long)]
        event: Option<String>,
        /// Pas du balayage en tr/min.
        #[arg(long, default_value_t = 250.)]
        step: f32,
    },
    /// Diagnostic : volume, hauteur et tête de lecture de chaque couche, régime par régime.
    EngineProbe {
        bundle: PathBuf,
        #[arg(long)]
        event: String,
        /// Pied levé (Load = 0) au lieu de l'accélération.
        #[arg(long)]
        off_load: bool,
        #[arg(long, default_value_t = 500.)]
        step: f32,
        /// Mises à jour du mixage par pas (≈ 21 ms chacune).
        #[arg(long, default_value_t = 8)]
        updates: u32,
    },
    /// [Expérimental] Recrée les boucles moteur d'une voiture depuis des enregistrements
    /// continus (balayage du régime). Écrit les WAV générés et, avec `--export`, le bundle.
    Generate {
        bundle: PathBuf,
        /// Évènement moteur de la voiture (voir `engine-map`), ex. `992`.
        #[arg(long)]
        event: String,
        /// Balayage en accélération (WAV).
        #[arg(long)]
        acc: PathBuf,
        /// Balayage pied levé (WAV) ; à défaut, l'accélération sert pour tout.
        #[arg(long)]
        dec: Option<PathBuf>,
        /// Harmonique dominante : explosions par tour (3 pour un 6 cylindres, 2 pour un 4, 4 pour un V8).
        #[arg(long, default_value_t = cxssm_core::generator::DEFAULT_ORDER)]
        order: f32,
        #[arg(long, default_value = "output/generated")]
        out: PathBuf,
        /// Exporte aussi le bundle modifié (nécessite l'encodeur FMOD).
        #[arg(long)]
        export: bool,
        #[arg(long, default_value_t = 80)]
        quality: u8,
    },
    /// Décode des sons du jeu en WAV (`--sound` répétable).
    ExtractSounds {
        bundle: PathBuf,
        #[arg(long)]
        bank: String,
        #[arg(long, required = true)]
        sound: Vec<String>,
        #[arg(long, default_value = "output/sounds")]
        out: PathBuf,
    },
    /// Liste les banques FMOD et leurs sons.
    List { bundle: PathBuf },
    /// Remplace un son et exporte le bundle modifié.
    Replace {
        bundle: PathBuf,
        /// Nom de la banque (voir `list`), ex. `V8`.
        #[arg(long)]
        bank: String,
        /// Nom exact du son dans la banque (voir `list`), ex. `sample_0003`.
        #[arg(long)]
        sound: String,
        /// Fichier audio de remplacement (WAV conseillé).
        #[arg(long)]
        file: PathBuf,
        /// Dossier de sortie.
        #[arg(long, default_value = "output")]
        out: PathBuf,
        /// Chemin de l'encodeur FMOD : `bin/fsbank` (Linux) ou `bin/fsbankcl.exe` (Windows).
        #[arg(long, env = "CXSSM_FSBANKCL")]
        fsbankcl: Option<PathBuf>,
        #[arg(long, default_value_t = 80)]
        quality: u8,
    },
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    match Cli::parse().cmd {
        Cmd::CheckUpdate => {
            use cxssm_core::update;
            println!("Version installée : {}", update::CURRENT_VERSION);
            match update::check()? {
                None => println!("À jour (aucune release plus récente sur github.com/{}).", update::REPO),
                Some(r) => {
                    println!("Nouvelle version : {} → {}", r.tag, r.page);
                    match r.asset {
                        Some(a) => println!("Archive : {} ({} Mo)", a.name, a.size / 1_000_000),
                        None => println!("Pas d'archive publiée pour ce système."),
                    }
                }
            }
        }
        Cmd::FindGame => {
            use cxssm_core::game;
            println!("Bibliothèques Steam examinées :");
            for dir in game::candidate_game_dirs() {
                println!("  {} {}", if dir.is_dir() { "✔" } else { "·" }, dir.display());
            }
            let found = game::find_bundles();
            if found.is_empty() {
                println!("Aucun {} trouvé.", game::BUNDLE_NAME);
            }
            for b in found {
                println!("→ {}", b.display());
            }
            println!("\nFMOD Engine :");
            let installs = cxssm_core::FmodInstall::detect();
            if installs.is_empty() {
                println!("  aucune installation détectée (https://www.fmod.com/download)");
            }
            for f in installs {
                let enc = f.fsbank.as_ref().map_or("encodeur absent".into(), |p| p.display().to_string());
                println!("→ {}  ({enc})", f.root.display());
            }
        }
        Cmd::Bundle { bundle } => {
            let b = cxssm_unity::Bundle::open(&bundle).context("ouverture du bundle")?;
            println!("UnityFS v{} – Unity {} – {:?}", b.format_version, b.unity_revision, b.compression);
            for n in &b.nodes {
                println!("  {:>12} o  {}", n.data.len(), n.path);
            }
        }
        Cmd::ExtractBanks { bundle, out } => {
            use cxssm_unity::{SerializedFile, TextAsset, CLASS_TEXT_ASSET};
            let b = cxssm_unity::Bundle::open(&bundle).context("ouverture du bundle")?;
            std::fs::create_dir_all(&out)?;
            let mut n = 0;
            for node in b.nodes.iter().filter(|n| !n.path.ends_with(".resS")) {
                let Ok(sf) = SerializedFile::parse(node.data.clone()) else { continue };
                for obj in sf.objects_of_class(CLASS_TEXT_ASSET) {
                    let ta = TextAsset::parse(sf.object_data(obj), sf.endian)?;
                    if ta.is_fmod_bank() {
                        let name = if ta.name.ends_with(".bank") { ta.name.clone() } else { format!("{}.bank", ta.name) };
                        std::fs::write(out.join(&name), &ta.script)?;
                        n += 1;
                    }
                }
            }
            println!("{n} banque(s) écrite(s) dans {}", out.display());
        }
        Cmd::EngineMap { bundle, event, step } => {
            use cxssm_core::engine;
            let fmod = cxssm_core::FmodInstall::resolve(None)
                .context("FMOD Engine introuvable : installez-le ou définissez CXSSM_FMOD_API")?;
            let lib = engine::load_library(&fmod.api)?;
            let p = ModProject::open(&bundle)?;
            let data = engine::EngineData::from_project(&p);
            let events = engine::list_events(&lib, &data)?;
            match event {
                None => {
                    for e in &events {
                        let params: Vec<_> = e.info.params.iter().map(|x| x.name.as_str()).collect();
                        println!("{:<8} {:<28} {}", p.banks[e.bank].name, e.name(), params.join(", "));
                    }
                    println!("{} évènements moteur", events.len());
                }
                Some(name) => {
                    let Some(e) = events.iter().find(|e| e.name() == name) else { bail!("évènement « {name} » introuvable") };
                    println!("{} ({})", e.info.path, p.banks[e.bank].name);
                    for m in engine::sweep(&lib, &data, e, step)? {
                        let s = &p.banks[m.sound.bank].fsb.samples[m.sound.sample];
                        println!(
                            "  {:<12} {:<11} {:>5.0} → {:>5.0} tr/min   pic {:>5.0} (vol {:.2})",
                            s.name,
                            if m.on_load { "accélération" } else { "pied levé" },
                            m.rpm_min, m.rpm_max, m.rpm_peak, m.peak_volume
                        );
                    }
                }
            }
        }
        Cmd::EngineProbe { bundle, event, off_load, step, updates } => {
            use cxssm_core::engine;
            let fmod = cxssm_core::FmodInstall::resolve(None).context("FMOD Engine introuvable")?;
            let lib = engine::load_library(&fmod.api)?;
            let p = ModProject::open(&bundle)?;
            let data = engine::EngineData::from_project(&p);
            let events = engine::list_events(&lib, &data)?;
            let Some(e) = events.iter().find(|e| e.name() == event) else { bail!("évènement « {event} » introuvable") };
            let (min, max) = e.rpm_range();
            let rpms: Vec<f32> = std::iter::successors(Some(min), |r| Some(r + step)).take_while(|&r| r <= max).collect();
            for (rpm, layers) in engine::probe(&lib, &data, e, !off_load, &rpms, updates)? {
                let cells: Vec<String> = layers
                    .iter()
                    .map(|l| {
                        let name = l.sound.map_or("?".into(), |r| p.banks[r.bank].fsb.samples[r.sample].name.clone());
                        format!("{name} v{:.2} p{:.3} @{}", l.volume, l.pitch, l.position_pcm)
                    })
                    .collect();
                println!("{rpm:>6.0}  {}", cells.join("  |  "));
            }
        }
        Cmd::Generate { bundle, event, acc, dec, order, out, export: do_export, quality } => {
            use cxssm_core::{engine, generator as gen};
            let fmod = cxssm_core::FmodInstall::resolve(None).context("FMOD Engine introuvable")?;
            let lib = engine::load_library(&fmod.api)?;
            let mut p = ModProject::open(&bundle)?;
            let data = engine::EngineData::from_project(&p);
            let events = engine::list_events(&lib, &data)?;
            let Some(e) = events.iter().find(|e| e.name() == event) else { bail!("évènement « {event} » introuvable") };

            // Enregistrements et suivi du régime.
            let mut recs = Vec::new();
            for (label, path) in [("accélération", Some(&acc)), ("pied levé", dec.as_ref())] {
                let Some(path) = path else { continue };
                let rec = gen::Recording::load(path)?;
                let tr = gen::track_rpm(&rec, order)?;
                let (lo, hi) = tr.range().context("régime introuvable dans l'enregistrement")?;
                let ok = tr.valid.iter().filter(|&&v| v).count();
                println!(
                    "{label:<12} {:>5.1} s  régime {lo:>5.0} → {hi:>5.0} tr/min  ({ok}/{} images exploitables)",
                    rec.duration_secs(),
                    tr.valid.len()
                );
                let step = (tr.rpm.len() / 12).max(1);
                let pts: Vec<String> = (0..tr.rpm.len())
                    .step_by(step)
                    .map(|i| format!("{:.1}s:{:.0}{}", tr.time(i, rec.rate), tr.rpm[i], if tr.valid[i] { "" } else { "?" }))
                    .collect();
                println!("             {}", pts.join("  "));
                if std::env::var_os("CXSSM_GEN_DEBUG").is_some() {
                    for i in 0..tr.rpm.len() {
                        let (a, b) = tr.salience[i];
                        println!("    {:5.2}s {:6.0} {a:5.1} {b:5.1}", tr.time(i, rec.rate), tr.rpm[i]);
                    }
                }
                recs.push((rec, tr));
            }

            // Un son peut servir dans les deux phases : on prend celle où il est le plus fort.
            let mut entries: Vec<engine::MapEntry> = Vec::new();
            for m in engine::sweep(&lib, &data, e, 100.)? {
                match entries.iter_mut().find(|x| x.sound == m.sound) {
                    Some(x) if x.peak_volume >= m.peak_volume => {}
                    Some(x) => *x = m,
                    None => entries.push(m),
                }
            }
            std::fs::create_dir_all(&out)?;
            println!("\n{} ({} sons utilisés)", e.info.path, entries.len());
            let mut generated = Vec::new();
            for m in &entries {
                let bank = &p.banks[m.sound.bank];
                let s = &bank.fsb.samples[m.sound.sample];
                let phase = if m.on_load { "accélération" } else { "pied levé" };
                let head = format!("  {:<12} {phase:<12} {:>5.0}–{:<5.0}", s.name, m.rpm_min, m.rpm_max);
                let Some(native) = m.native_rpm() else {
                    println!("{head}  hauteur fixe : conservé");
                    continue;
                };
                let spec = gen::LoopSpec {
                    rpm: native,
                    rate: s.frequency,
                    channels: s.channels,
                    seconds: (s.duration_secs() as f32).clamp(1., 2.5),
                    rms: gen::sample_rms(bank.fsb.codec, s)?,
                };
                // Enregistrement de la même phase en priorité, l'autre s'il ne couvre pas ce régime.
                let order = if m.on_load || recs.len() < 2 { [0, 1] } else { [1, 0] };
                let mut result: Result<(usize, gen::Generated), String> = Err(String::new());
                for k in order.into_iter().filter(|&k| k < recs.len()) {
                    let (rec, tr) = &recs[k];
                    match gen::make_loop(rec, tr, spec) {
                        Ok(g) => {
                            result = Ok((k, g));
                            break;
                        }
                        // On garde le message de l'enregistrement de la même phase.
                        Err(err) if k == order[0] => result = Err(err.to_string()),
                        Err(_) => {}
                    }
                }
                match result {
                    Ok((k, g)) => {
                        let path = out.join(format!("{}_{}.wav", bank.name, s.name));
                        gen::write_wav(&path, &g)?;
                        println!(
                            "{head}  naturel {native:>5.0} ← {:>5.0} tr/min à {:>4.1} s ({}, hauteur ×{:.2}, {:.2} s)",
                            g.source_rpm,
                            g.source_time,
                            if k == 0 { "accélération" } else { "pied levé" },
                            g.shift,
                            g.pcm.len() as f32 / f32::from(g.channels) / g.rate as f32,
                        );
                        generated.push((m.sound, path));
                    }
                    Err(err) => println!("{head}  naturel {native:>5.0} tr/min : {err} → original conservé"),
                }
            }
            // Les sons d'une banque sont partagés : prévenir si d'autres voitures les utilisent.
            let mut shared: Vec<(String, Vec<String>)> = Vec::new();
            for other in events.iter().filter(|o| o.bank == e.bank && o.info.path != e.info.path) {
                let used = engine::sweep(&lib, &data, other, 500.)?;
                let names: Vec<String> = generated
                    .iter()
                    .filter(|(r, _)| used.iter().any(|m| m.sound == *r))
                    .map(|(r, _)| p.banks[r.bank].fsb.samples[r.sample].name.clone())
                    .collect();
                if !names.is_empty() {
                    shared.push((other.name().to_string(), names));
                }
            }
            if !shared.is_empty() {
                println!("\n⚠ Sons partagés avec d'autres voitures de la banque (elles changeront aussi) :");
                for (car, names) in shared {
                    println!("  {car:<12} {}", names.join(", "));
                }
            }
            if do_export {
                let fsbankcl = fmod.fsbank.clone();
                for (r, path) in generated {
                    p.set_replacement(r, path);
                }
                let path = export::export(&p, &EncoderConfig { fsbankcl, quality }, &out)?;
                println!("bundle écrit : {}", path.display());
            }
        }
        Cmd::ExtractSounds { bundle, bank, sound, out } => {
            let p = ModProject::open(&bundle)?;
            let Some(b) = p.banks.iter().find(|b| b.name == bank) else { bail!("banque « {bank} » introuvable") };
            std::fs::create_dir_all(&out)?;
            for name in sound {
                let Some(s) = b.fsb.samples.iter().find(|s| s.name == name) else { bail!("son « {name} » introuvable") };
                let d = cxssm_core::preview::decode(b.fsb.codec, s)?;
                let path = out.join(format!("{bank}_{name}.{}", d.extension));
                std::fs::write(&path, &d.bytes)?;
                println!("{}", path.display());
            }
        }
        Cmd::List { bundle } => {
            let p = ModProject::open(&bundle)?;
            for (i, bank) in p.banks.iter().enumerate() {
                println!(
                    "\n[{i}] {}  ({} sons, codec {})",
                    bank.name,
                    bank.fsb.samples.len(),
                    bank.fsb.codec
                );
                for s in &bank.fsb.samples {
                    println!(
                        "    {:<40} {:>6} Hz  {} can.  {:>7.2} s  [{}]",
                        s.name,
                        s.frequency,
                        s.channels,
                        s.duration_secs(),
                        SoundCategory::guess(&bank.name, &s.name).label()
                    );
                }
            }
        }
        Cmd::Replace { bundle, bank, sound, file, out, fsbankcl, quality } => {
            let fsbankcl = fsbankcl.or_else(|| cxssm_core::FmodInstall::resolve(None).and_then(|f| f.fsbank));
            let mut p = ModProject::open(&bundle)?;
            let r: SoundRef = match p.sounds().find(|(_, b, s)| b.name == bank && s.name == sound) {
                Some((r, _, _)) => r,
                None => bail!("son « {bank} / {sound} » introuvable"),
            };
            p.set_replacement(r, file);
            println!("statut : {:?}", p.status(r));
            let path = export::export(&p, &EncoderConfig { fsbankcl, quality }, &out)?;
            println!("bundle écrit : {}", path.display());
        }
    }
    Ok(())
}
