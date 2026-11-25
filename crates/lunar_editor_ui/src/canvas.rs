//! Lienzo 2D del editor visual (ADR-020).
//!
//! Toda la lógica de coordenadas es pura y se prueba sin ventana. Los sprites
//! se dibujan con mallas de `egui` para respetar la rotación de la entidad.

use std::sync::Arc;

use egui::{Color32, DragAndDrop, PointerButton, Pos2, Rect, Sense, Stroke, TextureId, Ui, Vec2};
use lunar_scenes::{Component, Transform};

use crate::app::EditorApp;
use crate::assets::ASSET_PAYLOAD_PREFIX;
use crate::helpers::{
    angle_from_quaternion, contains_point, image_world_size, quaternion_from_angle, snap_to_grid,
};

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
    Move {
        pointer: Pos2,
        originals: Vec<(u64, Transform)>,
    },
    Scale {
        id: u64,
        pointer: Pos2,
        center: Pos2,
        original: Transform,
    },
    Rotate {
        id: u64,
        pointer: Pos2,
        center: Pos2,
        original: Transform,
    },
}

pub(crate) fn canvas_ui(app: &mut EditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.show_grid, "Rejilla");
        ui.checkbox(&mut app.snap, "Snap 0.25");
        ui.label(format!("Zoom {:.0}", app.zoom));
        ui.label("Arrastra assets aquí · rueda: zoom · botón derecho: paneo");
    });

    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
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
    paint_selection(ui, &items);

    let pointer = response.hover_pos();
    handle_zoom(app, ui, rect, pointer);
    handle_drop(app, ui, rect, pointer);
    handle_pointer(app, ui, &response, rect, view, pointer, &items);
    commit_drag(app, ui);
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
        let (dimensions, texture) = image.map_or(([32, 32], None), |asset| {
            (
                app.assets.dimensions(asset).unwrap_or([32, 32]),
                app.assets.texture(asset).map(egui::TextureHandle::id),
            )
        });
        let size = image_world_size(dimensions, transform.scale);
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

fn paint_selection(ui: &mut Ui, items: &[DrawItem]) {
    let selection_count = items.iter().filter(|item| item.selected).count();
    for item in items.iter().filter(|item| item.selected) {
        let bounds = Rect::from_points(&sprite_corners(item));
        ui.painter().rect_stroke(
            bounds,
            2.0,
            Stroke::new(1.5, Color32::LIGHT_BLUE),
            egui::StrokeKind::Outside,
        );
        if selection_count == 1 {
            let scale = bounds.max + Vec2::new(6.0, 6.0);
            ui.painter().rect_filled(
                Rect::from_center_size(scale, Vec2::splat(9.0)),
                2.0,
                Color32::LIGHT_GREEN,
            );
            let rotate = Pos2::new(bounds.center().x, bounds.min.y - 22.0);
            ui.painter().circle_filled(rotate, 5.0, Color32::YELLOW);
        }
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
}

fn handle_drop(app: &mut EditorApp, ui: &mut Ui, rect: Rect, pointer: Option<Pos2>) {
    if !ui.input(|input| input.pointer.any_released()) {
        return;
    }
    let Some(pointer) = pointer.filter(|point| rect.contains(*point)) else {
        return;
    };
    let Some(payload) = DragAndDrop::take_payload::<String>(ui.ctx()) else {
        return;
    };
    let Some(asset) = payload.strip_prefix(ASSET_PAYLOAD_PREFIX) else {
        return;
    };
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
        } else if let Some((id, kind)) = handle_at(items, pointer) {
            let transform = app
                .document
                .scene()
                .entities
                .iter()
                .find(|entity| entity.id == id)
                .map(|entity| entity.transform);
            if let Some(transform) = transform {
                let center = Pos2::new(transform.translation[0], transform.translation[1]);
                let center = world_to_screen([center.x, center.y], rect, view);
                app.drag = match kind {
                    HandleKind::Scale => CanvasDrag::Scale {
                        id,
                        pointer,
                        center,
                        original: transform,
                    },
                    HandleKind::Rotate => CanvasDrag::Rotate {
                        id,
                        pointer,
                        center,
                        original: transform,
                    },
                };
            }
        } else if let Some(id) = entity_at(app, pointer, view, rect) {
            let (ids, update_selection) = {
                let selection = app.document.selection();
                if selection.contains(&id) {
                    (selection.to_vec(), false)
                } else {
                    (vec![id], true)
                }
            };
            if update_selection {
                app.document.select(&ids);
            }
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
            id,
            pointer: start,
            center,
            original,
        } => {
            if let Some(pointer) = pointer {
                let start_distance = (start - center).length().max(1.0);
                let factor = ((pointer - center).length() / start_distance).clamp(0.1, 10.0);
                let mut transform = original;
                for value in &mut transform.scale {
                    *value = (*value * factor).abs().max(0.01);
                }
                app.preview.insert(id, transform);
            }
        }
        CanvasDrag::Rotate {
            id,
            pointer: start,
            center,
            original,
        } => {
            if let Some(pointer) = pointer {
                let start_angle = (start.y - center.y).atan2(start.x - center.x);
                let angle = (pointer.y - center.y).atan2(pointer.x - center.x);
                let mut transform = original;
                transform.rotation = quaternion_from_angle(
                    angle_from_quaternion(original.rotation) - (angle - start_angle),
                );
                app.preview.insert(id, transform);
            }
        }
    }

    if response.clicked() && !secondary {
        let Some(pointer) = pointer else {
            return;
        };
        let additive = ui.input(|input| input.modifiers.shift || input.modifiers.ctrl);
        if let Some(id) = entity_at(app, pointer, view, rect) {
            let mut selection = if additive {
                app.document.selection().to_vec()
            } else {
                Vec::new()
            };
            if additive && selection.contains(&id) {
                selection.retain(|selected| *selected != id);
            } else if !selection.contains(&id) {
                selection.push(id);
            }
            app.document.select(&selection);
        } else if !additive {
            app.document.select(&[]);
        }
    }
}

fn commit_drag(app: &mut EditorApp, ui: &mut Ui) {
    if !ui.input(|input| input.pointer.any_released()) {
        return;
    }
    let previews: Vec<(u64, Transform)> = app.preview.drain().collect();
    for (id, transform) in previews {
        let current = app
            .document
            .scene()
            .entities
            .iter()
            .find(|entity| entity.id == id)
            .map(|entity| entity.transform);
        if current != Some(transform) {
            app.apply(lunar_editor::EditorCommand::SetTransform { id, transform });
        }
    }
    app.drag = CanvasDrag::None;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandleKind {
    Scale,
    Rotate,
}

fn handle_at(items: &[DrawItem], pointer: Pos2) -> Option<(u64, HandleKind)> {
    let selected: Vec<_> = items.iter().filter(|item| item.selected).collect();
    if selected.len() != 1 {
        return None;
    }
    let item = selected[0];
    let bounds = Rect::from_points(&sprite_corners(item));
    let scale = bounds.max + Vec2::new(6.0, 6.0);
    if (pointer - scale).length() <= 8.0 {
        return Some((item.id, HandleKind::Scale));
    }
    let rotate = Pos2::new(bounds.center().x, bounds.min.y - 22.0);
    if (pointer - rotate).length() <= 8.0 {
        return Some((item.id, HandleKind::Rotate));
    }
    None
}

fn entity_at(app: &EditorApp, pointer: Pos2, view: View, rect: Rect) -> Option<u64> {
    let world = screen_to_world(pointer, rect, view);
    for entity in app.document.scene().entities.iter().rev() {
        let image = entity.components.iter().find_map(Component::asset_path);
        let Some(asset) = image else {
            continue;
        };
        let Some(dimensions) = app.assets.dimensions(asset) else {
            continue;
        };
        let transform = app
            .preview
            .get(&entity.id)
            .copied()
            .unwrap_or(entity.transform);
        let size = image_world_size(dimensions, transform.scale);
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
    fn visible_world_matches_view_size() {
        let view = View {
            center: [0.0, 0.0],
            zoom: 10.0,
        };
        assert_eq!(visible_world(rect(), view), [-40.0, -30.0, 40.0, 30.0]);
    }
}
