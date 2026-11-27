//! Historial de undo/redo del editor (E45, ADR-045).
//!
//! La ventana solo lee etiquetas del documento y delega el salto a
//! `EditorApp::undo_to` / `redo_to`, que repiten las operaciones normales para
//! mantener sincronizado el historial de `scripts.map.txt`.

use egui::Ui;

use crate::app::EditorApp;

/// Dibuja el historial de la sesión. No se persiste ni entra en la escena.
pub(crate) fn history_ui(app: &mut EditorApp, ui: &mut Ui) {
    ui.heading("Historial");
    ui.horizontal(|ui| {
        if ui
            .add_enabled(app.document.can_undo(), egui::Button::new("Deshacer"))
            .clicked()
        {
            app.undo();
        }
        if ui
            .add_enabled(app.document.can_redo(), egui::Button::new("Rehacer"))
            .clicked()
        {
            app.redo();
        }
    });
    ui.separator();

    ui.label("Pendiente de deshacer (más reciente primero):");
    let undo = app.document.undo_labels().to_vec();
    if undo.is_empty() {
        ui.label("—");
    }
    for (index, label) in undo.iter().enumerate().rev() {
        if ui
            .selectable_label(false, format!("{}  {label}", index + 1))
            .clicked()
        {
            app.undo_to(index);
        }
    }

    ui.separator();
    ui.label("Pendiente de rehacer (el siguiente primero):");
    let redo = app.document.redo_labels().to_vec();
    if redo.is_empty() {
        ui.label("—");
    }
    for (index, label) in redo.iter().enumerate().rev() {
        if ui
            .selectable_label(false, format!("{}  {label}", index + 1))
            .clicked()
        {
            app.redo_to(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Rect};
    use lunar_editor::{EditorCommand, EditorDocument};

    #[test]
    fn the_window_shows_command_labels() {
        let ctx = egui::Context::default();
        let mut app = crate::app::EditorApp::new(None, &ctx);
        app.document
            .apply(EditorCommand::CreateEntity { id: 1, name: None })
            .expect("create");
        app.document
            .apply(EditorCommand::RenameEntity {
                id: 1,
                name: Some("hero".to_owned()),
            })
            .expect("rename");
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 700.0))),
            ..Default::default()
        };
        for _ in 0..2 {
            let mut warmup = ctx.run_pass(input.clone(), |ctx| {
                egui::Window::new("Historial").show(ctx, |ui| history_ui(&mut app, ui));
            });
            warmup.textures_delta.clear();
        }
        let mut output = ctx.run_pass(input, |ctx| {
            egui::Window::new("Historial").show(ctx, |ui| history_ui(&mut app, ui));
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
            texts.iter().any(|text| text.contains("Renombrar entidad")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|text| text.contains("Crear entidad")));

        app.document = EditorDocument::new();
        assert!(app.document.undo_labels().is_empty());
    }
}
