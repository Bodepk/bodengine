//! Mensaje de estado, consola acoplada e historial del editor.
//!
//! Todas las escrituras del mensaje pasan por `EditorApp::set_status`, que
//! alimenta la barra de estado y guarda las últimas entradas en memoria. La
//! consola filtra por severidad sin depender de GPU ni de ficheros.

use egui::{Color32, Context, Ui};

use crate::app::EditorApp;

/// Número máximo de entradas conservadas en el historial.
pub(crate) const MAX_LOG: usize = 50;

/// Severidad de un mensaje de la consola.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    Info,
    Warn,
    Error,
}

/// Entrada del historial; el mensaje es el mismo que se muestra en la barra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogEntry {
    pub(crate) level: Level,
    pub(crate) message: String,
}

/// Filtro de severidades activo en la consola.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConsoleFilter {
    pub(crate) info: bool,
    pub(crate) warn: bool,
    pub(crate) error: bool,
}

impl Default for ConsoleFilter {
    fn default() -> Self {
        Self {
            info: true,
            warn: true,
            error: true,
        }
    }
}

impl ConsoleFilter {
    #[must_use]
    pub(crate) fn matches(self, level: Level) -> bool {
        match level {
            Level::Info => self.info,
            Level::Warn => self.warn,
            Level::Error => self.error,
        }
    }
}

/// Clasificación determinista de los mensajes de estado existentes, sin cambiar
/// las llamadas a `set_status` repartidas por la GUI.
#[must_use]
pub(crate) fn level_for(message: &str) -> Level {
    let lower = message.to_ascii_lowercase();
    if lower.starts_with("error")
        || lower.contains("no se pudo")
        || lower.contains("fallo")
        || lower.contains("asset no cargado")
    {
        Level::Error
    } else if lower.starts_with("aviso")
        || lower.starts_with("confirma")
        || lower.contains("sin guardar")
    {
        Level::Warn
    } else {
        Level::Info
    }
}

/// Añade un mensaje al historial, evitando repetir el último consecutivo y
/// conservando como máximo `MAX_LOG` entradas.
pub(crate) fn push(entries: &mut Vec<LogEntry>, message: impl Into<String>) {
    let message = message.into();
    if message.is_empty() || entries.last().is_some_and(|last| last.message == message) {
        return;
    }
    entries.push(LogEntry {
        level: level_for(&message),
        message,
    });
    if entries.len() > MAX_LOG {
        let excess = entries.len() - MAX_LOG;
        entries.drain(..excess);
    }
}

/// Acción elegida en la fila de confirmación de la consola.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Save,
    Discard,
    Cancel,
}

/// Panel de consola acoplado bajo el viewport, con filtros y confirmaciones.
pub(crate) fn console_ui(app: &mut EditorApp, ui: &mut Ui) {
    let mut choice = None;
    if let Some(action) = app.pending_action.clone() {
        ui.horizontal(|ui| {
            ui.colored_label(Color32::YELLOW, format!("Confirmar: {}", action.label()));
            if ui.button("Guardar y continuar").clicked() {
                choice = Some(Choice::Save);
            }
            if ui.button("No guardar").clicked() {
                choice = Some(Choice::Discard);
            }
            if ui.button("Cancelar").clicked() {
                choice = Some(Choice::Cancel);
            }
        });
    }

    let filter = app.console_filter;
    let counts = [
        count_for(&app.status_log, Level::Info),
        count_for(&app.status_log, Level::Warn),
        count_for(&app.status_log, Level::Error),
    ];
    let mut clear = false;
    let mut show_log = app.show_log;
    ui.horizontal(|ui| {
        ui.heading("Consola");
        ui.toggle_value(
            &mut app.console_filter.info,
            format!("Info ({})", counts[0]),
        );
        ui.toggle_value(
            &mut app.console_filter.warn,
            format!("Avisos ({})", counts[1]),
        );
        ui.toggle_value(
            &mut app.console_filter.error,
            format!("Errores ({})", counts[2]),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Historial").clicked() {
                show_log = !show_log;
            }
            if ui.button("Limpiar").clicked() {
                clear = true;
            }
        });
    });
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for entry in app.status_log.iter().rev() {
            if !filter.matches(entry.level) {
                continue;
            }
            let color = match entry.level {
                Level::Info => Color32::from_gray(210),
                Level::Warn => Color32::YELLOW,
                Level::Error => Color32::RED,
            };
            ui.colored_label(color, entry.message.clone());
        }
    });

    app.show_log = show_log;
    if clear {
        app.status_log.clear();
    }
    match choice {
        Some(Choice::Save) => app.save_and_continue(),
        Some(Choice::Discard) => app.discard_and_continue(),
        Some(Choice::Cancel) => app.cancel_pending(),
        None => {}
    }
}

/// Ventana flotante con el historial completo, sin filtrar.
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
                for entry in status_log.iter().rev() {
                    ui.monospace(entry.message.as_str());
                }
            }
            ui.separator();
            if ui.button("Limpiar").clicked() {
                status_log.clear();
            }
        });
}

/// Número de entradas de una severidad, para los contadores de los botones.
#[must_use]
pub(crate) fn count_for(entries: &[LogEntry], level: Level) -> usize {
    entries.iter().filter(|entry| entry.level == level).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Context;

    #[test]
    fn console_panel_draws_filters_and_messages() {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.set_status("Guardada escena");
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(900.0, 700.0),
            )),
            ..Default::default()
        };
        let mut first = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Consola de prueba").show(ctx, |ui| console_ui(&mut app, ui));
        });
        first.textures_delta.clear();
        let mut second = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Consola de prueba").show(ctx, |ui| console_ui(&mut app, ui));
        });
        second.textures_delta.clear();
        let mut output = ctx.run_pass(input, |ctx| {
            egui::Window::new("Consola de prueba").show(ctx, |ui| console_ui(&mut app, ui));
        });
        let texts: Vec<String> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                _ => None,
            })
            .collect();
        output.textures_delta.clear();
        assert!(
            texts.iter().any(|text| text.contains("Consola")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|text| text.contains("Info (")));
        assert!(texts.iter().any(|text| text.contains("Guardada escena")));
    }

    #[test]
    fn pushes_messages_in_order() {
        let mut log = Vec::new();
        push(&mut log, "primero");
        push(&mut log, "segundo");
        assert_eq!(
            log.iter()
                .map(|entry| entry.message.as_str())
                .collect::<Vec<_>>(),
            vec!["primero", "segundo"]
        );
    }

    #[test]
    fn skips_consecutive_duplicates() {
        let mut log = Vec::new();
        push(&mut log, "guardada");
        push(&mut log, "guardada");
        push(&mut log, "abierta");
        push(&mut log, "guardada");
        assert_eq!(
            log.iter()
                .map(|entry| entry.message.as_str())
                .collect::<Vec<_>>(),
            vec!["guardada", "abierta", "guardada"]
        );
    }

    #[test]
    fn ignores_empty_messages() {
        let mut log = vec![LogEntry {
            level: Level::Info,
            message: "primero".to_owned(),
        }];
        push(&mut log, "");
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn caps_the_history() {
        let mut log = Vec::new();
        for i in 0..MAX_LOG + 5 {
            push(&mut log, format!("mensaje {i}"));
        }
        assert_eq!(log.len(), MAX_LOG);
        assert_eq!(
            log.first().map(|entry| entry.message.as_str()),
            Some("mensaje 5")
        );
        assert_eq!(
            log.last().map(|entry| entry.message.as_str()),
            Some("mensaje 54")
        );
    }

    #[test]
    fn classifies_existing_status_messages() {
        assert_eq!(level_for("Guardada escena"), Level::Info);
        assert_eq!(level_for("No se pudo abrir escena"), Level::Error);
        assert_eq!(level_for("Asset no cargado: x.png"), Level::Error);
        assert_eq!(level_for("Confirma cómo guardar"), Level::Warn);
        assert_eq!(level_for("● sin guardar"), Level::Warn);
    }

    #[test]
    fn filter_hides_disabled_levels() {
        let filter = ConsoleFilter {
            info: true,
            warn: false,
            error: true,
        };
        assert!(filter.matches(Level::Info));
        assert!(!filter.matches(Level::Warn));
        assert!(filter.matches(Level::Error));
    }

    #[test]
    fn counts_entries_by_level() {
        let mut log = Vec::new();
        push(&mut log, "Guardada escena");
        push(&mut log, "No se pudo abrir escena");
        assert_eq!(count_for(&log, Level::Info), 1);
        assert_eq!(count_for(&log, Level::Error), 1);
        assert_eq!(count_for(&log, Level::Warn), 0);
    }
}
