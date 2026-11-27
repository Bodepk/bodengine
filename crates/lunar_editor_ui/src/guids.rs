//! GUIDs locales de assets y resolución de rutas (E16, ADR-034).
//!
//! El esquema de escenas no cambia: las referencias siguen siendo rutas
//! relativas. Cada asset tiene un `.meta` con su GUID y la raíz mantiene un
//! índice `guid=ruta` para poder reescribir las rutas después de mover archivos.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use lunar_scenes::{Component, Scene};

use crate::assets::AssetEntry;

/// Sufijo del fichero lateral que guarda el GUID de un asset.
pub(crate) const META_SUFFIX: &str = ".meta";
/// Nombre del índice `guid=ruta` en la raíz de assets.
pub(crate) const INDEX_FILE: &str = "guid_index.txt";
const GUID_HEX_LEN: usize = 32;

/// Índice en memoria: GUID actual, rutas actuales y alias de rutas movidas.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct GuidIndex {
    by_path: HashMap<String, String>,
    by_guid: HashMap<String, String>,
    aliases: HashMap<String, String>,
}

impl GuidIndex {
    /// Construye el índice actual a partir de los `.meta` leídos, conservando
    /// como alias las rutas que el índice anterior asociaba a cada GUID.
    #[must_use]
    pub(crate) fn from_pairs(pairs: &[(String, String)], previous: &GuidIndex) -> Self {
        let mut index = Self::default();
        for (path, guid) in pairs {
            index.by_path.insert(path.clone(), guid.clone());
            index.by_guid.insert(guid.clone(), path.clone());
        }
        for (guid, old_path) in &previous.by_guid {
            if index
                .by_guid
                .get(guid)
                .is_some_and(|current| current != old_path)
            {
                index.aliases.insert(old_path.clone(), guid.clone());
            }
        }
        for (old_path, guid) in &previous.aliases {
            index.aliases.insert(old_path.clone(), guid.clone());
        }
        index
    }

    /// Índice que solo contiene un mapa GUID → ruta leído de disco.
    #[must_use]
    pub(crate) fn from_previous(previous: HashMap<String, String>) -> Self {
        Self {
            by_guid: previous,
            ..Self::default()
        }
    }

    /// GUID conocido para una ruta actual.
    #[must_use]
    pub(crate) fn guid_for(&self, path: &str) -> Option<&str> {
        self.by_path.get(path).map(String::as_str)
    }

    /// Ruta actual de un GUID conocido.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn path_for(&self, guid: &str) -> Option<&str> {
        self.by_guid.get(guid).map(String::as_str)
    }

    /// Alias registrado para una ruta antigua.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn alias_for(&self, path: &str) -> Option<&str> {
        self.aliases.get(path).map(String::as_str)
    }

    /// Devuelve la ruta actual de una referencia, ya esté escrita con la ruta
    /// nueva o con un alias de una ruta movida.
    #[must_use]
    pub(crate) fn resolve(&self, path: &str) -> Option<String> {
        if self.by_path.contains_key(path) {
            return Some(path.to_owned());
        }
        let guid = self.aliases.get(path)?;
        self.by_guid.get(guid).cloned()
    }

    /// Verdadero cuando aún no se ha cargado ningún GUID.
    #[must_use]
    pub(crate) fn is_empty(&self) -> bool {
        self.by_guid.is_empty()
    }

    /// Número de assets con GUID conocido.
    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.by_guid.len()
    }

    /// Pares GUID → ruta actual, ordenados para que el fichero sea estable.
    #[must_use]
    pub(crate) fn current_entries(&self) -> Vec<(String, String)> {
        let mut entries: Vec<(String, String)> = self
            .by_guid
            .iter()
            .map(|(guid, path)| (guid.clone(), path.clone()))
            .collect();
        entries.sort();
        entries
    }
}

/// Valida la forma `32 hexadecimales` de un GUID.
#[must_use]
pub(crate) fn is_guid(value: &str) -> bool {
    value.len() == GUID_HEX_LEN && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Lee el GUID de un `.meta`, ignorando el resto de claves.
#[must_use]
pub(crate) fn parse_meta(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(value) = line.trim().strip_prefix("guid=") {
            let value = value.trim();
            if is_guid(value) {
                return Some(value.to_owned());
            }
        }
    }
    None
}

/// Codifica el `.meta` de un asset. Las claves de opciones se añadirán en E17.
#[must_use]
pub(crate) fn encode_meta(guid: &str) -> String {
    format!("guid={guid}\n")
}

/// Lee el índice `guid=ruta` de disco, ignorando líneas mal formadas.
#[must_use]
pub(crate) fn parse_index(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (guid, path) = line.split_once('=')?;
            let guid = guid.trim();
            let path = path.trim();
            if is_guid(guid) && !path.is_empty() {
                Some((guid.to_owned(), path.to_owned()))
            } else {
                None
            }
        })
        .collect()
}

/// Codifica el índice con una ordenación determinista.
#[must_use]
pub(crate) fn encode_index(entries: &[(String, String)]) -> String {
    let mut entries = entries.to_vec();
    entries.sort();
    let mut text = String::new();
    for (guid, path) in entries {
        text.push_str(&format!("{guid}={path}\n"));
    }
    text
}

/// GUID determinista a partir de una semilla, sin dependencias nuevas.
#[must_use]
pub(crate) fn generate_guid(seed: u64) -> String {
    fn half(seed: u64, salt: u64) -> u64 {
        let mut hash = 0xcbf29ce4_84222325 ^ salt;
        for byte in seed.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
    format!("{:016x}{:016x}", half(seed, 0x1234), half(seed, 0x5678))
}

static GUID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Semilla mezclando reloj, proceso y contador para evitar colisiones locales.
#[must_use]
pub(crate) fn fresh_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos() as u64);
    let counter = GUID_COUNTER.fetch_add(1, Ordering::Relaxed);
    nanos ^ (u64::from(std::process::id()) << 32) ^ counter.rotate_left(17)
}

fn meta_path(root: &Path, path: &str) -> PathBuf {
    root.join(format!("{path}{META_SUFFIX}"))
}

/// Lee los `.meta` existentes sin crear ni escribir ficheros. Es la lectura
/// que se hace al abrir o guardar una escena: solo resuelve GUIDs ya conocidos.
#[must_use]
pub(crate) fn load(root: &Path, entries: &[AssetEntry]) -> GuidIndex {
    let pairs: Vec<(String, String)> = entries
        .iter()
        .filter_map(|entry| {
            let guid = fs::read_to_string(meta_path(root, &entry.path))
                .ok()
                .and_then(|text| parse_meta(&text))?;
            Some((entry.path.clone(), guid))
        })
        .collect();
    let text = fs::read_to_string(root.join(INDEX_FILE)).unwrap_or_default();
    let previous = GuidIndex::from_previous(parse_index(&text));
    GuidIndex::from_pairs(&pairs, &previous)
}

/// Lee los `.meta` de los assets y crea uno para los que aún no lo tienen.
pub(crate) fn scan(root: &Path, entries: &[AssetEntry]) -> Vec<(String, String)> {
    let mut pairs = Vec::with_capacity(entries.len());
    for entry in entries {
        let full = meta_path(root, &entry.path);
        let guid = fs::read_to_string(&full)
            .ok()
            .and_then(|text| parse_meta(&text))
            .unwrap_or_else(|| {
                let guid = generate_guid(fresh_seed());
                if let Some(parent) = full.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&full, encode_meta(&guid));
                guid
            });
        pairs.push((entry.path.clone(), guid));
    }
    pairs
}

/// Reconstruye y persiste el índice a partir de los `.meta` actuales.
#[must_use]
pub(crate) fn rebuild(root: &Path, entries: &[AssetEntry]) -> GuidIndex {
    let pairs = scan(root, entries);
    let text = fs::read_to_string(root.join(INDEX_FILE)).unwrap_or_default();
    let previous = GuidIndex::from_previous(parse_index(&text));
    let index = GuidIndex::from_pairs(&pairs, &previous);
    let _ = fs::write(
        root.join(INDEX_FILE),
        encode_index(&index.current_entries()),
    );
    index
}

/// Componente clonado con la ruta de asset sustituida; los componentes sin
/// asset se devuelven igual.
#[must_use]
pub(crate) fn with_asset_path(component: &Component, path: &str) -> Component {
    match component {
        Component::Image { .. } => Component::Image {
            asset: path.to_owned(),
        },
        Component::Sprite {
            meta, animations, ..
        } => Component::Sprite {
            asset: path.to_owned(),
            meta: *meta,
            animations: animations.clone(),
        },
        Component::Audio {
            volume, autoplay, ..
        } => Component::Audio {
            asset: path.to_owned(),
            volume: *volume,
            autoplay: *autoplay,
        },
        Component::Mesh { primitive, .. } => Component::Mesh {
            asset: path.to_owned(),
            primitive: *primitive,
        },
        Component::Collider2D { .. }
        | Component::Collider3D { .. }
        | Component::PrefabRef { .. } => component.clone(),
    }
}

/// Cambios de componentes necesarios para que la escena apunte a las rutas
/// actuales. Uno solo por componente y en orden de entidad/índice estable.
#[must_use]
pub(crate) fn path_changes(scene: &Scene, index: &GuidIndex) -> Vec<(u64, usize, Component)> {
    let mut changes = Vec::new();
    for entity in &scene.entities {
        for (position, component) in entity.components.iter().enumerate() {
            let Some(path) = component.asset_path() else {
                continue;
            };
            let Some(replacement) = index.resolve(path) else {
                continue;
            };
            if replacement != path {
                changes.push((
                    entity.id,
                    position,
                    with_asset_path(component, &replacement),
                ));
            }
        }
    }
    changes
}

/// Referencias que no aparecen en el índice ni en sus alias, ordenadas y sin
/// repetir. Se reportan pero no se modifican: pueden ser assets eliminados.
#[must_use]
pub(crate) fn broken_paths(scene: &Scene, index: &GuidIndex) -> Vec<String> {
    if index.is_empty() {
        return Vec::new();
    }
    let mut paths: Vec<String> = scene
        .entities
        .iter()
        .flat_map(|entity| entity.components.iter())
        .filter_map(Component::asset_path)
        .filter(|path| index.resolve(path).is_none())
        .map(str::to_owned)
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use lunar_scenes::{AnimationClip, SceneEntity, SpriteMeta};

    fn guid(seed: u64) -> String {
        generate_guid(seed)
    }

    #[test]
    fn generated_guids_are_stable_hex_and_differ_by_seed() {
        let first = guid(7);
        assert!(is_guid(&first));
        assert_eq!(first, guid(7));
        assert_ne!(first, guid(8));
    }

    #[test]
    fn meta_round_trip_ignores_other_keys() {
        let id = guid(1);
        assert_eq!(parse_meta(&encode_meta(&id)), Some(id.clone()));
        assert_eq!(
            parse_meta(&format!("filter=nearest\nguid={id}\nwrap=clamp\n")),
            Some(id)
        );
        assert_eq!(parse_meta("guid=corto\n"), None);
    }

    #[test]
    fn index_round_trip_is_sorted_and_skips_bad_lines() {
        let first = guid(2);
        let second = guid(1);
        let text = format!("{second}=b.png\nmal=z.png\n{first}=a.png\n# comentario\n\n");
        let decoded = parse_index(&text);
        assert_eq!(decoded.get(&first).map(String::as_str), Some("a.png"));
        assert_eq!(decoded.get(&second).map(String::as_str), Some("b.png"));
        assert_eq!(decoded.len(), 2);
        assert_eq!(
            encode_index(
                &decoded
                    .iter()
                    .map(|(a, b)| (a.clone(), b.clone()))
                    .collect::<Vec<_>>()
            ),
            format!("{first}=a.png\n{second}=b.png\n")
        );
    }

    #[test]
    fn moving_an_asset_keeps_its_guid_and_registers_an_alias() {
        let id = guid(3);
        let previous =
            GuidIndex::from_previous(HashMap::from([(id.clone(), "old/a.png".to_owned())]));
        let current = [("new/a.png".to_owned(), id.clone())];
        let index = GuidIndex::from_pairs(&current, &previous);

        assert_eq!(index.guid_for("new/a.png"), Some(id.as_str()));
        assert_eq!(index.path_for(&id), Some("new/a.png"));
        assert_eq!(index.alias_for("old/a.png"), Some(id.as_str()));
        assert_eq!(index.resolve("old/a.png").as_deref(), Some("new/a.png"));
        assert_eq!(index.resolve("new/a.png").as_deref(), Some("new/a.png"));
        assert_eq!(index.resolve("missing.png"), None);
    }

    #[test]
    fn aliases_survive_a_second_move() {
        let id = guid(4);
        let first = GuidIndex::from_previous(HashMap::from([(id.clone(), "a.png".to_owned())]));
        let second = GuidIndex::from_pairs(&[("b.png".to_owned(), id.clone())], &first);
        let third = GuidIndex::from_pairs(&[("c.png".to_owned(), id.clone())], &second);

        assert_eq!(third.resolve("a.png").as_deref(), Some("c.png"));
        assert_eq!(third.resolve("b.png").as_deref(), Some("c.png"));
    }

    #[test]
    fn path_changes_rewrite_only_known_moved_assets() {
        let id = guid(5);
        let index = GuidIndex::from_pairs(
            &[("new.png".to_owned(), id.clone())],
            &GuidIndex::from_previous(HashMap::from([(id, "old.png".to_owned())])),
        );
        let mut scene = Scene::new();
        let mut entity = SceneEntity::new(1);
        entity.components.push(Component::Image {
            asset: "old.png".to_owned(),
        });
        entity.components.push(Component::Image {
            asset: "new.png".to_owned(),
        });
        entity.components.push(Component::Image {
            asset: "perdido.png".to_owned(),
        });
        entity.components.push(Component::PrefabRef {
            name: "sin asset".to_owned(),
        });
        scene.entities.push(entity);

        let changes = path_changes(&scene, &index);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].0, 1);
        assert_eq!(changes[0].1, 0);
        assert_eq!(changes[0].2.asset_path(), Some("new.png"));
    }

    #[test]
    fn with_asset_path_preserves_every_other_field() {
        let component = Component::Sprite {
            asset: "old.png".to_owned(),
            meta: SpriteMeta {
                frame_width: 16,
                frame_height: 16,
                columns: 2,
                rows: 1,
                fps: 8.0,
                looping: true,
            },
            animations: vec![AnimationClip {
                name: "idle".to_owned(),
                frames: vec![0, 1],
                fps: 8.0,
                looping: true,
            }],
        };
        let rewritten = with_asset_path(&component, "new.png");
        assert_eq!(rewritten.asset_path(), Some("new.png"));
        let Component::Sprite {
            meta, animations, ..
        } = rewritten
        else {
            panic!("sprite");
        };
        assert_eq!(meta.frame_width, 16);
        assert_eq!(animations[0].name, "idle");
    }

    #[test]
    fn scanning_a_directory_creates_one_meta_per_asset() {
        let root = std::env::temp_dir().join(format!(
            "lunar_guids_{}_{}",
            std::process::id(),
            fresh_seed()
        ));
        let _ = fs::create_dir_all(root.join("sprites"));
        let _ = fs::write(root.join("sprites/hero.png"), b"png");
        let entries = vec![AssetEntry {
            path: "sprites/hero.png".to_owned(),
            kind: crate::assets::AssetKind::Image,
        }];
        let pairs = scan(&root, &entries);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "sprites/hero.png");
        assert!(is_guid(&pairs[0].1));
        assert_eq!(
            parse_meta(&fs::read_to_string(root.join("sprites/hero.png.meta")).unwrap()),
            Some(pairs[0].1.clone())
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn loading_reads_existing_meta_without_creating_files() {
        let root = std::env::temp_dir().join(format!(
            "lunar_guids_load_{}_{}",
            std::process::id(),
            fresh_seed()
        ));
        let _ = fs::create_dir_all(&root);
        let entries = vec![AssetEntry {
            path: "hero.png".to_owned(),
            kind: crate::assets::AssetKind::Image,
        }];
        let index = load(&root, &entries);
        assert!(index.is_empty());
        assert!(!root.join("hero.png.meta").exists());
        assert!(!root.join(INDEX_FILE).exists());

        let id = guid(9);
        let _ = fs::write(root.join("hero.png.meta"), encode_meta(&id));
        let _ = fs::write(
            root.join(INDEX_FILE),
            encode_index(&[(id.clone(), "hero.png".to_owned())]),
        );
        let loaded = load(&root, &entries);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.guid_for("hero.png"), Some(id.as_str()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn broken_paths_reports_unknown_assets_only_when_an_index_exists() {
        let mut scene = Scene::new();
        let mut entity = SceneEntity::new(1);
        entity.components.push(Component::Image {
            asset: "perdido.png".to_owned(),
        });
        entity.components.push(Component::Image {
            asset: "perdido.png".to_owned(),
        });
        scene.entities.push(entity);

        assert!(broken_paths(&scene, &GuidIndex::default()).is_empty());
        let indexed =
            GuidIndex::from_pairs(&[("vivo.png".to_owned(), guid(10))], &GuidIndex::default());
        assert_eq!(broken_paths(&scene, &indexed), ["perdido.png".to_owned()]);
    }
}
