//! Audio con `rodio` (H8).
//!
//! `rodio` queda confinado en este crate (ADR-014). El store no abre
//! dispositivos y los tests no dependen de hardware de audio.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::io::Cursor;
use std::path::Path;

/// Identificador opaco de un sonido dentro de un [`AudioStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SoundHandle(u64);

impl SoundHandle {
    /// Devuelve el valor entero del handle. No es estable entre stores.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for SoundHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SoundHandle({})", self.0)
    }
}

/// Identificador opaco de una reproducción dentro de un [`AudioPlayer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudioHandle(u64);

impl AudioHandle {
    /// Devuelve el valor entero del handle. No es estable entre players.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for AudioHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AudioHandle({})", self.0)
    }
}

/// Errores al cargar sonidos, sin exponer `rodio` ni `std::io` en la API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioLoadError {
    /// Los bytes no contienen un WAV válido.
    Decode(String),
    /// No se pudo leer el archivo.
    Io(String),
    /// El buffer de audio está vacío.
    Empty,
    /// Se agotaron los identificadores de este store.
    HandleOverflow,
}

impl fmt::Display for AudioLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(message) => write!(f, "error decodificando audio: {message}"),
            Self::Io(message) => write!(f, "error leyendo audio: {message}"),
            Self::Empty => write!(f, "el buffer de audio está vacío"),
            Self::HandleOverflow => write!(f, "se agotaron los handles de sonido"),
        }
    }
}

impl Error for AudioLoadError {}

/// Errores de reproducción, sin exponer `rodio` ni `cpal` en la API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioError {
    /// No hay dispositivo de salida disponible.
    NoDevice(String),
    /// No se pudo iniciar la reproducción.
    Play(String),
    /// El handle no pertenece a este player.
    UnknownHandle,
    /// Se agotaron los identificadores de este player.
    HandleOverflow,
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDevice(message) => write!(f, "sin dispositivo de audio: {message}"),
            Self::Play(message) => write!(f, "error reproduciendo audio: {message}"),
            Self::UnknownHandle => write!(f, "handle de audio desconocido"),
            Self::HandleOverflow => write!(f, "se agotaron los handles de audio"),
        }
    }
}

impl Error for AudioError {}

/// Sonido WAV poseído como bytes; cada reproducción crea su propio decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundData {
    bytes: Vec<u8>,
}

impl SoundData {
    /// Devuelve los bytes WAV originales.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Número de bytes del sonido.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Indica si el sonido no tiene bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

/// Almacena sonidos validados y entrega handles opacos locales.
#[derive(Debug, Default)]
pub struct AudioStore {
    next_handle: u64,
    sounds: HashMap<SoundHandle, SoundData>,
}

impl AudioStore {
    /// Crea un store vacío.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Valida bytes WAV y los guarda.
    pub fn load_from_slice(&mut self, bytes: &[u8]) -> Result<SoundHandle, AudioLoadError> {
        if bytes.is_empty() {
            return Err(AudioLoadError::Empty);
        }
        validate_wav(bytes)?;
        self.insert(SoundData {
            bytes: bytes.to_vec(),
        })
    }

    /// Lee un archivo WAV del disco y lo guarda.
    pub fn load_from_path<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<SoundHandle, AudioLoadError> {
        let bytes = std::fs::read(path).map_err(|error| AudioLoadError::Io(error.to_string()))?;
        self.load_from_slice(&bytes)
    }

    /// Devuelve un sonido previamente cargado, si el handle pertenece a este store.
    #[must_use]
    pub fn get(&self, handle: SoundHandle) -> Option<&SoundData> {
        self.sounds.get(&handle)
    }

    /// Elimina un sonido. El handle nunca se reutiliza.
    pub fn remove(&mut self, handle: SoundHandle) -> Option<SoundData> {
        self.sounds.remove(&handle)
    }

    /// Número de sonidos almacenados.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sounds.len()
    }

    /// Indica si el store no contiene sonidos.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sounds.is_empty()
    }

    fn insert(&mut self, sound: SoundData) -> Result<SoundHandle, AudioLoadError> {
        let next = self
            .next_handle
            .checked_add(1)
            .ok_or(AudioLoadError::HandleOverflow)?;
        let handle = SoundHandle(next);
        self.next_handle = next;
        self.sounds.insert(handle, sound);
        Ok(handle)
    }
}

/// Reproductor que abre el dispositivo de salida solo cuando se reproduce.
pub struct AudioPlayer {
    sink: Option<rodio::MixerDeviceSink>,
    next_handle: u64,
    playing: HashMap<AudioHandle, rodio::Player>,
    master_volume: f32,
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self {
            sink: None,
            next_handle: 0,
            playing: HashMap::new(),
            master_volume: 1.0,
        }
    }
}

impl AudioPlayer {
    /// Crea un reproductor sin abrir ningún dispositivo.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reproduce un sonido y devuelve un handle para controlarlo.
    pub fn play(&mut self, sound: &SoundData) -> Result<AudioHandle, AudioError> {
        let player = {
            let sink = self.ensure_sink()?;
            rodio::play(sink.mixer(), Cursor::new(sound.bytes.clone()))
                .map_err(|error| AudioError::Play(error.to_string()))?
        };
        player.set_volume(self.master_volume);

        let next = self
            .next_handle
            .checked_add(1)
            .ok_or(AudioError::HandleOverflow)?;
        let handle = AudioHandle(next);
        self.next_handle = next;
        self.playing.insert(handle, player);
        Ok(handle)
    }

    /// Detiene una reproducción y libera su handle.
    pub fn stop(&mut self, handle: AudioHandle) -> bool {
        if let Some(player) = self.playing.remove(&handle) {
            player.stop();
            true
        } else {
            false
        }
    }

    /// Detiene todas las reproducciones activas.
    pub fn stop_all(&mut self) {
        for (_, player) in self.playing.drain() {
            player.stop();
        }
    }

    /// Cambia el volumen de una reproducción.
    pub fn set_volume(&self, handle: AudioHandle, volume: f32) -> Result<(), AudioError> {
        let player = self.playing.get(&handle).ok_or(AudioError::UnknownHandle)?;
        player.set_volume(clamp_volume(volume));
        Ok(())
    }

    /// Cambia el volumen maestro y el de todas las reproducciones activas.
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = clamp_volume(volume);
        for player in self.playing.values() {
            player.set_volume(self.master_volume);
        }
    }

    /// Devuelve el volumen maestro, ya limitado a `[0.0, 1.0]`.
    #[must_use]
    pub fn master_volume(&self) -> f32 {
        self.master_volume
    }

    /// Indica si una reproducción sigue activa.
    #[must_use]
    pub fn is_playing(&self, handle: AudioHandle) -> bool {
        self.playing
            .get(&handle)
            .is_some_and(|player| !player.empty())
    }

    /// Número de reproducciones registradas.
    #[must_use]
    pub fn len(&self) -> usize {
        self.playing.len()
    }

    /// Indica si no hay reproducciones registradas.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.playing.is_empty()
    }

    fn ensure_sink(&mut self) -> Result<&rodio::MixerDeviceSink, AudioError> {
        if self.sink.is_none() {
            let sink = rodio::DeviceSinkBuilder::open_default_sink()
                .map_err(|error| AudioError::NoDevice(error.to_string()))?;
            self.sink = Some(sink);
        }
        self.sink
            .as_ref()
            .ok_or(AudioError::NoDevice("sink no disponible".to_owned()))
    }
}

/// Limita un volumen al rango admitido por rodio.
#[must_use]
pub fn clamp_volume(volume: f32) -> f32 {
    volume.clamp(0.0, 1.0)
}

fn validate_wav(bytes: &[u8]) -> Result<(), AudioLoadError> {
    let _decoder = rodio::Decoder::new(Cursor::new(bytes.to_vec()))
        .map_err(|error| AudioLoadError::Decode(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn wav_bytes() -> Vec<u8> {
        let data_len = 2u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36u32 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);
        bytes
    }

    fn temp_wav_path() -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("lunar_audio_{}_{counter}.wav", std::process::id()))
    }

    #[test]
    fn load_from_slice_validates_wav_and_assigns_handles() {
        let wav = wav_bytes();
        let mut store = AudioStore::new();

        let first = store.load_from_slice(&wav).expect("valid WAV");
        let second = store.load_from_slice(&wav).expect("second WAV");

        assert_eq!(first.get(), 1);
        assert_eq!(second.get(), 2);
        assert_eq!(store.len(), 2);
        assert_eq!(store.get(first).expect("stored sound").as_bytes(), wav);
    }

    #[test]
    fn failures_do_not_consume_handles() {
        let mut store = AudioStore::new();
        assert_eq!(store.load_from_slice(b""), Err(AudioLoadError::Empty));
        assert!(matches!(
            store.load_from_slice(b"not a wav"),
            Err(AudioLoadError::Decode(_))
        ));
        assert!(store.is_empty());

        let handle = store.load_from_slice(&wav_bytes()).expect("valid WAV");
        assert_eq!(handle.get(), 1);
    }

    #[test]
    fn load_from_path_reads_file() {
        let path = temp_wav_path();
        std::fs::write(&path, wav_bytes()).expect("write WAV");
        let mut store = AudioStore::new();

        let handle = store.load_from_path(&path).expect("load WAV from path");
        assert_eq!(
            store.get(handle).expect("stored sound").len(),
            wav_bytes().len()
        );

        std::fs::remove_file(&path).expect("remove WAV");
    }

    #[test]
    fn removal_drops_sound_but_does_not_reuse_handle() {
        let mut store = AudioStore::new();
        let first = store.load_from_slice(&wav_bytes()).expect("first WAV");
        let removed = store.remove(first).expect("remove WAV");
        assert!(!removed.is_empty());
        assert!(store.get(first).is_none());

        let second = store.load_from_slice(&wav_bytes()).expect("second WAV");
        assert_eq!(second.get(), 2);
    }

    #[test]
    fn player_starts_without_device() {
        let player = AudioPlayer::new();
        assert!(player.is_empty());
        assert_eq!(player.master_volume(), 1.0);
        assert!(!player.is_playing(AudioHandle(1)));
    }

    #[test]
    fn volume_is_clamped_and_unknown_handles_are_rejected() {
        assert_eq!(clamp_volume(-1.0), 0.0);
        assert_eq!(clamp_volume(0.5), 0.5);
        assert_eq!(clamp_volume(4.0), 1.0);

        let mut player = AudioPlayer::new();
        player.set_master_volume(2.0);
        assert_eq!(player.master_volume(), 1.0);
        assert_eq!(
            player.set_volume(AudioHandle(1), 0.5),
            Err(AudioError::UnknownHandle)
        );
        assert!(!player.stop(AudioHandle(1)));
        player.stop_all();
        assert!(player.is_empty());
    }
}
