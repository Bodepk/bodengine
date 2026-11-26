use std::collections::HashMap;
use std::path::PathBuf;

use egui::{Context, Ui};
use lunar_editor::{EditorCommand, EditorDocument, EditorError};
use lunar_scenes::{Component, SceneError, Transform, normalize_asset_path};

use crate::assets::{AssetKind, AssetLibrary, assets_ui};
use crate::canvas::{CanvasDrag, canvas_ui};
use crate::helpers::{audio_component, image_component, next_entity_id};
use crate::recent;
use crate::{hierarchy, inspector};

/// Acción que espera a resolver el aviso de cambios sin guardar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PendingAction {
    New,
    Open(PathBuf),
}

impl PendingAction {
    pub(crate) fn label(&self) -> String {
        match self {
            Self::New => "crear escena nueva".to_owned(),
            Self::Open(path) => format!("abrir {}", path.display()),
        }
    }
}

/// Componente que se está editando en el inspector (ADR-030).
#[derive(Debug, Clone)]
pub(crate) struct ComponentEdit {
    pub(crate) id: u64,
    pub(crate) index: usize,
    pub(crate) component: Component,
}

#[derive(Debug)]
pub(crate) struct EditorApp {
    pub(crate) document: EditorDocument,
    pub(crate) path: PathBuf,
    pub(crate) path_text: String,
    pub(crate) name_buffer: String,
    pub(crate) asset_buffer: String,
    pub(crate) translation: [f32; 3],
    pub(crate) rotation: [f32; 4],
    pub(crate) rotation_degrees: [f32; 3],
    pub(crate) scale: [f32; 3],
    pub(crate) component_edit: Option<ComponentEdit>,
    synced_selection: Vec<u64>,
    synced_revision: u64,
    revision: u64,
    status: String,
    pub(crate) status_log: Vec<String>,
    pub(crate) show_log: bool,
    pub(crate) assets: AssetLibrary,
    pub(crate) assets_root_text: String,
    pub(crate) view_center: [f32; 2],
    pub(crate) zoom: f32,
    pub(crate) show_grid: bool,
    pub(crate) snap: bool,
    pub(crate) drag: CanvasDrag,
    pub(crate) preview: HashMap<u64, Transform>,
    pub(crate) pending_action: Option<PendingAction>,
    pub(crate) save_as_open: bool,
    pub(crate) save_as_buffer: String,
    pub(crate) recent: Vec<String>,
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
        let settings = crate::settings::load();
        let assets_root_text = settings.assets_root.clone();
        let mut assets = AssetLibrary::new(PathBuf::from(&assets_root_text));
        assets.reload(ctx);
        if let Some(error) = &assets.error {
            status = error.clone();
        }

        Self {
            document,
            path,
            path_text: path_text.clone(),
            name_buffer: String::new(),
            asset_buffer: String::from("assets/sprite.png"),
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            rotation_degrees: [0.0; 3],
            scale: [1.0; 3],
            component_edit: None,
            synced_selection: Vec::new(),
            synced_revision: 0,
            revision: 0,
            status: status.clone(),
            status_log: vec![status],
            show_log: false,
            assets,
            assets_root_text,
            view_center: settings.center,
            zoom: settings.zoom,
            show_grid: settings.show_grid,
            snap: settings.snap,
            drag: CanvasDrag::None,
            preview: HashMap::new(),
            pending_action: None,
            save_as_open: false,
            save_as_buffer: path_text.clone(),
            recent: recent::load(),
        }
    }

    /// Actualiza la barra de estado y registra el mensaje en el historial.
    /// Los mensajes vacíos limpian la barra sin añadir una entrada.
    pub(crate) fn set_status(&mut self, message: impl Into<String>) {
        let message = message.into();
        crate::status::push(&mut self.status_log, message.clone());
        self.status = message;
    }

    pub(crate) fn apply(&mut self, command: EditorCommand) {
        match self.document.apply(command) {
            Ok(()) => {
                self.revision += 1;
                self.set_status("");
            }
            Err(error) => self.set_status(error.to_string()),
        }
    }

    /// Persiste las preferencias de la GUI (ADR-031). Un fallo de escritura no
    /// altera el estado del editor.
    pub(crate) fn save_settings(&self) {
        let settings = crate::settings::EditorSettings {
            assets_root: self.assets_root_text.clone(),
            center: self.view_center,
            zoom: self.zoom,
            show_grid: self.show_grid,
            snap: self.snap,
        };
        let _ = crate::settings::save(&settings);
    }

    pub(crate) fn undo(&mut self) {
        if self.document.undo() {
            self.revision += 1;
            self.set_status("");
        } else {
            self.set_status("Nada para deshacer");
        }
    }

    pub(crate) fn redo(&mut self) {
        if self.document.redo() {
            self.revision += 1;
            self.set_status("");
        } else {
            self.set_status("Nada para rehacer");
        }
    }

    pub(crate) fn request_new(&mut self) {
        if self.document.is_dirty() {
            self.pending_action = Some(PendingAction::New);
            self.set_status("Confirma cómo guardar antes de crear una escena nueva");
        } else {
            self.new_document();
        }
    }

    pub(crate) fn request_open(&mut self) {
        let path = PathBuf::from(&self.path_text);
        if self.document.is_dirty() {
            self.pending_action = Some(PendingAction::Open(path));
            self.set_status("Confirma cómo guardar antes de abrir otra escena");
        } else {
            self.open_path(path);
        }
    }

    pub(crate) fn open_path(&mut self, path: PathBuf) {
        match EditorDocument::load_from_path(&path) {
            Ok(document) => {
                self.document = document;
                self.path = path.clone();
                self.path_text = path.to_string_lossy().into_owned();
                self.preview.clear();
                self.drag = CanvasDrag::None;
                self.revision += 1;
                self.set_status(format!("Abierta {path:?}"));
                self.remember(path);
            }
            Err(error) => self.set_status(format!("No se pudo abrir {path:?}: {error}")),
        }
    }

    pub(crate) fn open_recent(&mut self, path: String) {
        self.path_text = path;
        self.request_open();
    }

    pub(crate) fn begin_save_as(&mut self) {
        self.save_as_buffer = self.path_text.clone();
        self.save_as_open = true;
    }

    pub(crate) fn commit_save_as(&mut self, path_text: String) {
        self.save_as_open = false;
        if self.save_to(PathBuf::from(&path_text)) {
            self.path_text = path_text;
        }
    }

    /// Guarda en la ruta del documento, nunca en la del campo de texto, para
    /// que «Guardar» no sobrescriba una escena distinta si se editó la ruta.
    pub(crate) fn save(&mut self) -> bool {
        self.save_to(self.path.clone())
    }

    fn save_to(&mut self, path: PathBuf) -> bool {
        match self.document.save_to_path(&path) {
            Ok(()) => {
                self.path = path.clone();
                self.set_status(format!("Guardada {path:?}"));
                self.remember(path);
                true
            }
            Err(error) => {
                self.set_status(format!("No se pudo guardar {path:?}: {error}"));
                false
            }
        }
    }

    pub(crate) fn save_and_continue(&mut self) {
        if self.save() {
            self.continue_pending();
        }
    }

    pub(crate) fn discard_and_continue(&mut self) {
        self.continue_pending();
    }

    pub(crate) fn cancel_pending(&mut self) {
        self.pending_action = None;
        self.set_status("Acción cancelada");
    }

    fn continue_pending(&mut self) {
        if let Some(action) = self.pending_action.take() {
            match action {
                PendingAction::New => self.new_document(),
                PendingAction::Open(path) => self.open_path(path),
            }
        }
    }

    fn remember(&mut self, path: PathBuf) {
        let text = path.to_string_lossy().into_owned();
        self.recent = recent::remember(&self.recent, &text);
        let _ = recent::save(&self.recent);
    }

    pub(crate) fn new_document(&mut self) {
        self.document = EditorDocument::new();
        self.preview.clear();
        self.drag = CanvasDrag::None;
        self.path = PathBuf::from("scene.json");
        self.path_text = self.path.to_string_lossy().into_owned();
        self.revision += 1;
        self.set_status("Escena nueva");
    }

    pub(crate) fn delete_selected(&mut self) {
        let ids = self.document.selection().to_vec();
        for id in ids {
            self.apply(EditorCommand::DeleteEntity { id });
        }
    }

    pub(crate) fn duplicate_selected(&mut self) {
        let Some(id) = self.document.selection().first().copied() else {
            self.set_status("Selecciona una entidad para duplicar");
            return;
        };
        let new_id = next_entity_id(self.document.scene());
        self.apply(EditorCommand::DuplicateEntity { id, new_id });
    }

    pub(crate) fn reload_assets(&mut self, ctx: &Context) {
        self.assets = AssetLibrary::new(PathBuf::from(&self.assets_root_text));
        self.assets.reload(ctx);
        match &self.assets.error {
            Some(error) => self.set_status(error.clone()),
            None => self.set_status(format!(
                "{} asset(s) en {}",
                self.assets.entries.len(),
                self.assets.root.display()
            )),
        }
        self.save_settings();
    }

    pub(crate) fn create_entity_from_asset(&mut self, asset: &str, position: [f32; 2]) {
        let Some(kind) = self.assets.kind(asset) else {
            self.set_status(format!("Asset no cargado: {asset}"));
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
        self.set_status(format!("Creada entidad #{id} desde {asset}"));
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let keys = ctx.input(|input| crate::shortcuts::ShortcutKeys {
            command: input.modifiers.ctrl || input.modifiers.command,
            shift: input.modifiers.shift,
            save: input.key_pressed(egui::Key::S),
            open: input.key_pressed(egui::Key::O),
            undo: input.key_pressed(egui::Key::Z),
            redo: input.key_pressed(egui::Key::Y),
            delete: input.key_pressed(egui::Key::Delete),
            duplicate: input.key_pressed(egui::Key::D),
        });
        let actions = crate::shortcuts::resolve(keys);

        if actions.save {
            self.save();
        }
        if actions.save_as {
            self.begin_save_as();
        }
        if actions.open {
            self.request_open();
        }
        if actions.undo {
            self.undo();
        }
        if actions.redo {
            self.redo();
        }
        if actions.delete {
            self.delete_selected();
        }
        if actions.duplicate {
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
            self.rotation_degrees = [0.0; 3];
            self.scale = [1.0; 3];
            self.component_edit = None;
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
        self.rotation_degrees = crate::helpers::degrees_from_quaternion(entity.transform.rotation);
        self.scale = entity.transform.scale;

        let active = self.component_edit.take().and_then(|edit| {
            let current = self
                .document
                .scene()
                .entities
                .iter()
                .find(|candidate| candidate.id == edit.id)?;
            let component = current.components.get(edit.index)?.clone();
            Some(ComponentEdit { component, ..edit })
        });
        self.component_edit = active;
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
            ui.horizontal(|ui| {
                if self.status.is_empty() {
                    ui.label("Listo");
                } else {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, self.status.clone());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Historial").clicked() {
                        self.show_log = !self.show_log;
                    }
                });
            });
        });
        egui::CentralPanel::default().show(ui, |ui| canvas_ui(self, ui));
        crate::dialogs::show(self, &ctx);
        crate::status::log_window(self, &ctx);
    }
}
