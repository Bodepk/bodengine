use egui::{Button, Color32, DragAndDrop, Sense, Ui, Vec2};
use lunar_editor::EditorCommand;

use crate::app::EditorApp;
use crate::assets::ENTITY_PAYLOAD_PREFIX;
use crate::helpers::{child_ids, next_entity_id, root_ids};

pub(crate) fn toolbar_ui(app: &mut EditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.heading("Lunar Editor");
        ui.separator();
        if ui.button("Nuevo").clicked() {
            app.new_document();
        }
        ui.label("Ruta:");
        ui.add(
            egui::TextEdit::singleline(&mut app.path_text)
                .hint_text("scene.json")
                .desired_width(260.0),
        );
        if ui.button("Abrir").clicked() {
            app.open_path();
        }
        if ui.button("Guardar").clicked() {
            app.save();
        }
        ui.separator();
        if ui
            .add_enabled(app.document.can_undo(), Button::new("Deshacer"))
            .clicked()
        {
            app.undo();
        }
        if ui
            .add_enabled(app.document.can_redo(), Button::new("Rehacer"))
            .clicked()
        {
            app.redo();
        }
        if ui.button("Duplicar (Ctrl+D)").clicked() {
            app.duplicate_selected();
        }
        ui.separator();
        ui.label(format!(
            "{} entidad(es)",
            app.document.scene().entities.len()
        ));
        if app.document.is_dirty() {
            ui.colored_label(Color32::YELLOW, "● sin guardar");
        }
    });
}

pub(crate) fn hierarchy_ui(app: &mut EditorApp, ui: &mut Ui) {
    ui.heading("Jerarquía");
    ui.horizontal(|ui| {
        if ui.button("+ Entidad").clicked() {
            let id = next_entity_id(app.document.scene());
            app.apply(EditorCommand::CreateEntity {
                id,
                name: Some(format!("Entidad {id}")),
            });
        }
        let has_selection = !app.document.selection().is_empty();
        if ui
            .add_enabled(has_selection, Button::new("Borrar"))
            .clicked()
        {
            app.delete_selected();
        }
    });
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        for id in root_ids(app.document.scene()) {
            entity_row(app, ui, id, 0);
        }
        let (root_rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::hover());
        handle_root_drop(app, ui, root_rect);
    });
}

fn entity_row(app: &mut EditorApp, ui: &mut Ui, id: u64, depth: usize) {
    let Some(entity) = app
        .document
        .scene()
        .entities
        .iter()
        .find(|entity| entity.id == id)
        .cloned()
    else {
        return;
    };
    let selected = app.document.selection().contains(&id);
    let indent = "    ".repeat(depth);
    let label = format!(
        "{indent}{}  #{id}",
        entity.name.unwrap_or_else(|| "—".to_owned())
    );
    let response = ui.selectable_label(selected, label);

    if response.clicked() {
        let additive = ui.input(|input| input.modifiers.shift || input.modifiers.ctrl);
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
    }
    if response.drag_started() {
        DragAndDrop::set_payload(ui.ctx(), format!("{ENTITY_PAYLOAD_PREFIX}{id}"));
    }
    handle_entity_drop(app, ui, response.rect, id);

    for child in child_ids(app.document.scene(), id) {
        entity_row(app, ui, child, depth + 1);
    }
}

fn payload_source(ui: &Ui) -> Option<u64> {
    let payload = DragAndDrop::take_payload::<String>(ui.ctx())?;
    let source = payload.strip_prefix(ENTITY_PAYLOAD_PREFIX)?;
    source.parse::<u64>().ok()
}

fn handle_entity_drop(app: &mut EditorApp, ui: &Ui, rect: egui::Rect, target: u64) {
    if !ui.input(|input| input.pointer.any_released()) {
        return;
    }
    let Some(pointer) = ui.input(|input| input.pointer.latest_pos()) else {
        return;
    };
    if !rect.contains(pointer) {
        return;
    }
    let Some(source) = payload_source(ui) else {
        return;
    };
    if source == target {
        return;
    }
    let relative = (pointer.y - rect.min.y) / rect.height();
    if relative < 0.25 || relative > 0.75 {
        let index = app
            .document
            .scene()
            .entities
            .iter()
            .position(|entity| entity.id == target)
            .unwrap_or(0);
        let destination = if relative < 0.25 { index } else { index + 1 };
        app.apply(EditorCommand::MoveEntity {
            id: source,
            to: destination,
        });
    } else {
        app.apply(EditorCommand::SetParent {
            id: source,
            parent: Some(target),
        });
    }
}

fn handle_root_drop(app: &mut EditorApp, ui: &Ui, rect: egui::Rect) {
    if !ui.input(|input| input.pointer.any_released()) {
        return;
    }
    let Some(pointer) = ui.input(|input| input.pointer.latest_pos()) else {
        return;
    };
    if !rect.contains(pointer) {
        return;
    }
    let Some(source) = payload_source(ui) else {
        return;
    };
    app.apply(EditorCommand::SetParent {
        id: source,
        parent: None,
    });
}
