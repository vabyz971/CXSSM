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
    /// (`CXSSM_FMOD_API`, ou déduite de `CXSSM_FSBANKCL`).
    EngineMap {
        bundle: PathBuf,
        /// Nom de l'évènement (ex. `eng_V8_MGTC`).
        #[arg(long)]
        event: Option<String>,
        /// Pas du balayage en tr/min.
        #[arg(long, default_value_t = 250.)]
        step: f32,
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
            let api = engine::find_api_dir().context("dossier api FMOD introuvable : définissez CXSSM_FMOD_API")?;
            let lib = engine::load_library(&api)?;
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
