//! Lienzo 2D del editor visual (ADR-020).
//!
//! Toda la lógica de coordenadas es pura y se prueba sin ventana. Los sprites
//! se dibujan con mallas de `egui` para respetar la rotación de la entidad.

use std::sync::Arc;

use egui::{Color32, PointerButton, Pos2, Rect, Sense, Stroke, TextureId, Ui, Vec2};
use lunar_editor::EditorCommand;
use lunar_scenes::{Component, Transform};

use crate::app::EditorApp;
use crate::gizmo::{Originals, reference, reference_angle};
use crate::helpers::{angle_from_quaternion, contains_point, image_world_size, snap_to_grid};
use crate::play::PlayState;
use crate::tools::{TransformMode, TransformSpace};
use crate::view3d::{Drag3D, ViewMode};

/// Vista ortográfica del lienzo: centro en mundo y píxeles por unidad.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct View {
    pub(crate) center: [f32; 2],
    pub(crate) zoom: f32,
}

/// Convierte un punto del mundo a coordenadas de pantalla (Y hacia abajo).
#[must_use]
pub(crate) fn world_to_screen(point: [f32; 2], rect: Rect, view: View) -> Pos2 {
    rect.center()
        + Vec2::new(
            (point[0] - view.center[0]) * view.zoom,
            (view.center[1] - point[1]) * view.zoom,
        )
}

/// Convierte un punto de pantalla a coordenadas del mundo (Y hacia arriba).
#[must_use]
pub(crate) fn screen_to_world(point: Pos2, rect: Rect, view: View) -> [f32; 2] {
    let delta = point - rect.center();
    [
        view.center[0] + delta.x / view.zoom,
        view.center[1] - delta.y / view.zoom,
    ]
}

/// Límites del mundo visibles en el rectángulo del lienzo: `[min_x, min_y, max_x, max_y]`.
#[must_use]
pub(crate) fn visible_world(rect: Rect, view: View) -> [f32; 4] {
    let bottom_left = screen_to_world(rect.min, rect, view);
    let top_right = screen_to_world(rect.max, rect, view);
    [bottom_left[0], top_right[1], top_right[0], bottom_left[1]]
}

#[derive(Debug, Clone)]
struct DrawItem {
    id: u64,
    transform: Transform,
    center: Pos2,
    half_screen: Vec2,
    texture: Option<TextureId>,
    label: String,
    selected: bool,
}

/// Estado del gesto actual sobre el lienzo.
#[derive(Debug, Clone, Default)]
pub(crate) enum CanvasDrag {
    #[default]
    None,
    Pan {
        pointer: Pos2,
        center: [f32; 2],
    },
    Select {
        start: Pos2,
        current: Pos2,
        additive: bool,
    },
    Move {
        pointer: Pos2,
        originals: Vec<(u64, Transform)>,
    },
    Scale {
        pointer: Pos2,
        center: Pos2,
        pivot: [f32; 2],
        originals: Originals,
    },
    Rotate {
        pointer: Pos2,
        center: Pos2,
        pivot: [f32; 2],
        originals: Originals,
    },
}

pub(crate) fn canvas_ui(app: &mut EditorApp, ui: &mut Ui) {
    let mut preferences_changed = false;
    ui.horizontal(|ui| {
        preferences_changed |= ui.checkbox(&mut app.show_grid, "Rejilla").changed();
        preferences_changed |= ui.checkbox(&mut app.snap, "Snap 0.25").changed();
        ui.label(format!("Zoom {:.0}", app.zoom));
        ui.label(format!("Herramienta: {}", app.transform_mode.label()));
        ui.separator();
        if ui
            .selectable_label(app.transform_space == TransformSpace::Global, "Global (Q)")
            .clicked()
        {
            app.transform_space = TransformSpace::Global;
            preferences_changed = true;
        }
        if ui
            .selectable_label(app.transform_space == TransformSpace::Local, "Local (Q)")
            .clicked()
        {
            app.transform_space = TransformSpace::Local;
            preferences_changed = true;
        }
        ui.separator();
        if ui
            .selectable_label(app.view_mode == ViewMode::TwoD, ViewMode::TwoD.label())
            .clicked()
        {
            commit_preview(app);
            app.view_mode = ViewMode::TwoD;
            app.drag = CanvasDrag::None;
            app.view3d_drag = Drag3D::None;
        }
        if ui
            .selectable_label(app.view_mode == ViewMode::ThreeD, ViewMode::ThreeD.label())
            .clicked()
        {
            commit_preview(app);
            app.view_mode = ViewMode::ThreeD;
            app.drag = CanvasDrag::None;
            app.view3d_drag = Drag3D::None;
        }
        ui.label(
            "W mover · E rotar · R escalar · Q espacio · F encuadrar · rueda zoom · derecho paneo",
        );
        if app.play_state != PlayState::Stopped {
            ui.label(format!(
                "Play {:.2}s · {}",
                app.play_time,
                app.play_state.label()
            ));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.play_state != PlayState::Stopped && ui.button("■ Stop").clicked() {
                app.stop_play();
            }
            match app.play_state {
                PlayState::Stopped => {
                    if ui.button("▶ Play").clicked() {
                        app.play();
                    }
                }
                PlayState::Playing => {
                    if ui.button("⏸ Pausa").clicked() {
                        app.toggle_pause();
                    }
                }
                PlayState::Paused => {
                    if ui.button("▶ Reanudar").clicked() {
                        app.toggle_pause();
                    }
                }
            }
        });
    });
    if preferences_changed {
        app.save_settings();
    }

    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let current = View {
        center: app.view_center,
        zoom: app.zoom,
    };
    if !ui.ctx().egui_wants_keyboard_input() && ui.input(|input| input.key_pressed(egui::Key::F)) {
        let items = collect_items(app, rect, current);
        let fitted = fit_view(&items, rect, current, app.document.selection());
        app.view_center = fitted.center;
        app.zoom = fitted.zoom;
        app.save_settings();
    }

    let view = View {
        center: app.view_center,
        zoom: app.zoom,
    };
    ui.painter()
        .rect_filled(rect, 0.0, Color32::from_rgb(24, 26, 32));

    if app.show_grid {
        paint_grid(ui, rect, view);
    }

    let items = collect_items(app, rect, view);
    for item in &items {
        paint_item(ui, item);
    }
    paint_selection(
        ui,
        &items,
        app.transform_mode,
        app.transform_space,
        app.selection_anchor,
    );
    paint_marquee(ui, &app.drag);

    let pointer = response.hover_pos();
    if let Some(point) = pointer.filter(|point| rect.contains(*point)) {
        let world = screen_to_world(point, rect, view);
        ui.painter().text(
            rect.min + Vec2::new(8.0, rect.height() - 8.0),
            egui::Align2::LEFT_BOTTOM,
            format!("Cursor: {:.2}, {:.2}", world[0], world[1]),
            egui::FontId::proportional(11.0),
            Color32::from_gray(210),
        );
    }
    handle_zoom(app, ui, rect, pointer);
    if app.play_state == PlayState::Stopped {
        handle_drop(app, ui, rect, pointer);
        handle_pointer(app, ui, &response, rect, view, pointer, &items);
        commit_drag(app, ui, &items);
    }
}

fn paint_grid(ui: &mut Ui, rect: Rect, view: View) {
    let [min_x, min_y, max_x, max_y] = visible_world(rect, view);
    let step = if view.zoom < 24.0 { 1.0 } else { 0.25 };
    let painter = ui.painter();
    let mut x = (min_x / step).ceil() * step;
    while x <= max_x {
        let top = world_to_screen([x, max_y], rect, view);
        let bottom = world_to_screen([x, min_y], rect, view);
        painter.line_segment([top, bottom], Stroke::new(0.5, Color32::from_gray(45)));
        x += step;
    }
    let mut y = (min_y / step).ceil() * step;
    while y <= max_y {
        let left = world_to_screen([min_x, y], rect, view);
        let right = world_to_screen([max_x, y], rect, view);
        painter.line_segment([left, right], Stroke::new(0.5, Color32::from_gray(45)));
        y += step;
    }
}

fn collect_items(app: &EditorApp, rect: Rect, view: View) -> Vec<DrawItem> {
    let mut items = Vec::new();
    for entity in &app.document.scene().entities {
        let transform = app
            .preview
            .get(&entity.id)
            .copied()
            .unwrap_or(entity.transform);
        let image = entity.components.iter().find_map(Component::asset_path);
        let dimensions = image.and_then(|asset| app.assets.dimensions(asset));
        let texture =
            image.and_then(|asset| app.assets.texture(asset).map(egui::TextureHandle::id));
        let size = entity_world_size(dimensions, transform.scale);
        let center = world_to_screen(
            [transform.translation[0], transform.translation[1]],
            rect,
            view,
        );
        items.push(DrawItem {
            id: entity.id,
            transform,
            center,
            half_screen: Vec2::new(size[0] * view.zoom, size[1] * view.zoom) * 0.5,
            texture,
            label: entity
                .name
                .clone()
                .unwrap_or_else(|| format!("Entidad {}", entity.id)),
            selected: app.document.selection().contains(&entity.id),
        });
    }
    items
}

/// Tamaño en mundo de una entidad. Sin asset cargado se usa una celda de
/// `PIXELS_PER_UNIT`, es decir 1×1 unidades antes de la escala, para que las
/// entidades vacías también se puedan seleccionar en el lienzo.
#[must_use]
pub(crate) fn entity_world_size(dimensions: Option<[u32; 2]>, scale: [f32; 3]) -> [f32; 2] {
    image_world_size(dimensions.unwrap_or([32, 32]), scale)
}

/// Cuatro esquinas de pantalla en orden TL, TR, BR, BL, con rotación aplicada.
fn sprite_corners(item: &DrawItem) -> [Pos2; 4] {
    let angle = angle_from_quaternion(item.transform.rotation);
    let (sine, cosine) = angle.sin_cos();
    let offsets = [
        Vec2::new(-item.half_screen.x, item.half_screen.y),
        Vec2::new(item.half_screen.x, item.half_screen.y),
        Vec2::new(item.half_screen.x, -item.half_screen.y),
        Vec2::new(-item.half_screen.x, -item.half_screen.y),
    ];
    std::array::from_fn(|index| {
        let offset = offsets[index];
        let rotated = Vec2::new(
            offset.x * cosine - offset.y * sine,
            offset.x * sine + offset.y * cosine,
        );
        item.center + Vec2::new(rotated.x, -rotated.y)
    })
}

fn paint_item(ui: &mut Ui, item: &DrawItem) {
    let corners = sprite_corners(item);
    if let Some(texture) = item.texture {
        let mut mesh = egui::Mesh::with_texture(texture);
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        {
            let mut push = |position: Pos2, uv: Pos2| {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: position,
                    uv,
                    color: Color32::WHITE,
                });
            };
            push(corners[0], Pos2::new(0.0, 0.0));
            push(corners[1], Pos2::new(1.0, 0.0));
            push(corners[2], Pos2::new(1.0, 1.0));
            push(corners[3], Pos2::new(0.0, 1.0));
        }
        ui.painter().add(egui::Shape::Mesh(Arc::new(mesh)));
    } else {
        ui.painter().rect_filled(
            Rect::from_points(&corners),
            2.0,
            Color32::from_rgba_premultiplied(90, 110, 140, 160),
        );
    }
    ui.painter().text(
        Rect::from_points(&corners).min + Vec2::new(2.0, 2.0),
        egui::Align2::LEFT_TOP,
        format!("{} · #{}", item.label, item.id),
        egui::FontId::proportional(11.0),
        Color32::from_gray(230),
    );
}

/// Centro común de la selección: centro del rectángulo que la envuelve.
#[must_use]
fn selection_center(items: &[DrawItem]) -> Option<Pos2> {
    let mut selected = items
        .iter()
        .filter(|item| item.selected)
        .map(|item| Rect::from_points(&sprite_corners(item)));
    let first = selected.next()?;
    Some(
        selected
            .fold(first, |acc, bounds| acc.union(bounds))
            .center(),
    )
}

/// Tirador de rotación: 22 px por encima del borde superior.
#[must_use]
fn rotate_handle(bounds: Rect) -> Pos2 {
    Pos2::new(bounds.center().x, bounds.min.y - 22.0)
}

/// Tirador de escala: 6 px a la derecha y abajo del borde inferior derecho.
#[must_use]
fn scale_handle(bounds: Rect) -> Pos2 {
    bounds.max + Vec2::new(6.0, 6.0)
}

/// Transformaciones visibles de la selección (preview incluido), en orden de dibujo.
#[must_use]
fn selected_originals(items: &[DrawItem]) -> Originals {
    items
        .iter()
        .filter(|item| item.selected)
        .map(|item| (item.id, item.transform))
        .collect()
}

/// Origen del gizmo: centro del rectángulo envolvente en `Global`, origen de
/// la entidad ancla en `Local` (E14).
#[must_use]
fn gizmo_origin(items: &[DrawItem], space: TransformSpace, anchor: Option<u64>) -> Option<Pos2> {
    match space {
        TransformSpace::Global => selection_center(items),
        TransformSpace::Local => {
            let originals = selected_originals(items);
            let (id, _) = reference(&originals, anchor)?;
            items
                .iter()
                .find(|item| item.id == *id)
                .map(|item| item.center)
        }
    }
}

/// Ángulo con el que se orientan los tiradores: cero en `Global` y la rotación
/// Z de la entidad ancla en `Local` (E14).
#[must_use]
fn gizmo_angle(items: &[DrawItem], space: TransformSpace, anchor: Option<u64>) -> f32 {
    match space {
        TransformSpace::Global => 0.0,
        TransformSpace::Local => reference_angle(&selected_originals(items), anchor).unwrap_or(0.0),
    }
}

/// Gira la posición de un tirador alrededor del origen del gizmo. La pantalla
/// tiene Y hacia abajo, por lo que un giro positivo en el mundo es negativo aquí.
#[must_use]
fn handle_position(origin: Pos2, point: Pos2, angle: f32) -> Pos2 {
    let delta = point - origin;
    let (sine, cosine) = (-angle).sin_cos();
    origin
        + Vec2::new(
            delta.x * cosine - delta.y * sine,
            delta.x * sine + delta.y * cosine,
        )
}

fn paint_move_cross(ui: &mut Ui, center: Pos2) {
    ui.painter().circle_filled(center, 5.0, Color32::LIGHT_BLUE);
    let arm = 9.0;
    ui.painter().line_segment(
        [center - Vec2::new(arm, 0.0), center + Vec2::new(arm, 0.0)],
        Stroke::new(1.5, Color32::LIGHT_BLUE),
    );
    ui.painter().line_segment(
        [center - Vec2::new(0.0, arm), center + Vec2::new(0.0, arm)],
        Stroke::new(1.5, Color32::LIGHT_BLUE),
    );
}

fn paint_selection(
    ui: &mut Ui,
    items: &[DrawItem],
    mode: TransformMode,
    space: TransformSpace,
    anchor: Option<u64>,
) {
    for item in items.iter().filter(|item| item.selected) {
        ui.painter().rect_stroke(
            Rect::from_points(&sprite_corners(item)),
            2.0,
            Stroke::new(1.5, Color32::LIGHT_BLUE),
            egui::StrokeKind::Outside,
        );
    }
    let (Some(bounds), Some(origin)) = (
        items
            .iter()
            .filter(|item| item.selected)
            .map(|item| Rect::from_points(&sprite_corners(item)))
            .reduce(|acc, rect| acc.union(rect)),
        gizmo_origin(items, space, anchor),
    ) else {
        return;
    };
    let angle = gizmo_angle(items, space, anchor);
    // Un solo gizmo para toda la selección, en el origen elegido (E12 + E14).
    match mode {
        TransformMode::Move => paint_move_cross(ui, origin),
        TransformMode::Rotate => {
            ui.painter().circle_filled(
                handle_position(origin, rotate_handle(bounds), angle),
                5.0,
                Color32::YELLOW,
            );
        }
        TransformMode::Scale => {
            ui.painter().rect_filled(
                Rect::from_center_size(
                    handle_position(origin, scale_handle(bounds), angle),
                    Vec2::splat(9.0),
                ),
                2.0,
                Color32::LIGHT_GREEN,
            );
        }
    }
}

/// Vista que encuadra la selección, o toda la escena si no hay selección.
#[must_use]
fn fit_view(items: &[DrawItem], rect: Rect, view: View, selection: &[u64]) -> View {
    let targets: Vec<&DrawItem> = if selection.is_empty() {
        items.iter().collect()
    } else {
        items
            .iter()
            .filter(|item| selection.contains(&item.id))
            .collect()
    };
    let corners: Vec<Pos2> = targets
        .iter()
        .flat_map(|item| sprite_corners(item))
        .collect();
    if corners.is_empty() {
        return View {
            center: [0.0, 0.0],
            zoom: 48.0,
        };
    }

    let bounds = Rect::from_points(&corners);
    let margin = Vec2::splat(24.0);
    let available = Rect::from_min_max(rect.min + margin, rect.max - margin);
    let zoom = (available.width().max(1.0) / bounds.width().max(1.0))
        .min(available.height().max(1.0) / bounds.height().max(1.0))
        .clamp(8.0, 512.0);
    let center = screen_to_world(bounds.center(), rect, view);
    View { center, zoom }
}

/// Entidades cuyo rectángulo toca el área de selección, en orden de dibujo.
#[must_use]
fn selection_in_rect(items: &[DrawItem], area: Rect) -> Vec<u64> {
    items
        .iter()
        .filter(|item| Rect::from_points(&sprite_corners(item)).intersects(area))
        .map(|item| item.id)
        .collect()
}

fn paint_marquee(ui: &mut Ui, drag: &CanvasDrag) {
    if let CanvasDrag::Select { start, current, .. } = drag {
        let area = Rect::from_two_pos(*start, *current);
        ui.painter().rect_filled(
            area,
            0.0,
            Color32::from_rgba_premultiplied(90, 140, 200, 40),
        );
        ui.painter().rect_stroke(
            area,
            0.0,
            Stroke::new(1.0, Color32::LIGHT_BLUE),
            egui::StrokeKind::Inside,
        );
    }
}

fn handle_zoom(app: &mut EditorApp, ui: &mut Ui, rect: Rect, pointer: Option<Pos2>) {
    let scroll = ui.input(|input| input.smooth_scroll_delta.y);
    if scroll == 0.0 {
        return;
    }
    let Some(pointer) = pointer.filter(|point| rect.contains(*point)) else {
        return;
    };
    let before = screen_to_world(
        pointer,
        rect,
        View {
            center: app.view_center,
            zoom: app.zoom,
        },
    );
    let zoom = (app.zoom * (-scroll * 0.0015).exp()).clamp(8.0, 512.0);
    let after = screen_to_world(
        pointer,
        rect,
        View {
            center: app.view_center,
            zoom,
        },
    );
    app.view_center = [
        app.view_center[0] + before[0] - after[0],
        app.view_center[1] + before[1] - after[1],
    ];
    app.zoom = zoom;
    app.save_settings();
}

fn handle_drop(app: &mut EditorApp, ui: &mut Ui, rect: Rect, pointer: Option<Pos2>) {
    if !ui.input(|input| input.pointer.any_released()) {
        return;
    }
    let Some(pointer) = pointer.filter(|point| rect.contains(*point)) else {
        return;
    };
    let Some(drag) = crate::dnd::take_asset(ui.ctx()) else {
        return;
    };
    let asset = drag.path.as_str();
    let world = screen_to_world(
        pointer,
        rect,
        View {
            center: app.view_center,
            zoom: app.zoom,
        },
    );
    app.create_entity_from_asset(asset, world);
}

fn handle_pointer(
    app: &mut EditorApp,
    ui: &mut Ui,
    response: &egui::Response,
    rect: Rect,
    view: View,
    pointer: Option<Pos2>,
    items: &[DrawItem],
) {
    let secondary = ui.input(|input| {
        input.pointer.button_down(PointerButton::Secondary)
            || input.pointer.button_down(PointerButton::Middle)
    });

    if response.drag_started()
        && let Some(pointer) = pointer
    {
        if secondary {
            app.drag = CanvasDrag::Pan {
                pointer,
                center: app.view_center,
            };
        } else if multi_move_at(
            items,
            pointer,
            app.transform_mode,
            app.transform_space,
            app.selection_anchor,
        ) {
            let originals = selected_originals(items);
            app.drag = CanvasDrag::Move { pointer, originals };
        } else if let Some(kind) = handle_at(
            items,
            pointer,
            app.transform_mode,
            app.transform_space,
            app.selection_anchor,
        ) {
            if let Some(origin) = gizmo_origin(items, app.transform_space, app.selection_anchor) {
                let pivot = screen_to_world(origin, rect, view);
                app.drag = match kind {
                    HandleKind::Scale => CanvasDrag::Scale {
                        pointer,
                        center: origin,
                        pivot,
                        originals: selected_originals(items),
                    },
                    HandleKind::Rotate => CanvasDrag::Rotate {
                        pointer,
                        center: origin,
                        pivot,
                        originals: selected_originals(items),
                    },
                };
            }
        } else if let Some(id) = entity_at(app, pointer, view, rect) {
            let (ctrl, shift) = ui.input(|input| (input.modifiers.ctrl, input.modifiers.shift));
            app.select_clicked(id, ctrl, shift);
            let ids = app.document.selection().to_vec();
            let originals = ids
                .into_iter()
                .filter_map(|id| {
                    app.document
                        .scene()
                        .entities
                        .iter()
                        .find(|entity| entity.id == id)
                        .map(|entity| (id, entity.transform))
                })
                .collect();
            app.drag = CanvasDrag::Move { pointer, originals };
        } else {
            let additive = ui.input(|input| input.modifiers.shift || input.modifiers.ctrl);
            app.drag = CanvasDrag::Select {
                start: pointer,
                current: pointer,
                additive,
            };
        }
    }

    match app.drag.clone() {
        CanvasDrag::None => {}
        CanvasDrag::Pan {
            pointer: start,
            center,
        } => {
            if let Some(pointer) = pointer {
                app.view_center = [
                    center[0] - (pointer.x - start.x) / app.zoom,
                    center[1] + (pointer.y - start.y) / app.zoom,
                ];
            }
        }
        CanvasDrag::Select {
            start, additive, ..
        } => {
            if let Some(pointer) = pointer {
                app.drag = CanvasDrag::Select {
                    start,
                    current: pointer,
                    additive,
                };
            }
        }
        CanvasDrag::Move {
            pointer: start,
            originals,
        } => {
            if let Some(pointer) = pointer {
                let dx = (pointer.x - start.x) / app.zoom;
                let dy = -(pointer.y - start.y) / app.zoom;
                for (id, original) in originals {
                    let x = original.translation[0] + dx;
                    let y = original.translation[1] + dy;
                    let (x, y) = if app.snap {
                        (snap_to_grid(x), snap_to_grid(y))
                    } else {
                        (x, y)
                    };
                    let mut transform = original;
                    transform.translation = [x, y, original.translation[2]];
                    app.preview.insert(id, transform);
                }
            }
        }
        CanvasDrag::Scale {
            pointer: start,
            center,
            pivot,
            originals,
        } => {
            if let Some(pointer) = pointer {
                let start_distance = (start - center).length().max(1.0);
                let factor = ((pointer - center).length() / start_distance).clamp(0.1, 10.0);
                for (id, transform) in crate::gizmo::scale_group(&originals, pivot, factor) {
                    app.preview.insert(id, transform);
                }
            }
        }
        CanvasDrag::Rotate {
            pointer: start,
            center,
            pivot,
            originals,
        } => {
            if let Some(pointer) = pointer {
                let start_angle = (start.y - center.y).atan2(start.x - center.x);
                let angle = (pointer.y - center.y).atan2(pointer.x - center.x);
                for (id, transform) in
                    crate::gizmo::rotate_group(&originals, pivot, -(angle - start_angle))
                {
                    app.preview.insert(id, transform);
                }
            }
        }
    }

    if response.clicked() && !secondary {
        let Some(pointer) = pointer else {
            return;
        };
        let (ctrl, shift) = ui.input(|input| (input.modifiers.ctrl, input.modifiers.shift));
        if let Some(id) = entity_at(app, pointer, view, rect) {
            app.select_clicked(id, ctrl, shift);
        } else if !ctrl && !shift {
            app.clear_selection();
        }
    }
}

/// Convierte las transformaciones temporales en un único comando real. Lo usan
/// el lienzo 2D y la vista 3D: arrastrar N entidades selectas deja **una** entrada
/// en el historial y un solo `Ctrl+Z` las devuelve a su sitio (E12).
pub(crate) fn commit_preview(app: &mut EditorApp) {
    let previews: Vec<(u64, Transform)> = app.preview.drain().collect();
    let transforms: Vec<(u64, Transform)> = previews
        .into_iter()
        .filter(|(id, transform)| {
            app.document
                .scene()
                .entities
                .iter()
                .find(|entity| entity.id == *id)
                .is_some_and(|entity| entity.transform != *transform)
        })
        .collect();
    if !transforms.is_empty() {
        app.apply(EditorCommand::SetTransforms { transforms });
    }
}

fn commit_drag(app: &mut EditorApp, ui: &mut Ui, items: &[DrawItem]) {
    if !ui.input(|input| input.pointer.any_released()) {
        return;
    }
    let was_pan = matches!(app.drag, CanvasDrag::Pan { .. });
    if let CanvasDrag::Select {
        start,
        current,
        additive,
    } = app.drag.clone()
    {
        let area = Rect::from_two_pos(start, current);
        let mut selection = if additive {
            app.document.selection().to_vec()
        } else {
            Vec::new()
        };
        for id in selection_in_rect(items, area) {
            if !selection.contains(&id) {
                selection.push(id);
            }
        }
        app.document.select(&selection);
    }

    commit_preview(app);
    app.drag = CanvasDrag::None;
    if was_pan {
        app.save_settings();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandleKind {
    Scale,
    Rotate,
}

/// Tirador de movimiento de grupo: dos o más entidades selectas, en el origen
/// del gizmo del espacio elegido (E12 + E14).
#[must_use]
fn multi_move_at(
    items: &[DrawItem],
    pointer: Pos2,
    mode: TransformMode,
    space: TransformSpace,
    anchor: Option<u64>,
) -> bool {
    if mode != TransformMode::Move || items.iter().filter(|item| item.selected).count() < 2 {
        return false;
    }
    gizmo_origin(items, space, anchor).is_some_and(|origin| (pointer - origin).length() <= 8.0)
}

/// Tirador de rotación o escala del grupo (una o más entidades selectas).
#[must_use]
fn handle_at(
    items: &[DrawItem],
    pointer: Pos2,
    mode: TransformMode,
    space: TransformSpace,
    anchor: Option<u64>,
) -> Option<HandleKind> {
    let bounds = items
        .iter()
        .filter(|item| item.selected)
        .map(|item| Rect::from_points(&sprite_corners(item)))
        .reduce(|acc, rect| acc.union(rect))?;
    let origin = gizmo_origin(items, space, anchor)?;
    let angle = gizmo_angle(items, space, anchor);
    match mode {
        TransformMode::Move => None,
        TransformMode::Scale
            if (pointer - handle_position(origin, scale_handle(bounds), angle)).length() <= 8.0 =>
        {
            Some(HandleKind::Scale)
        }
        TransformMode::Rotate
            if (pointer - handle_position(origin, rotate_handle(bounds), angle)).length()
                <= 8.0 =>
        {
            Some(HandleKind::Rotate)
        }
        _ => None,
    }
}

fn entity_at(app: &EditorApp, pointer: Pos2, view: View, rect: Rect) -> Option<u64> {
    let world = screen_to_world(pointer, rect, view);
    for entity in app.document.scene().entities.iter().rev() {
        let image = entity.components.iter().find_map(Component::asset_path);
        let dimensions = image.and_then(|asset| app.assets.dimensions(asset));
        let transform = app
            .preview
            .get(&entity.id)
            .copied()
            .unwrap_or(entity.transform);
        let size = entity_world_size(dimensions, transform.scale);
        if contains_point(
            [transform.translation[0], transform.translation[1]],
            [size[0] * 0.5, size[1] * 0.5],
            angle_from_quaternion(transform.rotation),
            world,
        ) {
            return Some(entity.id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))
    }

    fn item(id: u64, center: Pos2) -> DrawItem {
        DrawItem {
            id,
            transform: Transform::default(),
            center,
            half_screen: Vec2::splat(10.0),
            texture: None,
            label: String::new(),
            selected: false,
        }
    }

    #[test]
    fn toolbar_shows_the_2d_and_3d_toggle() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))),
            ..Default::default()
        };
        let mut first = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Lienzo de prueba").show(ctx, |ui| canvas_ui(&mut app, ui));
        });
        first.textures_delta.clear();
        let mut second = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Lienzo de prueba").show(ctx, |ui| canvas_ui(&mut app, ui));
        });
        second.textures_delta.clear();
        let mut output = ctx.run_pass(input, |ctx| {
            egui::Window::new("Lienzo de prueba").show(ctx, |ui| canvas_ui(&mut app, ui));
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
        assert!(texts.iter().any(|text| text == "2D"), "{texts:?}");
        assert!(texts.iter().any(|text| text == "3D"), "{texts:?}");
        assert!(texts.iter().any(|text| text == "Global (Q)"), "{texts:?}");
        assert!(texts.iter().any(|text| text == "Local (Q)"), "{texts:?}");
    }

    #[test]
    fn rectangular_selection_includes_intersecting_items() {
        let items = [
            item(1, Pos2::new(100.0, 100.0)),
            item(2, Pos2::new(300.0, 100.0)),
        ];
        let area = Rect::from_min_max(Pos2::new(90.0, 90.0), Pos2::new(110.0, 110.0));
        assert_eq!(selection_in_rect(&items, area), vec![1]);

        let wide = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(400.0, 200.0));
        assert_eq!(selection_in_rect(&items, wide), vec![1, 2]);
    }

    #[test]
    fn frame_centers_the_selection_and_fits_the_zoom() {
        let items = [
            item(1, Pos2::new(380.0, 280.0)),
            item(2, Pos2::new(420.0, 320.0)),
        ];
        let view = View {
            center: [0.0, 0.0],
            zoom: 40.0,
        };
        let fitted = fit_view(&items, rect(), view, &[1, 2]);
        let expected = screen_to_world(Pos2::new(400.0, 300.0), rect(), view);
        assert!((fitted.center[0] - expected[0]).abs() < 1e-3);
        assert!((fitted.center[1] - expected[1]).abs() < 1e-3);
        assert!(fitted.zoom > 8.0 && fitted.zoom <= 512.0);

        let all = fit_view(&items, rect(), view, &[]);
        assert_eq!(all, fitted);
    }

    #[test]
    fn frame_without_items_restores_a_sensible_view() {
        let view = fit_view(
            &[],
            rect(),
            View {
                center: [5.0, 5.0],
                zoom: 100.0,
            },
            &[],
        );
        assert_eq!(
            view,
            View {
                center: [0.0, 0.0],
                zoom: 48.0
            }
        );
    }

    #[test]
    fn world_and_screen_round_trip() {
        let view = View {
            center: [3.0, -2.0],
            zoom: 40.0,
        };
        let point = [1.25, 4.5];
        let screen = world_to_screen(point, rect(), view);
        assert_eq!(screen_to_world(screen, rect(), view), point);
    }

    #[test]
    fn world_y_grows_upwards() {
        let view = View {
            center: [0.0, 0.0],
            zoom: 10.0,
        };
        let up = world_to_screen([0.0, 1.0], rect(), view);
        assert!(up.y < rect().center().y);
    }

    #[test]
    fn entities_without_assets_have_a_selectable_size() {
        assert_eq!(entity_world_size(None, [2.0, 3.0, 1.0]), [2.0, 3.0]);
        assert_eq!(
            entity_world_size(Some([64, 32]), [1.0, 1.0, 1.0]),
            [2.0, 1.0]
        );
    }

    #[test]
    fn visible_world_matches_view_size() {
        let view = View {
            center: [0.0, 0.0],
            zoom: 10.0,
        };
        assert_eq!(visible_world(rect(), view), [-40.0, -30.0, 40.0, 30.0]);
    }

    #[test]
    fn handles_keep_their_screen_offsets() {
        let bounds = Rect::from_min_max(Pos2::new(90.0, 90.0), Pos2::new(110.0, 110.0));
        assert_eq!(rotate_handle(bounds), Pos2::new(100.0, 68.0));
        assert_eq!(scale_handle(bounds), Pos2::new(116.0, 116.0));
    }

    #[test]
    fn only_the_active_tool_has_a_draggable_handle() {
        let mut selected = item(1, Pos2::new(100.0, 100.0));
        selected.selected = true;
        let items = [selected];
        let scale = Pos2::new(116.0, 116.0);
        let rotate = Pos2::new(100.0, 68.0);
        let space = TransformSpace::Global;
        assert_eq!(
            handle_at(&items, scale, TransformMode::Move, space, None),
            None
        );
        assert_eq!(
            handle_at(&items, rotate, TransformMode::Move, space, None),
            None
        );
        assert_eq!(
            handle_at(&items, scale, TransformMode::Scale, space, None),
            Some(HandleKind::Scale)
        );
        assert_eq!(
            handle_at(&items, rotate, TransformMode::Scale, space, None),
            None
        );
        assert_eq!(
            handle_at(&items, rotate, TransformMode::Rotate, space, None),
            Some(HandleKind::Rotate)
        );
        assert_eq!(
            handle_at(&items, scale, TransformMode::Rotate, space, None),
            None
        );
    }

    #[test]
    fn the_local_space_moves_the_gizmo_to_the_anchor_and_rotates_its_handles() {
        let mut first = item(1, Pos2::new(100.0, 100.0));
        first.selected = true;
        first.transform.rotation = crate::helpers::quaternion_from_angle(0.0);
        let mut second = item(2, Pos2::new(300.0, 100.0));
        second.selected = true;
        second.transform.rotation =
            crate::helpers::quaternion_from_angle(std::f32::consts::FRAC_PI_2);
        let items = [first, second];
        let center = Pos2::new(200.0, 100.0);
        assert_eq!(
            gizmo_origin(&items, TransformSpace::Global, Some(2)),
            Some(center)
        );
        assert_eq!(
            gizmo_origin(&items, TransformSpace::Local, Some(2)),
            Some(Pos2::new(300.0, 100.0))
        );
        assert_eq!(gizmo_angle(&items, TransformSpace::Global, Some(2)), 0.0);
        let angle = gizmo_angle(&items, TransformSpace::Local, Some(2));
        assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        let bounds = Rect::from_min_max(Pos2::new(90.0, 90.0), Pos2::new(310.0, 110.0));
        let global = handle_position(center, rotate_handle(bounds), 0.0);
        assert_eq!(global, Pos2::new(200.0, 68.0));
        let local = handle_position(Pos2::new(300.0, 100.0), rotate_handle(bounds), angle);
        assert!(
            (local.x - 268.0).abs() < 0.01 && (local.y - 200.0).abs() < 0.01,
            "el tirador local debe girar con el ancla: {local:?}"
        );
    }

    #[test]
    fn committing_a_multi_entity_preview_is_one_undo_step() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        for id in 1..=3 {
            app.document
                .apply(EditorCommand::CreateEntity { id, name: None })
                .expect("create entity");
        }
        let history = app.document.undo_len();
        app.preview.insert(
            1,
            Transform {
                translation: [1.0, 0.0, 0.0],
                ..Transform::default()
            },
        );
        app.preview.insert(
            3,
            Transform {
                translation: [3.0, 0.0, 0.0],
                ..Transform::default()
            },
        );

        commit_preview(&mut app);

        assert!(app.preview.is_empty());
        assert_eq!(app.document.undo_len(), history + 1);
        assert_eq!(
            app.document.scene().entities[0].transform.translation[0],
            1.0
        );
        assert_eq!(
            app.document.scene().entities[1].transform,
            Transform::default()
        );
        assert_eq!(
            app.document.scene().entities[2].transform.translation[0],
            3.0
        );

        assert!(app.document.undo());
        assert_eq!(
            app.document.scene().entities[0].transform,
            Transform::default()
        );
        assert_eq!(
            app.document.scene().entities[2].transform,
            Transform::default()
        );
        assert!(app.document.redo());
        assert_eq!(
            app.document.scene().entities[2].transform.translation[0],
            3.0
        );
    }

    #[test]
    fn group_rotate_and_scale_previews_are_one_undo_step() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.document = lunar_editor::EditorDocument::new();
        for id in 1..=2 {
            app.document
                .apply(EditorCommand::CreateEntity { id, name: None })
                .expect("create entity");
        }
        app.document
            .apply(EditorCommand::SetTransforms {
                transforms: vec![
                    (
                        1,
                        Transform {
                            translation: [0.0, 0.0, 0.0],
                            ..Transform::default()
                        },
                    ),
                    (
                        2,
                        Transform {
                            translation: [2.0, 0.0, 0.0],
                            ..Transform::default()
                        },
                    ),
                ],
            })
            .expect("place entities");
        let originals: Originals = app
            .document
            .scene()
            .entities
            .iter()
            .map(|entity| (entity.id, entity.transform))
            .collect();
        let history = app.document.undo_len();
        for (id, transform) in
            crate::gizmo::rotate_group(&originals, [1.0, 0.0], std::f32::consts::FRAC_PI_2)
        {
            app.preview.insert(id, transform);
        }
        commit_preview(&mut app);
        assert_eq!(app.document.undo_len(), history + 1);
        let rotated = app.document.scene().entities[1].transform.translation;
        for (value, target) in rotated.into_iter().zip([1.0, 1.0, 0.0]) {
            assert!((value - target).abs() < 1e-5, "{rotated:?}");
        }
        app.document.undo();
        assert_eq!(app.document.scene().entities[1].transform, originals[1].1);

        for (id, transform) in crate::gizmo::scale_group(&originals, [1.0, 0.0], 2.0) {
            app.preview.insert(id, transform);
        }
        commit_preview(&mut app);
        assert_eq!(app.document.undo_len(), history + 1);
        assert_eq!(
            app.document.scene().entities[0].transform.translation,
            [-1.0, 0.0, 0.0]
        );
    }

    #[test]
    fn committing_an_unchanged_preview_keeps_history_intact() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.document
            .apply(EditorCommand::CreateEntity { id: 1, name: None })
            .expect("create entity");
        let history = app.document.undo_len();
        app.preview.insert(1, Transform::default());

        commit_preview(&mut app);

        assert_eq!(app.document.undo_len(), history);
        assert!(!app.document.can_redo());
    }

    #[test]
    fn committing_preview_while_playing_does_not_mutate_the_scene() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.document
            .apply(EditorCommand::CreateEntity { id: 1, name: None })
            .expect("create entity");
        app.play();
        app.preview.insert(
            1,
            Transform {
                translation: [9.0, 0.0, 0.0],
                ..Transform::default()
            },
        );

        commit_preview(&mut app);

        assert_eq!(
            app.document.scene().entities[0].transform,
            Transform::default()
        );
        assert!(
            app.status_log
                .last()
                .is_some_and(|entry| entry.message.contains("Detén"))
        );
        app.stop_play();
    }

    #[test]
    fn group_move_handle_sits_at_the_center_of_the_selection() {
        let mut first = item(1, Pos2::new(100.0, 100.0));
        let mut second = item(2, Pos2::new(300.0, 200.0));
        first.selected = true;
        second.selected = true;
        let items = [first, second];
        assert_eq!(
            selection_center(&items),
            Some(Pos2::new(200.0, 150.0)),
            "el tirador debe estar en el centro del rectángulo que envuelve la selección"
        );
        assert!(multi_move_at(
            &items,
            Pos2::new(200.0, 150.0),
            TransformMode::Move,
            TransformSpace::Global,
            None
        ));
        assert!(!multi_move_at(
            &items,
            Pos2::new(260.0, 150.0),
            TransformMode::Move,
            TransformSpace::Global,
            None
        ));
        assert!(!multi_move_at(
            &items,
            Pos2::new(200.0, 150.0),
            TransformMode::Scale,
            TransformSpace::Global,
            None
        ));
    }

    #[test]
    fn group_move_handle_needs_more_than_one_selected_entity() {
        let mut only = item(7, Pos2::new(100.0, 100.0));
        only.selected = true;
        assert!(!multi_move_at(
            &[only],
            Pos2::new(100.0, 100.0),
            TransformMode::Move,
            TransformSpace::Global,
            None
        ));
    }
}
