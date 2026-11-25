use egui::Ui;
use lunar_editor::EditorCommand;
use lunar_scenes::Transform;

use crate::app::EditorApp;
use crate::helpers::{
    audio_component, collider2_component, component_label, drag_array, image_component,
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

    ui.heading(format!("Inspector · #{id}"));
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
    changed |= drag_array(ui, "Rotación", &mut app.rotation, 0.01);
    changed |= drag_array(ui, "Escala", &mut app.scale, 0.05);
    if changed {
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
        let clicked = ui
            .horizontal(|ui| {
                ui.label(component_label(component));
                ui.button("×").clicked()
            })
            .inner;
        if clicked {
            remove_index = Some(index);
            break;
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
    });
    ui.horizontal(|ui| {
        if ui.button("+ Audio").clicked() {
            app.apply(EditorCommand::AddComponent {
                id,
                component: audio_component(&app.asset_buffer, app.volume, app.autoplay),
            });
        }
        if ui.button("+ Col2D").clicked() {
            app.apply(EditorCommand::AddComponent {
                id,
                component: collider2_component(),
            });
        }
    });
    ui.add(egui::Slider::new(&mut app.volume, 0.0..=1.0).text("Volumen"));
    ui.checkbox(&mut app.autoplay, "Autoplay");
}
