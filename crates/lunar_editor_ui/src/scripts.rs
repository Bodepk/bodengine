//! Enlaces de comportamientos declarativos (E20, ADR-033/040).
//!
//! El mapa es un fichero lateral de proyecto, no una variante de `Component`.
//! El parseo, la validación y la escritura son puros y no dependen de GPU.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Nombre del fichero de enlaces junto a la escena.
pub(crate) const MAP_FILE: &str = "scripts.map.txt";
/// Nombre del fichero donde el juego C++ escribe los problemas encontrados.
pub(crate) const DIAGNOSTICS_FILE: &str = "scripts.diagnostics.txt";

/// Comportamiento declarativo disponible en el inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Behavior {
    Patrol,
    Follow,
    Spin,
    Bob,
}

impl Behavior {
    pub(crate) const ALL: [Self; 4] = [Self::Patrol, Self::Follow, Self::Spin, Self::Bob];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Patrol => "Patrol",
            Self::Follow => "Follow",
            Self::Spin => "Spin",
            Self::Bob => "Bob",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|behavior| behavior.name().eq_ignore_ascii_case(text))
    }

    /// Parámetros esperados, con su valor por defecto y su tipo.
    pub(crate) fn schema(self) -> &'static [(&'static str, &'static str, ParamKind)] {
        match self {
            Self::Patrol => &[
                ("speed", "1", ParamKind::Float),
                ("radius", "2", ParamKind::Float),
                ("axis", "x", ParamKind::Axis),
            ],
            Self::Follow => &[
                ("target", "0", ParamKind::Entity),
                ("speed", "1", ParamKind::Float),
                ("radius", "0.5", ParamKind::Float),
            ],
            Self::Spin => &[
                ("speed", "90", ParamKind::Float),
                ("axis", "z", ParamKind::Axis),
            ],
            Self::Bob => &[
                ("speed", "1", ParamKind::Float),
                ("amplitude", "0.5", ParamKind::Float),
            ],
        }
    }
}

impl fmt::Display for Behavior {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Tipo de un parámetro, usado para validar el texto del fichero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamKind {
    Float,
    Entity,
    /// `x`, `y` o `z`, también para el eje de rotación.
    Axis,
}

/// Valida un valor antes de guardarlo desde el inspector.
pub(crate) fn valid_param(behavior: Behavior, key: &str, value: &str) -> bool {
    behavior
        .schema()
        .iter()
        .find(|(name, _, _)| *name == key)
        .is_some_and(|(_, _, kind)| kind.accepts(value))
}

impl ParamKind {
    fn accepts(self, value: &str) -> bool {
        match self {
            Self::Float => value.parse::<f32>().is_ok_and(|number| number.is_finite()),
            Self::Entity => value.parse::<u64>().is_ok(),
            Self::Axis => matches!(value.to_ascii_lowercase().as_str(), "x" | "y" | "z"),
        }
    }
}

/// Enlace de una entidad con un comportamiento y sus parámetros.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Binding {
    pub(crate) behavior: Behavior,
    pub(crate) params: BTreeMap<String, String>,
}

impl Binding {
    pub(crate) fn new(behavior: Behavior) -> Self {
        let params = behavior
            .schema()
            .iter()
            .map(|(key, default, _)| ((*key).to_owned(), (*default).to_owned()))
            .collect();
        Self { behavior, params }
    }

    fn from_text(line: usize, text: &str, errors: &mut Vec<String>) -> Option<Self> {
        let mut fields = text.split(';');
        let behavior = fields
            .next()
            .and_then(|name| Behavior::parse(name.trim()))
            .ok_or_else(|| format!("línea {line}: comportamiento desconocido en «{text}»"));
        let behavior = match behavior {
            Ok(behavior) => behavior,
            Err(error) => {
                errors.push(error);
                return None;
            }
        };
        let mut params = BTreeMap::new();
        for field in fields {
            let Some((key, value)) = field.split_once('=') else {
                errors.push(format!("línea {line}: parámetro mal escrito «{field}»"));
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            let Some((_, _, kind)) = behavior.schema().iter().find(|(name, _, _)| *name == key)
            else {
                errors.push(format!(
                    "línea {line}: {behavior} no acepta el parámetro «{key}»"
                ));
                continue;
            };
            if !kind.accepts(value) {
                errors.push(format!(
                    "línea {line}: valor no válido «{value}» para {key}"
                ));
                continue;
            }
            params.insert(key.to_owned(), value.to_owned());
        }
        for (key, default, _) in behavior.schema() {
            params
                .entry((*key).to_owned())
                .or_insert_with(|| (*default).to_owned());
        }
        Some(Self { behavior, params })
    }
}

/// Mapa de enlaces por identificador de entidad, ordenado para escrituras estables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ScriptMap {
    bindings: BTreeMap<u64, Binding>,
}

impl ScriptMap {
    pub(crate) fn get(&self, id: u64) -> Option<&Binding> {
        self.bindings.get(&id)
    }

    pub(crate) fn set(&mut self, id: u64, binding: Option<Binding>) {
        match binding {
            Some(binding) => {
                self.bindings.insert(id, binding);
            }
            None => {
                self.bindings.remove(&id);
            }
        }
    }

    /// Descarta enlaces de entidades que ya no existen.
    pub(crate) fn retain(&mut self, ids: &[u64]) {
        self.bindings.retain(|id, _| ids.contains(id));
    }

    pub(crate) fn encode(&self) -> String {
        let mut text = String::from("# lunar scripts.map.txt (ADR-033)\n");
        for (id, binding) in &self.bindings {
            text.push_str(&format!("{id}={}", binding.behavior));
            for (key, value) in &binding.params {
                text.push_str(&format!(";{key}={value}"));
            }
            text.push('\n');
        }
        text
    }

    pub(crate) fn parse(text: &str) -> (Self, Vec<String>) {
        let mut map = Self::default();
        let mut errors = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((id, value)) = line.split_once('=') else {
                errors.push(format!("línea {}: falta «=» en «{line}»", index + 1));
                continue;
            };
            let Ok(id) = id.trim().parse::<u64>() else {
                errors.push(format!(
                    "línea {}: identificador no válido «{}»",
                    index + 1,
                    id.trim()
                ));
                continue;
            };
            if let Some(binding) = Binding::from_text(index + 1, value.trim(), &mut errors) {
                map.bindings.insert(id, binding);
            }
        }
        (map, errors)
    }
}

/// Escribe un fichero completo usando un temporal en el mismo directorio.
pub(crate) fn atomic_write(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".tmp-{}", std::process::id()));
    let temporary = PathBuf::from(name);
    fs::write(&temporary, text)?;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(_) if path.exists() => {
            fs::remove_file(path)?;
            match fs::rename(&temporary, path) {
                Ok(()) => Ok(()),
                Err(error) => {
                    let _ = fs::remove_file(&temporary);
                    Err(error)
                }
            }
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

/// Lee los avisos escritos por el juego C++, descartando comentarios y vacíos.
pub(crate) fn diagnostics(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_round_trip_is_sorted_and_complete() {
        let mut map = ScriptMap::default();
        map.set(9, Some(Binding::new(Behavior::Bob)));
        map.set(2, Some(Binding::new(Behavior::Patrol)));
        let text = map.encode();
        let (decoded, errors) = ScriptMap::parse(&text);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(decoded, map);
        assert!(text.find("2=Patrol").unwrap() < text.find("9=Bob").unwrap());
    }

    #[test]
    fn invalid_lines_are_reported_and_ignored() {
        let text = concat!(
            "# comentario\n",
            "1=Patrol;speed=3;axis=w;extra=1\n",
            "mal=Spin\n",
            "2=Unknown\n",
            "3=Bob;amplitude=abc\n",
            "4=Follow;target=7;speed=2\n",
        );
        let (map, errors) = ScriptMap::parse(text);
        assert_eq!(errors.len(), 5, "{errors:?}");
        assert_eq!(map.get(1).unwrap().params["speed"], "3");
        assert_eq!(map.get(1).unwrap().params["axis"], "x");
        assert_eq!(map.get(4).unwrap().behavior, Behavior::Follow);
        assert!(map.get(2).is_none());
        // El parámetro malo se ignora, pero el enlace se conserva con su valor por defecto.
        assert_eq!(map.get(3).unwrap().params["amplitude"], "0.5");
    }

    #[test]
    fn retain_removes_bindings_of_deleted_entities() {
        let mut map = ScriptMap::default();
        map.set(1, Some(Binding::new(Behavior::Spin)));
        map.set(2, Some(Binding::new(Behavior::Bob)));
        map.retain(&[2]);
        assert!(map.get(1).is_none());
        assert!(map.get(2).is_some());
    }

    #[test]
    fn atomic_write_replaces_an_existing_file() {
        let path = std::env::temp_dir().join(format!(
            "lunar_scripts_{}_{}.txt",
            std::process::id(),
            MAP_FILE
        ));
        atomic_write(&path, "primero\n").unwrap();
        atomic_write(&path, "segundo\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "segundo\n");
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn diagnostics_ignores_comments_and_empty_lines() {
        assert_eq!(
            diagnostics("# cabezo\n\n línea 3: valor malo \n"),
            ["línea 3: valor malo"]
        );
    }
}
