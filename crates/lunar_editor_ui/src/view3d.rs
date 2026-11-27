//! Vista 3D CPU del editor visual (ADR-032).
//!
//! Toda la geometría y la proyección son funciones puras, sin GPU ni types de
//! `wgpu`. La vista se dibuja con el mismo `egui::Painter` del lienzo 2D.

use egui::{Color32, PointerButton, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use lunar_scenes::{Component, SceneEntity, Transform};

use crate::app::EditorApp;
use crate::play::PlayState;
use crate::tools::TransformMode;

const GIZMO_LENGTH: f32 = 2.0;
const PICK_RADIUS: f32 = 8.0;

/// Límites de la distancia de la cámara al objetivo.
pub(crate) const MIN_DISTANCE: f32 = 1.0;
pub(crate) const MAX_DISTANCE: f32 = 200.0;

const NEAR: f32 = 0.1;
const FOV: f32 = std::f32::consts::FRAC_PI_3;
const GRID_EXTENT: f32 = 10.0;

/// Modo del panel central. El editor arranca en 2D (ADR-020).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ViewMode {
    #[default]
    TwoD,
    ThreeD,
}

impl ViewMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::TwoD => "2D",
            Self::ThreeD => "3D",
        }
    }
}

/// Cámara orbital: ángulos en radianes, distancia al objetivo y objetivo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Camera3D {
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) distance: f32,
    pub(crate) target: [f32; 3],
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            yaw: -0.6,
            pitch: 0.4,
            distance: 12.0,
            target: [0.0; 3],
        }
    }
}

/// Eje del gizmo de traslación.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub(crate) fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }

    pub(crate) fn vector(self) -> [f32; 3] {
        match self {
            Self::X => [1.0, 0.0, 0.0],
            Self::Y => [0.0, 1.0, 0.0],
            Self::Z => [0.0, 0.0, 1.0],
        }
    }

    pub(crate) fn color(self) -> Color32 {
        match self {
            Self::X => Color32::from_rgb(230, 90, 90),
            Self::Y => Color32::from_rgb(110, 210, 110),
            Self::Z => Color32::from_rgb(100, 150, 240),
        }
    }
}

/// Estado del arrastre del gizmo 3D. La cámara se mueve sin estado propio.
#[derive(Debug, Clone, Default)]
pub(crate) enum Drag3D {
    #[default]
    None,
    Gizmo {
        id: u64,
        pointer: Pos2,
        axis: Axis,
        original: Transform,
    },
}

/// Sustituye valores no finitos y acota los campos a rangos válidos.
#[must_use]
pub(crate) fn clamp_camera(camera: Camera3D) -> Camera3D {
    let finite = |value: f32, fallback: f32| {
        if value.is_finite() { value } else { fallback }
    };
    let default = Camera3D::default();
    Camera3D {
        yaw: finite(camera.yaw, default.yaw),
        pitch: finite(camera.pitch, default.pitch).clamp(-1.5, 1.5),
        distance: finite(camera.distance, default.distance).clamp(MIN_DISTANCE, MAX_DISTANCE),
        target: camera.target.map(|value| finite(value, 0.0)),
    }
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f32; 3], factor: f32) -> [f32; 3] {
    [a[0] * factor, a[1] * factor, a[2] * factor]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = dot(v, v).sqrt();
    if length <= f32::EPSILON {
        [0.0; 3]
    } else {
        scale(v, 1.0 / length)
    }
}

/// Posición del ojo en el mundo, con Y hacia arriba.
#[must_use]
pub(crate) fn eye(camera: Camera3D) -> [f32; 3] {
    let horizontal = camera.distance * camera.pitch.cos();
    add(
        camera.target,
        [
            horizontal * camera.yaw.sin(),
            camera.distance * camera.pitch.sin(),
            horizontal * camera.yaw.cos(),
        ],
    )
}

/// Base de la cámara: derecha, arriba y avance (hacia el objetivo).
#[must_use]
pub(crate) fn basis(camera: Camera3D) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let forward = normalize(sub(camera.target, eye(camera)));
    let right = normalize(cross(forward, [0.0, 1.0, 0.0]));
    let right = if dot(right, right) <= f32::EPSILON {
        [1.0, 0.0, 0.0]
    } else {
        right
    };
    (right, cross(right, forward), forward)
}

/// Proyecta un punto del mundo a pantalla. Devuelve `None` si queda detrás del plano cercano.
#[must_use]
pub(crate) fn project(point: [f32; 3], camera: Camera3D, rect: Rect) -> Option<Pos2> {
    let (right, up, forward) = basis(camera);
    let delta = sub(point, eye(camera));
    let depth = dot(delta, forward);
    if depth <= NEAR || !depth.is_finite() {
        return None;
    }
    let focal = 0.5 * rect.height() / (0.5 * FOV).tan();
    Some(
        rect.center()
            + Vec2::new(
                dot(delta, right) / depth * focal,
                -dot(delta, up) / depth * focal,
            ),
    )
}

/// Rota un vector por un cuaternión `(x, y, z, w)`.
#[must_use]
pub(crate) fn rotate(vector: [f32; 3], quaternion: [f32; 4]) -> [f32; 3] {
    let axis = [quaternion[0], quaternion[1], quaternion[2]];
    let twice = scale(cross(axis, vector), 2.0);
    add(add(vector, scale(twice, quaternion[3])), cross(axis, twice))
}

const CORNERS: [[f32; 3]; 8] = [
    [-1.0, -1.0, -1.0],
    [1.0, -1.0, -1.0],
    [1.0, 1.0, -1.0],
    [-1.0, 1.0, -1.0],
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, 1.0],
    [1.0, 1.0, 1.0],
    [-1.0, 1.0, 1.0],
];

/// Aristas de la caja alámbrica: anillo inferior, superior y verticales.
pub(crate) const EDGES: [(usize, usize); 12] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

/// Ocho esquinas de la caja unitaria transformada por la entidad.
#[must_use]
pub(crate) fn box_corners(transform: Transform) -> [[f32; 3]; 8] {
    let half = transform.scale.map(|value| 0.5 * value.abs());
    let mut corners = [[0.0; 3]; 8];
    for (index, sign) in CORNERS.iter().enumerate() {
        let local = [sign[0] * half[0], sign[1] * half[1], sign[2] * half[2]];
        corners[index] = add(transform.translation, rotate(local, transform.rotation));
    }
    corners
}

/// Transformación que se debe dibujar: la simulación escribe en `preview`.
#[must_use]
pub(crate) fn displayed_transform(app: &EditorApp, entity: &SceneEntity) -> Transform {
    app.preview
        .get(&entity.id)
        .copied()
        .unwrap_or(entity.transform)
}

/// Distancia de un punto a un segmento en pantalla, para picking y gizmos.
#[must_use]
pub(crate) fn point_segment_distance(point: Pos2, start: Pos2, end: Pos2) -> f32 {
    let segment = end - start;
    let length_sq = segment.length_sq();
    if length_sq <= f32::EPSILON {
        return point.distance(start);
    }
    let t = ((point - start).dot(segment) / length_sq).clamp(0.0, 1.0);
    point.distance(start + segment * t)
}

/// Desplazamiento en pantalla de la punta de un eje respecto a su origen.
#[must_use]
pub(crate) fn axis_screen_delta(
    camera: Camera3D,
    rect: Rect,
    center: [f32; 3],
    axis: Axis,
) -> Option<Vec2> {
    let vector = axis.vector();
    let tip = [
        center[0] + vector[0] * GIZMO_LENGTH,
        center[1] + vector[1] * GIZMO_LENGTH,
        center[2] + vector[2] * GIZMO_LENGTH,
    ];
    Some(project(tip, camera, rect)? - project(center, camera, rect)?)
}

/// Convierte el arrastre en pantalla a unidades de mundo sobre el eje elegido.
#[must_use]
pub(crate) fn translation_from_drag(delta: Vec2, axis_screen: Vec2) -> Option<f32> {
    let length_sq = axis_screen.length_sq();
    if length_sq < 1.0 {
        return None;
    }
    Some(delta.dot(axis_screen) / length_sq * GIZMO_LENGTH)
}

/// Traslación resultante, opcionalmente ajustada a la rejilla de 0.25 unidades.
#[must_use]
pub(crate) fn translate_axis(
    original: Transform,
    axis: Axis,
    amount: f32,
    snap: bool,
) -> Transform {
    let mut transform = original;
    let value = transform.translation[axis.index()] + amount;
    transform.translation[axis.index()] = if snap {
        (value * 4.0).round() / 4.0
    } else {
        value
    };
    transform
}

/// Eje del gizmo bajo el puntero, si hay alguno a `PICK_RADIUS` píxeles.
#[must_use]
pub(crate) fn pick_axis(
    camera: Camera3D,
    rect: Rect,
    pointer: Pos2,
    center: [f32; 3],
) -> Option<Axis> {
    let origin = project(center, camera, rect)?;
    let mut closest = None;
    let mut best = PICK_RADIUS;
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        let delta = axis_screen_delta(camera, rect, center, axis)?;
        let distance = point_segment_distance(pointer, origin, origin + delta);
        if distance < best {
            best = distance;
            closest = Some(axis);
        }
    }
    closest
}

/// ¿Toca el puntero alguna arista o el centro de la caja proyectada?
#[must_use]
pub(crate) fn box_hit(camera: Camera3D, rect: Rect, pointer: Pos2, corners: [[f32; 3]; 8]) -> bool {
    let projected: Vec<Option<Pos2>> = corners
        .iter()
        .map(|corner| project(*corner, camera, rect))
        .collect();
    if projected
        .iter()
        .flatten()
        .any(|point| point.distance(pointer) <= PICK_RADIUS)
    {
        return true;
    }
    EDGES.iter().any(|(start, end)| {
        if let (Some(a), Some(b)) = (projected[*start], projected[*end]) {
            point_segment_distance(pointer, a, b) <= 6.0
        } else {
            false
        }
    })
}

/// Las plantillas de cámara y luz crean marcadores de transformación (ADR-032).
#[must_use]
pub(crate) fn is_marker(entity: &SceneEntity) -> bool {
    entity
        .name
        .as_deref()
        .is_some_and(|name| name.starts_with("Cámara") || name.starts_with("Luz"))
}

/// Comprueba el picking sobre la cruz de ejes de un marcador.
#[must_use]
pub(crate) fn marker_hit(camera: Camera3D, rect: Rect, pointer: Pos2, center: [f32; 3]) -> bool {
    [Axis::X, Axis::Y, Axis::Z].iter().any(|axis| {
        let vector = scale(axis.vector(), 0.6);
        let start = sub(center, vector);
        let end = add(center, vector);
        if let (Some(a), Some(b)) = (project(start, camera, rect), project(end, camera, rect)) {
            point_segment_distance(pointer, a, b) <= PICK_RADIUS
        } else {
            false
        }
    })
}

fn color_for(entity: &SceneEntity, selected: bool) -> Color32 {
    if selected {
        Color32::from_rgb(120, 170, 255)
    } else if entity
        .components
        .iter()
        .any(|component| matches!(component, Component::Mesh { .. }))
    {
        Color32::from_rgb(96, 200, 255)
    } else if entity
        .components
        .iter()
        .any(|component| matches!(component, Component::Collider3D { .. }))
    {
        Color32::from_rgb(120, 220, 140)
    } else {
        Color32::from_gray(190)
    }
}

fn paint_grid(ui: &mut Ui, rect: Rect, camera: Camera3D) {
    let painter = ui.painter();
    let stroke = Stroke::new(0.5, Color32::from_gray(70));
    let extent = GRID_EXTENT as i32;
    for i in -extent..=extent {
        let value = i as f32;
        if let (Some(a), Some(b)) = (
            project([value, 0.0, -GRID_EXTENT], camera, rect),
            project([value, 0.0, GRID_EXTENT], camera, rect),
        ) {
            painter.line_segment([a, b], stroke);
        }
        if let (Some(a), Some(b)) = (
            project([-GRID_EXTENT, 0.0, value], camera, rect),
            project([GRID_EXTENT, 0.0, value], camera, rect),
        ) {
            painter.line_segment([a, b], stroke);
        }
    }
}

fn paint_axes(ui: &mut Ui, rect: Rect, camera: Camera3D) {
    for (axis, color) in [
        ([GRID_EXTENT, 0.0, 0.0], Color32::from_rgb(220, 90, 90)),
        ([0.0, GRID_EXTENT, 0.0], Color32::from_rgb(110, 210, 110)),
        ([0.0, 0.0, GRID_EXTENT], Color32::from_rgb(100, 150, 240)),
    ] {
        if let (Some(origin), Some(tip)) =
            (project([0.0; 3], camera, rect), project(axis, camera, rect))
        {
            ui.painter()
                .line_segment([origin, tip], Stroke::new(1.5, color));
        }
    }
}

fn paint_entities(app: &EditorApp, ui: &mut Ui, rect: Rect, camera: Camera3D) {
    for entity in &app.document.scene().entities {
        let transform = displayed_transform(app, entity);
        let selected = app.document.selection().contains(&entity.id);
        let color = color_for(entity, selected);
        let stroke = Stroke::new(if selected { 2.0 } else { 1.0 }, color);
        if is_marker(entity) {
            for axis in [Axis::X, Axis::Y, Axis::Z] {
                let vector = scale(axis.vector(), 0.6);
                if let (Some(a), Some(b)) = (
                    project(sub(transform.translation, vector), camera, rect),
                    project(add(transform.translation, vector), camera, rect),
                ) {
                    ui.painter().line_segment([a, b], stroke);
                }
            }
        } else {
            let projected: Vec<Option<Pos2>> = box_corners(transform)
                .iter()
                .map(|corner| project(*corner, camera, rect))
                .collect();
            for (start, end) in EDGES {
                if let (Some(a), Some(b)) = (projected[start], projected[end]) {
                    ui.painter().line_segment([a, b], stroke);
                }
            }
        }
        if let Some(center) = project(transform.translation, camera, rect) {
            let name = entity
                .name
                .clone()
                .unwrap_or_else(|| format!("Entidad {}", entity.id));
            ui.painter().text(
                center + Vec2::new(6.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{name} #{}", entity.id),
                egui::FontId::proportional(11.0),
                color,
            );
        }
    }
}

fn selected_entity(app: &EditorApp) -> Option<(u64, Transform)> {
    let selection = app.document.selection();
    if selection.len() != 1 {
        return None;
    }
    let id = selection[0];
    app.document
        .scene()
        .entities
        .iter()
        .find(|entity| entity.id == id)
        .map(|entity| (id, displayed_transform(app, entity)))
}

fn paint_gizmo(app: &EditorApp, ui: &mut Ui, rect: Rect, camera: Camera3D) {
    if app.transform_mode != TransformMode::Move {
        return;
    }
    let Some((_, transform)) = selected_entity(app) else {
        return;
    };
    let Some(origin) = project(transform.translation, camera, rect) else {
        return;
    };
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        if let Some(delta) = axis_screen_delta(camera, rect, transform.translation, axis) {
            let tip = origin + delta;
            ui.painter()
                .line_segment([origin, tip], Stroke::new(2.0, axis.color()));
            ui.painter().circle_filled(tip, 4.0, axis.color());
        }
    }
}

fn handle_selection(
    app: &mut EditorApp,
    ui: &Ui,
    rect: Rect,
    camera: Camera3D,
    response: &egui::Response,
) {
    if app.play_state != PlayState::Stopped {
        return;
    }
    let Some(pointer) = response.hover_pos().filter(|point| rect.contains(*point)) else {
        return;
    };
    if response.drag_started_by(PointerButton::Primary)
        && let Some((id, transform)) = selected_entity(app)
        && app.transform_mode == TransformMode::Move
        && app.ensure_editable()
        && let Some(axis) = pick_axis(camera, rect, pointer, transform.translation)
    {
        app.view3d_drag = Drag3D::Gizmo {
            id,
            pointer,
            axis,
            original: transform,
        };
    }
    if response.clicked() {
        let additive = ui.input(|input| input.modifiers.shift || input.modifiers.ctrl);
        let hit = app
            .document
            .scene()
            .entities
            .iter()
            .find(|entity| {
                let transform = displayed_transform(app, entity);
                if is_marker(entity) {
                    marker_hit(camera, rect, pointer, transform.translation)
                } else {
                    box_hit(camera, rect, pointer, box_corners(transform))
                }
            })
            .map(|entity| entity.id);
        let mut selection = if additive {
            app.document.selection().to_vec()
        } else {
            Vec::new()
        };
        match hit {
            Some(id) if additive && selection.contains(&id) => {
                selection.retain(|selected| *selected != id);
            }
            Some(id) if !selection.contains(&id) => selection.push(id),
            None if !additive => {}
            _ => {}
        }
        app.document.select(&selection);
    }
}

fn apply_gizmo(app: &mut EditorApp, rect: Rect, camera: Camera3D, pointer: Option<Pos2>) {
    let Drag3D::Gizmo {
        id,
        pointer: start,
        axis,
        original,
    } = app.view3d_drag
    else {
        return;
    };
    let Some(pointer) = pointer else {
        return;
    };
    let Some(axis_screen) = axis_screen_delta(camera, rect, original.translation, axis) else {
        return;
    };
    if let Some(amount) = translation_from_drag(pointer - start, axis_screen) {
        app.preview
            .insert(id, translate_axis(original, axis, amount, app.snap));
    }
}

fn finish_drag(app: &mut EditorApp, ui: &Ui) {
    if matches!(app.view3d_drag, Drag3D::None) {
        return;
    }
    if ui.input(|input| input.pointer.any_released()) {
        crate::canvas::commit_preview(app);
        app.view3d_drag = Drag3D::None;
    }
}

fn handle_camera(app: &mut EditorApp, ui: &mut Ui, rect: Rect, response: &egui::Response) {
    if let Some(pointer) = response.hover_pos().filter(|point| rect.contains(*point)) {
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll != 0.0 && rect.contains(pointer) {
            app.camera3d.distance =
                (app.camera3d.distance * (1.0 - scroll * 0.001)).clamp(MIN_DISTANCE, MAX_DISTANCE);
        }
    }
    if response.dragged_by(PointerButton::Secondary) {
        let delta = response.drag_delta();
        app.camera3d.yaw -= delta.x * 0.01;
        app.camera3d.pitch += delta.y * 0.01;
    }
    if response.dragged_by(PointerButton::Middle) {
        let delta = response.drag_delta();
        let (right, up, _) = basis(app.camera3d);
        let step = app.camera3d.distance * 0.002;
        let shift = add(scale(right, -delta.x * step), scale(up, delta.y * step));
        app.camera3d.target = add(app.camera3d.target, shift);
    }
    app.camera3d = clamp_camera(app.camera3d);
}

/// Dibuja el panel central cuando el modo 3D está activo.
pub(crate) fn view3d_ui(app: &mut EditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label("Vista 3D");
        ui.label(format!("Distancia {:.1}", app.camera3d.distance));
        ui.label("Derecho orbitar · central desplazar · rueda zoom");
        ui.label(if app.transform_mode == TransformMode::Move {
            "Arrastra los ejes para trasladar"
        } else {
            "Rotar/escalar desde el inspector"
        });
        if ui.button("Restablecer cámara").clicked() {
            app.camera3d = Camera3D::default();
        }
    });

    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    ui.painter()
        .rect_filled(rect, 0.0, Color32::from_rgb(24, 26, 32));
    paint_grid(ui, rect, app.camera3d);
    paint_axes(ui, rect, app.camera3d);
    paint_entities(app, ui, rect, app.camera3d);
    paint_gizmo(app, ui, rect, app.camera3d);
    let pointer = response.hover_pos().filter(|point| rect.contains(*point));
    handle_camera(app, ui, rect, &response);
    handle_selection(app, ui, rect, app.camera3d, &response);
    apply_gizmo(app, rect, app.camera3d, pointer);
    finish_drag(app, ui);
    ui.painter().text(
        rect.min + Vec2::new(8.0, rect.height() - 8.0),
        egui::Align2::LEFT_BOTTOM,
        format!(
            "Cámara: yaw {:.2} · pitch {:.2} · objetivo {:?}",
            app.camera3d.yaw, app.camera3d.pitch, app.camera3d.target
        ),
        egui::FontId::proportional(11.0),
        Color32::from_gray(210),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use lunar_editor::{EditorCommand, EditorDocument};

    fn rect() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))
    }

    #[test]
    fn the_target_projects_to_the_center() {
        let camera = Camera3D::default();
        let center = project(camera.target, camera, rect()).unwrap();
        let expected = rect().center();
        assert!((center.x - expected.x).abs() < 0.01, "{center:?}");
        assert!((center.y - expected.y).abs() < 0.01, "{center:?}");
    }

    #[test]
    fn points_behind_the_camera_are_not_drawn() {
        let camera = Camera3D::default();
        let origin = eye(camera);
        let behind = add(origin, sub(origin, camera.target));
        assert!(project(behind, camera, rect()).is_none());
    }

    #[test]
    fn camera_is_clamped_and_sanitized() {
        let camera = clamp_camera(Camera3D {
            yaw: f32::NAN,
            pitch: 9.0,
            distance: -4.0,
            target: [f32::INFINITY, 2.0, 3.0],
        });
        assert!(camera.yaw.is_finite());
        assert_eq!(camera.pitch, 1.5);
        assert_eq!(camera.distance, MIN_DISTANCE);
        assert_eq!(camera.target, [0.0, 2.0, 3.0]);
        let far = clamp_camera(Camera3D {
            distance: 9999.0,
            ..Camera3D::default()
        });
        assert_eq!(far.distance, MAX_DISTANCE);
    }

    #[test]
    fn identity_rotation_keeps_the_local_corner() {
        let rotated = rotate([0.5, 0.25, -0.75], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(rotated, [0.5, 0.25, -0.75]);
    }

    #[test]
    fn box_corners_follow_translation_and_scale() {
        let corners = box_corners(Transform {
            translation: [1.0, 2.0, 3.0],
            scale: [2.0, 4.0, 6.0],
            ..Transform::default()
        });
        assert_eq!(corners.len(), 8);
        assert_eq!(corners[0], [0.0, 0.0, 0.0]);
        assert_eq!(corners[6], [2.0, 4.0, 6.0]);
        assert_eq!(EDGES.len(), 12);
    }

    #[test]
    fn the_axis_under_the_pointer_is_picked() {
        let camera = Camera3D::default();
        let center = [0.0; 3];
        let delta = axis_screen_delta(camera, rect(), center, Axis::X).unwrap();
        let tip = project(center, camera, rect()).unwrap() + delta;
        assert_eq!(pick_axis(camera, rect(), tip, center), Some(Axis::X));
        assert_eq!(
            pick_axis(camera, rect(), Pos2::new(-500.0, -500.0), center),
            None
        );
    }

    #[test]
    fn drag_maps_to_world_units_along_the_axis() {
        assert_eq!(
            translation_from_drag(Vec2::new(50.0, 0.0), Vec2::new(100.0, 0.0)),
            Some(1.0)
        );
        assert_eq!(
            translation_from_drag(Vec2::new(50.0, 0.0), Vec2::ZERO),
            None
        );
    }

    #[test]
    fn snapping_rounds_to_quarter_units() {
        let original = Transform::default();
        assert_eq!(
            translate_axis(original, Axis::Y, 0.13, true).translation[1],
            0.25
        );
        assert_eq!(
            translate_axis(original, Axis::Y, 0.13, false).translation[1],
            0.13
        );
    }

    #[test]
    fn picking_hits_only_the_projected_box() {
        let camera = Camera3D::default();
        let corners = box_corners(Transform::default());
        let center = project([0.0; 3], camera, rect()).unwrap();
        assert!(box_hit(camera, rect(), center, corners));
        assert!(!box_hit(camera, rect(), Pos2::new(-500.0, -500.0), corners));
    }

    #[test]
    fn gizmo_preview_commits_one_transform_and_undoes() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.document = EditorDocument::new();
        app.apply(EditorCommand::CreateEntity { id: 1, name: None });
        let original = app.document.scene().entities[0].transform;
        let moved = translate_axis(original, Axis::X, 1.5, false);
        app.preview.insert(1, moved);
        let history = app.document.undo_len();
        crate::canvas::commit_preview(&mut app);
        assert!(app.preview.is_empty());
        assert_eq!(
            app.document.scene().entities[0].transform.translation[0],
            1.5
        );
        assert_eq!(app.document.undo_len(), history + 1);
        app.undo();
        assert_eq!(app.document.scene().entities[0].transform, original);
    }

    #[test]
    fn camera_and_light_templates_are_markers() {
        let marker = |name: &str| SceneEntity {
            id: 1,
            name: Some(name.to_string()),
            parent: None,
            transform: Transform::default(),
            components: Vec::new(),
        };
        assert!(is_marker(&marker("Cámara 2D 7")));
        assert!(is_marker(&marker("Luz 2D 8")));
        assert!(!is_marker(&marker("Malla 9")));
        assert!(!is_marker(&SceneEntity::new(10)));
    }

    #[test]
    fn marker_cross_is_pickable() {
        let camera = Camera3D::default();
        let origin = project([0.0; 3], camera, rect()).unwrap();
        let delta = axis_screen_delta(camera, rect(), [0.0; 3], Axis::X).unwrap();
        assert!(marker_hit(camera, rect(), origin + delta * 0.3, [0.0; 3]));
        assert!(!marker_hit(
            camera,
            rect(),
            Pos2::new(-500.0, -500.0),
            [0.0; 3]
        ));
    }

    #[test]
    fn view_modes_have_stable_labels() {
        assert_eq!(ViewMode::TwoD.label(), "2D");
        assert_eq!(ViewMode::ThreeD.label(), "3D");
        assert_eq!(ViewMode::default(), ViewMode::TwoD);
    }

    #[test]
    fn the_3d_panel_draws_camera_controls() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))),
            ..Default::default()
        };
        let mut first = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Vista 3D de prueba").show(ctx, |ui| view3d_ui(&mut app, ui));
        });
        first.textures_delta.clear();
        let mut second = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Vista 3D de prueba").show(ctx, |ui| view3d_ui(&mut app, ui));
        });
        second.textures_delta.clear();
        let mut output = ctx.run_pass(input, |ctx| {
            egui::Window::new("Vista 3D de prueba").show(ctx, |ui| view3d_ui(&mut app, ui));
        });
        output.textures_delta.clear();
        let texts: Vec<String> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                _ => None,
            })
            .collect();
        assert!(
            texts.iter().any(|text| text.contains("Vista 3D")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|text| text.contains("Restablecer cámara")));
        assert!(texts.iter().any(|text| text.contains("Derecho orbitar")));
    }
}
