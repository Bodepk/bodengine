//! Lógica pura de prefabs del editor (E15, ADR-035).
//!
//! El formato vive en `lunar_scenes`; aquí solo se calculan las entidades que
//! se van a insertar. Ninguna función muta la escena: las mutaciones pasan por
//! `EditorDocument::apply` a través de `EditorApp`.

use lunar_scenes::{Component, Scene, SceneEntity};

use crate::helpers::next_entity_id;

/// Número máximo de copias que la GUI permite instanciar de una vez.
pub(crate) const MAX_INSTANCE_COUNT: usize = 10;
/// Separación en X entre instancias consecutivas, en unidades de mundo.
pub(crate) const INSTANCE_SPACING: f32 = 2.0;

/// Acota el contador del menú «+ Crear» al rango permitido.
#[must_use]
pub(crate) fn clamp_instance_count(value: usize) -> usize {
    value.clamp(1, MAX_INSTANCE_COUNT)
}

/// Nombres de las plantillas, en el orden en que se guardaron en la escena.
#[must_use]
pub(crate) fn prefab_names(scene: &Scene) -> Vec<String> {
    scene
        .prefabs
        .iter()
        .map(|prefab| prefab.name.clone())
        .collect()
}

/// Número de entidades que contiene una plantilla.
#[must_use]
pub(crate) fn prefab_entity_count(scene: &Scene, name: &str) -> usize {
    scene
        .prefabs
        .iter()
        .find(|prefab| prefab.name == name)
        .map_or(0, |prefab| prefab.entities.len())
}

/// Cuenta los `PrefabRef` que apuntan a una plantilla, incluidas las de otras
/// plantillas, para poder avisar al borrarla.
#[must_use]
pub(crate) fn count_prefab_refs(scene: &Scene, name: &str) -> usize {
    scene
        .entities
        .iter()
        .chain(
            scene
                .prefabs
                .iter()
                .flat_map(|prefab| prefab.entities.iter()),
        )
        .map(|entity| {
            entity
                .components
                .iter()
                .filter(|component| matches!(component, Component::PrefabRef { name: found } if found == name))
                .count()
        })
        .sum()
}

/// Instancias listas para insertar con `PasteEntities`.
///
/// Reutiliza `Scene::instantiate_prefab` para remapear ids y jerarquía. Cada
/// instancia se desplaza en X para que no se solapen y sus raíces reciben un
/// `PrefabRef` de procedencia si todavía no tenían uno.
#[must_use]
pub(crate) fn instantiate(scene: &Scene, name: &str, count: usize) -> Vec<SceneEntity> {
    let mut next = next_entity_id(scene);
    let mut instances = Vec::new();
    for index in 0..clamp_instance_count(count) {
        let Some(mut entities) = scene.instantiate_prefab(name, next) else {
            return Vec::new();
        };
        next = entities
            .iter()
            .map(|entity| entity.id)
            .max()
            .unwrap_or(next)
            .saturating_add(1);
        for entity in &mut entities {
            if entity.parent.is_none() {
                entity.transform.translation[0] += INSTANCE_SPACING * index as f32;
                if !entity
                    .components
                    .iter()
                    .any(|component| matches!(component, Component::PrefabRef { .. }))
                {
                    entity.components.push(Component::PrefabRef {
                        name: name.to_owned(),
                    });
                }
            }
        }
        instances.extend(entities);
    }
    instances
}

#[cfg(test)]
mod tests {
    use super::*;
    use lunar_scenes::{Prefab, Transform};

    fn scene_with_prefab() -> Scene {
        let mut root = SceneEntity::new(10);
        root.name = Some("Raíz".to_owned());
        let mut child = SceneEntity::new(11);
        child.name = Some("Hijo".to_owned());
        child.parent = Some(10);
        let mut existing = SceneEntity::new(1);
        existing.name = Some("Escena".to_owned());
        let mut scene = Scene::new();
        scene.entities.push(existing);
        scene.prefabs.push(Prefab {
            name: "arbol".to_owned(),
            entities: vec![root, child],
        });
        scene
    }

    #[test]
    fn instance_count_is_clamped() {
        assert_eq!(clamp_instance_count(0), 1);
        assert_eq!(clamp_instance_count(3), 3);
        assert_eq!(clamp_instance_count(99), MAX_INSTANCE_COUNT);
    }

    #[test]
    fn prefab_queries_report_names_sizes_and_references() {
        let mut scene = scene_with_prefab();
        scene.entities[0].components.push(Component::PrefabRef {
            name: "arbol".to_owned(),
        });
        scene.prefabs[0].entities[0]
            .components
            .push(Component::PrefabRef {
                name: "arbol".to_owned(),
            });

        assert_eq!(prefab_names(&scene), vec!["arbol".to_owned()]);
        assert_eq!(prefab_entity_count(&scene, "arbol"), 2);
        assert_eq!(prefab_entity_count(&scene, "otro"), 0);
        assert_eq!(count_prefab_refs(&scene, "arbol"), 2);
        assert_eq!(count_prefab_refs(&scene, "otro"), 0);
    }

    #[test]
    fn instantiating_remaps_ids_and_preserves_the_hierarchy() {
        let scene = scene_with_prefab();
        let entities = instantiate(&scene, "arbol", 1);

        assert_eq!(entities.len(), 2);
        assert_eq!(entities[0].id, 2);
        assert_eq!(entities[1].id, 3);
        assert_eq!(entities[1].parent, Some(2));
        assert_eq!(entities[0].parent, None);
        assert_eq!(
            entities[0].components,
            vec![Component::PrefabRef {
                name: "arbol".to_owned()
            }]
        );
        assert!(entities[1].components.is_empty());
    }

    #[test]
    fn several_instances_get_fresh_ids_and_a_fixed_offset() {
        let scene = scene_with_prefab();
        let entities = instantiate(&scene, "arbol", 2);

        let ids: Vec<u64> = entities.iter().map(|entity| entity.id).collect();
        assert_eq!(ids, vec![2, 3, 4, 5]);
        assert_eq!(entities[0].transform.translation, [0.0, 0.0, 0.0]);
        assert_eq!(
            entities[2].transform.translation,
            [INSTANCE_SPACING, 0.0, 0.0]
        );
        assert_eq!(entities[1].transform, Transform::default());
        assert_eq!(entities[3].transform, Transform::default());
    }

    #[test]
    fn an_unknown_prefab_produces_no_entities() {
        let scene = scene_with_prefab();
        assert!(instantiate(&scene, "inexistente", 2).is_empty());
    }

    #[test]
    fn existing_prefab_refs_are_not_overwritten() {
        let mut scene = scene_with_prefab();
        scene.prefabs[0].entities[0]
            .components
            .push(Component::PrefabRef {
                name: "otro".to_owned(),
            });
        let entities = instantiate(&scene, "arbol", 1);

        assert_eq!(
            entities[0].components,
            vec![Component::PrefabRef {
                name: "otro".to_owned()
            }]
        );
    }
}
