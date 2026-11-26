//! Preferencias persistentes del editor (ADR-031).
//!
//! Texto plano `clave=valor`, sin dependencias nuevas. La codificación y la
//! decodificación son puras; la lectura y escritura reales son funciones delgadas.

use std::path::PathBuf;

use crate::config::config_dir;

pub(crate) const DEFAULT_ASSETS_ROOT: &str = "examples/sprite_platformer/assets";
pub(crate) const MIN_ZOOM: f32 = 8.0;
pub(crate) const MAX_ZOOM: f32 = 512.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditorSettings {
    pub(crate) assets_root: String,
    pub(crate) center: [f32; 2],
    pub(crate) zoom: f32,
    pub(crate) show_grid: bool,
    pub(crate) snap: bool,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            assets_root: DEFAULT_ASSETS_ROOT.to_owned(),
            center: [0.0, 0.0],
            zoom: 48.0,
            show_grid: true,
            snap: false,
        }
    }
}

/// Serializa las preferencias, una clave por línea.
#[must_use]
pub(crate) fn encode(settings: &EditorSettings) -> String {
    let root = settings.assets_root.replace(['\r', '\n'], "");
    format!(
        "assets_root={root}\ncenter_x={}\ncenter_y={}\nzoom={}\nshow_grid={}\nsnap={}\n",
        settings.center[0], settings.center[1], settings.zoom, settings.show_grid, settings.snap
    )
}

/// Lee las preferencias; cada campo inválido cae a su valor por defecto.
#[must_use]
pub(crate) fn decode(contents: &str) -> EditorSettings {
    let mut settings = EditorSettings::default();
    for line in contents.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key {
            "assets_root" => {
                if !value.is_empty() {
                    settings.assets_root = value.to_owned();
                }
            }
            "center_x" => {
                if let Some(number) = finite(value) {
                    settings.center[0] = number;
                }
            }
            "center_y" => {
                if let Some(number) = finite(value) {
                    settings.center[1] = number;
                }
            }
            "zoom" => {
                if let Some(number) = finite(value) {
                    settings.zoom = number.clamp(MIN_ZOOM, MAX_ZOOM);
                }
            }
            "show_grid" => settings.show_grid = boolean(value, settings.show_grid),
            "snap" => settings.snap = boolean(value, settings.snap),
            _ => {}
        }
    }
    settings
}

fn finite(value: &str) -> Option<f32> {
    value
        .parse::<f32>()
        .ok()
        .filter(|number| number.is_finite())
}

fn boolean(value: &str, fallback: bool) -> bool {
    if value.eq_ignore_ascii_case("true") {
        true
    } else if value.eq_ignore_ascii_case("false") {
        false
    } else {
        fallback
    }
}

/// Ruta de las preferencias, con el mismo directorio que `recent.txt`.
#[must_use]
pub(crate) fn file_path() -> PathBuf {
    config_dir()
        .map(|dir| dir.join("settings.txt"))
        .unwrap_or_else(|| PathBuf::from("lunar_editor_settings.txt"))
}

/// Carga las preferencias; un fichero ausente o ilegible significa «por defecto».
#[must_use]
pub(crate) fn load() -> EditorSettings {
    std::fs::read_to_string(file_path())
        .map(|contents| decode(&contents))
        .unwrap_or_default()
}

/// Guarda las preferencias, creando el directorio si hace falta.
pub(crate) fn save(settings: &EditorSettings) -> std::io::Result<()> {
    let path = file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, encode(settings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_survive_a_round_trip() {
        let settings = EditorSettings {
            assets_root: "assets/nave".to_owned(),
            center: [3.5, -2.25],
            zoom: 96.0,
            show_grid: false,
            snap: true,
        };
        assert_eq!(decode(&encode(&settings)), settings);
    }

    #[test]
    fn invalid_fields_fall_back_independently() {
        let text = "assets_root=\ncenter_x=NaN\ncenter_y=4\nzoom=inf\nshow_grid=quizá\nsnap=true\n";
        let settings = decode(text);
        assert_eq!(settings.assets_root, DEFAULT_ASSETS_ROOT);
        assert_eq!(settings.center, [0.0, 4.0]);
        assert_eq!(settings.zoom, 48.0);
        assert!(settings.show_grid);
        assert!(settings.snap);
    }

    #[test]
    fn zoom_is_clamped_to_the_canvas_range() {
        assert_eq!(decode("zoom=1").zoom, MIN_ZOOM);
        assert_eq!(decode("zoom=9999").zoom, MAX_ZOOM);
    }

    #[test]
    fn unknown_keys_and_newlines_in_paths_are_ignored() {
        let text = "version=9\nassets_root=assets/ruta\ncon\nsalto\n";
        assert_eq!(decode(text).assets_root, "assets/ruta");
    }
}
