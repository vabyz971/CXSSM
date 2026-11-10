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
