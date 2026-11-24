//! Escenas serializables en JSON con esquema versionado (H9, ADR-008).
//!
//! Este crate solo define datos y validación: no abre ventanas, dispositivos
//! GPU ni de audio, y no depende de Rapier, rodio, image ni gltf.

use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Versión del esquema que acepta este crate.
pub const SCENE_VERSION: u32 = 1;

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

impl Component {
    /// Devuelve la ruta de asset del componente, si tiene una.
    #[must_use]
    pub fn asset_path(&self) -> Option<&str> {
        match self {
            Self::Mesh { asset, .. } | Self::Image { asset } | Self::Audio { asset, .. } => {
                Some(asset)
            }
            Self::Collider2D { .. } | Self::Collider3D { .. } => None,
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

/// Escena versionada con entidades y jerarquía validada.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub entities: Vec<SceneEntity>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            version: SCENE_VERSION,
            name: None,
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

    /// Decodifica y valida una escena JSON.
    pub fn from_json(json: &str) -> Result<Self, SceneError> {
        let scene: Self =
            serde_json::from_str(json).map_err(|error| SceneError::Decode(error.to_string()))?;
        scene.validate()?;
        Ok(scene)
    }

    /// Lee y valida una escena desde disco.
    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self, SceneError> {
        let bytes =
            std::fs::read_to_string(path).map_err(|error| SceneError::Io(error.to_string()))?;
        Self::from_json(&bytes)
    }

    /// Escribe la escena a disco como JSON legible.
    pub fn save_to_path<P: AsRef<Path>>(&self, path: P) -> Result<(), SceneError> {
        let json = self.to_json()?;
        std::fs::write(path, json).map_err(|error| SceneError::Io(error.to_string()))
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
                if let Some(path) = component.asset_path() {
                    validate_asset_path(path)?;
                }
                if let Component::Audio { volume, .. } = component
                    && (!volume.is_finite() || !(0.0..=1.0).contains(volume))
                {
                    return Err(SceneError::InvalidVolume(*volume));
                }
            }
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
        assert!(json.contains("\"version\": 1"));

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
        let json = r#"{"version":2,"entities":[]}"#;
        assert_eq!(
            Scene::from_json(json),
            Err(SceneError::UnsupportedVersion(2))
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
}
