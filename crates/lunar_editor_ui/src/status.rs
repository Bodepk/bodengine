//! Mensaje de estado e historial del editor.
//!
//! Todas las escrituras del mensaje pasan por `EditorApp::set_status`, que
//! alimenta la barra de estado y guarda las últimas entradas en memoria. Sin
//! dependencias nuevas ni ficheros: el historial vive solo durante la sesión.

use egui::Context;

use crate::app::EditorApp;

/// Número máximo de entradas conservadas en el historial.
pub(crate) const MAX_LOG: usize = 50;

/// Añade un mensaje al historial, evitando repetir el último consecutivo y
/// conservando como máximo `MAX_LOG` entradas.
pub(crate) fn push(entries: &mut Vec<String>, message: impl Into<String>) {
    let message = message.into();
    if message.is_empty() || entries.last().is_some_and(|last| last == &message) {
        return;
    }
    entries.push(message);
    if entries.len() > MAX_LOG {
        let excess = entries.len() - MAX_LOG;
        entries.drain(..excess);
    }
}

/// Ventana flotante con el historial, abierta desde la barra de estado.
pub(crate) fn log_window(app: &mut EditorApp, ctx: &Context) {
    let EditorApp {
        show_log,
        status_log,
        ..
    } = app;
    egui::Window::new("Historial de estado")
        .open(show_log)
        .default_size([420.0, 260.0])
        .scroll(true)
        .show(ctx, |ui| {
            if status_log.is_empty() {
                ui.label("Sin mensajes todavía");
            } else {
                for message in status_log.iter().rev() {
                    ui.monospace(message.as_str());
                }
            }
            ui.separator();
            if ui.button("Limpiar").clicked() {
                status_log.clear();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pushes_messages_in_order() {
        let mut log = Vec::new();
        push(&mut log, "primero");
        push(&mut log, "segundo");
        assert_eq!(log, vec!["primero", "segundo"]);
    }

    #[test]
    fn skips_consecutive_duplicates() {
        let mut log = Vec::new();
        push(&mut log, "guardada");
        push(&mut log, "guardada");
        push(&mut log, "abierta");
        push(&mut log, "guardada");
        assert_eq!(log, vec!["guardada", "abierta", "guardada"]);
    }

    #[test]
    fn ignores_empty_messages() {
        let mut log = vec!["primero".to_owned()];
        push(&mut log, "");
        assert_eq!(log, vec!["primero"]);
    }

    #[test]
    fn caps_the_history() {
        let mut log = Vec::new();
        for i in 0..MAX_LOG + 5 {
            push(&mut log, format!("mensaje {i}"));
        }
        assert_eq!(log.len(), MAX_LOG);
        assert_eq!(log.first().map(String::as_str), Some("mensaje 5"));
        assert_eq!(log.last().map(String::as_str), Some("mensaje 54"));
    }
}
