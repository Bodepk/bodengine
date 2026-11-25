//! Escenas serializables en JSON con esquema versionado (H9, ADR-008).
//!
//! Este crate solo define datos y validación: no abre ventanas, dispositivos
//! GPU ni de audio, y no depende de Rapier, rodio, image ni gltf.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Versión canónica del esquema (ADR-027).
pub const SCENE_VERSION: u32 = 2;
/// Última versión legible mediante el convertidor explícito.
pub const SCENE_VERSION_V1: u32 = 1;

/// Errores de escena, sin exponer `serde_json` ni `std::io` en la API.
#[derive(Debug, Clone, PartialEq)]
pub enum SceneError {
    /// No se pudo leer o escribir el archivo.
    Io(String),
    /// No se pudo codificar la escena.
    Encode(String),
    /// El JSON no respeta el esquema.
    Decode(String),
    /// El documento usa una versión de esquema desconocida.
    UnsupportedVersion(u32),
    /// Dos entidades comparten el mismo id.
    DuplicateEntity(u64),
    /// El padre referenciado no existe.
    UnknownParent { child: u64, parent: u64 },
    /// La jerarquía contiene un ciclo que empieza en esta entidad.
    Cycle(u64),
    /// Una referencia de asset no es una ruta relativa válida.
    InvalidAssetPath(String),
    /// El volumen de un componente de audio está fuera de `[0.0, 1.0]`.
    InvalidVolume(f32),
    /// Una hoja de sprites tiene dimensiones o rejilla inválidas.
    InvalidSpriteMeta(String),
    /// Un clip de animación no existe o referencia fotogramas fuera de la rejilla.
    InvalidAnimation(String),
    /// Un prefab tiene un nombre duplicado.
    DuplicatePrefab(String),
    /// Una referencia a prefab no se resuelve.
    UnknownPrefab(String),
    /// Un prefab se referencia a sí mismo, directa o indirectamente.
    PrefabCycle(String),
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(f, "error de archivo de escena: {message}"),
            Self::Encode(message) => write!(f, "error codificando escena: {message}"),
            Self::Decode(message) => write!(f, "error decodificando escena: {message}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "versión de escena no soportada: {version}")
            }
            Self::DuplicateEntity(id) => write!(f, "id de entidad duplicado: {id}"),
            Self::UnknownParent { child, parent } => {
                write!(
                    f,
                    "entidad {child} referencia un padre inexistente: {parent}"
                )
            }
            Self::Cycle(id) => write!(f, "ciclo en la jerarquía desde la entidad {id}"),
            Self::InvalidAssetPath(path) => write!(f, "ruta de asset inválida: {path}"),
            Self::InvalidVolume(volume) => write!(f, "volumen fuera de rango: {volume}"),
            Self::InvalidSpriteMeta(message) => {
                write!(f, "metadatos de sprite inválidos: {message}")
            }
            Self::InvalidAnimation(message) => write!(f, "animación inválida: {message}"),
            Self::DuplicatePrefab(name) => write!(f, "prefab duplicado: {name}"),
            Self::UnknownPrefab(name) => write!(f, "prefab desconocido: {name}"),
            Self::PrefabCycle(name) => write!(f, "ciclo de prefabs desde: {name}"),
        }
    }
}

impl Error for SceneError {}

/// Transformación local de una entidad.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    pub translation: [f32; 3],
    /// Cuaternión en orden `(x, y, z, w)`.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        }
    }
}

/// Forma de un colisionador 2D, en datos POD sin tipos de Rapier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape2D {
    Ball { radius: f32 },
    Cuboid { half_extents: [f32; 2] },
}

/// Forma de un colisionador 3D, en datos POD sin tipos de Rapier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape3D {
    Ball { radius: f32 },
    Cuboid { half_extents: [f32; 3] },
    Capsule { half_height: f32, radius: f32 },
}

/// Tipo de cuerpo de un colisionador.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyType {
    Dynamic,
    Fixed,
    Sensor,
}

/// Componente serializable de una entidad.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Component {
    Mesh {
        asset: String,
        primitive: Option<u32>,
    },
    Image {
        asset: String,
    },
    Sprite {
        asset: String,
        #[serde(default)]
        meta: SpriteMeta,
        #[serde(default)]
        animations: Vec<AnimationClip>,
    },
    Audio {
        asset: String,
        volume: f32,
        autoplay: bool,
    },
    Collider2D {
        shape: Shape2D,
        body: BodyType,
    },
    Collider3D {
        shape: Shape3D,
        body: BodyType,
    },
    PrefabRef {
        name: String,
    },
}

/// Recorte y velocidad por defecto de una hoja de sprites.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpriteMeta {
    pub frame_width: u32,
    pub frame_height: u32,
    pub columns: u32,
    pub rows: u32,
    pub fps: f32,
    pub looping: bool,
}

impl Default for SpriteMeta {
    fn default() -> Self {
        Self {
            frame_width: 1,
            frame_height: 1,
            columns: 1,
            rows: 1,
            fps: 0.0,
            looping: true,
        }
    }
}

impl SpriteMeta {
    /// Celda única, usada por el convertidor de imágenes v1.
    #[must_use]
    pub const fn single_cell() -> Self {
        Self {
            frame_width: 1,
            frame_height: 1,
            columns: 1,
            rows: 1,
            fps: 0.0,
            looping: true,
        }
    }

    /// Número total de fotogramas de la hoja.
    #[must_use]
    pub const fn frame_count(self) -> u32 {
        self.columns.saturating_mul(self.rows)
    }
}

/// Clip de animación sobre los fotogramas de una hoja.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationClip {
    pub name: String,
    pub frames: Vec<u32>,
    pub fps: f32,
    pub looping: bool,
}

/// Plantilla de entidades declarada en la escena.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prefab {
    pub name: String,
    #[serde(default)]
    pub entities: Vec<SceneEntity>,
}

impl Component {
    /// Devuelve la ruta de asset del componente, si tiene una.
    #[must_use]
    pub fn asset_path(&self) -> Option<&str> {
        match self {
            Self::Mesh { asset, .. }
            | Self::Image { asset }
            | Self::Sprite { asset, .. }
            | Self::Audio { asset, .. } => Some(asset),
            Self::Collider2D { .. } | Self::Collider3D { .. } | Self::PrefabRef { .. } => None,
        }
    }
}

/// Entidad serializable con jerarquía y componentes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneEntity {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<u64>,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default)]
    pub components: Vec<Component>,
}

impl SceneEntity {
    /// Crea una entidad vacía con transformación identidad.
    #[must_use]
    pub fn new(id: u64) -> Self {
        Self {
            id,
            name: None,
            parent: None,
            transform: Transform::default(),
            components: Vec::new(),
        }
    }
}

/// Cabecera mínima para decidir el convertidor antes de descodificar el resto.
#[derive(Deserialize)]
struct VersionHeader {
    version: u32,
}

/// Tipos de la versión 1, aislados para no reintroducir campos en v2.
pub mod v1 {
    use super::*;

    /// Escena v1 canónica de ADR-008.
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct SceneV1 {
        version: u32,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        entities: Vec<EntityV1>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct EntityV1 {
        id: u64,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        parent: Option<u64>,
        #[serde(default)]
        transform: Transform,
        #[serde(default)]
        components: Vec<ComponentV1>,
    }

    #[derive(Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum ComponentV1 {
        Mesh {
            asset: String,
            primitive: Option<u32>,
        },
        Image {
            asset: String,
        },
        Audio {
            asset: String,
            volume: f32,
            autoplay: bool,
        },
        Collider2D {
            shape: Shape2D,
            body: BodyType,
        },
        Collider3D {
            shape: Shape3D,
            body: BodyType,
        },
    }

    /// Migra un documento v1 a v2 sin tocar los campos compartidos.
    pub fn upgrade(json: &str) -> Result<Scene, SceneError> {
        let old: SceneV1 =
            serde_json::from_str(json).map_err(|error| SceneError::Decode(error.to_string()))?;
        if old.version != SCENE_VERSION_V1 {
            return Err(SceneError::UnsupportedVersion(old.version));
        }
        let entities = old
            .entities
            .into_iter()
            .map(|entity| SceneEntity {
                id: entity.id,
                name: entity.name,
                parent: entity.parent,
                transform: entity.transform,
                components: entity
                    .components
                    .into_iter()
                    .map(|component| match component {
                        ComponentV1::Mesh { asset, primitive } => Component::Mesh {
                            asset: normalize_asset_path(&asset),
                            primitive,
                        },
                        ComponentV1::Image { asset } => Component::Sprite {
                            asset: normalize_asset_path(&asset),
                            meta: SpriteMeta::single_cell(),
                            animations: Vec::new(),
                        },
                        ComponentV1::Audio {
                            asset,
                            volume,
                            autoplay,
                        } => Component::Audio {
                            asset: normalize_asset_path(&asset),
                            volume,
                            autoplay,
                        },
                        ComponentV1::Collider2D { shape, body } => {
                            Component::Collider2D { shape, body }
                        }
                        ComponentV1::Collider3D { shape, body } => {
                            Component::Collider3D { shape, body }
                        }
                    })
                    .collect(),
            })
            .collect();
        Ok(Scene {
            version: SCENE_VERSION,
            name: old.name,
            prefabs: Vec::new(),
            entities,
        })
    }
}

/// Escena versionada con entidades y jerarquía validada.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prefabs: Vec<Prefab>,
    #[serde(default)]
    pub entities: Vec<SceneEntity>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            version: SCENE_VERSION,
            name: None,
            prefabs: Vec::new(),
            entities: Vec::new(),
        }
    }
}

impl Scene {
    /// Crea una escena vacía con la versión actual.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Codifica la escena como JSON legible.
    pub fn to_json(&self) -> Result<String, SceneError> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|error| SceneError::Encode(error.to_string()))
    }

    /// Decodifica y valida una escena JSON, migrando v1 de forma explícita.
    pub fn from_json(json: &str) -> Result<Self, SceneError> {
        let header: VersionHeader =
            serde_json::from_str(json).map_err(|error| SceneError::Decode(error.to_string()))?;
        let scene = match header.version {
            SCENE_VERSION => {
                serde_json::from_str(json).map_err(|error| SceneError::Decode(error.to_string()))?
            }
            SCENE_VERSION_V1 => v1::upgrade(json)?,
            other => return Err(SceneError::UnsupportedVersion(other)),
        };
        scene.validate()?;
        Ok(scene)
    }

    /// Lee y valida una escena desde disco.
    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self, SceneError> {
        let bytes =
            std::fs::read_to_string(path).map_err(|error| SceneError::Io(error.to_string()))?;
        Self::from_json(&bytes)
    }

    /// Escribe la escena a disco como JSON legible, creando los directorios que falten.
    pub fn save_to_path<P: AsRef<Path>>(&self, path: P) -> Result<(), SceneError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|error| SceneError::Io(error.to_string()))?;
        }
        let json = self.to_json()?;
        std::fs::write(path, json).map_err(|error| SceneError::Io(error.to_string()))
    }

    /// Convierte las barras invertidas de las rutas de assets en `/` y valida el resultado.
    ///
    /// No cambia la versión del esquema: ADR-008 sigue exigiendo rutas relativas.
    pub fn normalize_asset_paths(&mut self) -> Result<(), SceneError> {
        for entity in &mut self.entities {
            for component in &mut entity.components {
                match component {
                    Component::Mesh { asset, .. }
                    | Component::Image { asset }
                    | Component::Sprite { asset, .. }
                    | Component::Audio { asset, .. } => {
                        let normalized = normalize_asset_path(asset);
                        validate_asset_path(&normalized)?;
                        *asset = normalized;
                    }
                    Component::Collider2D { .. }
                    | Component::Collider3D { .. }
                    | Component::PrefabRef { .. } => {}
                }
            }
        }
        Ok(())
    }

    /// Comprueba versión, IDs, jerarquía, rutas y volúmenes.
    pub fn validate(&self) -> Result<(), SceneError> {
        if self.version != SCENE_VERSION {
            return Err(SceneError::UnsupportedVersion(self.version));
        }

        let mut ids = HashSet::with_capacity(self.entities.len());
        for entity in &self.entities {
            if !ids.insert(entity.id) {
                return Err(SceneError::DuplicateEntity(entity.id));
            }
        }
        for entity in &self.entities {
            if let Some(parent) = entity.parent
                && !ids.contains(&parent)
            {
                return Err(SceneError::UnknownParent {
                    child: entity.id,
                    parent,
                });
            }
            for component in &entity.components {
                Self::validate_component(component)?;
                if let Component::PrefabRef { name } = component
                    && !self.prefabs.iter().any(|prefab| prefab.name == *name)
                {
                    return Err(SceneError::UnknownPrefab(name.clone()));
                }
            }
        }

        let mut prefab_names = HashSet::with_capacity(self.prefabs.len());
        for prefab in &self.prefabs {
            if prefab.name.is_empty() || !prefab_names.insert(prefab.name.as_str()) {
                return Err(SceneError::DuplicatePrefab(prefab.name.clone()));
            }
            let mut prefab_ids = HashSet::with_capacity(prefab.entities.len());
            for entity in &prefab.entities {
                if !prefab_ids.insert(entity.id) {
                    return Err(SceneError::DuplicateEntity(entity.id));
                }
                for component in &entity.components {
                    Self::validate_component(component)?;
                }
            }
            for entity in &prefab.entities {
                if let Some(parent) = entity.parent
                    && !prefab_ids.contains(&parent)
                {
                    return Err(SceneError::UnknownParent {
                        child: entity.id,
                        parent,
                    });
                }
            }
        }
        for prefab in &self.prefabs {
            let mut visiting = Vec::new();
            Self::check_prefab_cycle(prefab, &self.prefabs, &mut visiting)?;
        }

        let by_id = |id: u64| self.entities.iter().find(|entity| entity.id == id);
        for entity in &self.entities {
            let mut seen = HashSet::new();
            seen.insert(entity.id);
            let mut current = entity;
            while let Some(parent) = current.parent {
                if !seen.insert(parent) {
                    return Err(SceneError::Cycle(entity.id));
                }
                current = by_id(parent).ok_or(SceneError::UnknownParent {
                    child: current.id,
                    parent,
                })?;
            }
        }
        Ok(())
    }

    /// Valida un componente sin depender de la escena que lo contiene.
    fn validate_component(component: &Component) -> Result<(), SceneError> {
        if let Some(path) = component.asset_path() {
            validate_asset_path(path)?;
        }
        match component {
            Component::Audio { volume, .. } => {
                if !volume.is_finite() || !(0.0..=1.0).contains(volume) {
                    return Err(SceneError::InvalidVolume(*volume));
                }
            }
            Component::Sprite {
                meta, animations, ..
            } => {
                if meta.frame_width == 0
                    || meta.frame_height == 0
                    || meta.columns == 0
                    || meta.rows == 0
                    || !meta.fps.is_finite()
                    || meta.fps < 0.0
                {
                    return Err(SceneError::InvalidSpriteMeta(format!("{meta:?}")));
                }
                let count = meta.frame_count();
                let mut names = HashSet::with_capacity(animations.len());
                for clip in animations {
                    if clip.name.is_empty()
                        || !names.insert(clip.name.as_str())
                        || !clip.fps.is_finite()
                        || clip.fps < 0.0
                        || clip.frames.is_empty()
                        || clip.frames.iter().any(|frame| *frame >= count)
                    {
                        return Err(SceneError::InvalidAnimation(clip.name.clone()));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Detecta ciclos entre prefabs siguiendo sus `PrefabRef`.
    fn check_prefab_cycle(
        prefab: &Prefab,
        prefabs: &[Prefab],
        visiting: &mut Vec<String>,
    ) -> Result<(), SceneError> {
        if visiting.iter().any(|name| name == &prefab.name) {
            return Err(SceneError::PrefabCycle(prefab.name.clone()));
        }
        visiting.push(prefab.name.clone());
        for entity in &prefab.entities {
            for component in &entity.components {
                if let Component::PrefabRef { name } = component {
                    let Some(child) = prefabs.iter().find(|candidate| candidate.name == *name)
                    else {
                        return Err(SceneError::UnknownPrefab(name.clone()));
                    };
                    Self::check_prefab_cycle(child, prefabs, visiting)?;
                }
            }
        }
        visiting.pop();
        Ok(())
    }

    /// Instancia un prefab con ids remapeados, sin mutar la escena.
    #[must_use]
    pub fn instantiate_prefab(&self, name: &str, next_id: u64) -> Option<Vec<SceneEntity>> {
        let prefab = self.prefabs.iter().find(|prefab| prefab.name == name)?;
        let mut map = HashMap::with_capacity(prefab.entities.len());
        for (id, entity) in (next_id..).zip(prefab.entities.iter()) {
            map.insert(entity.id, id);
        }
        Some(
            prefab
                .entities
                .iter()
                .map(|entity| SceneEntity {
                    id: map[&entity.id],
                    name: entity.name.clone(),
                    parent: entity.parent.and_then(|parent| map.get(&parent).copied()),
                    transform: entity.transform,
                    components: entity.components.clone(),
                })
                .collect(),
        )
    }
}

/// Normaliza el separador de una ruta escrita en Windows al formato de ADR-008.
#[must_use]
pub fn normalize_asset_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn validate_asset_path(path: &str) -> Result<(), SceneError> {
    let valid = !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.split('/').any(|part| part == "..");
    if valid {
        Ok(())
    } else {
        Err(SceneError::InvalidAssetPath(path.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn sample_scene() -> Scene {
        let mut root = SceneEntity::new(1);
        root.name = Some("Root".to_owned());
        root.components.push(Component::Mesh {
            asset: "models/ship.glb".to_owned(),
            primitive: Some(0),
        });
        root.components.push(Component::Audio {
            asset: "sfx/engine.wav".to_owned(),
            volume: 0.5,
            autoplay: true,
        });

        let mut child = SceneEntity::new(2);
        child.parent = Some(1);
        child.transform.translation = [1.0, 2.0, 3.0];
        child.components.push(Component::Collider3D {
            shape: Shape3D::Ball { radius: 0.5 },
            body: BodyType::Dynamic,
        });

        Scene {
            version: SCENE_VERSION,
            name: Some("Demo".to_owned()),
            prefabs: Vec::new(),
            entities: vec![root, child],
        }
    }

    fn temp_scene_path() -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("lunar_scene_{}_{counter}.json", std::process::id()))
    }

    #[test]
    fn round_trip_json_preserves_scene() {
        let scene = sample_scene();
        let json = scene.to_json().expect("encode scene");
        assert!(json.contains("\"version\": 2"));

        let decoded = Scene::from_json(&json).expect("decode scene");
        assert_eq!(decoded, scene);
        assert_eq!(decoded.entities[1].parent, Some(1));
    }

    #[test]
    fn load_and_save_path() {
        let path = temp_scene_path();
        let scene = sample_scene();
        scene.save_to_path(&path).expect("save scene");

        let loaded = Scene::load_from_path(&path).expect("load scene");
        assert_eq!(loaded, scene);

        std::fs::remove_file(&path).expect("remove scene");
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let json = r#"{"version":3,"entities":[]}"#;
        assert_eq!(
            Scene::from_json(json),
            Err(SceneError::UnsupportedVersion(3))
        );
    }

    #[test]
    fn duplicate_ids_and_unknown_parent_are_rejected() {
        let mut scene = sample_scene();
        scene.entities[1].id = 1;
        assert_eq!(scene.validate(), Err(SceneError::DuplicateEntity(1)));

        let mut scene = sample_scene();
        scene.entities[1].parent = Some(99);
        assert_eq!(
            scene.validate(),
            Err(SceneError::UnknownParent {
                child: 2,
                parent: 99
            })
        );
    }

    #[test]
    fn cycles_are_rejected() {
        let mut scene = sample_scene();
        scene.entities[0].parent = Some(2);
        assert_eq!(scene.validate(), Err(SceneError::Cycle(1)));
    }

    #[test]
    fn asset_paths_and_volumes_are_validated() {
        let mut scene = sample_scene();
        if let Component::Mesh { asset, .. } = &mut scene.entities[0].components[0] {
            *asset = "../secret.glb".to_owned();
        }
        assert!(matches!(
            scene.validate(),
            Err(SceneError::InvalidAssetPath(_))
        ));

        let mut scene = sample_scene();
        if let Component::Audio { volume, .. } = &mut scene.entities[0].components[1] {
            *volume = 2.0;
        }
        assert_eq!(scene.validate(), Err(SceneError::InvalidVolume(2.0)));
    }

    #[test]
    fn unknown_fields_and_bad_json_are_rejected() {
        let json = r#"{"version":1,"entities":[],"future":true}"#;
        assert!(matches!(Scene::from_json(json), Err(SceneError::Decode(_))));
        assert!(matches!(
            Scene::from_json("not json"),
            Err(SceneError::Decode(_))
        ));
    }

    #[test]
    fn default_scene_is_empty_and_valid() {
        let scene = Scene::new();
        assert!(scene.validate().is_ok());
        assert!(scene.entities.is_empty());
        assert_eq!(scene.version, SCENE_VERSION);
    }

    #[test]
    fn windows_asset_paths_can_be_normalized() {
        assert_eq!(normalize_asset_path(r"models\ship.glb"), "models/ship.glb");

        let mut scene = sample_scene();
        if let Component::Mesh { asset, .. } = &mut scene.entities[0].components[0] {
            *asset = r"models\ship.glb".to_owned();
        }
        assert!(matches!(
            scene.validate(),
            Err(SceneError::InvalidAssetPath(_))
        ));

        scene.normalize_asset_paths().expect("normalize paths");
        assert_eq!(scene.version, SCENE_VERSION);
        assert!(scene.validate().is_ok());
        assert_eq!(
            scene.entities[0].components[0].asset_path(),
            Some("models/ship.glb")
        );
    }

    #[test]
    fn normalization_still_rejects_traversal() {
        let mut scene = sample_scene();
        if let Component::Mesh { asset, .. } = &mut scene.entities[0].components[0] {
            *asset = r"..\secret.glb".to_owned();
        }
        assert!(matches!(
            scene.normalize_asset_paths(),
            Err(SceneError::InvalidAssetPath(_))
        ));
    }

    #[test]
    fn save_creates_missing_parent_directories() {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("lunar_scene_dir_{}_{counter}", std::process::id()));
        let path = directory.join("nested").join("scene.json");

        sample_scene().save_to_path(&path).expect("save scene");
        assert!(path.exists());
        assert_eq!(
            Scene::load_from_path(&path).expect("load scene"),
            sample_scene()
        );

        std::fs::remove_dir_all(&directory).expect("remove scene directory");
    }

    #[test]
    fn v1_images_upgrade_to_single_cell_sprites() {
        let json = r#"{"version":1,"name":"vieja","entities":[{"id":1,"name":"Nave","components":[{"kind":"image","asset":"sprites\\nave.png"}]}]}"#;
        let scene = Scene::from_json(json).expect("migrar v1");
        assert_eq!(scene.version, SCENE_VERSION);
        assert_eq!(scene.name.as_deref(), Some("vieja"));
        assert_eq!(scene.entities.len(), 1);
        let Component::Sprite {
            asset,
            meta,
            animations,
        } = &scene.entities[0].components[0]
        else {
            panic!("image no migrada a sprite");
        };
        assert_eq!(asset, "sprites/nave.png");
        assert_eq!(*meta, SpriteMeta::single_cell());
        assert!(animations.is_empty());
        let encoded = scene.to_json().expect("emitir v2");
        assert!(encoded.contains("\"version\": 2"));
        assert!(encoded.contains("\"kind\": \"sprite\""));
        assert!(!encoded.contains("\"kind\": \"image\""));
        assert_eq!(Scene::from_json(&encoded).expect("releer v2"), scene);
    }

    #[test]
    fn v1_layout_is_not_accepted_as_v2() {
        let json = r#"{"version":1,"prefabs":[],"entities":[]}"#;
        assert!(matches!(Scene::from_json(json), Err(SceneError::Decode(_))));
        assert_eq!(
            Scene::from_json(r#"{"version":3,"entities":[]}"#).unwrap_err(),
            SceneError::UnsupportedVersion(3)
        );
    }

    #[test]
    fn sprites_validate_clips_and_prefabs() {
        let mut scene = Scene::new();
        let mut ship = SceneEntity::new(1);
        ship.components.push(Component::Sprite {
            asset: "sheet.png".to_owned(),
            meta: SpriteMeta {
                frame_width: 16,
                frame_height: 16,
                columns: 4,
                rows: 2,
                fps: 12.0,
                looping: true,
            },
            animations: vec![AnimationClip {
                name: "idle".to_owned(),
                frames: vec![0, 1, 2, 3],
                fps: 8.0,
                looping: true,
            }],
        });
        ship.components.push(Component::PrefabRef {
            name: "motor".to_owned(),
        });
        scene.entities.push(ship);
        let mut motor = SceneEntity::new(10);
        motor.name = Some("Motor".to_owned());
        let mut base = SceneEntity::new(11);
        base.parent = Some(10);
        scene.prefabs.push(Prefab {
            name: "motor".to_owned(),
            entities: vec![motor, base],
        });
        scene.validate().expect("escena v2 válida");

        let instances = scene
            .instantiate_prefab("motor", 100)
            .expect("instanciar prefab");
        assert_eq!(
            instances.iter().map(|entity| entity.id).collect::<Vec<_>>(),
            vec![100, 101]
        );
        assert_eq!(instances[1].parent, Some(100));
        assert!(scene.instantiate_prefab("ausente", 1).is_none());

        let Component::Sprite {
            asset,
            meta,
            mut animations,
        } = scene.entities[0].components.remove(0)
        else {
            panic!("sprite esperado");
        };
        animations[0].frames[0] = 8;
        scene.entities[0].components.push(Component::Sprite {
            asset,
            meta,
            animations,
        });
        assert!(matches!(
            scene.validate(),
            Err(SceneError::InvalidAnimation(_))
        ));
        let sprite = scene.entities[0]
            .components
            .iter_mut()
            .find_map(|component| match component {
                Component::Sprite { animations, .. } => Some(animations),
                _ => None,
            })
            .expect("sprite esperado");
        sprite[0].frames[0] = 0;
        scene.prefabs[0].name = "ciclo".to_owned();
        scene.prefabs[0].entities[0]
            .components
            .push(Component::PrefabRef {
                name: "motor".to_owned(),
            });
        scene.prefabs.push(Prefab {
            name: "motor".to_owned(),
            entities: vec![SceneEntity {
                components: vec![Component::PrefabRef {
                    name: "ciclo".to_owned(),
                }],
                ..SceneEntity::new(20)
            }],
        });
        assert!(matches!(scene.validate(), Err(SceneError::PrefabCycle(_))));
    }

    #[test]
    fn native_v2_example_loads() {
        let json = include_str!("../../../examples/scene_demo/assets/sprites.scene.json");
        let scene = Scene::from_json(json).expect("escena v2 de ejemplo");
        assert_eq!(scene.version, SCENE_VERSION);
        assert_eq!(scene.prefabs.len(), 1);
        assert_eq!(scene.instantiate_prefab("nave", 100).unwrap().len(), 1);
        assert!(
            scene
                .to_json()
                .expect("emitir v2")
                .contains("\"version\": 2")
        );
    }

    #[test]
    fn sprite_metadata_rejects_invalid_geometry() {
        let mut entity = SceneEntity::new(1);
        entity.components.push(Component::Sprite {
            asset: "sheet.png".to_owned(),
            meta: SpriteMeta {
                frame_width: 0,
                ..SpriteMeta::single_cell()
            },
            animations: Vec::new(),
        });
        let mut scene = Scene::new();
        scene.entities.push(entity);
        assert!(matches!(
            scene.validate(),
            Err(SceneError::InvalidSpriteMeta(_))
        ));
    }
}
