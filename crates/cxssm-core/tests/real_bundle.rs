//! Tests sur le vrai `fmod_assets_all.bundle` (ignorés par défaut).
//!
//! ```sh
//! CXSSM_TEST_BUNDLE=/chemin/fmod_assets_all.bundle cargo test -p cxssm-core -- --ignored
//! ```

use cxssm_core::ModProject;
use cxssm_fmod::Fsb5;

fn bundle_path() -> Option<String> {
    std::env::var("CXSSM_TEST_BUNDLE").ok()
}

/// Ré-écrire un FSB5 sans modification doit redonner les mêmes octets.
#[test]
#[ignore]
fn fsb5_roundtrip_is_identical() {
    let Some(path) = bundle_path() else { return };
    let p = ModProject::open(path).expect("ouverture");
    let mut diffs = Vec::new();
    for b in &p.banks {
        let orig = b.bank.fsb_bytes();
        let re = b.fsb.to_bytes();
        // Le chunk SND peut contenir du padding après le FSB5.
        if orig.get(..re.len()) != Some(&re[..]) {
            let first = orig.iter().zip(&re).position(|(a, b)| a != b);
            let u = |d: &[u8], o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
            diffs.push(format!(
                "{} : {} o → {} o, 1re diff {:?} | shdr {}→{} names {}→{} data {}→{} | 0x3C+shdr+names mod 32 = {}",
                b.name, orig.len(), re.len(), first,
                u(orig, 12), u(&re, 12), u(orig, 16), u(&re, 16), u(orig, 20), u(&re, 20),
                (0x3C + u(orig, 12) + u(orig, 16)) % 32
            ));
        }
        Fsb5::parse(&re).expect("relecture");
    }
    assert!(diffs.is_empty(), "{} banque(s) différente(s) :\n{}", diffs.len(), diffs.join("\n"));
}

/// Le premier son de chaque banque doit pouvoir être décodé pour la pré-écoute
/// (on mesure aussi le temps, qui conditionne la réactivité du bouton ▶).
#[test]
#[ignore]
fn sounds_decode_for_preview() {
    let Some(path) = bundle_path() else { return };
    let p = ModProject::open(path).expect("ouverture");
    let mut errors = Vec::new();
    for b in &p.banks {
        let s = &b.fsb.samples[0];
        let t = std::time::Instant::now();
        let res = cxssm_core::preview::decode(b.fsb.codec, &b.fsb.samples[0]);
        eprintln!("{:<32} {:>6.1} s audio, {} can. → {:?}", b.name, s.duration_secs(), s.channels, t.elapsed());
        match res {
            Ok(d) if d.bytes.starts_with(b"RIFF") => {}
            Ok(_) => errors.push(format!("{} : sortie inattendue", b.name)),
            Err(e) => errors.push(format!("{} : {e}", b.name)),
        }
    }
    assert!(errors.is_empty(), "{} erreur(s) :\n{}", errors.len(), errors.join("\n"));
}

#[test]
#[ignore]
fn decode_one_short_sound() {
    let Some(path) = bundle_path() else { return };
    let p = ModProject::open(path).expect("ouverture");
    let b = &p.banks[0];
    let t = std::time::Instant::now();
    let d = cxssm_core::preview::decode(b.fsb.codec, &b.fsb.samples[0]).expect("décodage");
    eprintln!("{} #0 ({:.2} s) → {} o en {:?}", b.name, b.fsb.samples[0].duration_secs(), d.bytes.len(), t.elapsed());
}
