use std::collections::HashMap;

use egui::Ui;
use lunar_scenes::{BodyType, Component, Scene, SceneEntity, Shape2D};

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

/// IDs en orden de dibujo de la jerarquía (padres antes que hijos).
#[must_use]
pub(crate) fn hierarchy_ids(scene: &Scene) -> Vec<u64> {
    fn walk(scene: &Scene, parent: Option<u64>, out: &mut Vec<u64>) {
        for entity in &scene.entities {
            if entity.parent == parent {
                out.push(entity.id);
                walk(scene, Some(entity.id), out);
            }
        }
    }

    let mut ids = Vec::with_capacity(scene.entities.len());
    walk(scene, None, &mut ids);
    for entity in &scene.entities {
        if !ids.contains(&entity.id) {
            ids.push(entity.id);
        }
    }
    ids
}

/// Selección resultante de clic normal (reemplaza) o Ctrl+clic (alterna).
#[must_use]
pub(crate) fn toggle_selection(current: &[u64], id: u64, additive: bool) -> Vec<u64> {
    if !additive {
        return vec![id];
    }
    let mut next = current.to_vec();
    if let Some(index) = next.iter().position(|selected| *selected == id) {
        next.remove(index);
    } else {
        next.push(id);
    }
    next
}

/// Rango contiguo entre dos IDs según el orden de la jerarquía, en ambos sentidos.
#[must_use]
pub(crate) fn range_ids(ordered: &[u64], anchor: u64, target: u64) -> Vec<u64> {
    let (Some(start), Some(end)) = (
        ordered.iter().position(|id| *id == anchor),
        ordered.iter().position(|id| *id == target),
    ) else {
        return Vec::new();
    };
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    ordered[start..=end].to_vec()
}

/// Añade IDs a la selección sin duplicados, conservando el orden previo.
#[must_use]
pub(crate) fn union_selection(current: &[u64], extra: &[u64]) -> Vec<u64> {
    let mut next = current.to_vec();
    for id in extra {
        if !next.contains(id) {
            next.push(*id);
        }
    }
    next
}

/// Copia de las entidades seleccionadas en orden de jerarquía. La jerarquía
/// interna del grupo se conserva; los padres fuera de la selección se dejan
/// como raíces para que el pegado sea seguro también en otra escena (E13).
#[must_use]
pub(crate) fn selection_entities(scene: &Scene, ids: &[u64]) -> Vec<SceneEntity> {
    hierarchy_ids(scene)
        .into_iter()
        .filter(|id| ids.contains(id))
        .filter_map(|id| {
            scene
                .entities
                .iter()
                .find(|entity| entity.id == id)
                .cloned()
        })
        .map(|mut entity| {
            entity.parent = entity.parent.filter(|parent| ids.contains(parent));
            entity
        })
        .collect()
}

/// Pares `(id original, id nuevo)` consecutivos y libres, en orden de jerarquía.
#[must_use]
pub(crate) fn duplicate_pairs(scene: &Scene, ids: &[u64]) -> Vec<(u64, u64)> {
    let mut next = next_entity_id(scene);
    hierarchy_ids(scene)
        .into_iter()
        .filter(|id| ids.contains(id))
        .map(|id| {
            let pair = (id, next);
            next = next.saturating_add(1);
            pair
        })
        .collect()
}

/// Ids libres para el portapapeles y remapeo de los padres que están en él.
#[must_use]
pub(crate) fn remap_for_paste(clipboard: &[SceneEntity], scene: &Scene) -> Vec<SceneEntity> {
    let mut next = next_entity_id(scene);
    let mut remap = HashMap::with_capacity(clipboard.len());
    for entity in clipboard {
        remap.insert(entity.id, next);
        next = next.saturating_add(1);
    }
    clipboard
        .iter()
        .cloned()
        .map(|mut entity| {
            entity.id = remap[&entity.id];
            entity.parent = entity.parent.and_then(|parent| remap.get(&parent).copied());
            entity
        })
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

pub(crate) fn sprite_component(asset: &str) -> Component {
    Component::Sprite {
        asset: lunar_scenes::normalize_asset_path(asset),
        meta: lunar_scenes::SpriteMeta::single_cell(),
        animations: Vec::new(),
    }
}

pub(crate) fn prefab_component(name: &str) -> Component {
    Component::PrefabRef {
        name: name.to_owned(),
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
/// Paso del snap de rotación, en grados (ADR-041).
pub(crate) const SNAP_DEGREES: f32 = 15.0;

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

/// Cuaternión de los ángulos Euler XYZ en grados (ADR-030).
///
/// Componemos `qz * qy * qx`, así que una rotación solo en Z coincide con
/// `quaternion_from_angle` y con el lienzo 2D.
#[must_use]
pub(crate) fn quaternion_from_degrees(degrees: [f32; 3]) -> [f32; 4] {
    let [x, y, z] = degrees.map(|angle| angle.to_radians() * 0.5);
    let (sx, cx) = x.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sz, cz) = z.sin_cos();
    [
        sx * cy * cz - cx * sy * sz,
        cx * sy * cz + sx * cy * sz,
        cx * cy * sz - sx * sy * cz,
        cx * cy * cz + sx * sy * sz,
    ]
}

/// Ángulos Euler XYZ en grados de un cuaternión (ADR-030).
#[must_use]
pub(crate) fn degrees_from_quaternion(rotation: [f32; 4]) -> [f32; 3] {
    let [x, y, z, w] = rotation;
    let roll = (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y));
    let pitch = (2.0 * (w * y - z * x)).clamp(-1.0, 1.0).asin();
    let yaw = (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z));
    [roll.to_degrees(), pitch.to_degrees(), yaw.to_degrees()]
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

/// Redondea grados al paso de 15°, saneando valores no finitos (ADR-041).
#[must_use]
pub(crate) fn snap_degrees(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    (value / SNAP_DEGREES).round() * SNAP_DEGREES
}

/// Redondea un ángulo en radianes al paso de 15°.
#[must_use]
pub(crate) fn snap_angle(radians: f32) -> f32 {
    snap_degrees(radians.to_degrees()).to_radians()
}

/// Redondea una escala al paso de 0.25, sin llegar a cero (ADR-041).
#[must_use]
pub(crate) fn snap_scale(value: f32) -> f32 {
    if !value.is_finite() {
        return GRID_STEP;
    }
    ((value / GRID_STEP).round() * GRID_STEP).max(GRID_STEP)
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
    fn degrees_round_trip_through_quaternion() {
        for degrees in [[0.0, 0.0, 0.0], [10.0, -20.0, 90.0], [-30.0, 45.0, 180.0]] {
            let rotation = quaternion_from_degrees(degrees);
            let recovered = degrees_from_quaternion(rotation);
            for (left, right) in degrees.iter().zip(recovered) {
                let difference = (left - right + 540.0) % 360.0 - 180.0;
                assert!(difference.abs() < 1e-3, "{degrees:?} -> {recovered:?}");
            }
        }
    }

    #[test]
    fn z_degrees_match_the_canvas_angle() {
        let degrees = [0.0, 0.0, 90.0];
        assert_eq!(
            quaternion_from_degrees(degrees),
            quaternion_from_angle(90.0_f32.to_radians())
        );
        let recovered = degrees_from_quaternion(quaternion_from_degrees(degrees));
        assert!((recovered[2] - 90.0).abs() < 1e-3);
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

    #[test]
    fn snap_degrees_uses_fifteen_degree_steps() {
        assert_eq!(snap_degrees(22.0), 15.0);
        assert_eq!(snap_degrees(-37.0), -30.0);
        assert_eq!(snap_degrees(f32::NAN), 0.0);
        assert!(
            (snap_angle(std::f32::consts::FRAC_PI_2) - std::f32::consts::FRAC_PI_2).abs() < 1e-6
        );
    }

    #[test]
    fn snap_scale_keeps_quarter_steps_and_never_zero() {
        assert_eq!(snap_scale(1.13), 1.25);
        assert_eq!(snap_scale(0.1), 0.25);
        assert_eq!(snap_scale(f32::INFINITY), 0.25);
    }

    #[test]
    fn toggle_selection_replaces_or_alternates() {
        assert_eq!(toggle_selection(&[1, 2], 3, false), vec![3]);
        assert_eq!(toggle_selection(&[1, 2], 3, true), vec![1, 2, 3]);
        assert_eq!(toggle_selection(&[1, 2], 1, true), vec![2]);
        assert_eq!(toggle_selection(&[1, 2], 9, true), vec![1, 2, 9]);
    }

    #[test]
    fn range_ids_are_inclusive_and_work_backwards() {
        let ordered = [1, 4, 7, 9];
        assert_eq!(range_ids(&ordered, 4, 9), vec![4, 7, 9]);
        assert_eq!(range_ids(&ordered, 9, 4), vec![4, 7, 9]);
        assert_eq!(range_ids(&ordered, 7, 7), vec![7]);
        assert!(range_ids(&ordered, 1, 99).is_empty());
    }

    #[test]
    fn union_selection_keeps_order_and_deduplicates() {
        assert_eq!(union_selection(&[9, 1], &[1, 4, 9]), vec![9, 1, 4]);
        assert_eq!(union_selection(&[], &[2]), vec![2]);
    }

    #[test]
    fn hierarchy_ids_walk_parents_before_children() {
        let mut scene = Scene::new();
        for id in [4, 3, 2, 1] {
            scene.entities.push(SceneEntity::new(id));
        }
        scene.entities[1].parent = Some(4);
        scene.entities[3].parent = Some(2);
        assert_eq!(hierarchy_ids(&scene), vec![4, 3, 2, 1]);
    }

    fn clipboard_scene() -> Scene {
        let mut scene = Scene::new();
        for id in 1..=3 {
            let mut entity = SceneEntity::new(id);
            entity.name = Some(format!("E{id}"));
            scene.entities.push(entity);
        }
        scene.entities[1].parent = Some(1);
        scene.entities[2].parent = Some(2);
        scene
    }

    #[test]
    fn copying_a_group_keeps_internal_parents_and_clears_external_ones() {
        let scene = clipboard_scene();
        let copied = selection_entities(&scene, &[1, 2]);
        assert_eq!(
            copied.iter().map(|entity| entity.id).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(copied[0].parent, None);
        assert_eq!(copied[1].parent, Some(1));
        assert_eq!(copied[0].name.as_deref(), Some("E1"));
    }

    #[test]
    fn duplicate_pairs_are_fresh_consecutive_and_hierarchy_ordered() {
        let mut scene = Scene::new();
        for id in [3, 2, 1] {
            scene.entities.push(SceneEntity::new(id));
        }
        scene.entities[1].parent = Some(1);
        assert_eq!(duplicate_pairs(&scene, &[1, 2]), vec![(1, 4), (2, 5)]);
    }

    #[test]
    fn pasted_entities_get_fresh_ids_and_remapped_parents() {
        let scene = clipboard_scene();
        let clipboard = selection_entities(&scene, &[1, 2]);
        let pasted = remap_for_paste(&clipboard, &scene);
        assert_eq!(
            pasted.iter().map(|entity| entity.id).collect::<Vec<_>>(),
            vec![4, 5]
        );
        assert_eq!(pasted[0].parent, None);
        assert_eq!(pasted[1].parent, Some(4));
        assert_eq!(pasted[0].name.as_deref(), Some("E1"));
    }
}
