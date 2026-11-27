//! Núcleo de editor headless (H10, ADR-015).
//!
//! No abre ventanas ni dispositivos: posee un [`Scene`], una selección efímera
//! y un historial de comandos. Toda mutación se valida con las reglas de
//! ADR-008 antes de entrar en el historial.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::path::Path;

use lunar_scenes::{Component, Prefab, Scene, SceneEntity, SceneError, Transform};

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
    /// Borra varias entidades como una sola operación (E12).
    DeleteEntities { ids: Vec<u64> },
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
    /// Sustituye varios componentes por índice en un solo paso (E16).
    SetComponents {
        changes: Vec<(u64, usize, Component)>,
    },
    /// Elimina un componente por índice.
    RemoveComponent { id: u64, index: usize },
    /// Reordena una entidad dentro del vector de la escena (ADR-020).
    MoveEntity { id: u64, to: usize },
    /// Clona una entidad con un identificador nuevo (ADR-020).
    DuplicateEntity { id: u64, new_id: u64 },
    /// Clona varias entidades con ids nuevos y jerarquía remapeada (E13).
    DuplicateEntities {
        /// Pares `(id original, id nuevo)` en el orden en que se insertan.
        copies: Vec<(u64, u64)>,
        /// Sufijo determinista que se añade al nombre de cada copia.
        suffix: String,
    },
    /// Inserta entidades del portapapeles con ids ya remapeados (E13).
    PasteEntities { entities: Vec<SceneEntity> },
    /// Guarda la selección como plantilla reutilizable (E15, ADR-035).
    SavePrefab {
        name: String,
        entities: Vec<SceneEntity>,
    },
    /// Borra una plantilla y limpia sus referencias (E15, ADR-035).
    DeletePrefab { name: String },
    /// Aplica varias transformaciones como una sola operación (E12).
    ///
    /// Es la forma en que un arrastre con multi-selección produce un único
    /// paso de undo. Un id desconocido invalida todo el comando.
    SetTransforms { transforms: Vec<(u64, Transform)> },
    /// Propaga transformaciones y componentes de una plantilla a sus
    /// instancias en un solo paso (E43, ADR-043).
    ApplyPrefab {
        name: String,
        transforms: Vec<(u64, Transform)>,
        components: Vec<(u64, usize, Component)>,
    },
    /// Sustituye las entidades de una plantilla existente (E43, ADR-043).
    UpdatePrefab {
        name: String,
        entities: Vec<SceneEntity>,
    },
    /// Marca un cambio en los sidecars de material del editor (E47). No toca
    /// la escena: existe solo para que el historial y el undo cubran también
    /// los ficheros que viven fuera del JSON v2.
    MaterialEdit,
}

impl EditorCommand {
    /// Etiqueta estable del comando para el historial de la GUI (ADR-045).
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::CreateEntity { .. } => "Crear entidad",
            Self::DeleteEntity { .. } => "Borrar entidad",
            Self::DeleteEntities { .. } => "Borrar entidades",
            Self::RenameEntity { .. } => "Renombrar entidad",
            Self::SetParent { .. } => "Cambiar padre",
            Self::SetTransform { .. } => "Cambiar transformación",
            Self::AddComponent { .. } => "Añadir componente",
            Self::SetComponent { .. } => "Editar componente",
            Self::SetComponents { .. } => "Editar componentes",
            Self::RemoveComponent { .. } => "Quitar componente",
            Self::MoveEntity { .. } => "Reordenar entidad",
            Self::DuplicateEntity { .. } => "Duplicar entidad",
            Self::DuplicateEntities { .. } => "Duplicar selección",
            Self::PasteEntities { .. } => "Pegar entidades",
            Self::SavePrefab { .. } => "Guardar plantilla",
            Self::DeletePrefab { .. } => "Borrar plantilla",
            Self::SetTransforms { .. } => "Mover entidades",
            Self::ApplyPrefab { .. } => "Propagar plantilla",
            Self::UpdatePrefab { .. } => "Actualizar plantilla",
            Self::MaterialEdit => "Editar material",
        }
    }
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
    /// Etiquetas paralelas a `undo` / `redo` (ADR-045).
    undo_labels: Vec<String>,
    redo_labels: Vec<String>,
}

impl Default for EditorDocument {
    fn default() -> Self {
        Self {
            scene: Scene::new(),
            selection: Vec::new(),
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            undo_labels: Vec::new(),
            redo_labels: Vec::new(),
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

    /// Número de operaciones pendientes de rehacer.
    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Etiquetas de las operaciones pendientes de deshacer, de antigua a reciente.
    #[must_use]
    pub fn undo_labels(&self) -> &[String] {
        &self.undo_labels
    }

    /// Etiquetas de las operaciones deshechas, de antigua a la próxima a rehacer.
    #[must_use]
    pub fn redo_labels(&self) -> &[String] {
        &self.redo_labels
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
        self.undo_labels.push(command.label().to_owned());
        self.redo.clear();
        self.redo_labels.clear();
        Ok(())
    }

    /// Deshace la última operación. Devuelve `false` si no hay historial.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        let label = self.undo_labels.pop().unwrap_or_default();
        self.redo.push(Snapshot::from_document(self));
        self.redo_labels.push(label);
        previous.restore_into(self);
        self.dirty = true;
        true
    }

    /// Rehace la última operación deshecha. Devuelve `false` si no hay nada.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let label = self.redo_labels.pop().unwrap_or_default();
        self.undo.push(Snapshot::from_document(self));
        self.undo_labels.push(label);
        next.restore_into(self);
        self.dirty = true;
        true
    }

    /// Deshace hasta dejar `remaining` operaciones en la pila de deshacer.
    pub fn undo_to(&mut self, remaining: usize) -> bool {
        let target = remaining.min(self.undo.len());
        while self.undo.len() > target {
            if !self.undo() {
                return false;
            }
        }
        true
    }

    /// Rehace hasta dejar `remaining` operaciones en la pila de rehacer.
    pub fn redo_to(&mut self, remaining: usize) -> bool {
        let target = remaining.min(self.redo.len());
        while self.redo.len() > target {
            if !self.redo() {
                return false;
            }
        }
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
        EditorCommand::DeleteEntities { ids } => {
            // Validar el lote completo antes de tocar la escena: así un id
            // desconocido no deja una multiselección borrada a medias.
            for id in ids {
                position_of(scene, *id)?;
            }
            let parents: HashMap<u64, Option<u64>> = scene
                .entities
                .iter()
                .map(|entity| (entity.id, entity.parent))
                .collect();
            for id in ids {
                let position = position_of(scene, *id)?;
                let removed = scene.entities.remove(position);
                for entity in &mut scene.entities {
                    if entity.parent == Some(*id) {
                        entity.parent = surviving_parent(&parents, ids, removed.parent);
                    }
                }
            }
            selection.retain(|selected| !ids.contains(selected));
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
        EditorCommand::SetComponents { changes } => {
            // Validar el lote completo antes de escribir: un índice inválido
            // no debe dejar algunos componentes reescritos (E16).
            for (id, index, _) in changes {
                let entity = entity_mut(scene, *id)?;
                if *index >= entity.components.len() {
                    return Err(EditorError::ComponentIndex {
                        id: *id,
                        index: *index,
                    });
                }
            }
            for (id, index, component) in changes {
                entity_mut(scene, *id)?.components[*index] = component.clone();
            }
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
        EditorCommand::DuplicateEntities { copies, suffix } => {
            // Validar el lote completo antes de insertar nada: un original
            // desconocido o un id nuevo repetido no deja copias a medias.
            let mut originals = Vec::with_capacity(copies.len());
            let mut fresh = Vec::with_capacity(copies.len());
            for (original, new_id) in copies {
                position_of(scene, *original)?;
                if scene.entities.iter().any(|entity| entity.id == *new_id)
                    || fresh.contains(new_id)
                {
                    return Err(SceneError::DuplicateEntity(*new_id).into());
                }
                if originals.contains(original) {
                    return Err(SceneError::DuplicateEntity(*original).into());
                }
                originals.push(*original);
                fresh.push(*new_id);
            }
            let remap: HashMap<u64, u64> = copies.iter().copied().collect();
            for (original, new_id) in copies {
                let mut entity = entity_mut(scene, *original)?.clone();
                entity.id = *new_id;
                if let Some(name) = entity.name.as_mut() {
                    name.push_str(suffix);
                }
                // Un hijo cuyo padre también se copia pasa a colgar de la
                // copia del padre; el resto conserva su padre original.
                entity.parent = entity
                    .parent
                    .map(|parent| remap.get(&parent).copied().unwrap_or(parent));
                scene.entities.push(entity);
            }
            *selection = fresh;
        }
        EditorCommand::PasteEntities { entities } => {
            // Los ids ya vienen remapeados por la capa de UI. Se comprueba el
            // lote entero antes de insertar para que un choque de ids no deje
            // una mitad del portapapeles dentro de la escena.
            let mut ids = Vec::with_capacity(entities.len());
            for entity in entities {
                if scene
                    .entities
                    .iter()
                    .any(|existing| existing.id == entity.id)
                    || ids.contains(&entity.id)
                {
                    return Err(SceneError::DuplicateEntity(entity.id).into());
                }
                ids.push(entity.id);
            }
            scene.entities.extend(entities.iter().cloned());
            *selection = ids;
        }
        EditorCommand::SavePrefab { name, entities } => {
            if scene.prefabs.iter().any(|prefab| prefab.name == *name) {
                return Err(SceneError::DuplicatePrefab(name.clone()).into());
            }
            let mut ids = HashSet::with_capacity(entities.len());
            for entity in entities {
                if !ids.insert(entity.id) {
                    return Err(SceneError::DuplicateEntity(entity.id).into());
                }
            }
            scene.prefabs.push(Prefab {
                name: name.clone(),
                entities: entities.clone(),
            });
        }
        EditorCommand::DeletePrefab { name } => {
            let position = scene
                .prefabs
                .iter()
                .position(|prefab| prefab.name == *name)
                .ok_or_else(|| SceneError::UnknownPrefab(name.clone()))?;
            scene.prefabs.remove(position);
            strip_prefab_refs(&mut scene.entities, name);
            for prefab in &mut scene.prefabs {
                strip_prefab_refs(&mut prefab.entities, name);
            }
        }
        EditorCommand::SetTransforms { transforms } => {
            // `scene` ya es una copia: validamos todos los ids antes de escribir
            // para que un lote inválido no deje una escena a medias.
            for (id, _) in transforms {
                position_of(scene, *id)?;
            }
            for (id, transform) in transforms {
                entity_mut(scene, *id)?.transform = *transform;
            }
        }
        EditorCommand::ApplyPrefab {
            name,
            transforms,
            components,
        } => {
            if !scene.prefabs.iter().any(|prefab| prefab.name == *name) {
                return Err(SceneError::UnknownPrefab(name.clone()).into());
            }
            // Validación completa antes de escribir: ni un id desconocido ni
            // un índice inválido dejan una propagación a medias.
            for (id, _) in transforms {
                position_of(scene, *id)?;
            }
            for (id, index, _) in components {
                let entity = entity_mut(scene, *id)?;
                if *index >= entity.components.len() {
                    return Err(EditorError::ComponentIndex {
                        id: *id,
                        index: *index,
                    });
                }
            }
            for (id, transform) in transforms {
                entity_mut(scene, *id)?.transform = *transform;
            }
            for (id, index, component) in components {
                entity_mut(scene, *id)?.components[*index] = component.clone();
            }
        }
        EditorCommand::UpdatePrefab { name, entities } => {
            let prefab = scene
                .prefabs
                .iter_mut()
                .find(|prefab| prefab.name == *name)
                .ok_or_else(|| SceneError::UnknownPrefab(name.clone()))?;
            let mut ids = HashSet::with_capacity(entities.len());
            for entity in entities {
                if !ids.insert(entity.id) {
                    return Err(SceneError::DuplicateEntity(entity.id).into());
                }
            }
            prefab.entities = entities.clone();
        }
        EditorCommand::MaterialEdit => {}
    }
    Ok(())
}

/// Elimina las referencias a una plantilla borrada para que las instancias
/// existentes sigan pasando la validación de ADR-027.
fn strip_prefab_refs(entities: &mut [SceneEntity], name: &str) {
    for entity in entities {
        entity
            .components
            .retain(|component| !matches!(component, Component::PrefabRef { name: referenced } if referenced == name));
    }
}

/// Primer ancestro que sobrevive al lote de borrado, o `None` si no hay ninguno.
fn surviving_parent(
    parents: &HashMap<u64, Option<u64>>,
    removed: &[u64],
    mut parent: Option<u64>,
) -> Option<u64> {
    while let Some(id) = parent {
        if !removed.contains(&id) {
            return Some(id);
        }
        parent = parents.get(&id).copied().flatten();
    }
    None
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
    use lunar_scenes::{BodyType, SCENE_VERSION, Shape2D, Shape3D};
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
    fn material_edit_marks_history_without_touching_the_scene() {
        let mut document = EditorDocument::new();
        let before = document.scene().clone();

        document
            .apply(EditorCommand::MaterialEdit)
            .expect("material edit");
        assert_eq!(EditorCommand::MaterialEdit.label(), "Editar material");
        assert_eq!(document.scene(), &before);
        assert_eq!(document.undo_len(), 1);
        assert!(document.is_dirty());

        assert!(document.undo());
        assert_eq!(document.undo_len(), 0);
        assert_eq!(document.redo_len(), 1);
        assert!(document.redo());
        assert_eq!(document.scene(), &before);
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

    #[test]
    fn set_transforms_is_one_atomic_undo_step() {
        let mut document = EditorDocument::new();
        for id in 1..=3 {
            document
                .apply(EditorCommand::CreateEntity {
                    id,
                    name: Some(format!("E{id}")),
                })
                .expect("create");
        }
        let history = document.undo_len();
        let transforms = vec![
            (
                1,
                Transform {
                    translation: [1.0, 0.0, 0.0],
                    ..Transform::default()
                },
            ),
            (
                3,
                Transform {
                    translation: [3.0, 0.0, 0.0],
                    ..Transform::default()
                },
            ),
        ];
        document
            .apply(EditorCommand::SetTransforms {
                transforms: transforms.clone(),
            })
            .expect("batch");

        assert_eq!(document.undo_len(), history + 1);
        assert_eq!(document.scene().entities[0].transform.translation[0], 1.0);
        assert_eq!(document.scene().entities[1].transform, Transform::default());
        assert_eq!(document.scene().entities[2].transform.translation[0], 3.0);

        assert!(document.undo());
        assert_eq!(document.scene().entities[0].transform, Transform::default());
        assert_eq!(document.scene().entities[2].transform, Transform::default());
        assert!(document.redo());
        assert_eq!(document.scene().entities[0].transform.translation[0], 1.0);
    }

    #[test]
    fn an_invalid_transform_batch_leaves_the_scene_untouched() {
        let mut document = EditorDocument::new();
        document
            .apply(EditorCommand::CreateEntity { id: 1, name: None })
            .expect("create");
        let before = document.scene().clone();
        let history = document.undo_len();

        let result = document.apply(EditorCommand::SetTransforms {
            transforms: vec![
                (
                    1,
                    Transform {
                        translation: [9.0, 0.0, 0.0],
                        ..Transform::default()
                    },
                ),
                (
                    404,
                    Transform {
                        translation: [7.0, 0.0, 0.0],
                        ..Transform::default()
                    },
                ),
            ],
        });

        assert!(result.is_err());
        assert_eq!(document.scene(), &before);
        assert_eq!(document.undo_len(), history);
        assert!(!document.redo());
    }

    fn chain_document() -> EditorDocument {
        let mut document = EditorDocument::new();
        for id in 1..=4 {
            document
                .apply(EditorCommand::CreateEntity { id, name: None })
                .expect("create");
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
    }

    #[test]
    fn deleting_a_multi_selection_is_one_undo_step() {
        let mut document = chain_document();
        document.select(&[2, 3]);
        let history = document.undo_len();

        document
            .apply(EditorCommand::DeleteEntities { ids: vec![2, 3] })
            .expect("delete batch");

        assert_eq!(document.undo_len(), history + 1);
        let ids: Vec<u64> = document
            .scene()
            .entities
            .iter()
            .map(|entity| entity.id)
            .collect();
        assert_eq!(ids, vec![1, 4]);
        assert!(document.selection().is_empty());

        assert!(document.undo());
        let ids: Vec<u64> = document
            .scene()
            .entities
            .iter()
            .map(|entity| entity.id)
            .collect();
        assert_eq!(ids, vec![1, 2, 3, 4]);
        assert_eq!(document.scene().entities[1].parent, Some(1));
        assert_eq!(document.scene().entities[2].parent, Some(2));
    }

    #[test]
    fn deleting_a_parent_reparents_surviving_children_to_its_parent() {
        let mut document = chain_document();
        document
            .apply(EditorCommand::DeleteEntities { ids: vec![2] })
            .expect("delete parent");

        let remaining: Vec<&SceneEntity> = document.scene().entities.iter().collect();
        assert_eq!(remaining.len(), 3);
        assert_eq!(remaining[1].id, 3);
        assert_eq!(remaining[1].parent, Some(1));
    }

    #[test]
    fn an_invalid_delete_batch_leaves_the_scene_untouched() {
        let mut document = chain_document();
        let before = document.scene().clone();
        let history = document.undo_len();

        let result = document.apply(EditorCommand::DeleteEntities { ids: vec![1, 404] });

        assert!(result.is_err());
        assert_eq!(document.scene(), &before);
        assert_eq!(document.undo_len(), history);
        assert!(!document.redo());
    }

    fn chain_with_names() -> EditorDocument {
        let mut document = EditorDocument::new();
        for id in 1..=3 {
            document
                .apply(EditorCommand::CreateEntity {
                    id,
                    name: Some(format!("E{id}")),
                })
                .expect("create");
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
            .apply(EditorCommand::AddComponent {
                id: 2,
                component: Component::Image {
                    asset: "sprites/hero.png".to_owned(),
                },
            })
            .expect("add image");
        document
    }

    #[test]
    fn duplicating_a_batch_preserves_hierarchy_and_is_one_undo_step() {
        let mut document = chain_with_names();
        let history = document.undo_len();

        document
            .apply(EditorCommand::DuplicateEntities {
                copies: vec![(1, 10), (2, 11)],
                suffix: " copia".to_owned(),
            })
            .expect("duplicate batch");

        assert_eq!(document.undo_len(), history + 1);
        assert_eq!(document.selection(), &[10, 11]);
        let entity = |id: u64| {
            document
                .scene()
                .entities
                .iter()
                .find(|candidate| candidate.id == id)
                .cloned()
                .expect("entity")
        };
        assert_eq!(entity(10).name.as_deref(), Some("E1 copia"));
        assert_eq!(entity(10).parent, None);
        assert_eq!(entity(11).name.as_deref(), Some("E2 copia"));
        assert_eq!(entity(11).parent, Some(10));
        assert_eq!(entity(11).components, entity(2).components);
        assert_eq!(entity(3).parent, Some(2));

        assert!(document.undo());
        assert_eq!(document.scene().entities.len(), 3);
        assert_eq!(document.selection(), &[3]);
        assert!(document.redo());
        assert_eq!(document.scene().entities.len(), 5);
    }

    #[test]
    fn an_invalid_duplicate_batch_leaves_the_scene_untouched() {
        let mut document = chain_with_names();
        let before = document.scene().clone();
        let history = document.undo_len();

        assert!(
            document
                .apply(EditorCommand::DuplicateEntities {
                    copies: vec![(1, 10), (404, 11)],
                    suffix: String::new(),
                })
                .is_err()
        );
        assert_eq!(document.scene(), &before);
        assert_eq!(document.undo_len(), history);
        assert!(!document.redo());

        assert_eq!(
            document.apply(EditorCommand::DuplicateEntities {
                copies: vec![(1, 2)],
                suffix: String::new(),
            }),
            Err(SceneError::DuplicateEntity(2).into())
        );
        assert_eq!(document.scene(), &before);
    }

    #[test]
    fn pasting_a_batch_uses_fresh_ids_and_is_one_undo_step() {
        let mut document = chain_with_names();
        let history = document.undo_len();
        let mut first = SceneEntity::new(10);
        first.name = Some("E1".to_owned());
        let mut second = SceneEntity::new(11);
        second.parent = Some(10);

        document
            .apply(EditorCommand::PasteEntities {
                entities: vec![first, second],
            })
            .expect("paste batch");

        assert_eq!(document.undo_len(), history + 1);
        assert_eq!(document.selection(), &[10, 11]);
        assert_eq!(document.scene().entities.len(), 5);
        assert_eq!(document.scene().entities[4].parent, Some(10));

        assert!(document.undo());
        assert_eq!(document.scene().entities.len(), 3);
        assert!(document.redo());
        assert_eq!(document.scene().entities.len(), 5);
    }

    #[test]
    fn an_invalid_paste_leaves_the_scene_untouched() {
        let mut document = chain_with_names();
        let before = document.scene().clone();
        let history = document.undo_len();
        let mut child = SceneEntity::new(11);
        child.parent = Some(999);

        assert!(
            document
                .apply(EditorCommand::PasteEntities {
                    entities: vec![SceneEntity::new(2), child.clone()],
                })
                .is_err()
        );
        assert_eq!(
            document.apply(EditorCommand::PasteEntities {
                entities: vec![SceneEntity::new(10), child],
            }),
            Err(SceneError::UnknownParent {
                child: 11,
                parent: 999,
            }
            .into())
        );
        assert_eq!(document.scene(), &before);
        assert_eq!(document.undo_len(), history);
        assert!(!document.redo());
    }

    #[test]
    fn setting_several_components_is_one_undoable_step() {
        let mut document = EditorDocument::new();
        document
            .apply(EditorCommand::CreateEntity { id: 1, name: None })
            .expect("create");
        document
            .apply(EditorCommand::AddComponent {
                id: 1,
                component: Component::Collider2D {
                    shape: Shape2D::Ball { radius: 0.5 },
                    body: BodyType::Fixed,
                },
            })
            .expect("collider");
        document
            .apply(EditorCommand::AddComponent {
                id: 1,
                component: Component::Image {
                    asset: "viejo.png".to_owned(),
                },
            })
            .expect("image");
        let history = document.undo_len();

        document
            .apply(EditorCommand::SetComponents {
                changes: vec![(
                    1,
                    1,
                    Component::Image {
                        asset: "nuevo.png".to_owned(),
                    },
                )],
            })
            .expect("rewrite path");

        assert_eq!(document.undo_len(), history + 1);
        assert_eq!(
            document.scene().entities[0].components[1].asset_path(),
            Some("nuevo.png")
        );
        assert!(document.undo());
        assert_eq!(
            document.scene().entities[0].components[1].asset_path(),
            Some("viejo.png")
        );
        assert!(document.redo());
        assert_eq!(
            document.scene().entities[0].components[1].asset_path(),
            Some("nuevo.png")
        );
    }

    #[test]
    fn an_invalid_component_batch_leaves_the_scene_untouched() {
        let mut document = EditorDocument::new();
        document
            .apply(EditorCommand::CreateEntity { id: 1, name: None })
            .expect("create");
        document
            .apply(EditorCommand::AddComponent {
                id: 1,
                component: Component::Image {
                    asset: "viejo.png".to_owned(),
                },
            })
            .expect("image");
        let before = document.scene().clone();
        let history = document.undo_len();

        assert_eq!(
            document.apply(EditorCommand::SetComponents {
                changes: vec![
                    (
                        1,
                        0,
                        Component::Image {
                            asset: "nuevo.png".to_owned()
                        },
                    ),
                    (
                        1,
                        9,
                        Component::Image {
                            asset: "roto.png".to_owned()
                        },
                    ),
                ],
            }),
            Err(EditorError::ComponentIndex { id: 1, index: 9 })
        );
        assert_eq!(document.scene(), &before);
        assert_eq!(document.undo_len(), history);
    }

    #[test]
    fn history_labels_track_undo_redo_and_jumps() {
        let mut document = EditorDocument::new();
        assert!(document.undo_labels().is_empty());
        document
            .apply(EditorCommand::CreateEntity { id: 1, name: None })
            .expect("create");
        document
            .apply(EditorCommand::RenameEntity {
                id: 1,
                name: Some("hero".to_owned()),
            })
            .expect("rename");
        assert_eq!(
            document.undo_labels(),
            &["Crear entidad".to_owned(), "Renombrar entidad".to_owned()]
        );

        assert!(document.undo_to(0));
        assert_eq!(document.redo_len(), 2);
        assert!(document.undo_labels().is_empty());
        assert_eq!(
            document.redo_labels(),
            &["Renombrar entidad".to_owned(), "Crear entidad".to_owned()]
        );

        assert!(document.redo_to(1));
        assert_eq!(document.undo_labels(), &["Crear entidad".to_owned()]);
        assert_eq!(document.redo_labels(), &["Renombrar entidad".to_owned()]);

        document
            .apply(EditorCommand::DeleteEntity { id: 1 })
            .expect("delete");
        assert_eq!(
            document.undo_labels(),
            &["Crear entidad".to_owned(), "Borrar entidad".to_owned()]
        );
        assert!(document.redo_labels().is_empty());
    }

    #[test]
    fn saving_a_prefab_is_one_undoable_step() {
        let mut document = chain_with_names();
        let history = document.undo_len();
        let root = document.scene().entities[0].clone();
        let child = document.scene().entities[1].clone();

        document
            .apply(EditorCommand::SavePrefab {
                name: "enemigo".to_owned(),
                entities: vec![root, child],
            })
            .expect("save prefab");

        assert_eq!(document.undo_len(), history + 1);
        assert_eq!(document.scene().prefabs.len(), 1);
        assert_eq!(document.scene().prefabs[0].name, "enemigo");
        assert_eq!(document.scene().prefabs[0].entities.len(), 2);
        assert_eq!(document.scene().prefabs[0].entities[1].parent, Some(1));

        assert!(document.undo());
        assert!(document.scene().prefabs.is_empty());
        assert!(document.redo());
        assert_eq!(document.scene().prefabs.len(), 1);
    }

    #[test]
    fn applying_prefab_changes_is_one_undoable_atomic_step() {
        let mut document = chain_with_names();
        let root = document.scene().entities[0].clone();
        document
            .apply(EditorCommand::SavePrefab {
                name: "enemigo".to_owned(),
                entities: vec![root.clone()],
            })
            .expect("save prefab");
        let before = document.scene().clone();
        let history = document.undo_len();
        let transform = Transform {
            translation: [5.0, 0.0, 0.0],
            ..root.transform
        };
        assert_eq!(
            document.apply(EditorCommand::ApplyPrefab {
                name: "enemigo".to_owned(),
                transforms: vec![(root.id, transform)],
                components: vec![(
                    root.id,
                    9,
                    Component::PrefabRef {
                        name: "enemigo".to_owned()
                    }
                )],
            }),
            Err(EditorError::ComponentIndex {
                id: root.id,
                index: 9
            })
        );
        assert_eq!(document.scene(), &before);
        assert_eq!(document.undo_len(), history);

        document
            .apply(EditorCommand::ApplyPrefab {
                name: "enemigo".to_owned(),
                transforms: vec![(root.id, transform)],
                components: Vec::new(),
            })
            .expect("apply prefab");
        assert_eq!(document.scene().entities[0].transform, transform);
        assert_eq!(document.undo_len(), history + 1);
        assert!(document.undo());
        assert_eq!(document.scene(), &before);
    }

    #[test]
    fn updating_a_prefab_replaces_its_entities_and_is_undoable() {
        let mut document = chain_with_names();
        let original = document.scene().entities[0].clone();
        document
            .apply(EditorCommand::SavePrefab {
                name: "enemigo".to_owned(),
                entities: vec![original.clone()],
            })
            .expect("save prefab");
        let before = document.scene().clone();
        let history = document.undo_len();
        let replacement = SceneEntity::new(77);

        assert_eq!(
            document.apply(EditorCommand::UpdatePrefab {
                name: "desconocida".to_owned(),
                entities: vec![replacement.clone()],
            }),
            Err(SceneError::UnknownPrefab("desconocida".to_owned()).into())
        );
        assert_eq!(document.scene(), &before);
        assert_eq!(document.undo_len(), history);

        document
            .apply(EditorCommand::UpdatePrefab {
                name: "enemigo".to_owned(),
                entities: vec![replacement.clone()],
            })
            .expect("update prefab");
        assert_eq!(document.scene().prefabs[0].entities, vec![replacement]);
        assert_eq!(document.undo_len(), history + 1);
        assert!(document.undo());
        assert_eq!(document.scene(), &before);
    }

    #[test]
    fn an_invalid_prefab_leaves_the_scene_untouched() {
        let mut document = chain_with_names();
        let before = document.scene().clone();
        let history = document.undo_len();
        let entities = vec![document.scene().entities[0].clone()];

        document
            .apply(EditorCommand::SavePrefab {
                name: "enemigo".to_owned(),
                entities: entities.clone(),
            })
            .expect("first prefab");

        let with_prefab = document.scene().clone();
        assert_eq!(
            document.apply(EditorCommand::SavePrefab {
                name: "enemigo".to_owned(),
                entities: entities.clone(),
            }),
            Err(SceneError::DuplicatePrefab("enemigo".to_owned()).into())
        );
        assert_eq!(document.scene(), &with_prefab);

        assert_eq!(
            document.apply(EditorCommand::SavePrefab {
                name: "roto".to_owned(),
                entities: vec![SceneEntity::new(7), SceneEntity::new(7)],
            }),
            Err(SceneError::DuplicateEntity(7).into())
        );
        let mut cyclic = SceneEntity::new(8);
        cyclic.components.push(Component::PrefabRef {
            name: "ciclo".to_owned(),
        });
        assert_eq!(
            document.apply(EditorCommand::SavePrefab {
                name: "ciclo".to_owned(),
                entities: vec![cyclic],
            }),
            Err(SceneError::PrefabCycle("ciclo".to_owned()).into())
        );
        assert_eq!(document.scene(), &with_prefab);
        assert_eq!(document.scene().entities, before.entities);
        assert_eq!(document.undo_len(), history + 1);
    }

    #[test]
    fn deleting_a_prefab_strips_references_and_undo_restores_them() {
        let mut document = EditorDocument::new();
        document
            .apply(EditorCommand::CreateEntity {
                id: 1,
                name: Some("Instancia".to_owned()),
            })
            .expect("create instance");
        document
            .apply(EditorCommand::SavePrefab {
                name: "base".to_owned(),
                entities: vec![SceneEntity::new(2)],
            })
            .expect("save base");
        document
            .apply(EditorCommand::AddComponent {
                id: 1,
                component: Component::PrefabRef {
                    name: "base".to_owned(),
                },
            })
            .expect("add ref");
        let history = document.undo_len();

        document
            .apply(EditorCommand::DeletePrefab {
                name: "base".to_owned(),
            })
            .expect("delete base");

        assert!(document.scene().prefabs.is_empty());
        assert!(document.scene().entities[0].components.is_empty());
        assert_eq!(document.undo_len(), history + 1);

        assert!(document.undo());
        assert_eq!(document.scene().prefabs.len(), 1);
        assert_eq!(
            document.scene().entities[0].components,
            vec![Component::PrefabRef {
                name: "base".to_owned()
            }]
        );
        assert!(document.redo());
        assert!(document.scene().prefabs.is_empty());
    }

    #[test]
    fn deleting_an_unknown_prefab_is_rejected() {
        let mut document = chain_with_names();
        let before = document.scene().clone();
        let history = document.undo_len();

        assert_eq!(
            document.apply(EditorCommand::DeletePrefab {
                name: "inexistente".to_owned(),
            }),
            Err(SceneError::UnknownPrefab("inexistente".to_owned()).into())
        );
        assert_eq!(document.scene(), &before);
        assert_eq!(document.undo_len(), history);
    }
}
