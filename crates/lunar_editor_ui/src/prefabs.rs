//! Lógica pura de prefabs del editor (E15, ADR-035).
//!
//! El formato vive en `lunar_scenes`; aquí solo se calculan las entidades que
//! se van a insertar. Ninguna función muta la escena: las mutaciones pasan por
//! `EditorDocument::apply` a través de `EditorApp`.

use lunar_scenes::{Component, Scene, SceneEntity, Transform};

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

/// Cambios que propagan una plantilla a sus instancias (E43, ADR-043).
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct PrefabChanges {
    pub(crate) transforms: Vec<(u64, Transform)>,
    pub(crate) components: Vec<(u64, usize, Component)>,
    /// Grupos omitidos porque su estructura no coincide con la plantilla.
    pub(crate) skipped: usize,
}

/// Empareja cada instancia de `name` con su plantilla y calcula los cambios.
///
/// Las raíces se detectan por `PrefabRef` y sus descendientes se recorren en
/// orden de jerarquía, sin atravesar otra raíz del mismo prefab. Un grupo con
/// distinto número de entidades se omite: aplicar cambios posicionales a una
/// jerarquía distinta sería silenciosamente incorrecto.
#[must_use]
pub(crate) fn propagate(scene: &Scene, name: &str) -> PrefabChanges {
    let Some(prefab) = scene.prefabs.iter().find(|prefab| prefab.name == name) else {
        return PrefabChanges::default();
    };
    let roots: Vec<u64> = scene
        .entities
        .iter()
        .filter(|entity| {
            entity.components.iter().any(|component| {
                matches!(component, Component::PrefabRef { name: found } if found == name)
            })
        })
        .map(|entity| entity.id)
        .collect();
    let mut changes = PrefabChanges::default();
    for root in &roots {
        let mut group = Vec::new();
        collect_instance(scene, *root, *root, &roots, &mut group);
        if group.len() != prefab.entities.len() {
            changes.skipped += 1;
            continue;
        }
        for (instance, template) in group.iter().zip(prefab.entities.iter()) {
            let Some(entity) = scene.entities.iter().find(|entity| entity.id == *instance) else {
                continue;
            };
            let transform = if *instance == *root {
                // La traslación de la raíz es la colocación de la instancia;
                // los cambios de rotación y escala sí se propagan.
                Transform {
                    translation: entity.transform.translation,
                    rotation: template.transform.rotation,
                    scale: template.transform.scale,
                }
            } else {
                template.transform
            };
            changes.transforms.push((*instance, transform));
            for (index, component) in template.components.iter().enumerate() {
                if index >= entity.components.len()
                    || matches!(component, Component::PrefabRef { .. })
                    || matches!(
                        entity.components.get(index),
                        Some(Component::PrefabRef { .. })
                    )
                {
                    continue;
                }
                changes
                    .components
                    .push((*instance, index, component.clone()));
            }
        }
    }
    changes
}

/// Copia de la selección, con sus descendientes, lista para sustituir una
/// plantilla. Se limpian los `PrefabRef` para que la plantilla no herede la
/// procedencia de la instancia que se está usando de ejemplo.
#[must_use]
pub(crate) fn template_from_scene(scene: &Scene, ids: &[u64]) -> Vec<SceneEntity> {
    let mut group = Vec::new();
    for id in ids {
        collect_instance(scene, *id, *id, &[], &mut group);
    }
    let mut entities = crate::helpers::selection_entities(scene, &group);
    for entity in &mut entities {
        entity
            .components
            .retain(|component| !matches!(component, Component::PrefabRef { .. }));
    }
    entities
}

fn collect_instance(scene: &Scene, root: u64, id: u64, roots: &[u64], out: &mut Vec<u64>) {
    if out.contains(&id) {
        return;
    }
    out.push(id);
    for entity in &scene.entities {
        if entity.parent == Some(id) && entity.id != root && !roots.contains(&entity.id) {
            collect_instance(scene, root, entity.id, roots, out);
        }
    }
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

    #[test]
    fn propagation_updates_components_and_descendant_transforms() {
        let mut scene = scene_with_prefab();
        scene.prefabs[0].entities[0].transform.rotation = [0.0, 0.707, 0.0, 0.707];
        scene.prefabs[0].entities[1].transform.translation = [1.0, 2.0, 3.0];
        scene.prefabs[0].entities[1]
            .components
            .push(Component::Audio {
                asset: "sonido.wav".to_owned(),
                volume: 0.5,
                autoplay: false,
            });
        let mut instances = instantiate(&scene, "arbol", 1);
        instances[0].transform.translation = [7.0, 0.0, 0.0];
        instances[1].components.push(Component::Image {
            asset: "viejo.png".to_owned(),
        });
        scene.entities.extend(instances);

        let changes = propagate(&scene, "arbol");

        assert_eq!(changes.skipped, 0);
        let root = changes
            .transforms
            .iter()
            .find(|(id, _)| *id == 2)
            .map(|(_, transform)| *transform)
            .expect("raíz");
        assert_eq!(root.translation, [7.0, 0.0, 0.0]);
        assert_eq!(root.rotation, [0.0, 0.707, 0.0, 0.707]);
        let child = changes
            .transforms
            .iter()
            .find(|(id, _)| *id == 3)
            .map(|(_, transform)| *transform)
            .expect("hijo");
        assert_eq!(child.translation, [1.0, 2.0, 3.0]);
        assert_eq!(
            changes.components,
            vec![(
                3,
                0,
                Component::Audio {
                    asset: "sonido.wav".to_owned(),
                    volume: 0.5,
                    autoplay: false,
                }
            )]
        );
    }

    #[test]
    fn instances_with_a_different_structure_are_skipped() {
        let mut scene = scene_with_prefab();
        let mut instances = instantiate(&scene, "arbol", 1);
        let mut extra = SceneEntity::new(99);
        extra.parent = Some(instances[0].id);
        instances.push(extra);
        scene.entities.extend(instances);

        let changes = propagate(&scene, "arbol");

        assert_eq!(changes.skipped, 1);
        assert!(changes.transforms.is_empty());
        assert!(changes.components.is_empty());
    }

    #[test]
    fn an_unknown_prefab_propagates_nothing() {
        let scene = scene_with_prefab();
        assert_eq!(propagate(&scene, "inexistente"), PrefabChanges::default());
    }

    #[test]
    fn a_template_can_be_rebuilt_from_an_instance() {
        let mut scene = scene_with_prefab();
        let instances = instantiate(&scene, "arbol", 1);
        let root_id = instances[0].id;
        scene.entities.extend(instances);
        let child = scene
            .entities
            .iter_mut()
            .find(|entity| entity.parent == Some(root_id))
            .expect("hijo");
        child.transform.translation = [4.0, 5.0, 6.0];
        child.components.push(Component::Audio {
            asset: "sonido.wav".to_owned(),
            volume: 0.5,
            autoplay: false,
        });

        let template = template_from_scene(&scene, &[root_id]);

        assert_eq!(template.len(), 2);
        assert_eq!(template[0].components, Vec::new());
        assert_eq!(template[1].parent, Some(template[0].id));
        assert_eq!(template[1].transform.translation, [4.0, 5.0, 6.0]);
        assert_eq!(
            template[1].components,
            vec![Component::Audio {
                asset: "sonido.wav".to_owned(),
                volume: 0.5,
                autoplay: false,
            }]
        );
    }
}
