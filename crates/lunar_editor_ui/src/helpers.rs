use egui::Ui;
use lunar_scenes::{BodyType, Component, Scene, Shape2D};

/// Siguiente id libre: el mayor existente más uno, empezando por 1.
#[must_use]
pub fn next_entity_id(scene: &Scene) -> u64 {
    scene
        .entities
        .iter()
        .map(|entity| entity.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}

pub(crate) fn root_ids(scene: &Scene) -> Vec<u64> {
    scene
        .entities
        .iter()
        .filter(|entity| entity.parent.is_none())
        .map(|entity| entity.id)
        .collect()
}

pub(crate) fn child_ids(scene: &Scene, parent: u64) -> Vec<u64> {
    scene
        .entities
        .iter()
        .filter(|entity| entity.parent == Some(parent))
        .map(|entity| entity.id)
        .collect()
}

pub(crate) fn image_component(asset: &str) -> Component {
    Component::Image {
        asset: lunar_scenes::normalize_asset_path(asset),
    }
}

pub(crate) fn audio_component(asset: &str, volume: f32, autoplay: bool) -> Component {
    Component::Audio {
        asset: lunar_scenes::normalize_asset_path(asset),
        volume,
        autoplay,
    }
}

pub(crate) fn collider2_component() -> Component {
    Component::Collider2D {
        shape: Shape2D::Ball { radius: 0.5 },
        body: BodyType::Fixed,
    }
}

pub(crate) fn component_label(component: &Component) -> String {
    match component {
        Component::Mesh { asset, primitive } => format!("Malla: {asset} ({primitive:?})"),
        Component::Image { asset } => format!("Imagen: {asset}"),
        Component::Audio {
            asset,
            volume,
            autoplay,
        } => format!("Audio: {asset} ({volume:.2}, auto={autoplay})"),
        Component::Collider2D { shape, body } => format!("Col2D: {shape:?} ({body:?})"),
        Component::Collider3D { shape, body } => format!("Col3D: {shape:?} ({body:?})"),
    }
}

pub(crate) fn drag_array<const N: usize>(
    ui: &mut Ui,
    label: &str,
    values: &mut [f32; N],
    speed: f32,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        for value in values.iter_mut() {
            changed |= ui.add(egui::DragValue::new(value).speed(speed)).changed();
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use lunar_scenes::{SCENE_VERSION, SceneEntity};

    #[test]
    fn next_id_starts_at_one_and_uses_maximum() {
        assert_eq!(next_entity_id(&Scene::new()), 1);
        let mut scene = Scene::new();
        scene.entities.push(SceneEntity::new(1));
        scene.entities.push(SceneEntity::new(5));
        assert_eq!(next_entity_id(&scene), 6);
    }

    #[test]
    fn hierarchy_separates_roots_from_children() {
        let mut scene = Scene::new();
        for id in [1, 2, 3] {
            scene.entities.push(SceneEntity::new(id));
        }
        scene.entities[1].parent = Some(1);
        assert_eq!(root_ids(&scene), vec![1, 3]);
        assert_eq!(child_ids(&scene, 1), vec![2]);
        assert!(child_ids(&scene, 2).is_empty());
    }

    #[test]
    fn default_components_keep_the_scene_valid() {
        let mut entity = SceneEntity::new(1);
        entity.components.push(image_component("assets/sprite.png"));
        entity
            .components
            .push(audio_component("assets/boom.wav", 0.4, true));
        entity.components.push(collider2_component());
        let mut scene = Scene {
            version: SCENE_VERSION,
            name: None,
            entities: vec![entity],
        };
        scene.normalize_asset_paths().expect("normalize paths");
        assert!(scene.validate().is_ok());
        assert_eq!(
            component_label(&scene.entities[0].components[0]),
            "Imagen: assets/sprite.png"
        );
    }
}
