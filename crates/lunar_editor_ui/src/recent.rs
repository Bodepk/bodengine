//! Lista de escenas recientes del editor (ADR-020).
//!
//! Se guarda como texto plano en el directorio de configuración del usuario.
//! No añade dependencias: `serde_json` queda reservado para el esquema de
//! escenas y aquí basta una línea por ruta.

use std::path::PathBuf;

/// Número máximo de rutas recordadas.
pub(crate) const MAX_RECENT: usize = 8;

/// Devuelve la lista con `path` delante, sin duplicados y acotada.
#[must_use]
pub(crate) fn remember(entries: &[String], path: &str) -> Vec<String> {
    let mut next = Vec::with_capacity(MAX_RECENT.min(entries.len() + 1));
    next.push(path.to_owned());
    next.extend(
        entries
            .iter()
            .filter(|entry| entry.as_str() != path)
            .cloned(),
    );
    next.truncate(MAX_RECENT);
    next
}

/// Lee una lista guardada, descartando líneas vacías y el exceso.
#[must_use]
pub(crate) fn decode(contents: &str) -> Vec<String> {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(MAX_RECENT)
        .map(str::to_owned)
        .collect()
}

/// Serializa una lista a texto plano, una ruta por línea.
#[must_use]
pub(crate) fn encode(entries: &[String]) -> String {
    let mut text = entries
        .iter()
        .take(MAX_RECENT)
        .map(|entry| entry.replace(['\r', '\n'], ""))
        .collect::<Vec<_>>()
        .join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

/// Ruta del fichero de recientes, con raíz por sistema operativo (ADR-031).
#[must_use]
pub(crate) fn file_path() -> PathBuf {
    crate::config::config_dir()
        .map(|dir| dir.join("recent.txt"))
        .unwrap_or_else(|| PathBuf::from("lunar_editor_recent.txt"))
}

/// Carga los recientes; un fichero ausente o ilegible significa lista vacía.
#[must_use]
pub(crate) fn load() -> Vec<String> {
    std::fs::read_to_string(file_path())
        .map(|contents| decode(&contents))
        .unwrap_or_default()
}

/// Guarda los recientes, creando el directorio si hace falta.
pub(crate) fn save(entries: &[String]) -> std::io::Result<()> {
    let path = file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, encode(entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remember_moves_path_to_front_without_duplicates() {
        let entries = vec!["b.json".to_owned(), "a.json".to_owned()];
        assert_eq!(
            remember(&entries, "a.json"),
            vec!["a.json".to_owned(), "b.json".to_owned()]
        );
        assert_eq!(remember(&entries, "c.json").len(), 3);
    }

    #[test]
    fn remember_caps_the_list() {
        let entries: Vec<String> = (0..12).map(|index| format!("{index}.json")).collect();
        assert_eq!(remember(&entries, "12.json").len(), MAX_RECENT);
    }

    #[test]
    fn encode_and_decode_round_trip() {
        let entries = vec!["a.json".to_owned(), "b.json".to_owned()];
        assert_eq!(decode(&encode(&entries)), entries);
        assert!(decode("  \n\n").is_empty());
    }

    #[test]
    fn decode_ignores_lines_beyond_the_limit() {
        let text = (0..12)
            .map(|index| format!("{index}.json"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(decode(&text).len(), MAX_RECENT);
    }
}
