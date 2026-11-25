use std::path::PathBuf;

use egui::Ui;
use lunar_editor::{EditorCommand, EditorDocument, EditorError};
use lunar_scenes::SceneError;

use crate::{graph, hierarchy, inspector};

#[derive(Debug)]
pub(crate) struct EditorApp {
    pub(crate) document: EditorDocument,
    pub(crate) path: PathBuf,
    pub(crate) path_text: String,
    pub(crate) name_buffer: String,
    pub(crate) asset_buffer: String,
    pub(crate) translation: [f32; 3],
    pub(crate) rotation: [f32; 4],
    pub(crate) scale: [f32; 3],
    pub(crate) volume: f32,
    pub(crate) autoplay: bool,
    synced_selection: Vec<u64>,
    synced_revision: u64,
    revision: u64,
    pub(crate) status: String,
}

impl EditorApp {
    pub(crate) fn new(path: Option<PathBuf>) -> Self {
        let path = path.unwrap_or_else(|| PathBuf::from("scene.json"));
        let mut status = String::from("Listo");
        let document = match EditorDocument::load_from_path(&path) {
            Ok(document) => document,
            Err(EditorError::Scene(SceneError::Io(_))) => {
                status = format!("Sin escena en {}; guarda para crearla", path.display());
                EditorDocument::new()
            }
            Err(error) => {
                status = format!("No se pudo abrir {}: {error}", path.display());
                EditorDocument::new()
            }
        };
        let path_text = path.to_string_lossy().into_owned();

        Self {
            document,
            path,
            path_text,
            name_buffer: String::new(),
            asset_buffer: String::from("assets/sprite.png"),
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
            volume: 0.5,
            autoplay: false,
            synced_selection: Vec::new(),
            synced_revision: 0,
            revision: 0,
            status,
        }
    }

    pub(crate) fn apply(&mut self, command: EditorCommand) {
        match self.document.apply(command) {
            Ok(()) => {
                self.revision += 1;
                self.status.clear();
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    pub(crate) fn undo(&mut self) {
        if self.document.undo() {
            self.revision += 1;
            self.status.clear();
        } else {
            self.status = String::from("Nada para deshacer");
        }
    }

    pub(crate) fn redo(&mut self) {
        if self.document.redo() {
            self.revision += 1;
            self.status.clear();
        } else {
            self.status = String::from("Nada para rehacer");
        }
    }

    pub(crate) fn open_path(&mut self) {
        let path = PathBuf::from(&self.path_text);
        match EditorDocument::load_from_path(&path) {
            Ok(document) => {
                self.document = document;
                self.path = path.clone();
                self.revision += 1;
                self.status = format!("Abierta {path:?}");
            }
            Err(error) => self.status = format!("No se pudo abrir {path:?}: {error}"),
        }
    }

    pub(crate) fn save(&mut self) {
        let path = PathBuf::from(&self.path_text);
        match self.document.save_to_path(&path) {
            Ok(()) => {
                self.path = path.clone();
                self.status = format!("Guardada {path:?}");
            }
            Err(error) => self.status = format!("No se pudo guardar {path:?}: {error}"),
        }
    }

    pub(crate) fn new_document(&mut self) {
        self.document = EditorDocument::new();
        self.path = PathBuf::from("scene.json");
        self.path_text = self.path.to_string_lossy().into_owned();
        self.revision += 1;
        self.status = String::from("Escena nueva");
    }

    pub(crate) fn delete_selected(&mut self) {
        if let Some(id) = self.document.selection().first().copied() {
            self.apply(EditorCommand::DeleteEntity { id });
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (save, undo, redo, delete) = ctx.input(|input| {
            let command = input.modifiers.ctrl || input.modifiers.command;
            let shift = input.modifiers.shift;
            (
                command && input.key_pressed(egui::Key::S),
                command && !shift && input.key_pressed(egui::Key::Z),
                command
                    && ((shift && input.key_pressed(egui::Key::Z))
                        || input.key_pressed(egui::Key::Y)),
                input.key_pressed(egui::Key::Delete),
            )
        });

        if save {
            self.save();
        }
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if delete {
            self.delete_selected();
        }
    }

    fn sync_inspector(&mut self) {
        let selection = self.document.selection().to_vec();
        if self.synced_selection == selection && self.synced_revision == self.revision {
            return;
        }
        self.synced_selection = selection.clone();
        self.synced_revision = self.revision;

        let Some(id) = selection.first().copied() else {
            self.name_buffer.clear();
            self.translation = [0.0; 3];
            self.rotation = [0.0, 0.0, 0.0, 1.0];
            self.scale = [1.0; 3];
            return;
        };
        let Some(entity) = self
            .document
            .scene()
            .entities
            .iter()
            .find(|entity| entity.id == id)
            .cloned()
        else {
            return;
        };
        self.name_buffer = entity.name.unwrap_or_default();
        self.translation = entity.transform.translation;
        self.rotation = entity.transform.rotation;
        self.scale = entity.transform.scale;
    }
}

impl eframe::App for EditorApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_shortcuts(&ctx);
        self.sync_inspector();

        egui::Panel::top("toolbar").show(ui, |ui| hierarchy::toolbar_ui(self, ui));
        egui::Panel::left("hierarchy")
            .default_size(250.0)
            .show(ui, |ui| hierarchy::hierarchy_ui(self, ui));
        egui::Panel::right("inspector")
            .default_size(330.0)
            .show(ui, |ui| inspector::inspector_ui(self, ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            if self.status.is_empty() {
                ui.label("Listo");
            } else {
                ui.colored_label(egui::Color32::LIGHT_GREEN, self.status.clone());
            }
        });
        egui::CentralPanel::default().show(ui, |ui| graph::graph_ui(self, ui));
    }
}
