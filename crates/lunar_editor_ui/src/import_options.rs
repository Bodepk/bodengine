//! Opciones de importación y vigilancia de cambios en disco (E17, ADR-036).
//!
//! Las opciones se guardan en el `.meta` de ADR-034 como líneas `clave=valor`.
//! No se añaden dependencias: el filtro y el wrap se aplican a las miniaturas
//! de `egui`, la escala de glTF al crear entidades y el resto se conserva como
//! metadatos para el runtime.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::assets::AssetEntry;

/// Filtro de miniatura seleccionable en el content browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextureFilter {
    Nearest,
    Linear,
}

impl TextureFilter {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "nearest" => Some(Self::Nearest),
            "linear" => Some(Self::Linear),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Nearest => "nearest",
            Self::Linear => "linear",
        }
    }
}

/// Modo de repetición de la textura en la vista previa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextureWrap {
    Clamp,
    Repeat,
    Mirror,
}

impl TextureWrap {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "clamp" => Some(Self::Clamp),
            "repeat" => Some(Self::Repeat),
            "mirror" => Some(Self::Mirror),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Clamp => "clamp",
            Self::Repeat => "repeat",
            Self::Mirror => "mirror",
        }
    }
}

/// Opciones de una imagen, con valores por defecto deterministas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextureImportOptions {
    pub(crate) filter: TextureFilter,
    pub(crate) wrap: TextureWrap,
}

impl Default for TextureImportOptions {
    fn default() -> Self {
        Self {
            filter: TextureFilter::Nearest,
            wrap: TextureWrap::Clamp,
        }
    }
}

impl TextureImportOptions {
    /// Traducción a las opciones de textura de `egui`, sin tocar `lunar_assets`.
    #[must_use]
    pub(crate) fn egui(self) -> egui::TextureOptions {
        let filter = match self.filter {
            TextureFilter::Nearest => egui::TextureFilter::Nearest,
            TextureFilter::Linear => egui::TextureFilter::Linear,
        };
        egui::TextureOptions {
            magnification: filter,
            minification: filter,
            wrap_mode: match self.wrap {
                TextureWrap::Clamp => egui::TextureWrapMode::ClampToEdge,
                TextureWrap::Repeat => egui::TextureWrapMode::Repeat,
                TextureWrap::Mirror => egui::TextureWrapMode::MirroredRepeat,
            },
            mipmap_mode: None,
        }
    }
}

/// Opciones de un glTF: escala aplicable en el editor y banderas de metadatos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GltfImportOptions {
    pub(crate) scale: f32,
    pub(crate) materials: bool,
    pub(crate) animations: bool,
}

impl Default for GltfImportOptions {
    fn default() -> Self {
        Self {
            scale: 1.0,
            materials: true,
            animations: true,
        }
    }
}

/// Opciones de audio conservadas como metadatos del proyecto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AudioImportOptions {
    pub(crate) channels: u16,
    pub(crate) sample_rate: u32,
}

impl Default for AudioImportOptions {
    fn default() -> Self {
        Self {
            channels: 2,
            sample_rate: 44_100,
        }
    }
}

/// Todas las opciones soportadas por el `.meta` de un asset.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct ImportOptions {
    pub(crate) texture: TextureImportOptions,
    pub(crate) gltf: GltfImportOptions,
    pub(crate) audio: AudioImportOptions,
}

const KEYS: [&str; 7] = [
    "texture.filter",
    "texture.wrap",
    "gltf.scale",
    "gltf.materials",
    "gltf.animations",
    "audio.channels",
    "audio.sample_rate",
];

/// Lee las opciones reconocidas, ignorando claves desconocidas o valores malos.
#[must_use]
pub(crate) fn decode(text: &str) -> ImportOptions {
    let mut options = ImportOptions::default();
    let mut values: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once('=')
            && KEYS.contains(&key.trim())
        {
            values.insert(key.trim(), value.trim());
        }
    }
    if let Some(filter) = values
        .get("texture.filter")
        .and_then(|v| TextureFilter::parse(v))
    {
        options.texture.filter = filter;
    }
    if let Some(wrap) = values
        .get("texture.wrap")
        .and_then(|v| TextureWrap::parse(v))
    {
        options.texture.wrap = wrap;
    }
    if let Some(scale) = values
        .get("gltf.scale")
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= 100.0)
    {
        options.gltf.scale = scale;
    }
    if let Some(materials) = parse_bool(values.get("gltf.materials").copied()) {
        options.gltf.materials = materials;
    }
    if let Some(animations) = parse_bool(values.get("gltf.animations").copied()) {
        options.gltf.animations = animations;
    }
    if let Some(channels) = values
        .get("audio.channels")
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|value| *value == 1 || *value == 2)
    {
        options.audio.channels = channels;
    }
    if let Some(rate) = values
        .get("audio.sample_rate")
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|value| (8_000..=192_000).contains(value))
    {
        options.audio.sample_rate = rate;
    }
    options
}

fn parse_bool(value: Option<&str>) -> Option<bool> {
    match value?.to_ascii_lowercase().as_str() {
        "1" | "true" | "si" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

/// Reescribe solo las claves reconocidas, conservando GUID y claves futuras.
#[must_use]
pub(crate) fn merge(text: &str, options: &ImportOptions) -> String {
    let mut out: String = text
        .lines()
        .filter(|line| {
            line.split_once('=')
                .is_none_or(|(key, _)| !KEYS.contains(&key.trim()))
        })
        .map(|line| format!("{line}\n"))
        .collect();
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!(
        "texture.filter={}\n",
        options.texture.filter.as_str()
    ));
    out.push_str(&format!("texture.wrap={}\n", options.texture.wrap.as_str()));
    out.push_str(&format!("gltf.scale={}\n", options.gltf.scale));
    out.push_str(&format!(
        "gltf.materials={}\n",
        u8::from(options.gltf.materials)
    ));
    out.push_str(&format!(
        "gltf.animations={}\n",
        u8::from(options.gltf.animations)
    ));
    out.push_str(&format!("audio.channels={}\n", options.audio.channels));
    out.push_str(&format!(
        "audio.sample_rate={}\n",
        options.audio.sample_rate
    ));
    out
}

fn meta_path(root: &Path, path: &str) -> std::path::PathBuf {
    root.join(format!("{path}{}", crate::guids::META_SUFFIX))
}

/// Opciones leídas del `.meta`; si no existe se usan los valores por defecto.
#[must_use]
pub(crate) fn load_for(root: &Path, path: &str) -> ImportOptions {
    fs::read_to_string(meta_path(root, path))
        .map(|text| decode(&text))
        .unwrap_or_default()
}

/// Guarda las opciones sin perder el GUID ni las claves desconocidas.
pub(crate) fn save_for(root: &Path, path: &str, options: &ImportOptions) -> Result<(), String> {
    let full = meta_path(root, path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = fs::read_to_string(&full).unwrap_or_default();
    fs::write(&full, merge(&text, options)).map_err(|error| error.to_string())
}

/// Huella de un archivo en disco, en nanosegundos desde `UNIX_EPOCH`.
#[must_use]
pub(crate) fn stamp(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos() as u64)
}

/// Muestras actuales de los assets listados en la biblioteca.
#[must_use]
pub(crate) fn sample(root: &Path, entries: &[AssetEntry]) -> Vec<(String, u64)> {
    entries
        .iter()
        .filter(|entry| root.join(&entry.path).exists())
        .map(|entry| (entry.path.clone(), stamp(&root.join(&entry.path))))
        .collect()
}

/// Vigilante cooperativo: compara huellas sin hilos ni dependencias nuevas.
#[derive(Debug, Default)]
pub(crate) struct ReimportWatcher {
    stamps: HashMap<String, u64>,
    last_poll: u64,
}

impl ReimportWatcher {
    /// Rutas nuevas o modificadas desde la última muestra. `now` y `interval`
    /// son inyectables para que el sondeo sea determinista en los tests.
    #[must_use]
    pub(crate) fn poll(
        &mut self,
        current: &[(String, u64)],
        now: u64,
        interval: u64,
    ) -> Vec<String> {
        if now.saturating_sub(self.last_poll) < interval {
            return Vec::new();
        }
        self.last_poll = now;
        let mut changed: Vec<String> = current
            .iter()
            .filter(|(path, stamp)| self.stamps.get(path).is_none_or(|old| old != stamp))
            .map(|(path, _)| path.clone())
            .collect();
        self.stamps.clear();
        self.stamps
            .extend(current.iter().map(|(path, stamp)| (path.clone(), *stamp)));
        changed.sort();
        changed.dedup();
        changed
    }

    /// Rutas que estaban en la última muestra y ya no están presentes.
    #[must_use]
    pub(crate) fn missing(&self, current: &[(String, u64)]) -> Vec<String> {
        let present: Vec<&str> = current.iter().map(|(path, _)| path.as_str()).collect();
        let mut missing: Vec<String> = self
            .stamps
            .keys()
            .filter(|path| !present.contains(&path.as_str()))
            .cloned()
            .collect();
        missing.sort();
        missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetKind;

    fn meta(guid: &str) -> String {
        format!("guid={guid}\nfuture.key=valor\n")
    }

    #[test]
    fn options_round_trip_preserves_guid_and_unknown_keys() {
        let options = ImportOptions {
            texture: TextureImportOptions {
                filter: TextureFilter::Linear,
                wrap: TextureWrap::Mirror,
            },
            gltf: GltfImportOptions {
                scale: 0.5,
                materials: false,
                animations: true,
            },
            audio: AudioImportOptions {
                channels: 1,
                sample_rate: 48_000,
            },
        };
        let text = merge(&meta("abc"), &options);
        assert!(text.starts_with("guid=abc\nfuture.key=valor\n"));
        assert_eq!(decode(&text), options);
    }

    #[test]
    fn invalid_values_fall_back_independently() {
        let text = "texture.filter=mal\ntexture.wrap=repeat\ngltf.scale=-2\ngltf.scale=3\ngltf.materials=quizá\naudio.channels=7\naudio.sample_rate=1\n";
        let options = decode(text);
        assert_eq!(options.texture.filter, TextureFilter::Nearest);
        assert_eq!(options.texture.wrap, TextureWrap::Repeat);
        assert_eq!(options.gltf.scale, 3.0);
        assert!(options.gltf.materials);
        assert_eq!(options.audio.channels, 2);
        assert_eq!(options.audio.sample_rate, 44_100);
    }

    #[test]
    fn texture_options_map_to_egui_without_new_dependencies() {
        let nearest = TextureImportOptions::default().egui();
        let linear = TextureImportOptions {
            filter: TextureFilter::Linear,
            wrap: TextureWrap::Repeat,
        }
        .egui();
        assert_ne!(nearest.magnification, linear.magnification);
        assert_ne!(nearest.wrap_mode, linear.wrap_mode);
    }

    #[test]
    fn watcher_reports_changes_new_and_missing_with_injected_time() {
        let mut watcher = ReimportWatcher::default();
        let first = [("a.png".to_owned(), 10_u64)];
        assert_eq!(watcher.poll(&first, 100, 100), ["a.png"]);
        assert_eq!(watcher.poll(&first, 110, 100), Vec::<String>::new());
        let second = [("a.png".to_owned(), 11), ("b.png".to_owned(), 20)];
        assert_eq!(watcher.poll(&second, 200, 100), ["a.png", "b.png"]);
        assert_eq!(watcher.missing(&second), Vec::<String>::new());
        assert_eq!(watcher.missing(&[("b.png".to_owned(), 20)]), ["a.png"]);
    }

    #[test]
    fn sample_reads_modified_time_from_disk() {
        let root = std::env::temp_dir().join(format!(
            "lunar_import_{}_{}",
            std::process::id(),
            crate::guids::generate_guid(77)
        ));
        let _ = fs::create_dir_all(&root);
        let file = root.join("a.png");
        let _ = fs::write(&file, b"png");
        let entries = [AssetEntry {
            path: "a.png".to_owned(),
            kind: AssetKind::Image,
        }];
        let samples = sample(&root, &entries);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].0, "a.png");
        assert!(samples[0].1 > 0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn save_for_creates_a_meta_with_options_when_missing() {
        let root = std::env::temp_dir().join(format!(
            "lunar_import_save_{}_{}",
            std::process::id(),
            crate::guids::generate_guid(78)
        ));
        let _ = fs::create_dir_all(&root);
        let _ = fs::write(root.join("a.png"), b"png");
        let options = ImportOptions::default();
        assert!(save_for(&root, "a.png", &options).is_ok());
        let text = fs::read_to_string(root.join("a.png.meta")).unwrap();
        assert_eq!(decode(&text), options);
        let _ = fs::remove_dir_all(&root);
    }
}
