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
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogEntry {
    pub(crate) level: Level,
    pub(crate) message: String,
    /// Segundos transcurridos desde el arranque del editor.
    pub(crate) elapsed: f32,
    /// Veces que se repitió el mensaje de forma consecutiva.
    pub(crate) count: usize,
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

impl Level {
    #[must_use]
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Warn => "Aviso",
            Self::Error => "Error",
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

/// Añade un mensaje al historial. Los mensajes consecutivos iguales se
/// agrupan en la última entrada (contador `×N`) en vez de descartarse, y se
/// conservan como máximo `MAX_LOG` entradas.
pub(crate) fn push(entries: &mut Vec<LogEntry>, message: impl Into<String>, elapsed: f32) {
    let message = message.into();
    if message.is_empty() {
        return;
    }
    if let Some(last) = entries.last_mut()
        && last.message == message
    {
        last.count += 1;
        return;
    }
    entries.push(LogEntry {
        level: level_for(&message),
        message,
        elapsed,
        count: 1,
    });
    if entries.len() > MAX_LOG {
        let excess = entries.len() - MAX_LOG;
        entries.drain(..excess);
    }
}

/// Entradas visibles, de la más reciente a la más antigua, tras aplicar el
/// filtro de severidad y la búsqueda por substring insensible a mayúsculas.
#[must_use]
pub(crate) fn visible_entries(
    entries: &[LogEntry],
    filter: ConsoleFilter,
    search: &str,
) -> Vec<LogEntry> {
    let query = search.to_ascii_lowercase();
    entries
        .iter()
        .rev()
        .filter(|entry| {
            filter.matches(entry.level)
                && (query.is_empty() || entry.message.to_ascii_lowercase().contains(&query))
        })
        .cloned()
        .collect()
}

/// Texto plano de una selección de entradas, para copiar al portapapeles.
#[must_use]
pub(crate) fn format_entries(entries: &[LogEntry]) -> String {
    entries
        .iter()
        .map(|entry| {
            let repeat = if entry.count > 1 {
                format!(" (×{})", entry.count)
            } else {
                String::new()
            };
            format!(
                "[+{:.2}s] {} {}{repeat}",
                entry.elapsed,
                entry.level.label(),
                entry.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
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
    let mut copy = false;
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
            if ui.button("Copiar").clicked() {
                copy = true;
            }
            if ui.button("Limpiar").clicked() {
                clear = true;
            }
        });
    });
    ui.add(
        egui::TextEdit::singleline(&mut app.console_search)
            .hint_text("Buscar en consola…")
            .desired_width(220.0),
    );
    let visible = visible_entries(&app.status_log, filter, &app.console_search);
    ui.label(format!(
        "{} de {} mensaje(s)",
        visible.len(),
        app.status_log.len()
    ));
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for entry in &visible {
            let color = match entry.level {
                Level::Info => Color32::from_gray(210),
                Level::Warn => Color32::YELLOW,
                Level::Error => Color32::RED,
            };
            ui.horizontal(|ui| {
                ui.monospace(format!("[+{:.2}s]", entry.elapsed));
                ui.colored_label(color, entry.message.clone());
                if entry.count > 1 {
                    ui.colored_label(Color32::GRAY, format!("×{}", entry.count));
                }
            });
        }
    });

    app.show_log = show_log;
    if clear {
        app.status_log.clear();
        app.console_search.clear();
    }
    if copy {
        ui.ctx().copy_text(format_entries(&visible));
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
                    ui.horizontal(|ui| {
                        ui.monospace(format!("[+{:.2}s]", entry.elapsed));
                        ui.monospace(format!(
                            "{} {}{}",
                            entry.level.label(),
                            entry.message,
                            if entry.count > 1 {
                                format!(" (×{})", entry.count)
                            } else {
                                String::new()
                            }
                        ));
                    });
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
        assert!(texts.iter().any(|text| text.contains("Buscar en consola")));
        assert!(texts.iter().any(|text| text.contains("Copiar")));
        assert!(texts.iter().any(|text| text.contains("mensaje(s)")));
    }

    #[test]
    fn pushes_messages_in_order() {
        let mut log = Vec::new();
        push(&mut log, "primero", 1.0);
        push(&mut log, "segundo", 2.0);
        assert_eq!(
            log.iter()
                .map(|entry| entry.message.as_str())
                .collect::<Vec<_>>(),
            vec!["primero", "segundo"]
        );
        assert_eq!(log[0].elapsed, 1.0);
        assert_eq!(log[1].elapsed, 2.0);
    }

    #[test]
    fn groups_only_consecutive_repeats() {
        let mut log = Vec::new();
        push(&mut log, "guardada", 1.0);
        push(&mut log, "guardada", 1.1);
        push(&mut log, "guardada", 1.2);
        push(&mut log, "abierta", 2.0);
        push(&mut log, "guardada", 3.0);
        assert_eq!(
            log.iter()
                .map(|entry| entry.message.as_str())
                .collect::<Vec<_>>(),
            vec!["guardada", "abierta", "guardada"]
        );
        assert_eq!(
            log.iter().map(|entry| entry.count).collect::<Vec<_>>(),
            vec![3, 1, 1]
        );
        assert_eq!(log[0].elapsed, 1.0);
    }

    #[test]
    fn ignores_empty_messages() {
        let mut log = vec![LogEntry {
            level: Level::Info,
            message: "primero".to_owned(),
            elapsed: 0.0,
            count: 1,
        }];
        push(&mut log, "", 0.1);
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn caps_the_history() {
        let mut log = Vec::new();
        for i in 0..MAX_LOG + 5 {
            push(&mut log, format!("mensaje {i}"), i as f32);
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
        push(&mut log, "Guardada escena", 0.0);
        push(&mut log, "No se pudo abrir escena", 0.0);
        assert_eq!(count_for(&log, Level::Info), 1);
        assert_eq!(count_for(&log, Level::Error), 1);
        assert_eq!(count_for(&log, Level::Warn), 0);
    }

    #[test]
    fn search_combines_with_the_severity_filter() {
        let mut log = Vec::new();
        push(&mut log, "Guardada escena.json", 1.0);
        push(&mut log, "Abierta escena.json", 2.0);
        push(&mut log, "No se pudo guardar otro.json", 3.0);
        let filter = ConsoleFilter {
            error: false,
            ..Default::default()
        };
        let found = visible_entries(&log, filter, "ESCENA");
        assert_eq!(
            found.iter().map(|e| e.message.as_str()).collect::<Vec<_>>(),
            vec!["Abierta escena.json", "Guardada escena.json"]
        );
        let all = visible_entries(&log, ConsoleFilter::default(), "");
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].message, "No se pudo guardar otro.json");
    }

    #[test]
    fn copied_text_keeps_time_level_and_repeat_count() {
        let mut log = Vec::new();
        push(&mut log, "Guardada escena", 1.5);
        push(&mut log, "Guardada escena", 1.6);
        let text = format_entries(&visible_entries(&log, ConsoleFilter::default(), ""));
        assert_eq!(text, "[+1.50s] Info Guardada escena (×2)");
        assert!(format_entries(&[]).is_empty());
    }
}
