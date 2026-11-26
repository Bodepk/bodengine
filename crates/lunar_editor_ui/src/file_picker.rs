//! Explorador de archivos integrado para elegir escenas JSON (ADR-020).
//!
//! No hay diálogo nativo ni dependencias nuevas: se recorre el sistema de
//! archivos con `std::fs` y la ventana se dibuja con `egui`. Solo se listan
//! carpetas y archivos `.json`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use egui::{Align2, Context, Vec2};

use crate::app::EditorApp;

/// Para qué se abre el explorador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickerMode {
    Open,
    Save,
}

/// Entrada listada en una carpeta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) directory: bool,
}

/// Estado de la ventana del explorador.
#[derive(Debug, Clone)]
pub(crate) struct FilePicker {
    pub(crate) mode: PickerMode,
    pub(crate) directory_text: String,
    pub(crate) name: String,
    pub(crate) entries: Vec<Entry>,
    pub(crate) loaded_directory: String,
    pub(crate) error: Option<String>,
    /// Evita que el clic que abre la ventana active un botón de ella.
    ready: bool,
}

impl FilePicker {
    pub(crate) fn new(mode: PickerMode, directory: PathBuf, name: String) -> Self {
        let mut picker = Self {
            mode,
            directory_text: directory.to_string_lossy().into_owned(),
            name,
            entries: Vec::new(),
            loaded_directory: String::new(),
            error: None,
            ready: false,
        };
        picker.refresh();
        picker
    }

    pub(crate) fn refresh(&mut self) {
        match read_directory(Path::new(&self.directory_text)) {
            Ok(mut entries) => {
                sort_entries(&mut entries);
                self.entries = entries;
                self.error = None;
            }
            Err(error) => {
                self.entries.clear();
                self.error = Some(error.to_string());
            }
        }
        self.loaded_directory = self.directory_text.clone();
    }
}

/// Devuelve `.` cuando la ruta no tiene carpeta padre.
#[must_use]
pub(crate) fn parent_dir(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

/// Acepta `.json` sin distinguir mayúsculas.
#[must_use]
pub(crate) fn is_scene_file(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

/// Añade `.json` solo si el nombre no tiene extensión.
#[must_use]
pub(crate) fn with_json_extension(name: &str) -> String {
    if !name.is_empty() && Path::new(name).extension().is_none() {
        format!("{name}.json")
    } else {
        name.to_owned()
    }
}

/// Carpetas primero y después archivos, ordenados sin distinguir mayúsculas.
pub(crate) fn sort_entries(entries: &mut [Entry]) {
    entries.sort_by(|left, right| {
        left.directory
            .cmp(&right.directory)
            .reverse()
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
}

/// Lista carpetas y escenas JSON de `dir`, omitiendo entradas ocultas.
pub(crate) fn read_directory(dir: &Path) -> std::io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in std::fs::read_dir(dir)? {
        let item = item?;
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let kind = item.file_type()?;
        if kind.is_dir() {
            entries.push(Entry {
                name,
                directory: true,
            });
        } else if kind.is_file() && is_scene_file(&name) {
            entries.push(Entry {
                name,
                directory: false,
            });
        }
    }
    Ok(entries)
}

/// Dibuja la ventana del explorador, si está abierta.
pub(crate) fn show(app: &mut EditorApp, ctx: &Context) {
    let Some(mut picker) = app.picker.take() else {
        return;
    };
    if !picker.ready {
        picker.ready = true;
        app.picker = Some(picker);
        return;
    }
    if picker.loaded_directory != picker.directory_text {
        picker.refresh();
    }

    let mut accept = false;
    let mut cancel = false;
    egui::Window::new("Examinar escenas")
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .collapsible(false)
        .resizable(true)
        .default_size([480.0, 420.0])
        .show(ctx, |ui| {
            ui.label("Carpeta:");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut picker.directory_text)
                        .hint_text("carpeta")
                        .desired_width(320.0),
                );
                if ui.button("Subir").clicked() {
                    let parent = parent_dir(Path::new(&picker.directory_text));
                    picker.directory_text = parent.to_string_lossy().into_owned();
                }
                if ui.button("Actualizar").clicked() {
                    picker.refresh();
                }
            });
            if let Some(error) = &picker.error {
                ui.colored_label(egui::Color32::RED, error.clone());
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    let entries = picker.entries.clone();
                    for (index, entry) in entries.iter().enumerate() {
                        ui.push_id(index, |ui| {
                            let label = if entry.directory {
                                format!("{}/", entry.name)
                            } else {
                                entry.name.clone()
                            };
                            let selected = !entry.directory && picker.name == entry.name;
                            let response = ui.selectable_label(selected, label);
                            if response.clicked() {
                                if entry.directory {
                                    let next = Path::new(&picker.directory_text).join(&entry.name);
                                    picker.directory_text = next.to_string_lossy().into_owned();
                                } else {
                                    picker.name = entry.name.clone();
                                }
                            }
                            if !entry.directory && response.double_clicked() {
                                accept = true;
                            }
                        });
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Archivo:");
                ui.add(
                    egui::TextEdit::singleline(&mut picker.name)
                        .hint_text("scene.json")
                        .desired_width(240.0),
                );
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                accept |= ui.button("Aceptar").clicked();
                cancel = ui.button("Cancelar").clicked();
            });
        });

    if cancel {
        app.picker = None;
    } else if accept {
        let dir = PathBuf::from(&picker.directory_text);
        let name = with_json_extension(&picker.name);
        let text = dir.join(name).to_string_lossy().into_owned();
        match picker.mode {
            PickerMode::Open => {
                app.path_text = text;
                app.request_open();
            }
            PickerMode::Save => {
                app.save_as_buffer = text;
                app.save_as_open = true;
            }
        }
        app.picker = None;
    } else {
        app.picker = Some(picker);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Context;

    #[test]
    fn scene_files_are_detected_case_insensitively() {
        assert!(is_scene_file("scene.json"));
        assert!(is_scene_file("SCENE.JSON"));
        assert!(!is_scene_file("scene.json.bak"));
        assert!(!is_scene_file("scene"));
    }

    #[test]
    fn missing_extension_is_added() {
        assert_eq!(with_json_extension("scene"), "scene.json");
        assert_eq!(with_json_extension("scene.txt"), "scene.txt");
        assert_eq!(with_json_extension(""), "");
    }

    #[test]
    fn parent_falls_back_to_current_directory() {
        assert_eq!(parent_dir(Path::new("scene.json")), Path::new("."));
        assert_eq!(
            parent_dir(Path::new("assets/scene.json")),
            Path::new("assets")
        );
    }

    #[test]
    fn entries_put_directories_first_and_sort_by_name() {
        let mut entries = vec![
            Entry {
                name: "zeta.json".to_owned(),
                directory: false,
            },
            Entry {
                name: "Assets".to_owned(),
                directory: true,
            },
            Entry {
                name: "alpha.json".to_owned(),
                directory: false,
            },
        ];
        sort_entries(&mut entries);
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Assets", "alpha.json", "zeta.json"]
        );
    }

    fn fixture_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/scene_demo/assets")
    }

    #[test]
    fn tracked_scene_fixture_is_listed() {
        let entries = read_directory(&fixture_dir()).expect("directorio");
        assert!(
            entries
                .iter()
                .any(|entry| entry.name == "sprites.scene.json" && !entry.directory),
            "la fixture v2 debe aparecer como archivo"
        );
    }

    #[test]
    fn picker_window_draws_the_directory_contents() {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.picker = Some(FilePicker::new(
            PickerMode::Open,
            fixture_dir(),
            String::new(),
        ));
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(900.0, 700.0),
            )),
            ..Default::default()
        };
        let mut first = ctx.run_pass(input.clone(), |ctx| show(&mut app, ctx));
        first.textures_delta.clear();
        let mut second = ctx.run_pass(input.clone(), |ctx| show(&mut app, ctx));
        second.textures_delta.clear();
        let mut output = ctx.run_pass(input, |ctx| show(&mut app, ctx));
        let texts: Vec<String> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                _ => None,
            })
            .collect();
        output.textures_delta.clear();
        assert!(texts.iter().any(|text| text.contains("Examinar escenas")));
        assert!(texts.iter().any(|text| text.contains("sprites.scene.json")));
        assert!(app.picker.is_some());
    }
}
