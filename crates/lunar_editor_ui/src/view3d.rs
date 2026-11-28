//! Vista 3D CPU del editor visual (ADR-032).
//!
//! Toda la geometría y la proyección son funciones puras, sin GPU ni types de
//! `wgpu`. La vista se dibuja con el mismo `egui::Painter` del lienzo 2D.

use egui::{Color32, PointerButton, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};
use lunar_scenes::{Component, SceneEntity, Transform};

use crate::app::EditorApp;
use crate::assets::AssetKind;
use crate::dnd::AssetDrag;
use crate::environment::{Environment, SKY_BANDS, sky_color};
use crate::gizmo::{Originals, group_pivot, space_axes, translate_group};
use crate::lights::{AMBIENT, Light, LightKind};
use crate::play::PlayState;
use crate::tools::{TransformMode, TransformSpace};

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

/// Relleno de la vista 3D. El modo inicial conserva la vista alámbrica de ADR-032.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ShadingMode {
    #[default]
    Wireframe,
    Solid,
    Lit,
}

impl ShadingMode {
    pub(crate) const ALL: [Self; 3] = [Self::Wireframe, Self::Solid, Self::Lit];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Wireframe => "Alámbrico",
            Self::Solid => "Sólido",
            Self::Lit => "Iluminado",
        }
    }

    fn fills(self) -> bool {
        self != Self::Wireframe
    }

    fn lit(self) -> bool {
        self == Self::Lit
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
        pointer: Pos2,
        /// Origen del gizmo en el momento de empezar el arrastre.
        pivot: [f32; 3],
        /// Vector unitario del eje en el espacio elegido (E14).
        vector: [f32; 3],
        /// Todas las entidades selectas; un solo `SetTransforms` al soltar.
        originals: Originals,
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

/// Caras de la caja unitaria con su normal local hacia fuera (ADR-037).
pub(crate) const FACES: [([usize; 4], [f32; 3]); 6] = [
    ([1, 2, 6, 5], [1.0, 0.0, 0.0]),
    ([0, 4, 7, 3], [-1.0, 0.0, 0.0]),
    ([3, 7, 6, 2], [0.0, 1.0, 0.0]),
    ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
    ([4, 5, 6, 7], [0.0, 0.0, 1.0]),
    ([0, 3, 2, 1], [0.0, 0.0, -1.0]),
];

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

/// Cara proyectada, lista para pintar con el `Painter` de egui.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FaceDraw {
    pub(crate) points: [Pos2; 4],
    pub(crate) color: Color32,
    pub(crate) depth: f32,
}

/// Factor de luz direccional, con suelo para que las caras ocultas no sean negras.
#[must_use]
pub(crate) fn shade_factor(normal: [f32; 3]) -> f32 {
    let light = normalize([-0.4, 0.8, 0.6]);
    let diffuse = dot(normalize(normal), light).max(0.0);
    (0.35 + 0.65 * diffuse).clamp(0.0, 1.0)
}

/// Aplica un factor RGB por canal sin alterar el alfa.
fn shade_color_rgb(base: Color32, tint: [f32; 3]) -> Color32 {
    let channel =
        |value: u8, factor: f32| (f32::from(value) * factor.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgba_premultiplied(
        channel(base.r(), tint[0]),
        channel(base.g(), tint[1]),
        channel(base.b(), tint[2]),
        base.a(),
    )
}

/// Luz resuelta para sombrear: posición leída de la entidad marcadora (ADR-050).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LightSample {
    pub(crate) kind: LightKind,
    pub(crate) color: [f32; 3],
    pub(crate) intensity: f32,
    pub(crate) direction: [f32; 3],
    pub(crate) position: [f32; 3],
    pub(crate) range: f32,
}

/// Resuelve las luces del editor, incluida la posición de las puntuales.
#[must_use]
pub(crate) fn light_samples(app: &EditorApp) -> Vec<LightSample> {
    app.light_map
        .iter()
        .map(|(id, light)| LightSample {
            kind: light.kind,
            color: light.color,
            intensity: light.intensity,
            direction: light.direction,
            position: app
                .document
                .scene()
                .entities
                .iter()
                .find(|entity| entity.id == id)
                .map(|entity| displayed_transform(app, entity).translation)
                .unwrap_or_default(),
            range: light.range,
        })
        .collect()
}

/// Respuesta RGB de las luces para una normal y un centro. Sin luces se
/// conserva la direccional fija de ADR-037/049 (ADR-050).
#[must_use]
pub(crate) fn light_response(
    samples: &[LightSample],
    center: [f32; 3],
    normal: [f32; 3],
) -> [f32; 3] {
    if samples.is_empty() {
        return [shade_factor(normal); 3];
    }
    let normal = normalize(normal);
    let mut out = [AMBIENT; 3];
    for sample in samples {
        let (direction, attenuation) = match sample.kind {
            LightKind::Directional => (normalize(sample.direction), 1.0),
            LightKind::Point => {
                let delta = sub(sample.position, center);
                let distance = dot(delta, delta).sqrt();
                if sample.range <= 0.0 || distance >= sample.range {
                    continue;
                }
                (
                    normalize(delta),
                    (1.0 - distance / sample.range).clamp(0.0, 1.0),
                )
            }
        };
        let diffuse = dot(normal, direction).max(0.0);
        let amount = diffuse * attenuation * sample.intensity.clamp(0.0, 2.0);
        for channel in 0..3 {
            out[channel] += amount * sample.color[channel].clamp(0.0, 1.0);
        }
    }
    out.map(|value| {
        if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            AMBIENT
        }
    })
}

/// Caras de la caja ordenadas de lejos a cerca, con un factor RGB por normal.
fn faces_with<F>(
    transform: Transform,
    camera: Camera3D,
    rect: Rect,
    base: Color32,
    mode: ShadingMode,
    fog: Option<&Environment>,
    tint: F,
) -> Vec<FaceDraw>
where
    F: Fn([f32; 3]) -> [f32; 3],
{
    if !mode.fills() {
        return Vec::new();
    }
    let corners = box_corners(transform);
    let (_, _, forward) = basis(camera);
    let origin = eye(camera);
    let mut faces = Vec::with_capacity(FACES.len());
    for (indices, local_normal) in FACES {
        let projected = indices.map(|index| project(corners[index], camera, rect));
        let [Some(a), Some(b), Some(c), Some(d)] = projected else {
            continue;
        };
        let depth = indices
            .iter()
            .map(|index| dot(sub(corners[*index], origin), forward))
            .sum::<f32>()
            / 4.0;
        if !depth.is_finite() {
            continue;
        }
        let factor = if mode.lit() {
            tint(rotate(local_normal, transform.rotation))
        } else {
            [1.0; 3]
        };
        let shaded = shade_color_rgb(base, factor);
        let color = fog.map_or(shaded, |env| {
            crate::environment::apply_fog(shaded, env, crate::environment::fog_factor(env, depth))
        });
        faces.push(FaceDraw {
            points: [a, b, c, d],
            color,
            depth,
        });
    }
    faces.sort_by(|left, right| right.depth.total_cmp(&left.depth));
    faces
}

/// Caras con la luz fija de ADR-037: fallback y comparación de tests.
#[must_use]
pub(crate) fn shaded_faces(
    transform: Transform,
    camera: Camera3D,
    rect: Rect,
    base: Color32,
    mode: ShadingMode,
) -> Vec<FaceDraw> {
    faces_with(transform, camera, rect, base, mode, None, |normal| {
        [shade_factor(normal); 3]
    })
}

/// Caras con las luces del editor (ADR-050).
fn lit_faces(
    app: &EditorApp,
    entity: &SceneEntity,
    camera: Camera3D,
    rect: Rect,
    base: Color32,
    samples: &[LightSample],
) -> Vec<FaceDraw> {
    let transform = displayed_transform(app, entity);
    let center = transform.translation;
    faces_with(
        transform,
        camera,
        rect,
        base,
        ShadingMode::Lit,
        Some(&app.environment),
        |normal| light_response(samples, center, normal),
    )
}

/// Color del material asignado a una entidad, si está cargado (ADR-049).
#[must_use]
pub(crate) fn material_fill_color(app: &EditorApp, entity: &SceneEntity) -> Option<Color32> {
    app.material_map
        .get(entity.id)
        .and_then(|path| app.material_library.get(path))
        .map(|material| material.preview_color())
}

/// Color base de las caras: en `Lit` usa el material asignado (ADR-049).
#[must_use]
pub(crate) fn fill_color(app: &EditorApp, entity: &SceneEntity, selected: bool) -> Color32 {
    if app.shading_mode.lit() {
        material_fill_color(app, entity).unwrap_or_else(|| color_for(entity, selected))
    } else {
        color_for(entity, selected)
    }
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

/// Desplazamiento en pantalla de la punta de un vector unitario respecto a su origen.
#[must_use]
pub(crate) fn axis_screen_delta_vector(
    camera: Camera3D,
    rect: Rect,
    center: [f32; 3],
    vector: [f32; 3],
) -> Option<Vec2> {
    let tip = add(center, scale(vector, GIZMO_LENGTH));
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

/// Índice del vector del gizmo bajo el puntero, si hay alguno a `PICK_RADIUS`
/// píxeles. Acepta ejes de mundo o del espacio local (E14).
#[must_use]
pub(crate) fn pick_axis_vector(
    camera: Camera3D,
    rect: Rect,
    pointer: Pos2,
    center: [f32; 3],
    vectors: [[f32; 3]; 3],
) -> Option<usize> {
    let origin = project(center, camera, rect)?;
    let mut closest = None;
    let mut best = PICK_RADIUS;
    for (index, vector) in vectors.into_iter().enumerate() {
        let delta = axis_screen_delta_vector(camera, rect, center, vector)?;
        let distance = point_segment_distance(pointer, origin, origin + delta);
        if distance < best {
            best = distance;
            closest = Some(index);
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

/// Entidad bajo el puntero, compartida por selección y arrastre de materiales.
#[must_use]
pub(crate) fn entity_at(
    app: &EditorApp,
    camera: Camera3D,
    rect: Rect,
    pointer: Pos2,
) -> Option<u64> {
    app.document
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
        .map(|entity| entity.id)
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

/// Fondo del viewport con bandas de cielo deterministas (ADR-051).
fn paint_sky(app: &EditorApp, ui: &mut Ui, rect: Rect) {
    let step = rect.height() / SKY_BANDS as f32;
    for index in 0..SKY_BANDS {
        let t = (index as f32 + 0.5) / SKY_BANDS as f32;
        let top = rect.min.y + index as f32 * step;
        let band = Rect::from_min_max(
            Pos2::new(rect.min.x, top),
            Pos2::new(rect.max.x, top + step + 0.5),
        );
        ui.painter()
            .rect_filled(band, 0.0, sky_color(&app.environment, t));
    }
}

/// Color de la cruz de un marcador: el de su luz cuando tiene una (ADR-050).
fn marker_color(app: &EditorApp, entity: &SceneEntity, selected: bool) -> Color32 {
    app.light_map
        .get(entity.id)
        .map_or_else(|| color_for(entity, selected), Light::preview_color)
}

fn paint_entities(app: &EditorApp, ui: &mut Ui, rect: Rect, camera: Camera3D) {
    if app.shading_mode.fills() {
        let samples = if app.shading_mode.lit() {
            light_samples(app)
        } else {
            Vec::new()
        };
        let mut faces = Vec::new();
        for entity in &app.document.scene().entities {
            if is_marker(entity) {
                continue;
            }
            let transform = displayed_transform(app, entity);
            let selected = app.document.selection().contains(&entity.id);
            let base = fill_color(app, entity, selected);
            if app.shading_mode.lit() {
                faces.extend(lit_faces(app, entity, camera, rect, base, &samples));
            } else {
                faces.extend(shaded_faces(
                    transform,
                    camera,
                    rect,
                    base,
                    app.shading_mode,
                ));
            }
        }
        faces.sort_by(|left, right| right.depth.total_cmp(&left.depth));
        for face in faces {
            ui.painter().add(egui::Shape::convex_polygon(
                face.points.to_vec(),
                face.color,
                Stroke::NONE,
            ));
        }
    }
    for entity in &app.document.scene().entities {
        let transform = displayed_transform(app, entity);
        let selected = app.document.selection().contains(&entity.id);
        let color = if is_marker(entity) {
            marker_color(app, entity, selected)
        } else {
            color_for(entity, selected)
        };
        let stroke = Stroke::new(
            if selected {
                2.0
            } else if app.shading_mode.fills() {
                0.5
            } else {
                1.0
            },
            color,
        );
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

/// Transformaciones visibles de toda la selección, en orden de jerarquía.
#[must_use]
fn selection_transforms(app: &EditorApp) -> Originals {
    let selection = app.document.selection();
    app.document
        .scene()
        .entities
        .iter()
        .filter(|entity| selection.contains(&entity.id))
        .map(|entity| (entity.id, displayed_transform(app, entity)))
        .collect()
}

/// Rotación del ancla, usada para orientar los ejes en espacio local.
fn reference_rotation(app: &EditorApp, originals: &Originals) -> [f32; 4] {
    crate::gizmo::reference(originals, app.selection_anchor)
        .map_or([0.0, 0.0, 0.0, 1.0], |(_, transform)| transform.rotation)
}

fn paint_gizmo(app: &EditorApp, ui: &mut Ui, rect: Rect, camera: Camera3D) {
    if app.transform_mode != TransformMode::Move {
        return;
    }
    let originals = selection_transforms(app);
    let Some(pivot) = group_pivot(&originals, app.transform_space, app.selection_anchor) else {
        return;
    };
    let Some(origin) = project(pivot, camera, rect) else {
        return;
    };
    let rotation = reference_rotation(app, &originals);
    for (index, vector) in space_axes(app.transform_space, rotation)
        .into_iter()
        .enumerate()
    {
        if let Some(delta) = axis_screen_delta_vector(camera, rect, pivot, vector) {
            let color = [Axis::X, Axis::Y, Axis::Z][index].color();
            let tip = origin + delta;
            ui.painter()
                .line_segment([origin, tip], Stroke::new(2.0, color));
            ui.painter().circle_filled(tip, 4.0, color);
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
        && app.transform_mode == TransformMode::Move
        && app.ensure_editable()
    {
        let originals = selection_transforms(app);
        if let Some(pivot) = group_pivot(&originals, app.transform_space, app.selection_anchor) {
            let rotation = reference_rotation(app, &originals);
            let axes = space_axes(app.transform_space, rotation);
            if let Some(index) = pick_axis_vector(camera, rect, pointer, pivot, axes) {
                app.view3d_drag = Drag3D::Gizmo {
                    pointer,
                    pivot,
                    vector: axes[index],
                    originals,
                };
            }
        }
    }
    if response.clicked() {
        let hit = entity_at(app, camera, rect, pointer);
        let (ctrl, shift) = ui.input(|input| (input.modifiers.ctrl, input.modifiers.shift));
        if let Some(id) = hit {
            app.select_clicked(id, ctrl, shift);
        } else if !ctrl && !shift {
            app.clear_selection();
        }
    }
}

fn apply_gizmo(app: &mut EditorApp, rect: Rect, camera: Camera3D, pointer: Option<Pos2>) {
    let Drag3D::Gizmo {
        pointer: start,
        pivot,
        vector,
        ref originals,
    } = app.view3d_drag
    else {
        return;
    };
    let Some(pointer) = pointer else {
        return;
    };
    let Some(axis_screen) = axis_screen_delta_vector(camera, rect, pivot, vector) else {
        return;
    };
    if let Some(amount) = translation_from_drag(pointer - start, axis_screen) {
        let delta = scale(vector, amount);
        for (id, transform) in translate_group(originals, delta, app.snap) {
            app.preview.insert(id, transform);
        }
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
    ui.horizontal_wrapped(|ui| {
        ui.label("Vista 3D");
        egui::ComboBox::from_label("Sombreado")
            .selected_text(app.shading_mode.label())
            .show_ui(ui, |ui| {
                for mode in ShadingMode::ALL {
                    if ui
                        .selectable_label(app.shading_mode == mode, mode.label())
                        .clicked()
                    {
                        app.shading_mode = mode;
                    }
                }
            });
        let assigned = app
            .document
            .selection()
            .first()
            .and_then(|id| app.material_map.get(*id))
            .unwrap_or("Sin material");
        ui.label(format!("Material: {assigned}"));
        ui.label(format!("Luces: {}", app.light_map.len()));
        let environment_label = format!(
            "Entorno: {}{}",
            app.environment.mode.label(),
            if app.environment.fog { " + niebla" } else { "" }
        );
        if ui
            .button(environment_label)
            .on_hover_text("Abrir el editor de cielo y niebla")
            .clicked()
        {
            app.show_environment = true;
        }
        ui.label(format!("Distancia {:.1}", app.camera3d.distance));
        ui.label("Derecho orbitar · central desplazar · rueda zoom");
        ui.separator();
        if ui
            .selectable_label(app.transform_space == TransformSpace::Global, "Global (Q)")
            .clicked()
        {
            app.transform_space = TransformSpace::Global;
            app.save_settings();
        }
        if ui
            .selectable_label(app.transform_space == TransformSpace::Local, "Local (Q)")
            .clicked()
        {
            app.transform_space = TransformSpace::Local;
            app.save_settings();
        }
        ui.label(if app.transform_mode == TransformMode::Move {
            "Arrastra los ejes del grupo para trasladar"
        } else {
            "Rotar/escalar desde el inspector"
        });
        if ui.button("Restablecer cámara").clicked() {
            app.camera3d = Camera3D::default();
        }
    });

    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    paint_sky(app, ui, rect);
    paint_grid(ui, rect, app.camera3d);
    paint_axes(ui, rect, app.camera3d);
    paint_entities(app, ui, rect, app.camera3d);
    paint_gizmo(app, ui, rect, app.camera3d);
    let pointer = response.hover_pos().filter(|point| rect.contains(*point));
    handle_camera(app, ui, rect, &response);
    handle_selection(app, ui, rect, app.camera3d, &response);
    apply_gizmo(app, rect, app.camera3d, pointer);
    finish_drag(app, ui);
    handle_drop(app, ui, rect, &response);
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

/// Suelta de un asset en la vista 3D: mallas crean entidades y materiales se asignan (ADR-049).
fn handle_drop(app: &mut EditorApp, ui: &Ui, rect: Rect, response: &Response) {
    if !ui.input(|input| input.pointer.any_released()) {
        return;
    }
    let Some(pointer) = response.hover_pos().filter(|point| rect.contains(*point)) else {
        return;
    };
    let Some(drag) = crate::dnd::take_asset(ui.ctx()) else {
        return;
    };
    let camera = app.camera3d;
    apply_asset_drop(app, pointer, rect, camera, &drag);
}

/// Aplica un asset soltado sobre el punto de la vista 3D (ADR-049).
fn apply_asset_drop(
    app: &mut EditorApp,
    pointer: Pos2,
    rect: Rect,
    camera: Camera3D,
    drag: &AssetDrag,
) {
    match drag.kind {
        AssetKind::Mesh => app.create_entity_from_asset(&drag.path, [0.0, 0.0]),
        AssetKind::Material => {
            let Some(id) = entity_at(app, camera, rect, pointer) else {
                app.set_status("Suelta el material sobre una entidad para asignarlo");
                return;
            };
            app.assign_material_to_entity(id, Some(&drag.path));
        }
        _ => app.set_status("La vista 3D acepta mallas y materiales"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials::Material;
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
        let axes = [Axis::X, Axis::Y, Axis::Z].map(Axis::vector);
        let delta = axis_screen_delta_vector(camera, rect(), center, axes[0]).unwrap();
        let tip = project(center, camera, rect()).unwrap() + delta;
        assert_eq!(pick_axis_vector(camera, rect(), tip, center, axes), Some(0));
        assert_eq!(
            pick_axis_vector(camera, rect(), Pos2::new(-500.0, -500.0), center, axes),
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
        let moved = translate_group(&[(1, original)], [1.5, 0.0, 0.0], false);
        app.preview.insert(1, moved[0].1);
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
    fn the_local_axes_are_picked_at_their_rotated_tip() {
        let camera = Camera3D::default();
        let center = [0.0; 3];
        let rotation = crate::helpers::quaternion_from_angle(std::f32::consts::FRAC_PI_2);
        let axes = space_axes(TransformSpace::Local, rotation);
        let tip = project(center, camera, rect()).unwrap()
            + axis_screen_delta_vector(camera, rect(), center, axes[0]).unwrap();
        assert_eq!(pick_axis_vector(camera, rect(), tip, center, axes), Some(0));
        assert_eq!(
            pick_axis_vector(camera, rect(), Pos2::new(-500.0, -500.0), center, axes),
            None
        );
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
        let delta = axis_screen_delta_vector(camera, rect(), [0.0; 3], Axis::X.vector()).unwrap();
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
    fn shading_modes_have_stable_labels_and_wireframe_has_no_faces() {
        assert_eq!(ShadingMode::Wireframe.label(), "Alámbrico");
        assert_eq!(ShadingMode::Solid.label(), "Sólido");
        assert_eq!(ShadingMode::Lit.label(), "Iluminado");
        assert_eq!(ShadingMode::default(), ShadingMode::Wireframe);
        assert!(
            shaded_faces(
                Transform::default(),
                Camera3D::default(),
                rect(),
                Color32::WHITE,
                ShadingMode::Wireframe
            )
            .is_empty()
        );
    }

    #[test]
    fn lit_shading_depends_on_the_light_direction() {
        assert!(shade_factor([0.0, 1.0, 0.0]) > shade_factor([0.0, -1.0, 0.0]));
        assert!((shade_factor([0.0, -1.0, 0.0]) - 0.35).abs() < 0.001);
        assert!((0.0..=1.0).contains(&shade_factor([f32::NAN; 3])));
    }

    #[test]
    fn light_response_keeps_the_fixed_light_without_editor_lights() {
        assert_eq!(
            light_response(&[], [0.0; 3], [0.0, 1.0, 0.0]),
            [shade_factor([0.0, 1.0, 0.0]); 3]
        );
    }

    #[test]
    fn directional_lights_tint_each_channel() {
        let sample = LightSample {
            kind: LightKind::Directional,
            color: [1.0, 0.0, 0.0],
            intensity: 1.0,
            direction: [0.0, 1.0, 0.0],
            position: [0.0; 3],
            range: 10.0,
        };
        assert_eq!(
            light_response(&[sample], [0.0; 3], [0.0, 1.0, 0.0]),
            [1.0, AMBIENT, AMBIENT]
        );
        assert_eq!(
            light_response(&[sample], [0.0; 3], [0.0, -1.0, 0.0]),
            [AMBIENT; 3]
        );
    }

    #[test]
    fn point_lights_attenuate_and_stop_at_their_range() {
        let sample = LightSample {
            kind: LightKind::Point,
            color: [1.0; 3],
            intensity: 1.0,
            direction: [0.0; 3],
            position: [0.0, 2.0, 0.0],
            range: 4.0,
        };
        let near = light_response(&[sample], [0.0; 3], [0.0, 1.0, 0.0]);
        let far = light_response(&[sample], [0.0, -3.0, 0.0], [0.0, 1.0, 0.0]);
        assert!((near[0] - 0.85).abs() < 0.001, "{near:?}");
        assert_eq!(far, [AMBIENT; 3]);
    }

    #[test]
    fn several_lights_accumulate_and_clamp() {
        let sample = LightSample {
            kind: LightKind::Directional,
            color: [1.0; 3],
            intensity: 1.0,
            direction: [0.0, 1.0, 0.0],
            position: [0.0; 3],
            range: 10.0,
        };
        assert_eq!(
            light_response(&[sample, sample], [0.0; 3], [0.0, 1.0, 0.0]),
            [1.0; 3]
        );
    }

    #[test]
    fn lit_faces_use_the_editor_lights_and_the_marker_shows_their_color() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        let root = std::env::temp_dir().join(format!("lunar_view3d_light_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        app.path = root.join("scene.json");
        app.document = EditorDocument::new();
        app.apply(EditorCommand::CreateEntity { id: 1, name: None });
        app.commit_light(
            1,
            Some(Light {
                kind: LightKind::Directional,
                color: [1.0, 0.0, 0.0],
                intensity: 1.0,
                direction: [0.0, 1.0, 0.0],
                ..Light::default()
            }),
        );
        let samples = light_samples(&app);
        assert_eq!(samples.len(), 1);
        let entity = app.document.scene().entities[0].clone();
        let faces = lit_faces(
            &app,
            &entity,
            Camera3D::default(),
            rect(),
            Color32::from_gray(200),
            &samples,
        );
        assert!(
            faces.iter().any(|face| face.color.r() > face.color.g()),
            "{faces:?}"
        );
        assert_eq!(
            marker_color(&app, &entity, false),
            Color32::from_rgb(255, 0, 0)
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn shaded_faces_are_ordered_back_to_front() {
        let faces = shaded_faces(
            Transform {
                translation: [0.0, 0.0, 3.0],
                ..Transform::default()
            },
            Camera3D::default(),
            rect(),
            Color32::WHITE,
            ShadingMode::Solid,
        );
        assert_eq!(faces.len(), 6);
        assert!(faces.windows(2).all(|pair| pair[0].depth >= pair[1].depth));
        assert!(faces.iter().all(|face| face.color == Color32::WHITE));
        assert!(faces.iter().all(|face| face.points.len() == 4));
    }

    #[test]
    fn lit_faces_have_different_tones_and_solid_ones_do_not() {
        let base = Color32::from_rgb(200, 160, 110);
        let solid = shaded_faces(
            Transform::default(),
            Camera3D::default(),
            rect(),
            base,
            ShadingMode::Solid,
        );
        let lit = shaded_faces(
            Transform::default(),
            Camera3D::default(),
            rect(),
            base,
            ShadingMode::Lit,
        );
        assert!(solid.iter().all(|face| face.color == base));
        assert!(lit.iter().any(|face| face.color != lit[0].color));
        assert!(lit.iter().all(|face| face.color.a() == base.a()));
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
        assert!(texts.iter().any(|text| text == "Global (Q)"));
        assert!(texts.iter().any(|text| text == "Local (Q)"));
        assert!(texts.iter().any(|text| text.contains("Sombreado")));
        assert!(texts.iter().any(|text| text == "Alámbrico"));
        assert!(texts.iter().any(|text| text == "Material: Sin material"));
        assert!(texts.iter().any(|text| text == "Luces: 0"));
        assert!(texts.iter().any(|text| text == "Entorno: Degradado"));
    }

    #[test]
    fn material_fill_color_is_used_only_in_lit_mode() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.document = EditorDocument::new();
        app.apply(EditorCommand::CreateEntity { id: 1, name: None });
        let red = Color32::from_rgba_unmultiplied(255, 0, 0, 255);
        app.material_library.insert(
            "rojo.material.json".to_owned(),
            Material {
                albedo: [1.0, 0.0, 0.0, 1.0],
                roughness: 0.0,
                ..Material::default()
            },
        );
        app.material_map
            .set(1, Some("rojo.material.json".to_owned()));
        let entity = app.document.scene().entities[0].clone();

        assert_eq!(material_fill_color(&app, &entity), Some(red));
        app.shading_mode = ShadingMode::Lit;
        assert_eq!(fill_color(&app, &entity, true), red);
        app.shading_mode = ShadingMode::Solid;
        assert_ne!(fill_color(&app, &entity, true), red);
        app.material_map.set(1, None);
        assert_eq!(material_fill_color(&app, &entity), None);
    }

    #[test]
    fn dropping_a_material_assigns_it_to_the_entity_under_pointer() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        let root = std::env::temp_dir().join(format!("lunar_view3d_mat_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        app.path = root.join("scene.json");
        app.document = EditorDocument::new();
        app.apply(EditorCommand::CreateEntity { id: 1, name: None });
        app.material_library
            .insert("piedra.material.json".to_owned(), Material::default());
        let camera = app.camera3d;
        let pointer = project([0.0; 3], camera, rect()).unwrap();
        let drag = AssetDrag::new("piedra.material.json", AssetKind::Material);
        let history = app.document.undo_len();

        apply_asset_drop(&mut app, pointer, rect(), camera, &drag);

        assert_eq!(app.material_map.get(1), Some("piedra.material.json"));
        assert_eq!(app.document.undo_len(), history + 1);
        let sidecar = std::fs::read_to_string(app.material_map_path()).expect("sidecar");
        assert!(sidecar.contains("1=piedra.material.json"), "{sidecar}");
        app.undo();
        assert_eq!(app.material_map.get(1), None);
        assert_eq!(app.document.undo_len(), history);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dropping_a_material_on_empty_space_only_reports_it() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        let root = std::env::temp_dir().join(format!("lunar_view3d_empty_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        app.path = root.join("scene.json");
        app.document = EditorDocument::new();
        app.apply(EditorCommand::CreateEntity { id: 1, name: None });
        app.material_library
            .insert("piedra.material.json".to_owned(), Material::default());
        let camera = app.camera3d;
        let drag = AssetDrag::new("piedra.material.json", AssetKind::Material);
        let history = app.document.undo_len();

        apply_asset_drop(&mut app, Pos2::new(-500.0, -500.0), rect(), camera, &drag);

        assert_eq!(app.material_map.get(1), None);
        assert_eq!(app.document.undo_len(), history);
        assert!(app.status_log.iter().any(|entry| {
            entry
                .message
                .contains("Suelta el material sobre una entidad")
        }));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn viewport_background_uses_the_editor_environment() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.environment.mode = crate::environment::SkyMode::Gradient;
        app.environment.top = [1.0, 0.0, 0.0];
        app.environment.middle = [0.0, 1.0, 0.0];
        app.environment.bottom = [0.0, 0.0, 1.0];
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 700.0))),
            ..Default::default()
        };
        let mut first = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Vista 3D de prueba").show(ctx, |ui| view3d_ui(&mut app, ui));
        });
        first.textures_delta.clear();
        let mut output = ctx.run_pass(input, |ctx| {
            egui::Window::new("Vista 3D de prueba").show(ctx, |ui| view3d_ui(&mut app, ui));
        });
        let texts: Vec<String> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                _ => None,
            })
            .collect();
        output.textures_delta.clear();
        assert!(texts.iter().any(|text| text == "Entorno: Degradado"));
        assert_eq!(
            sky_color(&app.environment, 0.0),
            Color32::from_rgb(255, 0, 0)
        );
        assert_eq!(
            sky_color(&app.environment, 0.5),
            Color32::from_rgb(0, 255, 0)
        );
        assert_eq!(
            sky_color(&app.environment, 1.0),
            Color32::from_rgb(0, 0, 255)
        );
    }

    #[test]
    fn fog_fades_lit_faces_but_not_solid_ones() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.document = EditorDocument::new();
        app.apply(EditorCommand::CreateEntity { id: 1, name: None });
        app.light_map.set(1, Light::default());
        app.environment.fog = true;
        app.environment.fog_color = [1.0, 0.0, 0.0];
        app.environment.fog_start = 0.0;
        app.environment.fog_end = 100.0;
        let samples = light_samples(&app);
        let entity = app.document.scene().entities[0].clone();
        let lit = lit_faces(
            &app,
            &entity,
            Camera3D::default(),
            rect(),
            Color32::WHITE,
            &samples,
        );
        let solid = shaded_faces(
            Transform::default(),
            Camera3D::default(),
            rect(),
            Color32::WHITE,
            ShadingMode::Solid,
        );
        assert!(
            lit.iter().any(|face| face.color.r() > face.color.g()),
            "{lit:?}"
        );
        assert!(solid.iter().all(|face| face.color == Color32::WHITE));
    }
}
