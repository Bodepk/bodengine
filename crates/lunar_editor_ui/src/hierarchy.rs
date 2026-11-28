use egui::{Button, Color32, DragAndDrop, Sense, Ui, Vec2};
use lunar_editor::EditorCommand;

use crate::app::EditorApp;
use crate::assets::ENTITY_PAYLOAD_PREFIX;
use crate::file_picker::PickerMode;
use crate::helpers::{child_ids, root_ids};
use crate::tools::EntityTemplate;

pub(crate) fn toolbar_ui(app: &mut EditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.heading("Lunar Editor");
        ui.separator();
        if ui.button("Nuevo").clicked() {
            app.request_new();
        }
        ui.label("Ruta:");
        ui.add(
            egui::TextEdit::singleline(&mut app.path_text)
                .hint_text("scene.json")
                .desired_width(260.0),
        );
        if ui.button("Buscar…").clicked() {
            app.begin_picker(PickerMode::Open);
        }
        if ui.button("Abrir").clicked() {
            app.request_open();
        }
        if ui.button("Guardar").clicked() {
            app.save();
        }
        if ui.button("Guardar como").clicked() {
            app.begin_save_as();
        }
        let recent = app.recent.clone();
        egui::ComboBox::from_label("Recientes")
            .selected_text(if recent.is_empty() { "—" } else { "Abrir…" })
            .show_ui(ui, |ui| {
                if recent.is_empty() {
                    ui.label("Sin escenas recientes");
                }
                for path in recent {
                    if ui.selectable_label(false, path.clone()).clicked() {
                        app.open_recent(path);
                    }
                }
            });
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
        if ui.button("Historial").clicked() {
            app.show_history = !app.show_history;
        }
        if ui.button("Validar").clicked() {
            app.show_validation = !app.show_validation;
            app.run_validation();
        }
        if ui.button("Entorno").clicked() {
            app.show_environment = !app.show_environment;
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
    let has_selection = !app.document.selection().is_empty();
    ui.horizontal(|ui| {
        ui.menu_button("+ Crear", |ui| {
            ui.horizontal(|ui| {
                ui.label("Instancias:");
                ui.add(
                    egui::DragValue::new(&mut app.prefab_instance_count)
                        .range(1..=crate::prefabs::MAX_INSTANCE_COUNT),
                );
            });
            ui.separator();
            for template in EntityTemplate::ALL {
                if ui.button(template.label()).clicked() {
                    app.create_from_template(template);
                    ui.close();
                }
            }
            let prefabs = crate::prefabs::prefab_names(app.document.scene());
            if !prefabs.is_empty() {
                ui.separator();
                for name in prefabs {
                    let size = crate::prefabs::prefab_entity_count(app.document.scene(), &name);
                    ui.horizontal(|ui| {
                        if ui.button(format!("Instanciar: {name} ({size})")).clicked() {
                            app.instantiate_prefab(&name);
                            ui.close();
                        }
                        if ui.button("Aplicar").clicked() {
                            app.apply_prefab(&name);
                            ui.close();
                        }
                        if ui.button("Actualizar").clicked() {
                            app.update_prefab(&name);
                            ui.close();
                        }
                        if ui.button("×").clicked() {
                            app.delete_prefab(&name);
                            ui.close();
                        }
                    });
                }
            }
        });
        let can_paste = app.has_clipboard();
        ui.menu_button("Editar", |ui| {
            if ui
                .add_enabled(has_selection, Button::new("Duplicar (Ctrl+D)"))
                .clicked()
            {
                app.duplicate_selected();
                ui.close();
            }
            if ui
                .add_enabled(has_selection, Button::new("Copiar (Ctrl+C)"))
                .clicked()
            {
                app.copy_selected();
                ui.close();
            }
            if ui
                .add_enabled(has_selection, Button::new("Cortar (Ctrl+X)"))
                .clicked()
            {
                app.cut_selected();
                ui.close();
            }
            if ui
                .add_enabled(can_paste, Button::new("Pegar (Ctrl+V)"))
                .clicked()
            {
                app.paste_clipboard();
                ui.close();
            }
        });
        if ui
            .add_enabled(has_selection, Button::new("Borrar"))
            .clicked()
        {
            app.delete_selected();
        }
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.prefab_name_buffer)
                .hint_text("Nombre del prefab")
                .desired_width(110.0),
        );
        if ui
            .add_enabled(
                has_selection && !app.prefab_name_buffer.trim().is_empty(),
                Button::new("Guardar prefab"),
            )
            .clicked()
        {
            let name = app.prefab_name_buffer.clone();
            app.save_prefab(&name);
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
        let (ctrl, shift) = ui.input(|input| (input.modifiers.ctrl, input.modifiers.shift));
        app.select_clicked(id, ctrl, shift);
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
    let payload = DragAndDrop::payload::<String>(ui.ctx())?;
    let source = payload.strip_prefix(ENTITY_PAYLOAD_PREFIX)?;
    let id = source.parse::<u64>().ok()?;
    DragAndDrop::clear_payload(ui.ctx());
    Some(id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Rect};
    use lunar_editor::EditorDocument;

    #[test]
    fn the_hierarchy_exposes_the_edit_menu() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.document = EditorDocument::new();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 700.0))),
            ..Default::default()
        };
        let mut first = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Jerarquía").show(ctx, |ui| hierarchy_ui(&mut app, ui));
        });
        first.textures_delta.clear();
        let mut output = ctx.run_pass(input, |ctx| {
            egui::Window::new("Jerarquía").show(ctx, |ui| hierarchy_ui(&mut app, ui));
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
        assert!(texts.iter().any(|text| text == "Editar"), "{texts:?}");
        assert!(
            texts.iter().any(|text| text == "Guardar prefab"),
            "{texts:?}"
        );
    }
}
