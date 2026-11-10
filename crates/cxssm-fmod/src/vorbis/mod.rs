//! Décodage **FMOD Vorbis → PCM 16 bits** (pré-écoute, export des originaux).
//!
//! FMOD stocke des paquets Vorbis bruts, préfixés par leur taille (`u16` LE),
//! sans en-têtes Ogg. Il manque donc :
//! - l'en-tête d'identification : reconstruit à partir de la fréquence et des canaux ;
//! - l'en-tête « setup » (codebooks) : retrouvé par le CRC32 stocké dans le chunk
//!   d'en-tête de l'échantillon, dans la table [`lookup`].
//!
//! Les paquets sont ensuite décodés directement par `lewton` : aucun ré-encodage.

mod lookup;

use lewton::audio::{read_audio_packet_generic, PreviousWindowRight};
use lewton::header::{read_header_ident, read_header_setup};
use lewton::samples::InterleavedSamples;

use crate::error::{Error, Result};
use crate::fsb5::Sample;

/// Tailles de blocs utilisées par FMOD : 2^8 = 256 et 2^11 = 2048.
const BLOCK_SIZES: (u8, u8) = (8, 11);

/// Décode un échantillon FMOD Vorbis en PCM 16 bits entrelacé.
pub fn decode_pcm16(sample: &Sample) -> Result<Vec<i16>> {
    let crc = sample
        .vorbis_crc
        .ok_or_else(|| Error::Incompatible("CRC Vorbis absent de l'en-tête".into()))?;
    let setup_data = lookup::VORBIS_LOOKUP
        .get(&crc)
        .ok_or_else(|| Error::Incompatible(format!("en-tête Vorbis inconnu (CRC {crc:08X})")))?;
    let channels = sample.channels as u8;

    let ident = read_header_ident(&ident_header(sample.frequency, channels))
        .map_err(|e| Error::Incompatible(format!("en-tête Vorbis : {e:?}")))?;
    let setup = read_header_setup(setup_data, channels, BLOCK_SIZES)
        .map_err(|e| Error::Incompatible(format!("setup Vorbis : {e:?}")))?;

    let mut pcm = Vec::with_capacity(sample.num_samples as usize * channels as usize);
    let mut window = PreviousWindowRight::new();
    let d = &sample.data;
    let mut p = 0;
    while p + 2 <= d.len() {
        let size = u16::from_le_bytes([d[p], d[p + 1]]) as usize;
        p += 2;
        // 0 ou 0xFFFF : fin du flux (le reste n'est que du padding).
        if size == 0 || size == 0xFFFF || p + size > d.len() {
            break;
        }
        let packet = &d[p..p + size];
        p += size;
        let block: InterleavedSamples<i16> = read_audio_packet_generic(&ident, &setup, packet, &mut window)
            .map_err(|e| Error::Incompatible(format!("paquet Vorbis : {e:?}")))?;
        pcm.extend_from_slice(&block.samples);
    }
    // Le dernier bloc peut dépasser la durée réelle : on coupe au nombre de samples annoncé.
    pcm.truncate(sample.num_samples as usize * channels as usize);
    Ok(pcm)
}

/// En-tête d'identification Vorbis (spécification Vorbis I, § 4.2.2).
fn ident_header(sample_rate: u32, channels: u8) -> Vec<u8> {
    let mut h = Vec::with_capacity(30);
    h.push(1);
    h.extend_from_slice(b"vorbis");
    h.extend_from_slice(&0u32.to_le_bytes()); // version
    h.push(channels);
    h.extend_from_slice(&sample_rate.to_le_bytes());
    h.extend_from_slice(&[0; 12]); // débits max / nominal / min
    h.push((BLOCK_SIZES.1 << 4) | BLOCK_SIZES.0);
    h.push(1); // framing
    h
}
