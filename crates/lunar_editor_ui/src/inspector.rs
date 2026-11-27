use egui::Ui;
use lunar_editor::EditorCommand;
use lunar_scenes::Transform;

use crate::app::{ComponentEdit, EditorApp};
use crate::components::component_editor_ui;
use crate::helpers::{
    audio_component, collider2_component, component_label, drag_array, image_component,
    prefab_component, quaternion_from_degrees, sprite_component,
};

pub(crate) fn inspector_ui(app: &mut EditorApp, ui: &mut Ui) {
    let Some(id) = app.document.selection().first().copied() else {
        ui.heading("Inspector");
        ui.label("Selecciona una entidad en la jerarquía.");
        return;
    };
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

    let selection_count = app.document.selection().len();
    ui.heading(format!("Inspector · #{id}"));
    if selection_count > 1 {
        ui.label(format!(
            "{selection_count} entidades seleccionadas · los cambios de este panel afectan a #{id}"
        ));
    }
    ui.label("Nombre");
    let name_response =
        ui.add(egui::TextEdit::singleline(&mut app.name_buffer).hint_text("Sin nombre"));
    let enter = name_response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    if name_response.lost_focus() || enter {
        let current = entity.name.clone().unwrap_or_default();
        if app.name_buffer != current {
            let name = app.name_buffer.trim();
            app.apply(EditorCommand::RenameEntity {
                id,
                name: (!name.is_empty()).then(|| name.to_owned()),
            });
        }
    }

    ui.separator();
    ui.label("Transformación local");
    let mut changed = drag_array(ui, "Posición", &mut app.translation, 0.05);
    changed |= drag_array(ui, "Rotación (°)", &mut app.rotation_degrees, 1.0);
    changed |= drag_array(ui, "Escala", &mut app.scale, 0.05);
    if changed {
        app.rotation = quaternion_from_degrees(app.rotation_degrees);
        app.apply(EditorCommand::SetTransform {
            id,
            transform: Transform {
                translation: app.translation,
                rotation: app.rotation,
                scale: app.scale,
            },
        });
    }

    ui.separator();
    let parent_label = entity
        .parent
        .map_or_else(|| "Ninguno".to_owned(), |parent| format!("#{parent}"));
    let candidates: Vec<(u64, String)> = app
        .document
        .scene()
        .entities
        .iter()
        .filter(|other| other.id != id)
        .map(|other| {
            (
                other.id,
                format!(
                    "{}  #{}",
                    other.name.clone().unwrap_or_else(|| "—".to_owned()),
                    other.id
                ),
            )
        })
        .collect();
    egui::ComboBox::from_label("Padre")
        .selected_text(parent_label)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(entity.parent.is_none(), "Ninguno")
                .clicked()
            {
                app.apply(EditorCommand::SetParent { id, parent: None });
            }
            for (other_id, label) in candidates {
                if ui
                    .selectable_label(entity.parent == Some(other_id), label)
                    .clicked()
                {
                    app.apply(EditorCommand::SetParent {
                        id,
                        parent: Some(other_id),
                    });
                }
            }
        });

    ui.separator();
    ui.heading("Componentes");
    let mut remove_index = None;
    for (index, component) in entity.components.iter().enumerate() {
        let active = app
            .component_edit
            .as_ref()
            .is_some_and(|edit| edit.id == id && edit.index == index);
        let (select, remove) = ui
            .horizontal(|ui| {
                (
                    ui.selectable_label(active, component_label(component))
                        .clicked(),
                    ui.button("×").clicked(),
                )
            })
            .inner;
        if select {
            app.component_edit = Some(ComponentEdit {
                id,
                index,
                component: component.clone(),
            });
        }
        if remove {
            remove_index = Some(index);
        }
    }
    if let Some(index) = remove_index {
        app.apply(EditorCommand::RemoveComponent { id, index });
    }

    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.asset_buffer)
                .hint_text("assets/sprite.png")
                .desired_width(150.0),
        );
        if ui.button("+ Imagen").clicked() {
            app.apply(EditorCommand::AddComponent {
                id,
                component: image_component(&app.asset_buffer),
            });
        }
        if ui.button("+ Sprite").clicked() {
            app.apply(EditorCommand::AddComponent {
                id,
                component: sprite_component(&app.asset_buffer),
            });
        }
    });
    ui.horizontal(|ui| {
        if ui.button("+ Audio").clicked() {
            app.apply(EditorCommand::AddComponent {
                id,
                component: audio_component(&app.asset_buffer, 0.5, false),
            });
        }
        if ui.button("+ Col2D").clicked() {
            app.apply(EditorCommand::AddComponent {
                id,
                component: collider2_component(),
            });
        }
        if ui.button("+ Prefab").clicked() {
            match app.document.scene().prefabs.first() {
                Some(prefab) => {
                    let component = prefab_component(&prefab.name);
                    app.apply(EditorCommand::AddComponent { id, component });
                }
                None => app.set_status("No hay prefabs en la escena"),
            }
        }
    });

    ui.separator();
    ui.heading("Comportamiento");
    let current = app.script_map.get(id).cloned();
    let choice = current.as_ref().map_or_else(
        || "Ninguno".to_owned(),
        |binding| binding.behavior.name().to_owned(),
    );
    egui::ComboBox::from_label("Script")
        .selected_text(&choice)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(choice == "Ninguno", "Ninguno")
                .clicked()
            {
                app.apply_script_binding(id, None);
            }
            for behavior in crate::scripts::Behavior::ALL {
                let label = behavior.name();
                if ui.selectable_label(choice == label, label).clicked() {
                    app.apply_script_binding(id, Some(crate::scripts::Binding::new(behavior)));
                }
            }
        });
    if let Some(binding) = current {
        let mut params = binding.params.clone();
        let behavior = binding.behavior;
        for (key, _, _) in behavior.schema() {
            let Some(value) = params.get_mut(*key) else {
                continue;
            };
            let response = ui.add(
                egui::TextEdit::singleline(value)
                    .desired_width(90.0)
                    .hint_text(*key),
            );
            let enter =
                response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if response.lost_focus() || enter {
                let edited = value.clone();
                if crate::scripts::valid_param(behavior, key, &edited) {
                    let mut next = binding.clone();
                    next.params.insert((*key).to_owned(), edited);
                    app.apply_script_binding(id, Some(next));
                } else {
                    app.set_status(format!("Valor no válido para {key}: {edited}"));
                }
            }
        }
        ui.label(format!(
            "Enlace de {} guardado en scripts.map.txt al guardar la escena",
            behavior
        ));
    } else {
        ui.label("Sin comportamiento: elige uno para enlazarlo a esta entidad.");
    }

    ui.separator();
    component_editor_ui(app, ui);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Rect};

    #[test]
    fn multi_selection_shows_which_entity_the_inspector_edits() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        for id in 1..=2 {
            app.document
                .apply(EditorCommand::CreateEntity { id, name: None })
                .expect("create entity");
        }
        app.select_clicked(1, false, false);
        app.select_clicked(2, true, false);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 700.0))),
            ..Default::default()
        };
        let mut first = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Inspector").show(ctx, |ui| inspector_ui(&mut app, ui));
        });
        first.textures_delta.clear();
        let mut second = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Inspector").show(ctx, |ui| inspector_ui(&mut app, ui));
        });
        second.textures_delta.clear();
        let mut output = ctx.run_pass(input, |ctx| {
            egui::Window::new("Inspector").show(ctx, |ui| inspector_ui(&mut app, ui));
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
            texts
                .iter()
                .any(|text| text.contains("2 entidades seleccionadas")),
            "{texts:?}"
        );
    }

    #[test]
    fn inspector_shows_the_behavior_selector_and_its_parameters() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.document
            .apply(EditorCommand::CreateEntity { id: 1, name: None })
            .unwrap();
        app.apply_script_binding(
            1,
            Some(crate::scripts::Binding::new(
                crate::scripts::Behavior::Patrol,
            )),
        );
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 700.0))),
            ..Default::default()
        };
        let mut first = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Inspector").show(ctx, |ui| inspector_ui(&mut app, ui));
        });
        first.textures_delta.clear();
        let mut second = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Inspector").show(ctx, |ui| inspector_ui(&mut app, ui));
        });
        second.textures_delta.clear();
        let mut output = ctx.run_pass(input, |ctx| {
            egui::Window::new("Inspector").show(ctx, |ui| inspector_ui(&mut app, ui));
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
            texts.iter().any(|text| text == "Comportamiento"),
            "{texts:?}"
        );
        assert!(texts.iter().any(|text| text == "Patrol"), "{texts:?}");
        assert!(
            texts.iter().any(|text| text.contains("scripts.map.txt")),
            "{texts:?}"
        );
    }
}
