//! Núcleo de editor headless (H10, ADR-015).
//!
//! No abre ventanas ni dispositivos: posee un [`Scene`], una selección efímera
//! y un historial de comandos. Toda mutación se valida con las reglas de
//! ADR-008 antes de entrar en el historial.

use std::error::Error;
use std::fmt;
use std::path::Path;

use lunar_scenes::{Component, Scene, SceneEntity, SceneError, Transform};

/// Errores del documento de editor, sin exponer `serde_json` ni `std::io`.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorError {
    /// La escena resultante no respeta el esquema de ADR-008.
    Scene(SceneError),
    /// La entidad referenciada no existe.
    UnknownEntity(u64),
    /// El índice de componente no existe en la entidad.
    ComponentIndex { id: u64, index: usize },
}

impl fmt::Display for EditorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scene(error) => write!(f, "error de escena: {error}"),
            Self::UnknownEntity(id) => write!(f, "entidad desconocida: {id}"),
            Self::ComponentIndex { id, index } => {
                write!(f, "componente {index} inexistente en la entidad {id}")
            }
        }
    }
}

impl Error for EditorError {}

impl From<SceneError> for EditorError {
    fn from(error: SceneError) -> Self {
        Self::Scene(error)
    }
}

/// Operación visible del editor. Todas pasan por [`EditorDocument::apply`].
#[derive(Debug, Clone, PartialEq)]
pub enum EditorCommand {
    /// Crea una entidad con transformación identidad.
    CreateEntity { id: u64, name: Option<String> },
    /// Borra una entidad y rearma a sus hijos con su padre.
    DeleteEntity { id: u64 },
    /// Cambia el nombre de una entidad.
    RenameEntity { id: u64, name: Option<String> },
    /// Cambia el padre de una entidad; `None` la convierte en raíz.
    SetParent { id: u64, parent: Option<u64> },
    /// Sustituye la transformación local de una entidad.
    SetTransform { id: u64, transform: Transform },
    /// Añade un componente serializable al final de la entidad.
    AddComponent { id: u64, component: Component },
    /// Sustituye un componente por índice (ADR-030).
    SetComponent {
        id: u64,
        index: usize,
        component: Component,
    },
    /// Elimina un componente por índice.
    RemoveComponent { id: u64, index: usize },
    /// Reordena una entidad dentro del vector de la escena (ADR-020).
    MoveEntity { id: u64, to: usize },
    /// Clona una entidad con un identificador nuevo (ADR-020).
    DuplicateEntity { id: u64, new_id: u64 },
}

#[derive(Debug, Clone, PartialEq)]
struct Snapshot {
    scene: Scene,
    selection: Vec<u64>,
}

impl Snapshot {
    fn from_document(document: &EditorDocument) -> Self {
        Self {
            scene: document.scene.clone(),
            selection: document.selection.clone(),
        }
    }

    fn restore_into(self, document: &mut EditorDocument) {
        document.scene = self.scene;
        document.selection = self.selection;
    }
}

/// Documento editable con selección, criterio de guardado e historial.
///
/// La escena solo se expone por referencia: las mutaciones deben pasar por
/// `apply` para mantener la validación y el undo/redo.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorDocument {
    scene: Scene,
    selection: Vec<u64>,
    dirty: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Default for EditorDocument {
    fn default() -> Self {
        Self {
            scene: Scene::new(),
            selection: Vec::new(),
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
}

impl EditorDocument {
    /// Crea un documento vacío con la versión de esquema actual.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Crea un documento a partir de una escena ya validada.
    pub fn from_scene(scene: Scene) -> Result<Self, EditorError> {
        scene.validate()?;
        Ok(Self {
            scene,
            ..Self::default()
        })
    }

    /// Lee una escena desde disco y descarta el historial anterior.
    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self, EditorError> {
        let scene = Scene::load_from_path(path)?;
        Self::from_scene(scene)
    }

    /// Guarda la escena y marca el documento como limpio.
    pub fn save_to_path<P: AsRef<Path>>(&mut self, path: P) -> Result<(), EditorError> {
        self.scene.save_to_path(path)?;
        self.dirty = false;
        Ok(())
    }

    /// Devuelve la escena actual; no permite mutarla directamente.
    #[must_use]
    pub const fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Devuelve los IDs seleccionados, ya depurados de entidades inexistentes.
    #[must_use]
    pub fn selection(&self) -> &[u64] {
        &self.selection
    }

    /// Fija una selección efímera. No entra en el esquema ni en el historial.
    pub fn select(&mut self, ids: &[u64]) {
        self.selection = ids
            .iter()
            .copied()
            .filter(|id| self.scene.entities.iter().any(|entity| entity.id == *id))
            .collect();
    }

    /// Indica si hay cambios sin guardar.
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Indica si hay una operación pendiente de deshacer.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Indica si hay una operación pendiente de rehacer.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Número de operaciones pendientes de deshacer.
    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Valida, aplica y registra un comando. Un comando inválido no cambia nada.
    pub fn apply(&mut self, command: EditorCommand) -> Result<(), EditorError> {
        let mut next = self.scene.clone();
        let mut next_selection = self.selection.clone();
        mutate(&mut next, &mut next_selection, &command)?;
        next.validate()?;
        next_selection.retain(|id| next.entities.iter().any(|entity| entity.id == *id));

        let before = Snapshot::from_document(self);
        self.scene = next;
        self.selection = next_selection;
        self.dirty = true;
        self.undo.push(before);
        self.redo.clear();
        Ok(())
    }

    /// Deshace la última operación. Devuelve `false` si no hay historial.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(Snapshot::from_document(self));
        previous.restore_into(self);
        self.dirty = true;
        true
    }

    /// Rehace la última operación deshecha. Devuelve `false` si no hay nada.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(Snapshot::from_document(self));
        next.restore_into(self);
        self.dirty = true;
        true
    }
}

fn mutate(
    scene: &mut Scene,
    selection: &mut Vec<u64>,
    command: &EditorCommand,
) -> Result<(), EditorError> {
    match command {
        EditorCommand::CreateEntity { id, name } => {
            if scene.entities.iter().any(|entity| entity.id == *id) {
                return Err(SceneError::DuplicateEntity(*id).into());
            }
            let mut entity = SceneEntity::new(*id);
            entity.name = name.clone();
            scene.entities.push(entity);
            *selection = vec![*id];
        }
        EditorCommand::DeleteEntity { id } => {
            let position = position_of(scene, *id)?;
            let removed = scene.entities.remove(position);
            for entity in &mut scene.entities {
                if entity.parent == Some(*id) {
                    entity.parent = removed.parent;
                }
            }
            selection.retain(|selected| selected != id);
        }
        EditorCommand::RenameEntity { id, name } => {
            entity_mut(scene, *id)?.name = name.clone();
        }
        EditorCommand::SetParent { id, parent } => {
            entity_mut(scene, *id)?.parent = *parent;
        }
        EditorCommand::SetTransform { id, transform } => {
            entity_mut(scene, *id)?.transform = *transform;
        }
        EditorCommand::AddComponent { id, component } => {
            entity_mut(scene, *id)?.components.push(component.clone());
        }
        EditorCommand::SetComponent {
            id,
            index,
            component,
        } => {
            let entity = entity_mut(scene, *id)?;
            let Some(slot) = entity.components.get_mut(*index) else {
                return Err(EditorError::ComponentIndex {
                    id: *id,
                    index: *index,
                });
            };
            *slot = component.clone();
        }
        EditorCommand::RemoveComponent { id, index } => {
            let entity = entity_mut(scene, *id)?;
            if *index >= entity.components.len() {
                return Err(EditorError::ComponentIndex {
                    id: *id,
                    index: *index,
                });
            }
            entity.components.remove(*index);
        }
        EditorCommand::MoveEntity { id, to } => {
            let from = position_of(scene, *id)?;
            let destination = (*to).min(scene.entities.len() - 1);
            let entity = scene.entities.remove(from);
            scene.entities.insert(destination, entity);
        }
        EditorCommand::DuplicateEntity { id, new_id } => {
            if scene.entities.iter().any(|entity| entity.id == *new_id) {
                return Err(SceneError::DuplicateEntity(*new_id).into());
            }
            let mut entity = entity_mut(scene, *id)?.clone();
            entity.id = *new_id;
            scene.entities.push(entity);
            *selection = vec![*new_id];
        }
    }
    Ok(())
}

fn position_of(scene: &Scene, id: u64) -> Result<usize, EditorError> {
    scene
        .entities
        .iter()
        .position(|entity| entity.id == id)
        .ok_or(EditorError::UnknownEntity(id))
}

fn entity_mut(scene: &mut Scene, id: u64) -> Result<&mut SceneEntity, EditorError> {
    let position = position_of(scene, id)?;
    Ok(&mut scene.entities[position])
}

#[cfg(test)]
mod tests {
    use super::*;
    use lunar_scenes::{BodyType, SCENE_VERSION, Shape3D};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_editor_path() -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "lunar_editor_{}_{counter}.json",
            std::process::id()
        ))
    }

    #[test]
    fn create_rename_and_transform_undo_redo() {
        let mut document = EditorDocument::new();
        document
            .apply(EditorCommand::CreateEntity {
                id: 1,
                name: Some("Root".to_owned()),
            })
            .expect("create root");
        let transform = Transform {
            translation: [1.0, 2.0, 3.0],
            ..Transform::default()
        };
        document
            .apply(EditorCommand::SetTransform { id: 1, transform })
            .expect("set transform");
        document
            .apply(EditorCommand::RenameEntity {
                id: 1,
                name: Some("Player".to_owned()),
            })
            .expect("rename");

        assert_eq!(document.undo_len(), 3);
        assert!(document.can_undo());
        assert!(!document.can_redo());
        assert!(document.is_dirty());
        assert_eq!(document.scene().entities[0].name.as_deref(), Some("Player"));
        assert_eq!(document.scene().entities[0].transform, transform);

        assert!(document.undo());
        assert_eq!(document.scene().entities[0].name.as_deref(), Some("Root"));
        assert!(document.undo());
        assert_eq!(document.scene().entities[0].transform, Transform::default());
        assert!(document.undo());
        assert!(document.scene().entities.is_empty());
        assert!(!document.can_undo());

        assert!(document.redo());
        assert!(document.redo());
        assert!(document.redo());
        assert_eq!(document.scene().entities[0].name.as_deref(), Some("Player"));
        assert!(!document.can_redo());
    }

    #[test]
    fn delete_reparents_children_and_undo_restores() {
        let mut document = EditorDocument::new();
        for id in [1, 2, 3] {
            document
                .apply(EditorCommand::CreateEntity { id, name: None })
                .expect("create entity");
        }
        document
            .apply(EditorCommand::SetParent {
                id: 2,
                parent: Some(1),
            })
            .expect("parent 2");
        document
            .apply(EditorCommand::SetParent {
                id: 3,
                parent: Some(2),
            })
            .expect("parent 3");

        document
            .apply(EditorCommand::DeleteEntity { id: 2 })
            .expect("delete 2");
        let child = document
            .scene()
            .entities
            .iter()
            .find(|entity| entity.id == 3)
            .expect("child 3");
        assert_eq!(child.parent, Some(1));
        assert!(!document.selection().contains(&2));

        assert!(document.undo());
        let child = document
            .scene()
            .entities
            .iter()
            .find(|entity| entity.id == 3)
            .expect("child 3");
        assert_eq!(child.parent, Some(2));
        assert!(
            document
                .scene()
                .entities
                .iter()
                .any(|entity| entity.id == 2)
        );
    }

    #[test]
    fn invalid_commands_leave_document_untouched() {
        let mut document = EditorDocument::new();
        document
            .apply(EditorCommand::CreateEntity { id: 1, name: None })
            .expect("create entity");
        let before = document.scene().clone();

        assert_eq!(
            document.apply(EditorCommand::CreateEntity { id: 1, name: None }),
            Err(EditorError::Scene(SceneError::DuplicateEntity(1)))
        );
        assert_eq!(
            document.apply(EditorCommand::SetTransform {
                id: 99,
                transform: Transform::default(),
            }),
            Err(EditorError::UnknownEntity(99))
        );
        assert_eq!(
            document.apply(EditorCommand::SetParent {
                id: 1,
                parent: Some(1),
            }),
            Err(EditorError::Scene(SceneError::Cycle(1)))
        );
        assert_eq!(
            document.apply(EditorCommand::AddComponent {
                id: 1,
                component: Component::Audio {
                    asset: "boom.wav".to_owned(),
                    volume: 2.0,
                    autoplay: false,
                },
            }),
            Err(EditorError::Scene(SceneError::InvalidVolume(2.0)))
        );
        assert_eq!(document.scene(), &before);
        assert_eq!(document.undo_len(), 1);
    }

    #[test]
    fn selection_is_ephemeral_and_sanitized() {
        let mut document = EditorDocument::new();
        document
            .apply(EditorCommand::CreateEntity { id: 1, name: None })
            .expect("create 1");
        document
            .apply(EditorCommand::CreateEntity { id: 2, name: None })
            .expect("create 2");
        document.select(&[1, 2, 99]);
        assert_eq!(document.selection(), &[1, 2]);
        assert_eq!(document.scene().version, SCENE_VERSION);

        document
            .apply(EditorCommand::DeleteEntity { id: 2 })
            .expect("delete 2");
        assert_eq!(document.selection(), &[1]);
        assert!(document.undo());
        assert_eq!(document.selection(), &[1, 2]);
    }

    #[test]
    fn components_can_be_removed_by_index() {
        let mut document = EditorDocument::new();
        document
            .apply(EditorCommand::CreateEntity { id: 1, name: None })
            .expect("create entity");
        let component = Component::Collider3D {
            shape: Shape3D::Ball { radius: 0.5 },
            body: BodyType::Fixed,
        };
        document
            .apply(EditorCommand::AddComponent {
                id: 1,
                component: component.clone(),
            })
            .expect("add component");
        assert_eq!(document.scene().entities[0].components, vec![component]);

        assert_eq!(
            document.apply(EditorCommand::RemoveComponent { id: 1, index: 1 }),
            Err(EditorError::ComponentIndex { id: 1, index: 1 })
        );
        assert!(
            document
                .apply(EditorCommand::RemoveComponent { id: 1, index: 0 })
                .is_ok()
        );
        assert!(document.scene().entities[0].components.is_empty());
    }

    #[test]
    fn set_component_validates_and_undoes() {
        use lunar_scenes::{AnimationClip, SpriteMeta};

        let mut document = EditorDocument::new();
        document
            .apply(EditorCommand::CreateEntity { id: 1, name: None })
            .expect("create entity");
        let sprite = Component::Sprite {
            asset: "sheet.png".to_owned(),
            meta: SpriteMeta::single_cell(),
            animations: Vec::new(),
        };
        document
            .apply(EditorCommand::AddComponent {
                id: 1,
                component: sprite.clone(),
            })
            .expect("add sprite");

        let edited = Component::Sprite {
            asset: "sheet.png".to_owned(),
            meta: SpriteMeta {
                columns: 4,
                rows: 2,
                fps: 8.0,
                ..SpriteMeta::single_cell()
            },
            animations: vec![AnimationClip {
                name: "idle".to_owned(),
                frames: vec![0, 1, 2, 3],
                fps: 8.0,
                looping: true,
            }],
        };
        document
            .apply(EditorCommand::SetComponent {
                id: 1,
                index: 0,
                component: edited.clone(),
            })
            .expect("edit sprite");
        assert_eq!(document.scene().entities[0].components, vec![edited]);

        let invalid = Component::Sprite {
            asset: "sheet.png".to_owned(),
            meta: SpriteMeta {
                columns: 0,
                ..SpriteMeta::single_cell()
            },
            animations: Vec::new(),
        };
        assert!(
            document
                .apply(EditorCommand::SetComponent {
                    id: 1,
                    index: 0,
                    component: invalid,
                })
                .is_err()
        );
        assert_eq!(
            document.apply(EditorCommand::SetComponent {
                id: 1,
                index: 3,
                component: sprite.clone(),
            }),
            Err(EditorError::ComponentIndex { id: 1, index: 3 })
        );

        assert!(document.undo());
        assert_eq!(document.scene().entities[0].components, vec![sprite]);
    }

    #[test]
    fn save_and_load_reset_history() {
        let path = temp_editor_path();
        let mut document = EditorDocument::new();
        document
            .apply(EditorCommand::CreateEntity {
                id: 1,
                name: Some("Root".to_owned()),
            })
            .expect("create entity");
        document.save_to_path(&path).expect("save document");
        assert!(!document.is_dirty());

        let loaded = EditorDocument::load_from_path(&path).expect("load document");
        assert!(!loaded.can_undo());
        assert!(!loaded.can_redo());
        assert!(!loaded.is_dirty());
        assert!(loaded.selection().is_empty());
        assert_eq!(loaded.scene(), document.scene());

        std::fs::remove_file(&path).expect("remove document");
    }

    #[test]
    fn move_entity_reorders_and_undo_restores() {
        let mut document = EditorDocument::new();
        for id in [1, 2, 3] {
            document
                .apply(EditorCommand::CreateEntity { id, name: None })
                .expect("create entity");
        }
        document
            .apply(EditorCommand::MoveEntity { id: 3, to: 0 })
            .expect("move 3");
        let ids: Vec<u64> = document
            .scene()
            .entities
            .iter()
            .map(|entity| entity.id)
            .collect();
        assert_eq!(ids, vec![3, 1, 2]);

        document
            .apply(EditorCommand::MoveEntity { id: 3, to: 99 })
            .expect("clamped move");
        let ids: Vec<u64> = document
            .scene()
            .entities
            .iter()
            .map(|entity| entity.id)
            .collect();
        assert_eq!(ids, vec![1, 2, 3]);

        assert!(document.undo());
        let ids: Vec<u64> = document
            .scene()
            .entities
            .iter()
            .map(|entity| entity.id)
            .collect();
        assert_eq!(ids, vec![3, 1, 2]);
    }

    #[test]
    fn duplicate_entity_copies_data_and_undo_removes_it() {
        let mut document = EditorDocument::new();
        document
            .apply(EditorCommand::CreateEntity {
                id: 1,
                name: Some("Original".to_owned()),
            })
            .expect("create");
        document
            .apply(EditorCommand::AddComponent {
                id: 1,
                component: Component::Image {
                    asset: "sprites/frog.png".to_owned(),
                },
            })
            .expect("add image");
        document
            .apply(EditorCommand::DuplicateEntity { id: 1, new_id: 7 })
            .expect("duplicate");

        assert_eq!(document.scene().entities.len(), 2);
        assert_eq!(document.selection(), &[7]);
        let copy = &document.scene().entities[1];
        assert_eq!(copy.id, 7);
        assert_eq!(copy.name.as_deref(), Some("Original"));
        assert_eq!(copy.components, document.scene().entities[0].components);

        assert_eq!(
            document.apply(EditorCommand::DuplicateEntity { id: 1, new_id: 7 }),
            Err(SceneError::DuplicateEntity(7).into())
        );
        assert!(document.undo());
        assert_eq!(document.scene().entities.len(), 1);
    }
}
