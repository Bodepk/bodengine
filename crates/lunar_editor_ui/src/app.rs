use std::collections::HashMap;
use std::path::PathBuf;

use egui::{Context, Ui};
use lunar_editor::{EditorCommand, EditorDocument, EditorError};
use lunar_scenes::{Component, SceneError, Transform, normalize_asset_path};

use crate::assets::{AssetKind, AssetLibrary, assets_ui};
use crate::canvas::{CanvasDrag, canvas_ui};
use crate::helpers::{audio_component, image_component, next_entity_id};
use crate::{hierarchy, inspector};

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
    pub(crate) assets: AssetLibrary,
    pub(crate) assets_root_text: String,
    pub(crate) view_center: [f32; 2],
    pub(crate) zoom: f32,
    pub(crate) show_grid: bool,
    pub(crate) snap: bool,
    pub(crate) drag: CanvasDrag,
    pub(crate) preview: HashMap<u64, Transform>,
}

impl EditorApp {
    pub(crate) fn new(path: Option<PathBuf>, ctx: &Context) -> Self {
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
        let assets_root_text = String::from("examples/sprite_platformer/assets");
        let mut assets = AssetLibrary::new(PathBuf::from(&assets_root_text));
        assets.reload(ctx);
        if let Some(error) = &assets.error {
            status = error.clone();
        }

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
            assets,
            assets_root_text,
            view_center: [0.0, 0.0],
            zoom: 48.0,
            show_grid: true,
            snap: false,
            drag: CanvasDrag::None,
            preview: HashMap::new(),
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
                self.preview.clear();
                self.drag = CanvasDrag::None;
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
        self.preview.clear();
        self.drag = CanvasDrag::None;
        self.path = PathBuf::from("scene.json");
        self.path_text = self.path.to_string_lossy().into_owned();
        self.revision += 1;
        self.status = String::from("Escena nueva");
    }

    pub(crate) fn delete_selected(&mut self) {
        let ids = self.document.selection().to_vec();
        for id in ids {
            self.apply(EditorCommand::DeleteEntity { id });
        }
    }

    pub(crate) fn duplicate_selected(&mut self) {
        let Some(id) = self.document.selection().first().copied() else {
            self.status = String::from("Selecciona una entidad para duplicar");
            return;
        };
        let new_id = next_entity_id(self.document.scene());
        self.apply(EditorCommand::DuplicateEntity { id, new_id });
    }

    pub(crate) fn reload_assets(&mut self, ctx: &Context) {
        self.assets = AssetLibrary::new(PathBuf::from(&self.assets_root_text));
        self.assets.reload(ctx);
        match &self.assets.error {
            Some(error) => self.status = error.clone(),
            None => {
                self.status = format!(
                    "{} asset(s) en {}",
                    self.assets.entries.len(),
                    self.assets.root.display()
                )
            }
        }
    }

    pub(crate) fn create_entity_from_asset(&mut self, asset: &str, position: [f32; 2]) {
        let Some(kind) = self.assets.kind(asset) else {
            self.status = format!("Asset no cargado: {asset}");
            return;
        };
        let id = next_entity_id(self.document.scene());
        let name = PathBuf::from(asset)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("Entidad {id}"));
        self.apply(EditorCommand::CreateEntity {
            id,
            name: Some(name),
        });
        let component = match kind {
            AssetKind::Image => image_component(asset),
            AssetKind::Audio => audio_component(asset, 0.5, false),
            AssetKind::Mesh => Component::Mesh {
                asset: normalize_asset_path(asset),
                primitive: None,
            },
        };
        self.apply(EditorCommand::AddComponent { id, component });
        self.apply(EditorCommand::SetTransform {
            id,
            transform: Transform {
                translation: [position[0], position[1], 0.0],
                ..Transform::default()
            },
        });
        self.status = format!("Creada entidad #{id} desde {asset}");
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (save, undo, redo, delete, duplicate) = ctx.input(|input| {
            let command = input.modifiers.ctrl || input.modifiers.command;
            let shift = input.modifiers.shift;
            (
                command && input.key_pressed(egui::Key::S),
                command && !shift && input.key_pressed(egui::Key::Z),
                command
                    && ((shift && input.key_pressed(egui::Key::Z))
                        || input.key_pressed(egui::Key::Y)),
                input.key_pressed(egui::Key::Delete),
                command && input.key_pressed(egui::Key::D),
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
        if duplicate {
            self.duplicate_selected();
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
            .default_size(230.0)
            .show(ui, |ui| hierarchy::hierarchy_ui(self, ui));
        egui::Panel::left("assets")
            .default_size(240.0)
            .show(ui, |ui| assets_ui(self, ui));
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
        egui::CentralPanel::default().show(ui, |ui| canvas_ui(self, ui));
    }
}
