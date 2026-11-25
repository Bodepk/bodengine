use egui::{Button, Color32, Ui};
use lunar_editor::EditorCommand;

use crate::app::EditorApp;
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
    if ui.selectable_label(selected, label).clicked() {
        app.document.select(&[id]);
    }
    for child in child_ids(app.document.scene(), id) {
        entity_row(app, ui, child, depth + 1);
    }
}
