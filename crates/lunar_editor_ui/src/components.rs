//! Inspector de componentes v2 (ADR-030).
//!
//! La GUI edita una copia del componente activo y confirma cada cambio con
//! `SetComponent`, así que la validación y el undo siguen en `lunar_editor`.

use egui::Ui;
use lunar_editor::EditorCommand;
use lunar_scenes::{AnimationClip, BodyType, Component, Shape2D, Shape3D};

use crate::app::EditorApp;
use crate::helpers::drag_array;

/// Dibuja el editor del componente activo y confirma los cambios.
pub(crate) fn component_editor_ui(app: &mut EditorApp, ui: &mut Ui) {
    let Some(mut edit) = app.component_edit.take() else {
        ui.label("Selecciona un componente para editarlo.");
        return;
    };

    let prefabs: Vec<String> = app
        .document
        .scene()
        .prefabs
        .iter()
        .map(|prefab| prefab.name.clone())
        .collect();
    let mut changed = false;
    fields_ui(&mut edit.component, ui, &prefabs, &mut changed);

    app.component_edit = Some(edit.clone());
    if changed {
        app.apply(EditorCommand::SetComponent {
            id: edit.id,
            index: edit.index,
            component: edit.component,
        });
    }
}

fn fields_ui(component: &mut Component, ui: &mut Ui, prefabs: &[String], changed: &mut bool) {
    match component {
        Component::Mesh { asset, primitive } => {
            asset_ui(ui, asset, changed);
            ui.horizontal(|ui| {
                let mut enabled = primitive.is_some();
                if ui.checkbox(&mut enabled, "Primitiva").changed() {
                    *primitive = enabled.then_some(0);
                    *changed = true;
                }
                if let Some(value) = primitive
                    && ui.add(egui::DragValue::new(value).prefix("#")).changed()
                {
                    *changed = true;
                }
            });
        }
        Component::Image { asset } => asset_ui(ui, asset, changed),
        Component::Sprite {
            asset,
            meta,
            animations,
        } => {
            asset_ui(ui, asset, changed);
            ui.label("Hoja de sprites");
            *changed |= u32_ui(ui, "Ancho", &mut meta.frame_width);
            *changed |= u32_ui(ui, "Alto", &mut meta.frame_height);
            *changed |= u32_ui(ui, "Columnas", &mut meta.columns);
            *changed |= u32_ui(ui, "Filas", &mut meta.rows);
            *changed |= f32_ui(ui, "FPS", &mut meta.fps, 0.1);
            if ui.checkbox(&mut meta.looping, "Bucle").changed() {
                *changed = true;
            }

            ui.separator();
            ui.label("Animaciones");
            let mut remove_clip = None;
            for (clip_index, clip) in animations.iter_mut().enumerate() {
                clip_header(ui, clip_index, clip.name.clone(), |ui| {
                    text_ui(ui, "Nombre", &mut clip.name, changed);
                    *changed |= f32_ui(ui, "FPS", &mut clip.fps, 0.1);
                    if ui.checkbox(&mut clip.looping, "Bucle").changed() {
                        *changed = true;
                    }
                    frames_ui(ui, &mut clip.frames, changed);
                    if ui.button("Eliminar clip").clicked() {
                        remove_clip = Some(clip_index);
                    }
                });
            }
            if let Some(index) = remove_clip {
                animations.remove(index);
                *changed = true;
            }
            if ui.button("+ Clip").clicked() {
                animations.push(AnimationClip {
                    name: next_clip_name(animations),
                    frames: vec![0],
                    fps: 8.0,
                    looping: true,
                });
                *changed = true;
            }
        }
        Component::Audio {
            asset,
            volume,
            autoplay,
        } => {
            asset_ui(ui, asset, changed);
            if ui
                .add(egui::Slider::new(volume, 0.0..=1.0).text("Volumen"))
                .changed()
            {
                *changed = true;
            }
            if ui.checkbox(autoplay, "Autoplay").changed() {
                *changed = true;
            }
        }
        Component::Collider2D { shape, body } => {
            body_ui(ui, body, changed);
            shape2_ui(ui, shape, changed);
        }
        Component::Collider3D { shape, body } => {
            body_ui(ui, body, changed);
            shape3_ui(ui, shape, changed);
        }
        Component::PrefabRef { name } => {
            if prefabs.is_empty() {
                text_ui(ui, "Prefab", name, changed);
            } else {
                egui::ComboBox::from_label("Prefab")
                    .selected_text(name.as_str())
                    .show_ui(ui, |ui| {
                        for candidate in prefabs {
                            if ui
                                .selectable_label(name.as_str() == candidate.as_str(), candidate)
                                .clicked()
                            {
                                *name = candidate.clone();
                                *changed = true;
                            }
                        }
                    });
            }
        }
    }
}

/// Cabecera de un clip con id estable: el título incluye el nombre, que cambia
/// mientras se escribe, y egui lo usaría como id si no se indicara `id_salt`.
fn clip_header(ui: &mut Ui, index: usize, name: String, body: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(format!("Clip {} · {name}", index + 1))
        .id_salt(clip_id_salt(index))
        .show(ui, body);
}

fn clip_id_salt(index: usize) -> String {
    format!("lunar-editor-clip-{index}")
}

fn asset_ui(ui: &mut Ui, asset: &mut String, changed: &mut bool) {
    let response = text_ui(ui, "Asset", asset, changed);
    if crate::dnd::has_asset(ui.ctx()) {
        ui.painter().rect_filled(
            response.rect,
            2.0,
            egui::Color32::from_rgba_unmultiplied(90, 170, 255, 48),
        );
    }
    let pointer = ui.input(|input| input.pointer.latest_pos());
    let released_inside = ui.input(|input| input.pointer.any_released())
        && pointer.is_some_and(|point| response.rect.contains(point));
    if released_inside && let Some(drag) = crate::dnd::take_asset(ui.ctx()) {
        *asset = drag.path;
        *changed = true;
    }
    response.on_hover_text("Arrastra un asset desde el panel de Assets");
}

fn text_ui(ui: &mut Ui, label: &str, value: &mut String, changed: &mut bool) -> egui::Response {
    ui.label(label);
    let before = value.clone();
    let response = ui.add(egui::TextEdit::singleline(value).desired_width(240.0));
    let enter = response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    if (response.lost_focus() || enter) && *value != before {
        *changed = true;
    }
    response
}

fn u32_ui(ui: &mut Ui, label: &str, value: &mut u32) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(1.0)).changed()
    })
    .inner
}

fn f32_ui(ui: &mut Ui, label: &str, value: &mut f32, speed: f32) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(speed)).changed()
    })
    .inner
}

fn frames_ui(ui: &mut Ui, frames: &mut Vec<u32>, changed: &mut bool) {
    ui.label("Fotogramas");
    ui.horizontal(|ui| {
        for (index, frame) in frames.iter_mut().enumerate() {
            *changed |= u32_ui(ui, &format!("#{index}"), frame);
        }
        if ui.button("+").clicked() {
            frames.push(frames.len() as u32);
            *changed = true;
        }
    });
}

fn body_ui(ui: &mut Ui, body: &mut BodyType, changed: &mut bool) {
    ui.horizontal(|ui| {
        for (label, candidate) in [
            ("Dinámico", BodyType::Dynamic),
            ("Fijo", BodyType::Fixed),
            ("Sensor", BodyType::Sensor),
        ] {
            if ui.selectable_label(*body == candidate, label).clicked() {
                *body = candidate;
                *changed = true;
            }
        }
    });
}

fn shape2_ui(ui: &mut Ui, shape: &mut Shape2D, changed: &mut bool) {
    ui.horizontal(|ui| {
        if ui
            .selectable_label(matches!(shape, Shape2D::Ball { .. }), "Bola")
            .clicked()
        {
            *shape = Shape2D::Ball { radius: 0.5 };
            *changed = true;
        }
        if ui
            .selectable_label(matches!(shape, Shape2D::Cuboid { .. }), "Caja")
            .clicked()
        {
            *shape = Shape2D::Cuboid {
                half_extents: [0.5; 2],
            };
            *changed = true;
        }
    });
    match shape {
        Shape2D::Ball { radius } => *changed |= f32_ui(ui, "Radio", radius, 0.05),
        Shape2D::Cuboid { half_extents } => {
            *changed |= drag_array(ui, "Mitades", half_extents, 0.05);
        }
    }
}

fn shape3_ui(ui: &mut Ui, shape: &mut Shape3D, changed: &mut bool) {
    ui.horizontal(|ui| {
        if ui
            .selectable_label(matches!(shape, Shape3D::Ball { .. }), "Bola")
            .clicked()
        {
            *shape = Shape3D::Ball { radius: 0.5 };
            *changed = true;
        }
        if ui
            .selectable_label(matches!(shape, Shape3D::Cuboid { .. }), "Caja")
            .clicked()
        {
            *shape = Shape3D::Cuboid {
                half_extents: [0.5; 3],
            };
            *changed = true;
        }
        if ui
            .selectable_label(matches!(shape, Shape3D::Capsule { .. }), "Cápsula")
            .clicked()
        {
            *shape = Shape3D::Capsule {
                half_height: 0.5,
                radius: 0.25,
            };
            *changed = true;
        }
    });
    match shape {
        Shape3D::Ball { radius } => *changed |= f32_ui(ui, "Radio", radius, 0.05),
        Shape3D::Cuboid { half_extents } => {
            *changed |= drag_array(ui, "Mitades", half_extents, 0.05);
        }
        Shape3D::Capsule {
            half_height,
            radius,
        } => {
            *changed |= f32_ui(ui, "Media altura", half_height, 0.05);
            *changed |= f32_ui(ui, "Radio", radius, 0.05);
        }
    }
}

/// Nombre único para un clip nuevo, sin depender del orden del vector.
fn next_clip_name(animations: &[AnimationClip]) -> String {
    for index in 1.. {
        let name = format!("clip{index}");
        if animations.iter().all(|clip| clip.name != name) {
            return name;
        }
    }
    unreachable!("la secuencia de nombres es infinita")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(name: &str) -> AnimationClip {
        AnimationClip {
            name: name.to_owned(),
            frames: vec![0],
            fps: 8.0,
            looping: true,
        }
    }

    #[test]
    fn clip_header_stays_open_while_its_name_changes() {
        use egui::collapsing_header::CollapsingState;

        let ctx = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(400.0, 300.0),
            )),
            ..Default::default()
        };
        let mut header_id = None;

        let mut first = ctx.run_pass(input(), |ctx| {
            egui::Window::new("clips").show(ctx, |ui| {
                ui.vertical(|ui| {
                    let id = ui.make_persistent_id(clip_id_salt(0));
                    header_id = Some(id);
                    let mut state = CollapsingState::load_with_default_open(ctx, id, false);
                    state.set_open(true);
                    state.store(ctx);
                    clip_header(ui, 0, "clip1".to_owned(), |_| {});
                });
            });
        });
        first.textures_delta.clear();
        let id = header_id.expect("la cabecera tiene id");
        assert!(
            CollapsingState::load(&ctx, id).is_some_and(|state| state.is_open()),
            "la primera cabecera debe empezar abierta"
        );

        let mut second = ctx.run_pass(input(), |ctx| {
            egui::Window::new("clips").show(ctx, |ui| {
                clip_header(ui, 0, "clip2".to_owned(), |_| {});
            });
        });
        second.textures_delta.clear();
        assert!(
            CollapsingState::load(&ctx, id).is_some_and(|state| state.is_open()),
            "renombrar un clip no debe cerrar su cabecera"
        );
    }

    #[test]
    fn next_clip_name_avoids_duplicates() {
        assert_eq!(next_clip_name(&[]), "clip1");
        assert_eq!(next_clip_name(&[clip("clip1")]), "clip2");
        assert_eq!(next_clip_name(&[clip("clip1"), clip("clip3")]), "clip2");
    }
}
