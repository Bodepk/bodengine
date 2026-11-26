//! Diálogos del editor (ADR-020).
//!
//! `egui` no abre diálogos nativos: las confirmaciones se dibujan como
//! ventanas centradas y el estado pendiente vive en `EditorApp`, fuera del
//! documento. Se usa `Window` en lugar de `Modal` porque este último puede
//! cerrarse con el mismo clic que lo abre y pasar desapercibido.

use egui::{Align2, Context, Vec2};

use crate::app::EditorApp;

/// Dibuja los diálogos pendientes al final del frame.
pub(crate) fn show(app: &mut EditorApp, ctx: &Context) {
    unsaved_changes(app, ctx);
    save_as(app, ctx);
}

fn centered(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .collapsible(false)
        .resizable(false)
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
    centered("Cambios sin guardar").show(ctx, |ui| {
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
    let mut cancel = false;
    centered("Guardar como").show(ctx, |ui| {
        ui.label("Ruta del JSON de escena:");
        ui.add(
            egui::TextEdit::singleline(&mut app.save_as_buffer)
                .hint_text("scene.json")
                .desired_width(320.0),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            accept = ui.button("Guardar").clicked();
            cancel = ui.button("Cancelar").clicked();
        });
    });

    if accept {
        let path = app.save_as_buffer.clone();
        app.commit_save_as(path);
    } else if cancel {
        app.save_as_open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{EditorApp, PendingAction};

    fn input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(800.0, 600.0),
            )),
            ..Default::default()
        }
    }

    fn frame(ctx: &Context, app: &mut EditorApp, input: egui::RawInput) -> Vec<String> {
        let mut output = ctx.run_pass(input, |ctx| show(app, ctx));
        let texts = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                _ => None,
            })
            .collect();
        output.textures_delta.clear();
        texts
    }

    #[test]
    fn unsaved_dialog_draws_and_keeps_the_action_pending() {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.pending_action = Some(PendingAction::New);

        let _ = frame(&ctx, &mut app, input());
        let texts = frame(&ctx, &mut app, input());

        assert!(
            texts
                .iter()
                .any(|text| text.contains("Cambios sin guardar")),
            "el diálogo debe dibujar su título"
        );
        assert_eq!(app.pending_action, Some(PendingAction::New));
    }

    #[test]
    fn save_as_dialog_draws_and_stays_open() {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.save_as_open = true;

        let _ = frame(&ctx, &mut app, input());
        let texts = frame(&ctx, &mut app, input());

        assert!(texts.iter().any(|text| text.contains("Guardar como")));
        assert!(app.save_as_open);
    }

    #[test]
    fn dialog_survives_the_pointer_release_that_opened_it() {
        use egui::{Event, Modifiers, PointerButton, Pos2};

        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.apply(lunar_editor::EditorCommand::CreateEntity { id: 1, name: None });
        app.request_new();
        assert_eq!(app.pending_action, Some(PendingAction::New));

        let point = Pos2::new(10.0, 10.0);
        let mut release = input();
        release.events = vec![
            Event::PointerMoved(point),
            Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ];
        let _ = frame(&ctx, &mut app, release);

        assert_eq!(app.pending_action, Some(PendingAction::New));
    }
}
