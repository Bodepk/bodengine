//! Diálogos modales del editor (ADR-020).
//!
//! `egui` no abre diálogos nativos: las confirmaciones se dibujan con `Modal`
//! y el estado pendiente vive en `EditorApp`, fuera del documento.

use egui::{Context, Id};

use crate::app::EditorApp;

/// Dibuja todos los diálogos pendientes al final del frame.
pub(crate) fn show(app: &mut EditorApp, ctx: &Context) {
    unsaved_changes(app, ctx);
    save_as(app, ctx);
}

fn unsaved_changes(app: &mut EditorApp, ctx: &Context) {
    let Some(action) = app.pending_action.clone() else {
        return;
    };

    enum Choice {
        Save,
        Discard,
        Cancel,
    }

    let mut chosen = None;
    let response = egui::Modal::new(Id::unique("lunar-unsaved")).show(ctx, |ui| {
        ui.heading("Cambios sin guardar");
        ui.label("La escena actual tiene cambios sin guardar.");
        ui.label(format!("Acción pendiente: {}", action.label()));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Guardar y continuar").clicked() {
                chosen = Some(Choice::Save);
            }
            if ui.button("No guardar").clicked() {
                chosen = Some(Choice::Discard);
            }
            if ui.button("Cancelar").clicked() {
                chosen = Some(Choice::Cancel);
            }
        });
    });

    if response.should_close() && chosen.is_none() {
        chosen = Some(Choice::Cancel);
    }
    match chosen {
        Some(Choice::Save) => app.save_and_continue(),
        Some(Choice::Discard) => app.discard_and_continue(),
        Some(Choice::Cancel) => app.cancel_pending(),
        None => {}
    }
}

fn save_as(app: &mut EditorApp, ctx: &Context) {
    if !app.save_as_open {
        return;
    }

    let mut accept = false;
    let response = egui::Modal::new(Id::unique("lunar-save-as")).show(ctx, |ui| {
        ui.heading("Guardar como");
        ui.label("Ruta del JSON de escena:");
        ui.add(
            egui::TextEdit::singleline(&mut app.save_as_buffer)
                .hint_text("scene.json")
                .desired_width(320.0),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            accept = ui.button("Guardar").clicked();
            if ui.button("Cancelar").clicked() {
                app.save_as_open = false;
            }
        });
    });

    if accept {
        let path = app.save_as_buffer.clone();
        app.commit_save_as(path);
    } else if response.should_close() {
        app.save_as_open = false;
    }
}
