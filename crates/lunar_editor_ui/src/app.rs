use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use egui::{Context, Ui};
use lunar_editor::{EditorCommand, EditorDocument, EditorError};
use lunar_scenes::{Component, Scene, SceneEntity, SceneError, Transform, normalize_asset_path};

use crate::assets::{AssetKind, AssetLibrary, assets_ui};
use crate::canvas::{CanvasDrag, canvas_ui};
use crate::file_picker::{FilePicker, PickerMode, absolute_dir, parent_dir};
use crate::helpers::{
    audio_component, collider2_component, hierarchy_ids, image_component, next_entity_id,
    range_ids, toggle_selection, union_selection,
};
use crate::layout;
use crate::play::{PlayState, dynamic_ids, floor_y, step_vertical};
use crate::recent;
use crate::status::{ConsoleFilter, LogEntry};
use crate::tools::{EntityTemplate, TransformMode};
use crate::view3d::{Camera3D, Drag3D, ViewMode};
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
    /// Última entidad usada como ancla para la selección por rango (E12).
    selection_anchor: Option<u64>,
    /// Portapapeles en memoria (E13): no se guarda en la escena y sobrevive a
    /// abrir otra o crear una nueva.
    clipboard: Vec<SceneEntity>,
    synced_selection: Vec<u64>,
    synced_revision: u64,
    revision: u64,
    status: String,
    pub(crate) status_log: Vec<LogEntry>,
    pub(crate) console_filter: ConsoleFilter,
    pub(crate) console_search: String,
    pub(crate) show_log: bool,
    start_time: Instant,
    pub(crate) assets: AssetLibrary,
    pub(crate) assets_root_text: String,
    pub(crate) asset_search: String,
    pub(crate) asset_folder: String,
    pub(crate) asset_kind_filter: Option<AssetKind>,
    pub(crate) selected_asset: Option<String>,
    pub(crate) view_mode: ViewMode,
    pub(crate) camera3d: Camera3D,
    pub(crate) view3d_drag: Drag3D,
    pub(crate) view_center: [f32; 2],
    pub(crate) zoom: f32,
    pub(crate) show_grid: bool,
    pub(crate) snap: bool,
    pub(crate) transform_mode: TransformMode,
    pub(crate) drag: CanvasDrag,
    pub(crate) preview: HashMap<u64, Transform>,
    pub(crate) play_state: PlayState,
    pub(crate) play_time: f32,
    pub(crate) play_velocities: HashMap<u64, f32>,
    play_floor: f32,
    play_snapshot: Option<Scene>,
    pub(crate) pending_action: Option<PendingAction>,
    pub(crate) save_as_open: bool,
    pub(crate) save_as_buffer: String,
    pub(crate) picker: Option<FilePicker>,
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
        let start_time = Instant::now();
        let mut status_log = Vec::new();
        crate::status::push(&mut status_log, status.clone(), 0.0);
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
            selection_anchor: None,
            clipboard: Vec::new(),
            synced_selection: Vec::new(),
            synced_revision: 0,
            revision: 0,
            status,
            status_log,
            console_filter: ConsoleFilter::default(),
            console_search: String::new(),
            show_log: false,
            start_time,
            assets,
            assets_root_text,
            asset_search: String::new(),
            asset_folder: String::new(),
            asset_kind_filter: None,
            selected_asset: None,
            view_mode: ViewMode::default(),
            camera3d: Camera3D::default(),
            view3d_drag: Drag3D::default(),
            view_center: settings.center,
            zoom: settings.zoom,
            show_grid: settings.show_grid,
            snap: settings.snap,
            transform_mode: TransformMode::default(),
            drag: CanvasDrag::None,
            preview: HashMap::new(),
            play_state: PlayState::Stopped,
            play_time: 0.0,
            play_velocities: HashMap::new(),
            play_floor: 0.0,
            play_snapshot: None,
            pending_action: None,
            save_as_open: false,
            save_as_buffer: path_text.clone(),
            picker: None,
            recent: recent::load(),
        }
    }

    /// Actualiza la barra de estado y registra el mensaje en el historial.
    /// Los mensajes vacíos limpian la barra sin añadir una entrada.
    pub(crate) fn set_status(&mut self, message: impl Into<String>) {
        let message = message.into();
        let elapsed = self.start_time.elapsed().as_secs_f32();
        crate::status::push(&mut self.status_log, message.clone(), elapsed);
        self.status = message;
    }

    /// Única comprobación de que la escena se puede tocar. La usan tanto los
    /// atajos como los botones de la barra, para que ninguna acción escape al
    /// bloqueo durante Play (E6).
    pub(crate) fn ensure_editable(&mut self) -> bool {
        if self.play_state == PlayState::Stopped {
            true
        } else {
            self.set_status("Detén la simulación antes de editar");
            false
        }
    }

    pub(crate) fn apply(&mut self, command: EditorCommand) {
        if !self.ensure_editable() {
            return;
        }
        match self.document.apply(command) {
            Ok(()) => {
                self.revision += 1;
                self.set_status("");
            }
            Err(error) => self.set_status(error.to_string()),
        }
    }

    /// Selección compartida por jerarquía, lienzo 2D y vista 3D (E12): el clic
    /// normal reemplaza, Ctrl alterna y Shift amplía el rango desde el ancla.
    pub(crate) fn select_clicked(&mut self, id: u64, ctrl: bool, shift: bool) {
        if shift {
            let range = self
                .selection_anchor
                .and_then(|anchor| {
                    let ordered = hierarchy_ids(self.document.scene());
                    let range = range_ids(&ordered, anchor, id);
                    (!range.is_empty()).then_some(range)
                })
                .unwrap_or_else(|| vec![id]);
            let next = union_selection(self.document.selection(), &range);
            self.document.select(&next);
            return;
        }
        if ctrl {
            let next = toggle_selection(self.document.selection(), id, true);
            self.document.select(&next);
        } else {
            self.document.select(&[id]);
        }
        self.selection_anchor = Some(id);
    }

    /// Limpia la selección y su ancla para que un Shift posterior no herede un
    /// rango de otra escena.
    pub(crate) fn clear_selection(&mut self) {
        self.document.select(&[]);
        self.selection_anchor = None;
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
        if !self.ensure_editable() {
            return;
        }
        if self.document.undo() {
            self.revision += 1;
            self.set_status("");
        } else {
            self.set_status("Nada para deshacer");
        }
    }

    pub(crate) fn redo(&mut self) {
        if !self.ensure_editable() {
            return;
        }
        if self.document.redo() {
            self.revision += 1;
            self.set_status("");
        } else {
            self.set_status("Nada para rehacer");
        }
    }

    pub(crate) fn request_new(&mut self) {
        if !self.ensure_editable() {
            return;
        }
        if self.document.is_dirty() {
            self.pending_action = Some(PendingAction::New);
            self.set_status("Confirma cómo guardar antes de crear una escena nueva");
        } else {
            self.new_document();
        }
    }

    pub(crate) fn request_open(&mut self) {
        if !self.ensure_editable() {
            return;
        }
        let path = PathBuf::from(&self.path_text);
        if self.document.is_dirty() {
            self.pending_action = Some(PendingAction::Open(path));
            self.set_status("Confirma cómo guardar antes de abrir otra escena");
        } else {
            self.open_path(path);
        }
    }

    pub(crate) fn open_path(&mut self, path: PathBuf) {
        if !self.ensure_editable() {
            return;
        }
        match EditorDocument::load_from_path(&path) {
            Ok(document) => {
                self.document = document;
                self.selection_anchor = None;
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
        if !self.ensure_editable() {
            return;
        }
        self.save_as_buffer = self.path_text.clone();
        self.save_as_open = true;
    }

    /// Abre el explorador en la carpeta de la ruta correspondiente.
    pub(crate) fn begin_picker(&mut self, mode: PickerMode) {
        let text = match mode {
            PickerMode::Open => self.path_text.clone(),
            PickerMode::Save => self.save_as_buffer.clone(),
        };
        let path = absolute_dir(&PathBuf::from(&text));
        let directory = parent_dir(&path);
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.picker = Some(FilePicker::new(mode, directory, name));
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
        if !self.ensure_editable() {
            return false;
        }
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
        self.selection_anchor = None;
        self.preview.clear();
        self.drag = CanvasDrag::None;
        self.path = PathBuf::from("scene.json");
        self.path_text = self.path.to_string_lossy().into_owned();
        self.revision += 1;
        self.set_status("Escena nueva");
    }

    pub(crate) fn delete_selected(&mut self) {
        if !self.ensure_editable() {
            return;
        }
        let ids = self.document.selection().to_vec();
        if !ids.is_empty() {
            self.apply(EditorCommand::DeleteEntities { ids });
        }
    }

    /// Duplica toda la selección en un único comando atómico (E13).
    pub(crate) fn duplicate_selected(&mut self) {
        if !self.ensure_editable() {
            return;
        }
        let ids = self.document.selection().to_vec();
        if ids.is_empty() {
            self.set_status("Selecciona una entidad para duplicar");
            return;
        }
        let copies = crate::helpers::duplicate_pairs(self.document.scene(), &ids);
        let count = copies.len();
        let history = self.document.undo_len();
        self.apply(EditorCommand::DuplicateEntities {
            copies,
            suffix: " copia".to_owned(),
        });
        if self.document.undo_len() > history {
            self.set_status(format!("Duplicadas {count} entidad(es)"));
        }
    }

    /// Copia la selección al portapapeles sin tocar el documento ni el historial.
    pub(crate) fn copy_selected(&mut self) {
        if !self.ensure_editable() {
            return;
        }
        let ids = self.document.selection().to_vec();
        if ids.is_empty() {
            self.set_status("Selecciona una entidad para copiar");
            return;
        }
        self.clipboard = crate::helpers::selection_entities(self.document.scene(), &ids);
        self.set_status(format!("Copiadas {} entidad(es)", self.clipboard.len()));
    }

    /// Copia y borra en un solo paso de undo (E13).
    pub(crate) fn cut_selected(&mut self) {
        if !self.ensure_editable() {
            return;
        }
        let ids = self.document.selection().to_vec();
        if ids.is_empty() {
            self.set_status("Selecciona una entidad para cortar");
            return;
        }
        self.clipboard = crate::helpers::selection_entities(self.document.scene(), &ids);
        let count = self.clipboard.len();
        let history = self.document.undo_len();
        self.apply(EditorCommand::DeleteEntities { ids });
        if self.document.undo_len() > history {
            self.set_status(format!("Cortadas {count} entidad(es)"));
        }
    }

    /// Pega el portapapeles con ids nuevos en un solo paso de undo (E13).
    pub(crate) fn paste_clipboard(&mut self) {
        if !self.ensure_editable() {
            return;
        }
        if self.clipboard.is_empty() {
            self.set_status("Portapapeles vacío");
            return;
        }
        let entities = crate::helpers::remap_for_paste(&self.clipboard, self.document.scene());
        let count = entities.len();
        let history = self.document.undo_len();
        self.apply(EditorCommand::PasteEntities { entities });
        if self.document.undo_len() > history {
            self.set_status(format!("Pegadas {count} entidad(es)"));
        }
    }

    pub(crate) fn has_clipboard(&self) -> bool {
        !self.clipboard.is_empty()
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
        if !self.ensure_editable() {
            return;
        }
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

    /// Crea una entidad desde una plantilla, usando el primer asset del panel
    /// cuando la plantilla lo necesita. Todo pasa por `EditorDocument::apply`.
    pub(crate) fn create_from_template(&mut self, template: EntityTemplate) {
        if !self.ensure_editable() {
            return;
        }
        let component = match template.asset_kind() {
            Some(kind) => {
                let asset = self
                    .assets
                    .entries
                    .iter()
                    .find(|entry| entry.kind == kind)
                    .map(|entry| entry.path.clone());
                let Some(asset) = asset else {
                    let kind_name = match kind {
                        AssetKind::Image => "imagen",
                        AssetKind::Audio => "audio",
                        AssetKind::Mesh => "malla",
                    };
                    self.set_status(format!(
                        "No hay asset de {} en {}",
                        kind_name,
                        self.assets.root.display()
                    ));
                    return;
                };
                Some(match kind {
                    AssetKind::Image => image_component(&asset),
                    AssetKind::Audio => audio_component(&asset, 0.5, false),
                    AssetKind::Mesh => Component::Mesh {
                        asset: normalize_asset_path(&asset),
                        primitive: None,
                    },
                })
            }
            None => match template {
                EntityTemplate::Collider2D => Some(collider2_component()),
                _ => None,
            },
        };

        let id = next_entity_id(self.document.scene());
        self.apply(EditorCommand::CreateEntity {
            id,
            name: Some(template.name(id)),
        });
        if let Some(component) = component {
            self.apply(EditorCommand::AddComponent { id, component });
        }
        self.set_status(format!("Creada {} (#{id})", template.label()));
    }

    /// Captura la escena y comienza la simulación local.
    pub(crate) fn play(&mut self) {
        if self.play_state != PlayState::Stopped {
            return;
        }
        self.play_snapshot = Some(self.document.scene().clone());
        self.play_velocities = dynamic_ids(self.document.scene())
            .into_iter()
            .map(|id| (id, 0.0))
            .collect();
        self.play_floor = floor_y(self.document.scene());
        self.preview.clear();
        self.drag = CanvasDrag::None;
        self.play_time = 0.0;
        self.play_state = PlayState::Playing;
        self.set_status(format!(
            "Play: {} cuerpo(s) dinámico(s)",
            self.play_velocities.len()
        ));
    }

    /// Alterna entre juego y pausa. Sin efecto con la simulación detenida.
    pub(crate) fn toggle_pause(&mut self) {
        match self.play_state {
            PlayState::Playing => {
                self.play_state = PlayState::Paused;
                self.set_status("Play en pausa");
            }
            PlayState::Paused => {
                self.play_state = PlayState::Playing;
                self.set_status("Play reanudado");
            }
            PlayState::Stopped => {}
        }
    }

    /// Restaura la instantánea y vuelve al modo edición.
    pub(crate) fn stop_play(&mut self) {
        if self.play_state == PlayState::Stopped {
            return;
        }
        if let Some(snapshot) = self.play_snapshot.take()
            && self.document.scene() != &snapshot
        {
            // La simulación solo escribe en `preview`, así que esto no debería
            // ocurrir. Se restaura solo en ese caso para no perder el historial
            // de undo/redo en el camino normal (E6).
            match EditorDocument::from_scene(snapshot) {
                Ok(document) => {
                    self.document = document;
                    self.selection_anchor = None;
                }
                Err(error) => self.set_status(format!("No se pudo restaurar: {error}")),
            }
        }
        self.play_state = PlayState::Stopped;
        self.play_time = 0.0;
        self.play_velocities.clear();
        self.preview.clear();
        self.drag = CanvasDrag::None;
        self.view3d_drag = Drag3D::None;
        self.set_status("Play detenido; vista previa descartada");
    }

    /// Avanza la simulación. Los cambios viven en `preview`, nunca en la escena.
    pub(crate) fn tick_play(&mut self, dt: f32) {
        if self.play_state != PlayState::Playing {
            return;
        }
        let step = dt.clamp(0.0, crate::play::MAX_STEP);
        self.play_time += step;
        let ids: Vec<u64> = self.play_velocities.keys().copied().collect();
        for id in ids {
            let Some(original) = self
                .document
                .scene()
                .entities
                .iter()
                .find(|entity| entity.id == id)
                .map(|entity| entity.transform)
            else {
                continue;
            };
            let velocity = self.play_velocities.get(&id).copied().unwrap_or(0.0);
            let current_y = self
                .preview
                .get(&id)
                .map_or(original.translation[1], |transform| {
                    transform.translation[1]
                });
            let (y, velocity) = step_vertical(current_y, velocity, step, self.play_floor);
            self.play_velocities.insert(id, velocity);
            let mut transform = original;
            transform.translation[1] = y;
            self.preview.insert(id, transform);
        }
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
            copy: input.key_pressed(egui::Key::C),
            cut: input.key_pressed(egui::Key::X),
            paste: input.key_pressed(egui::Key::V),
            move_tool: input.key_pressed(egui::Key::W),
            rotate_tool: input.key_pressed(egui::Key::E),
            scale_tool: input.key_pressed(egui::Key::R),
            toggle_play: input.key_pressed(egui::Key::Space),
            stop_play: input.key_pressed(egui::Key::Escape),
        });
        let actions = crate::shortcuts::resolve(keys);

        if actions.toggle_play {
            match self.play_state {
                PlayState::Stopped => self.play(),
                _ => self.toggle_pause(),
            }
        }
        if actions.stop_play {
            self.stop_play();
        }
        if self.play_state != PlayState::Stopped {
            return;
        }

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
        if actions.copy {
            self.copy_selected();
        }
        if actions.cut {
            self.cut_selected();
        }
        if actions.paste {
            self.paste_clipboard();
        }
        if actions.move_tool {
            self.transform_mode = TransformMode::Move;
        }
        if actions.rotate_tool {
            self.transform_mode = TransformMode::Rotate;
        }
        if actions.scale_tool {
            self.transform_mode = TransformMode::Scale;
        }
        if self.view_mode == ViewMode::ThreeD && (actions.rotate_tool || actions.scale_tool) {
            self.set_status("En la vista 3D, rotar y escalar se hacen desde el inspector");
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
        let dt = ctx.input(|input| input.stable_dt);
        self.tick_play(dt);
        self.handle_shortcuts(&ctx);
        self.sync_inspector();

        egui::Panel::top("toolbar")
            .size_range(layout::TOOLBAR_HEIGHT..=layout::TOOLBAR_HEIGHT)
            .resizable(false)
            .show(ui, |ui| hierarchy::toolbar_ui(self, ui));
        egui::Panel::left("hierarchy")
            .size_range(layout::HIERARCHY_RANGE.0..=layout::HIERARCHY_RANGE.1)
            .default_size(layout::hierarchy_width(layout::HIERARCHY_WIDTH))
            .show(ui, |ui| hierarchy::hierarchy_ui(self, ui));
        egui::Panel::left("assets")
            .size_range(layout::ASSETS_RANGE.0..=layout::ASSETS_RANGE.1)
            .default_size(layout::assets_width(layout::ASSETS_WIDTH))
            .show(ui, |ui| assets_ui(self, ui));
        egui::Panel::right("inspector")
            .size_range(layout::INSPECTOR_RANGE.0..=layout::INSPECTOR_RANGE.1)
            .default_size(layout::inspector_width(layout::INSPECTOR_WIDTH))
            .show(ui, |ui| inspector::inspector_ui(self, ui));
        // El primero de los paneles inferiores queda pegado al borde de la
        // ventana; la consola se apila encima de la barra de estado.
        egui::Panel::bottom("status")
            .size_range(layout::STATUS_HEIGHT..=layout::STATUS_HEIGHT)
            .resizable(false)
            .show(ui, |ui| {
                if self.status.is_empty() {
                    ui.label("Listo");
                } else {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, self.status.clone());
                }
            });
        egui::Panel::bottom("console")
            .size_range(layout::CONSOLE_RANGE.0..=layout::CONSOLE_RANGE.1)
            .default_size(layout::console_height(layout::CONSOLE_HEIGHT))
            .show(ui, |ui| crate::status::console_ui(self, ui));
        egui::CentralPanel::default().show(ui, |ui| match self.view_mode {
            ViewMode::TwoD => canvas_ui(self, ui),
            ViewMode::ThreeD => crate::view3d::view3d_ui(self, ui),
        });
        crate::dialogs::show(self, &ctx);
        crate::status::log_window(self, &ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetEntry;
    use lunar_scenes::{BodyType, SceneEntity, Shape2D};

    fn scene_with_dynamic_entity() -> Scene {
        let mut entity = SceneEntity::new(1);
        entity.name = Some("Dinámica".to_owned());
        entity.transform.translation = [0.0, 3.0, 0.0];
        entity.components.push(Component::Collider2D {
            shape: Shape2D::Ball { radius: 0.5 },
            body: BodyType::Dynamic,
        });
        let mut scene = Scene::new();
        scene.entities.push(entity);
        scene
    }

    fn fresh_app() -> EditorApp {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.document = EditorDocument::new();
        app.assets.entries.clear();
        app
    }

    #[test]
    fn the_move_tool_is_active_on_startup() {
        let app = fresh_app();
        assert_eq!(app.transform_mode, TransformMode::Move);
    }

    #[test]
    fn templates_create_entities_through_apply() {
        let mut app = fresh_app();
        app.assets.entries.push(AssetEntry {
            path: "images/hero.png".to_owned(),
            kind: AssetKind::Image,
        });
        app.create_from_template(EntityTemplate::Empty);
        app.create_from_template(EntityTemplate::Sprite);
        app.create_from_template(EntityTemplate::Collider2D);
        app.create_from_template(EntityTemplate::Camera2D);
        let entities = &app.document.scene().entities;
        assert_eq!(entities.len(), 4);
        assert_eq!(entities[0].name.as_deref(), Some("Entidad 1"));
        assert!(
            matches!(entities[1].components.first(), Some(Component::Image { asset }) if asset == "images/hero.png")
        );
        assert!(matches!(
            entities[2].components.first(),
            Some(Component::Collider2D { .. })
        ));
        assert_eq!(entities[3].name.as_deref(), Some("Cámara 2D 4"));
        assert!(entities[3].components.is_empty());
        assert!(app.document.can_undo());
    }

    #[test]
    fn asset_templates_fail_without_mutating_the_scene() {
        let mut app = fresh_app();
        app.create_from_template(EntityTemplate::Audio);
        assert!(app.document.scene().entities.is_empty());
        assert!(app.status.starts_with("No hay asset"));
        assert!(!app.document.can_undo());
    }

    #[test]
    fn play_previews_motion_and_stop_restores_the_scene() {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.document = EditorDocument::from_scene(scene_with_dynamic_entity()).unwrap();
        app.play();
        assert_eq!(app.play_state, PlayState::Playing);
        assert_eq!(app.play_velocities.len(), 1);

        app.tick_play(0.1);
        let moved = app.preview[&1].translation[1];
        assert!(moved < 3.0);
        assert!(
            moved < 2.99,
            "la caída debe acumularse, no ser de un solo paso"
        );
        assert_eq!(
            app.document.scene().entities[0].transform.translation[1],
            3.0
        );

        app.toggle_pause();
        let frozen = app.preview[&1].translation[1];
        app.tick_play(0.1);
        assert_eq!(app.preview[&1].translation[1], frozen);
        app.toggle_pause();
        app.tick_play(0.1);
        assert!(app.preview[&1].translation[1] < frozen);

        app.stop_play();
        assert_eq!(app.play_state, PlayState::Stopped);
        assert!(app.preview.is_empty());
        assert!(app.play_velocities.is_empty());
        assert_eq!(
            app.document.scene().entities[0].transform.translation[1],
            3.0
        );
        assert!(!app.document.is_dirty());
    }

    #[test]
    fn play_does_not_snap_entities_upward() {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        let mut scene = scene_with_dynamic_entity();
        scene.entities[0].transform.translation[1] = -3.0;
        let mut ground = SceneEntity::new(2);
        ground.transform.translation = [0.0, -5.0, 0.0];
        ground.components.push(Component::Collider2D {
            shape: Shape2D::Ball { radius: 0.5 },
            body: BodyType::Fixed,
        });
        scene.entities.push(ground);
        app.document = EditorDocument::from_scene(scene).unwrap();
        app.play();
        app.tick_play(0.1);
        let y = app.preview[&1].translation[1];
        assert!(
            y < -3.0,
            "la entidad debe caer hacia abajo, no saltar a 0 ({y})"
        );
        app.stop_play();
    }

    #[test]
    fn editing_is_blocked_while_playing() {
        let mut app = fresh_app();
        app.play();
        app.apply(EditorCommand::CreateEntity { id: 99, name: None });
        assert!(app.document.scene().entities.is_empty());
        assert!(app.status.contains("Detén"));
        app.stop_play();
    }

    #[test]
    fn full_flow_create_select_edit_save_play_and_undo() {
        let ctx = Context::default();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("lunar-e6-flow-{unique}.scene.json"));
        let mut app = EditorApp::new(Some(path.clone()), &ctx);

        // Crear desde plantilla: la entidad queda seleccionada.
        app.create_from_template(EntityTemplate::Empty);
        let id = app.document.scene().entities[0].id;
        assert_eq!(app.document.selection(), &[id]);

        // Editar por los mismos comandos que usan inspector y gizmos.
        app.apply(EditorCommand::RenameEntity {
            id,
            name: Some("Nave".to_owned()),
        });
        app.apply(EditorCommand::SetTransform {
            id,
            transform: Transform {
                translation: [2.0, 4.0, 0.0],
                ..Transform::default()
            },
        });
        app.apply(EditorCommand::AddComponent {
            id,
            component: Component::Collider2D {
                shape: Shape2D::Ball { radius: 0.5 },
                body: BodyType::Dynamic,
            },
        });
        assert!(app.document.is_dirty());
        let history = app.document.undo_len();
        assert_eq!(history, 4);

        // Guardar y volver a leer desde disco.
        assert!(app.save());
        assert!(!app.document.is_dirty());
        assert!(path.exists());
        let saved = EditorDocument::load_from_path(&path).unwrap();
        assert_eq!(saved.scene().entities[0].name.as_deref(), Some("Nave"));
        assert_eq!(
            saved.scene().entities[0].transform.translation,
            [2.0, 4.0, 0.0]
        );

        // Play: la escena no se toca, guardar queda bloqueado, el historial
        // sobrevive al ciclo y la vista previa se descarta al detener.
        app.play();
        app.tick_play(0.1);
        assert!(app.preview[&id].translation[1] < 4.0);
        assert_eq!(
            app.document.scene().entities[0].transform.translation[1],
            4.0
        );
        assert!(!app.save());
        assert!(app.status.contains("Detén"));
        app.stop_play();
        assert!(app.preview.is_empty());
        assert_eq!(app.document.undo_len(), history);

        // Deshacer y rehacer siguen actuando sobre la escena real.
        app.undo();
        assert!(app.document.scene().entities[0].components.is_empty());
        app.redo();
        assert_eq!(app.document.scene().entities[0].components.len(), 1);
        assert_eq!(app.document.undo_len(), history);

        let messages: Vec<&str> = app
            .status_log
            .iter()
            .map(|entry| entry.message.as_str())
            .collect();
        assert!(messages.iter().any(|message| message.contains("Creada")));
        assert!(messages.iter().any(|message| message.contains("Guardada")));
        assert!(messages.iter().any(|message| message.contains("Play:")));
        assert!(messages.iter().any(|message| message.contains("detenido")));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn toolbar_actions_stay_locked_during_play() {
        let mut app = fresh_app();
        app.play();
        app.request_new();
        app.request_open();
        app.begin_save_as();
        app.duplicate_selected();
        app.copy_selected();
        app.cut_selected();
        app.paste_clipboard();
        app.delete_selected();
        assert!(app.pending_action.is_none());
        assert!(!app.save_as_open);
        assert!(!app.has_clipboard());
        assert!(app.document.scene().entities.is_empty());
        assert!(app.status.contains("Detén"));
        app.stop_play();
    }

    #[test]
    fn tool_keys_switch_the_active_mode() {
        let ctx = Context::default();
        let mut app = fresh_app();
        for (key, expected) in [
            (egui::Key::E, TransformMode::Rotate),
            (egui::Key::R, TransformMode::Scale),
            (egui::Key::W, TransformMode::Move),
        ] {
            let input = egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            };
            let mut output = ctx.run_pass(input, |ctx| app.handle_shortcuts(ctx));
            output.textures_delta.clear();
            assert_eq!(app.transform_mode, expected);
        }
    }

    fn app_with_entities(count: u64) -> EditorApp {
        let mut app = fresh_app();
        for id in 1..=count {
            app.document
                .apply(EditorCommand::CreateEntity {
                    id,
                    name: Some(format!("E{id}")),
                })
                .expect("create entity");
        }
        app
    }

    #[test]
    fn ctrl_click_toggles_selection_without_losing_other_entities() {
        let mut app = app_with_entities(4);
        app.select_clicked(1, false, false);
        assert_eq!(app.document.selection(), &[1]);
        app.select_clicked(3, true, false);
        assert_eq!(app.document.selection(), &[1, 3]);
        app.select_clicked(1, true, false);
        assert_eq!(app.document.selection(), &[3]);
        app.select_clicked(404, true, false);
        assert_eq!(app.document.selection(), &[3]);
    }

    #[test]
    fn shift_click_extends_the_range_from_the_last_anchor() {
        let mut app = app_with_entities(5);
        app.select_clicked(2, false, false);
        app.select_clicked(5, true, true);
        assert_eq!(app.document.selection(), &[2, 3, 4, 5]);
        // El ancla no cambia con Shift: seguir ampliando o recortar el rango
        // parte siempre de la misma entidad.
        app.select_clicked(4, true, true);
        assert_eq!(app.document.selection(), &[2, 3, 4, 5]);
        app.select_clicked(1, false, false);
        app.select_clicked(3, true, true);
        assert_eq!(app.document.selection(), &[1, 2, 3]);
    }

    #[test]
    fn shift_click_follows_hierarchy_order_not_scene_order() {
        let mut app = fresh_app();
        for id in [1, 3, 2] {
            app.document
                .apply(EditorCommand::CreateEntity {
                    id,
                    name: Some(format!("E{id}")),
                })
                .expect("create entity");
        }
        app.document
            .apply(EditorCommand::SetParent {
                id: 3,
                parent: Some(1),
            })
            .expect("set parent");
        app.select_clicked(1, false, false);
        app.select_clicked(2, true, true);
        assert_eq!(app.document.selection(), &[1, 3, 2]);
    }

    #[test]
    fn clearing_selection_resets_the_range_anchor() {
        let mut app = app_with_entities(5);
        app.select_clicked(2, false, false);
        app.clear_selection();
        assert!(app.document.selection().is_empty());
        app.select_clicked(5, true, true);
        assert_eq!(app.document.selection(), &[5]);
    }

    #[test]
    fn deleting_a_multi_selection_is_a_single_undo_step() {
        let mut app = app_with_entities(3);
        app.select_clicked(1, false, false);
        app.select_clicked(3, true, false);
        let history = app.document.undo_len();

        app.delete_selected();

        assert_eq!(app.document.undo_len(), history + 1);
        assert_eq!(app.document.scene().entities.len(), 1);
        assert_eq!(app.document.scene().entities[0].id, 2);
        assert!(app.document.selection().is_empty());
        app.document.undo();
        assert_eq!(app.document.scene().entities.len(), 3);
    }

    #[test]
    fn hierarchy_and_canvas_read_the_same_selection() {
        let ctx = Context::default();
        let mut app = app_with_entities(3);
        app.select_clicked(1, false, false);
        app.select_clicked(3, true, false);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_pass(input, |ctx| {
            egui::Window::new("Paneles").show(ctx, |ui| {
                crate::hierarchy::hierarchy_ui(&mut app, ui);
                crate::canvas::canvas_ui(&mut app, ui);
            });
        });
        output.textures_delta.clear();
        assert_eq!(app.document.selection(), &[1, 3]);
    }

    #[test]
    fn duplicating_a_multi_selection_is_one_undo_step() {
        let mut app = app_with_entities(3);
        app.select_clicked(1, false, false);
        app.select_clicked(3, true, false);
        let history = app.document.undo_len();

        app.duplicate_selected();

        assert_eq!(app.document.undo_len(), history + 1);
        let entities = &app.document.scene().entities;
        assert_eq!(entities.len(), 5);
        assert_eq!(entities[3].id, 4);
        assert_eq!(entities[3].name.as_deref(), Some("E1 copia"));
        assert_eq!(entities[4].id, 5);
        assert_eq!(entities[4].name.as_deref(), Some("E3 copia"));
        assert_eq!(app.document.selection(), &[4, 5]);
        app.document.undo();
        assert_eq!(app.document.scene().entities.len(), 3);
    }

    #[test]
    fn copying_a_group_keeps_internal_parents_and_drops_external_ones() {
        let mut app = app_with_entities(3);
        for (child, parent) in [(2, 1), (3, 2)] {
            app.document
                .apply(EditorCommand::SetParent {
                    id: child,
                    parent: Some(parent),
                })
                .expect("parent");
        }
        app.select_clicked(2, false, false);
        app.select_clicked(3, true, false);
        let history = app.document.undo_len();
        let dirty = app.document.is_dirty();

        app.copy_selected();

        assert_eq!(app.clipboard.len(), 2);
        assert_eq!(app.clipboard[0].id, 2);
        assert_eq!(app.clipboard[0].parent, None);
        assert_eq!(app.clipboard[1].id, 3);
        assert_eq!(app.clipboard[1].parent, Some(2));
        assert_eq!(app.document.undo_len(), history);
        assert_eq!(app.document.is_dirty(), dirty);
        assert!(app.has_clipboard());
    }

    #[test]
    fn cutting_removes_a_group_in_one_step_and_keeps_the_clipboard() {
        let mut app = app_with_entities(3);
        app.select_clicked(1, false, false);
        app.select_clicked(3, true, false);
        let history = app.document.undo_len();

        app.cut_selected();

        assert_eq!(app.document.undo_len(), history + 1);
        assert_eq!(app.document.scene().entities.len(), 1);
        assert_eq!(app.document.scene().entities[0].id, 2);
        assert_eq!(app.clipboard.len(), 2);
        assert!(app.document.selection().is_empty());
        assert!(app.status.contains("Cortadas"));
        app.document.undo();
        assert_eq!(app.document.scene().entities.len(), 3);
        assert_eq!(app.clipboard.len(), 2);
    }

    #[test]
    fn pasting_uses_fresh_ids_and_one_undo_step() {
        let mut app = app_with_entities(2);
        app.select_clicked(1, false, false);
        app.select_clicked(2, true, false);
        app.copy_selected();
        let history = app.document.undo_len();

        app.paste_clipboard();

        let ids: Vec<u64> = app
            .document
            .scene()
            .entities
            .iter()
            .map(|entity| entity.id)
            .collect();
        assert_eq!(ids, vec![1, 2, 3, 4]);
        assert_eq!(app.document.selection(), &[3, 4]);
        assert_eq!(app.document.undo_len(), history + 1);
        assert!(app.status.contains("Pegadas"));

        app.document.undo();
        assert_eq!(app.document.scene().entities.len(), 2);
        app.document.redo();
        assert_eq!(app.document.scene().entities.len(), 4);
    }

    #[test]
    fn pasting_twice_never_reuses_ids() {
        let mut app = app_with_entities(1);
        app.select_clicked(1, false, false);
        app.copy_selected();
        app.paste_clipboard();
        app.paste_clipboard();
        let ids: Vec<u64> = app
            .document
            .scene()
            .entities
            .iter()
            .map(|entity| entity.id)
            .collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn the_clipboard_survives_a_new_scene() {
        let mut app = app_with_entities(2);
        app.select_clicked(1, false, false);
        app.select_clicked(2, true, false);
        app.copy_selected();
        app.new_document();
        assert!(app.document.scene().entities.is_empty());
        assert!(app.has_clipboard());
        app.paste_clipboard();
        assert_eq!(app.document.scene().entities.len(), 2);
    }

    #[test]
    fn copy_cut_and_paste_are_locked_during_play() {
        let mut app = app_with_entities(2);
        app.select_clicked(1, false, false);
        app.select_clicked(2, true, false);
        app.copy_selected();
        let clipboard = app.clipboard.clone();
        app.play();
        app.copy_selected();
        app.cut_selected();
        app.paste_clipboard();
        assert_eq!(app.clipboard, clipboard);
        assert_eq!(app.document.scene().entities.len(), 2);
        assert!(app.status.contains("Detén"));
        app.stop_play();
    }

    #[test]
    fn ctrl_c_x_v_drive_the_clipboard_actions() {
        let ctx = Context::default();
        let mut app = app_with_entities(2);
        app.select_clicked(1, false, false);
        app.select_clicked(2, true, false);
        let history = app.document.undo_len();
        for (key, expected_history) in [
            (egui::Key::C, history),
            (egui::Key::X, history + 1),
            (egui::Key::V, history + 2),
        ] {
            let input = egui::RawInput {
                events: vec![
                    egui::Event::ModifiersChanged(egui::Modifiers::CTRL),
                    egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::CTRL,
                    },
                ],
                ..Default::default()
            };
            let mut output = ctx.run_pass(input, |ctx| app.handle_shortcuts(ctx));
            output.textures_delta.clear();
            assert_eq!(app.document.undo_len(), expected_history, "tecla {key:?}");
        }
        assert_eq!(app.document.scene().entities.len(), 2);
        assert_eq!(app.clipboard.len(), 2);
    }
}
