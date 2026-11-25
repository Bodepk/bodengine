use egui::{Button, Color32, Pos2, Rect, Stroke, Ui, Vec2};

use crate::app::EditorApp;

pub(crate) fn graph_ui(app: &mut EditorApp, ui: &mut Ui) {
    ui.heading("Vista de escena");
    ui.label("Diagrama de jerarquía. Selecciona un nodo para editarlo a la derecha.");
    ui.separator();

    let scene = app.document.scene();
    if scene.entities.is_empty() {
        ui.label("La escena está vacía. Usa «+ Entidad» para empezar.");
        return;
    }

    let selection = app.document.selection().to_vec();
    let candidates: Vec<(u64, String, Option<u64>)> = scene
        .entities
        .iter()
        .map(|entity| {
            (
                entity.id,
                entity.name.clone().unwrap_or_else(|| "—".to_owned()),
                entity.parent,
            )
        })
        .collect();
    let columns = ((ui.available_width() / 190.0).floor() as usize).clamp(1, 6);
    let origin = ui.min_rect().min + Vec2::new(0.0, 58.0);
    let node_size = Vec2::new(160.0, 46.0);
    let mut positions: Vec<(u64, Pos2, String)> = Vec::with_capacity(candidates.len());
    for (index, (id, name, _)) in candidates.iter().enumerate() {
        let position = origin
            + Vec2::new(
                24.0 + (index % columns) as f32 * 190.0,
                18.0 + (index / columns) as f32 * 76.0,
            );
        positions.push((*id, position, name.clone()));
    }

    for (id, _, parent) in &candidates {
        let Some(parent) = parent else {
            continue;
        };
        let Some((_, from, _)) = positions
            .iter()
            .find(|(parent_id, _, _)| parent_id == parent)
        else {
            continue;
        };
        let Some((_, to, _)) = positions.iter().find(|(child_id, _, _)| child_id == id) else {
            continue;
        };
        ui.painter().line_segment(
            [*from + Vec2::new(80.0, 46.0), *to + Vec2::new(80.0, 0.0)],
            Stroke::new(1.5, Color32::GRAY),
        );
    }

    let mut clicked = None;
    for (id, position, name) in &positions {
        let rect = Rect::from_min_size(*position, node_size);
        let fill = if selection.contains(id) {
            Color32::from_rgb(38, 96, 150)
        } else {
            Color32::from_gray(45)
        };
        if ui
            .put(rect, Button::new(format!("{name}  #{id}")).fill(fill))
            .clicked()
        {
            clicked = Some(*id);
        }
    }
    if let Some(id) = clicked {
        app.document.select(&[id]);
    }
}
