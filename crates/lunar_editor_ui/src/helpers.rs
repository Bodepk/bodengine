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
        Component::Sprite { asset, meta, .. } => {
            format!("Sprite: {asset} ({}x{})", meta.columns, meta.rows)
        }
        Component::Audio {
            asset,
            volume,
            autoplay,
        } => format!("Audio: {asset} ({volume:.2}, auto={autoplay})"),
        Component::Collider2D { shape, body } => format!("Col2D: {shape:?} ({body:?})"),
        Component::Collider3D { shape, body } => format!("Col3D: {shape:?} ({body:?})"),
        Component::PrefabRef { name } => format!("Prefab: {name}"),
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

/// Píxeles de sprite por unidad de mundo, coherente con ADR-018.
pub(crate) const PIXELS_PER_UNIT: f32 = 32.0;

/// Paso de la rejilla y del snap, en unidades de mundo.
pub(crate) const GRID_STEP: f32 = 0.25;

/// Ángulo 2D en radianes de un cuaternión `(x, y, z, w)`.
#[must_use]
pub(crate) fn angle_from_quaternion(rotation: [f32; 4]) -> f32 {
    2.0 * rotation[2].atan2(rotation[3])
}

/// Cuaternión 2D canónico para un ángulo en radianes.
#[must_use]
pub(crate) fn quaternion_from_angle(angle: f32) -> [f32; 4] {
    let half = angle * 0.5;
    [0.0, 0.0, half.sin(), half.cos()]
}

/// Tamaño en unidades de mundo de una imagen con su escala de entidad.
#[must_use]
pub(crate) fn image_world_size(dimensions: [u32; 2], scale: [f32; 3]) -> [f32; 2] {
    [
        dimensions[0] as f32 / PIXELS_PER_UNIT * scale[0].abs(),
        dimensions[1] as f32 / PIXELS_PER_UNIT * scale[1].abs(),
    ]
}

/// Comprueba si un punto del mundo cae en un rectángulo rotado.
#[must_use]
pub(crate) fn contains_point(
    center: [f32; 2],
    half_size: [f32; 2],
    angle: f32,
    point: [f32; 2],
) -> bool {
    let dx = point[0] - center[0];
    let dy = point[1] - center[1];
    let (sine, cosine) = angle.sin_cos();
    let local_x = dx * cosine + dy * sine;
    let local_y = -dx * sine + dy * cosine;
    local_x.abs() <= half_size[0] && local_y.abs() <= half_size[1]
}

/// Redondea al paso de rejilla más cercano.
#[must_use]
pub(crate) fn snap_to_grid(value: f32) -> f32 {
    (value / GRID_STEP).round() * GRID_STEP
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
            prefabs: Vec::new(),
            entities: vec![entity],
        };
        scene.normalize_asset_paths().expect("normalize paths");
        assert!(scene.validate().is_ok());
        assert_eq!(
            component_label(&scene.entities[0].components[0]),
            "Imagen: assets/sprite.png"
        );
    }

    #[test]
    fn angle_round_trip_uses_z_axis() {
        for angle in [0.0, 0.7, -1.3, std::f32::consts::FRAC_PI_2] {
            let rotation = quaternion_from_angle(angle);
            assert!((angle_from_quaternion(rotation) - angle).abs() < 1e-5);
        }
    }

    #[test]
    fn image_size_uses_pixels_per_unit_and_scale() {
        assert_eq!(image_world_size([32, 64], [2.0, 0.5, 1.0]), [2.0, 1.0]);
        assert_eq!(image_world_size([32, 32], [-1.0, -1.0, 1.0]), [1.0, 1.0]);
    }

    #[test]
    fn hit_test_respects_rotation() {
        assert!(contains_point([0.0, 0.0], [1.0, 0.25], 0.0, [0.9, 0.1]));
        assert!(!contains_point([0.0, 0.0], [1.0, 0.25], 0.0, [0.1, 0.9]));
        assert!(contains_point(
            [0.0, 0.0],
            [1.0, 0.25],
            std::f32::consts::FRAC_PI_2,
            [0.1, 0.9]
        ));
    }

    #[test]
    fn snap_uses_quarter_unit_step() {
        assert_eq!(snap_to_grid(1.13), 1.25);
        assert_eq!(snap_to_grid(-0.12), 0.0);
    }
}
