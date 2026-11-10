//! Lecture audio (pré-écoute) via rodio.

use std::io::Cursor;
use std::path::Path;

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};

pub struct Player {
    // Le flux doit rester vivant tant qu'on joue du son.
    _stream: OutputStream,
    handle: OutputStreamHandle,
    sink: Option<Sink>,
}

impl Player {
    pub fn new() -> Result<Self, String> {
        let (stream, handle) = OutputStream::try_default().map_err(|e| e.to_string())?;
        Ok(Self { _stream: stream, handle, sink: None })
    }

    /// Joue un son en mémoire (Ogg Vorbis ou WAV), en coupant le précédent.
    pub fn play_bytes(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        let source = Decoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
        self.play(source)
    }

    /// Joue un fichier du disque (WAV / OGG / FLAC).
    pub fn play_file(&mut self, path: &Path) -> Result<(), String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let source = Decoder::new(std::io::BufReader::new(file)).map_err(|e| e.to_string())?;
        self.play(source)
    }

    fn play<S>(&mut self, source: S) -> Result<(), String>
    where
        S: rodio::Source + Send + 'static,
        S::Item: rodio::Sample + Send,
        f32: rodio::cpal::FromSample<S::Item>,
    {
        self.stop();
        let sink = Sink::try_new(&self.handle).map_err(|e| e.to_string())?;
        sink.append(source);
        self.sink = Some(sink);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(s) = self.sink.take() {
            s.stop();
        }
    }

    pub fn is_playing(&self) -> bool {
        self.sink.as_ref().is_some_and(|s| !s.empty())
    }
}
