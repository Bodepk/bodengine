use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use egui::{Context, Ui};
use lunar_editor::{EditorCommand, EditorDocument, EditorError};
use lunar_scenes::{Component, Scene, SceneEntity, SceneError, Transform, normalize_asset_path};

use crate::assets::{AssetKind, AssetLibrary, AssetView, assets_ui};
use crate::canvas::{CanvasDrag, canvas_ui};
use crate::file_picker::{FilePicker, PickerMode, absolute_dir, parent_dir};
use crate::helpers::{
    audio_component, collider2_component, hierarchy_ids, image_component, next_entity_id,
    range_ids, toggle_selection, union_selection,
};
use crate::import_options::{ImportOptions, ReimportWatcher};
use crate::layout;
use crate::lights::{Light, LightMap, MAP_FILE as LIGHT_MAP_FILE};
use crate::materials::{MAP_FILE as MATERIAL_MAP_FILE, Material, MaterialLibrary, MaterialMap};
use crate::play::{PlayState, dynamic_ids, floor_y, step_vertical};
use crate::recent;
use crate::status::{ConsoleFilter, LogEntry};
use crate::tools::{EntityTemplate, TransformMode, TransformSpace};
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

/// Estado completo de los sidecars de material en un punto del historial (E47).
#[derive(Debug, Clone, PartialEq)]
struct MaterialSnapshot {
    library: MaterialLibrary,
    map: MaterialMap,
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
    /// Última entidad usada como ancla para la selección por rango (E12) y
    /// como origen del gizmo en espacio local (E14).
    pub(crate) selection_anchor: Option<u64>,
    /// Portapapeles en memoria (E13): no se guarda en la escena y sobrevive a
    /// abrir otra o crear una nueva.
    clipboard: Vec<SceneEntity>,
    /// Nombre introducido para guardar la selección como plantilla (E15).
    pub(crate) prefab_name_buffer: String,
    /// Copias a crear desde el menú «+ Crear» (E15), acotadas en la GUI.
    pub(crate) prefab_instance_count: usize,
    synced_selection: Vec<u64>,
    synced_revision: u64,
    revision: u64,
    status: String,
    pub(crate) status_log: Vec<LogEntry>,
    pub(crate) console_filter: ConsoleFilter,
    pub(crate) console_search: String,
    /// Línea en curso, historial y navegación de la consola de comandos (E19).
    pub(crate) console_input: String,
    pub(crate) console_history: Vec<String>,
    pub(crate) console_history_index: Option<usize>,
    pub(crate) console_draft: String,
    pub(crate) show_log: bool,
    /// Ventana de historial de comandos (E45); no se persiste.
    pub(crate) show_history: bool,
    /// Informe de validación y su ventana (E46); no se persiste.
    pub(crate) show_validation: bool,
    pub(crate) validation_issues: Vec<crate::validation::Issue>,
    /// Ventana de timeline y su reloj de vista previa (E42); no se persiste.
    pub(crate) show_timeline: bool,
    pub(crate) timeline_state: crate::timeline::TimelineState,
    start_time: Instant,
    pub(crate) assets: AssetLibrary,
    /// GUIDs leídos de los `.meta` y alias de rutas movidas (ADR-034).
    pub(crate) guid_index: crate::guids::GuidIndex,
    pub(crate) assets_root_text: String,
    pub(crate) asset_search: String,
    pub(crate) asset_folder: String,
    pub(crate) asset_kind_filter: Option<AssetKind>,
    pub(crate) asset_view: AssetView,
    pub(crate) selected_asset: Option<String>,
    /// Opciones del asset seleccionado y ruta a la que pertenecen (E17).
    pub(crate) asset_options: ImportOptions,
    asset_options_path: Option<String>,
    /// Sondeo cooperativo de huellas en disco (E17, ADR-036).
    reimport: ReimportWatcher,
    /// Enlaces declarativos del fichero lateral `scripts.map.txt` (E20).
    pub(crate) script_map: crate::scripts::ScriptMap,
    script_undo: Vec<crate::scripts::ScriptMap>,
    script_redo: Vec<crate::scripts::ScriptMap>,
    script_map_stamp: u64,
    script_diag_stamp: u64,
    script_last_poll: u64,
    /// Materiales editor-owned y su asignación por entidad (E47, ADR-047).
    pub(crate) material_library: MaterialLibrary,
    pub(crate) material_map: MaterialMap,
    material_undo: Vec<MaterialSnapshot>,
    material_redo: Vec<MaterialSnapshot>,
    material_map_stamp: u64,
    pub(crate) material_name_buffer: String,
    /// Luces editor-owned por entidad y su historial paralelo (E49, ADR-050).
    pub(crate) light_map: LightMap,
    light_undo: Vec<LightMap>,
    light_redo: Vec<LightMap>,
    light_map_stamp: u64,
    pub(crate) view_mode: ViewMode,
    /// Relleno de la vista 3D (ADR-037); no se persiste, como la cámara.
    pub(crate) shading_mode: crate::view3d::ShadingMode,
    pub(crate) camera3d: Camera3D,
    pub(crate) view3d_drag: Drag3D,
    pub(crate) view_center: [f32; 2],
    pub(crate) zoom: f32,
    pub(crate) show_grid: bool,
    pub(crate) snap: bool,
    pub(crate) transform_mode: TransformMode,
    /// Espacio del gizmo de grupo (E14), persistido con el resto de preferencias.
    pub(crate) transform_space: TransformSpace,
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
        let guid_index = crate::guids::load(&assets.root, &assets.entries);
        let mut reimport = ReimportWatcher::default();
        let samples = crate::import_options::sample(&assets.root, &assets.entries);
        let _ = reimport.poll(&samples, 0, 0);

        let mut app = Self {
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
            prefab_name_buffer: String::new(),
            prefab_instance_count: crate::prefabs::clamp_instance_count(1),
            synced_selection: Vec::new(),
            synced_revision: 0,
            revision: 0,
            status,
            status_log,
            console_filter: ConsoleFilter::default(),
            console_search: String::new(),
            console_input: String::new(),
            console_history: Vec::new(),
            console_history_index: None,
            console_draft: String::new(),
            show_log: false,
            show_history: false,
            show_validation: false,
            validation_issues: Vec::new(),
            show_timeline: false,
            timeline_state: crate::timeline::TimelineState::default(),
            start_time,
            assets,
            guid_index,
            assets_root_text,
            asset_search: String::new(),
            asset_folder: String::new(),
            asset_kind_filter: None,
            asset_view: settings.asset_view,
            selected_asset: None,
            asset_options: ImportOptions::default(),
            asset_options_path: None,
            reimport,
            script_map: crate::scripts::ScriptMap::default(),
            script_undo: Vec::new(),
            script_redo: Vec::new(),
            script_map_stamp: 0,
            script_diag_stamp: 0,
            script_last_poll: 0,
            material_library: MaterialLibrary::default(),
            material_map: MaterialMap::default(),
            material_undo: Vec::new(),
            material_redo: Vec::new(),
            material_map_stamp: 0,
            material_name_buffer: String::new(),
            light_map: LightMap::default(),
            light_undo: Vec::new(),
            light_redo: Vec::new(),
            light_map_stamp: 0,
            view_mode: ViewMode::default(),
            shading_mode: settings.shading_mode,
            camera3d: Camera3D::default(),
            view3d_drag: Drag3D::default(),
            view_center: settings.center,
            zoom: settings.zoom,
            show_grid: settings.show_grid,
            snap: settings.snap,
            transform_mode: TransformMode::default(),
            transform_space: settings.transform_space,
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
        };
        app.load_scripts();
        app.load_materials();
        app.load_lights();
        app
    }

    /// Ruta del fichero lateral de enlaces junto a la escena actual.
    pub(crate) fn script_map_path(&self) -> PathBuf {
        Self::sidecar_path(&self.path, crate::scripts::MAP_FILE)
    }

    fn script_diagnostics_path(&self) -> PathBuf {
        Self::sidecar_path(&self.path, crate::scripts::DIAGNOSTICS_FILE)
    }

    fn sidecar_path(scene: &Path, name: &str) -> PathBuf {
        scene.parent().unwrap_or_else(|| Path::new(".")).join(name)
    }

    /// Lee el mapa lateral y los diagnósticos del juego sin tocar la escena.
    pub(crate) fn load_scripts(&mut self) {
        let path = self.script_map_path();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let (map, errors) = crate::scripts::ScriptMap::parse(&text);
        self.script_map = map;
        self.script_map_stamp = crate::import_options::stamp(&path);
        self.script_undo.clear();
        self.script_redo.clear();
        for error in errors {
            self.set_status_level(crate::status::Level::Error, error);
        }
        self.load_script_diagnostics();
    }

    /// Ruta del fichero lateral de materiales junto a la escena actual.
    pub(crate) fn material_map_path(&self) -> PathBuf {
        Self::sidecar_path(&self.path, MATERIAL_MAP_FILE)
    }

    /// Lee materiales y asignaciones sin mutar la escena (E47, ADR-047).
    pub(crate) fn load_materials(&mut self) {
        self.material_library = MaterialLibrary::load(&self.assets.root);
        let path = self.material_map_path();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        self.material_map = MaterialMap::decode(&text);
        let ids: Vec<u64> = self
            .document
            .scene()
            .entities
            .iter()
            .map(|entity| entity.id)
            .collect();
        self.material_map.retain(&ids);
        self.material_map_stamp = crate::import_options::stamp(&path);
        self.material_undo.clear();
        self.material_redo.clear();
    }

    fn write_materials_for(&mut self, path: &Path) -> Result<(), String> {
        let sidecar = Self::sidecar_path(path, MATERIAL_MAP_FILE);
        match crate::scripts::atomic_write(&sidecar, &self.material_map.encode()) {
            Ok(()) => {
                self.material_map_stamp = crate::import_options::stamp(&sidecar);
                Ok(())
            }
            Err(error) => Err(format!("No se pudo guardar {}: {error}", sidecar.display())),
        }
    }

    fn material_snapshot(&self) -> MaterialSnapshot {
        MaterialSnapshot {
            library: self.material_library.clone(),
            map: self.material_map.clone(),
        }
    }

    fn restore_materials(&mut self, snapshot: MaterialSnapshot) {
        let previous = self.material_snapshot();
        self.material_library = snapshot.library;
        self.material_map = snapshot.map;
        if previous.library != self.material_library
            && let Err(error) = self
                .material_library
                .sync_to_disk(&self.assets.root, &previous.library)
        {
            self.set_status_level(crate::status::Level::Error, error);
        }
        if let Err(error) = self.write_materials_for(&self.path.clone()) {
            self.set_status_level(crate::status::Level::Error, error);
        }
    }

    /// Asigna el material seleccionado a la primera entidad seleccionada.
    /// El cambio queda apareado a un paso del historial y es deshacible.
    pub(crate) fn assign_material_to_selection(&mut self, path: Option<&str>) {
        if !self.ensure_editable() {
            return;
        }
        let Some(id) = self.document.selection().first().copied() else {
            self.set_status("Selecciona una entidad para asignar el material");
            return;
        };
        self.assign_material_to_entity(id, path);
    }

    /// Asigna un material a una entidad concreta, también desde la vista 3D (E48).
    pub(crate) fn assign_material_to_entity(&mut self, id: u64, path: Option<&str>) {
        if !self.ensure_editable() {
            return;
        }
        if let Some(path) = path
            && self.material_library.get(path).is_none()
        {
            self.set_status(format!("Material no cargado: {path}"));
            return;
        }
        if self.material_map.get(id) == path {
            return;
        }
        let previous = self.material_snapshot();
        if self.try_apply(EditorCommand::MaterialEdit).is_err() {
            return;
        }
        self.material_undo.push(previous);
        self.material_redo.clear();
        self.material_map.set(id, path.map(str::to_owned));
        if let Err(error) = self.write_materials_for(&self.path.clone()) {
            self.set_status_level(crate::status::Level::Error, error);
            return;
        }
        self.set_status(match path {
            Some(path) => format!("Material {path} asignado a #{id}"),
            None => format!("Material retirado de #{id}"),
        });
    }

    pub(crate) fn commit_material_edit(&mut self, path: &str, material: Material) {
        if !self.ensure_editable() {
            return;
        }
        if self.material_library.get(path) == Some(&material) {
            return;
        }
        let previous = self.material_snapshot();
        if self.try_apply(EditorCommand::MaterialEdit).is_err() {
            return;
        }
        self.material_undo.push(previous);
        self.material_redo.clear();
        self.material_library.insert(path.to_owned(), material);
        if let Err(error) = self.material_library.save(&self.assets.root, path) {
            self.set_status_level(
                crate::status::Level::Error,
                format!("No se pudo guardar {path}: {error}"),
            );
            return;
        }
        self.set_status(format!("Material guardado: {path}"));
    }

    /// Crea un material con nombre seguro y le asigna GUID mediante `.meta`.
    pub(crate) fn create_material(&mut self, ctx: &Context) {
        if !self.ensure_editable() {
            return;
        }
        let stem: String = self
            .material_name_buffer
            .trim()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                    character
                } else {
                    '_'
                }
            })
            .collect();
        if stem.is_empty() {
            self.set_status("Escribe un nombre para el material");
            return;
        }
        let path = format!("{stem}{}", crate::materials::MATERIAL_SUFFIX);
        if self.material_library.get(&path).is_some() {
            self.set_status(format!("Ya existe el material {path}"));
            return;
        }
        let material = Material {
            name: stem.clone(),
            ..Material::default()
        };
        let previous = self.material_snapshot();
        if self.try_apply(EditorCommand::MaterialEdit).is_err() {
            return;
        }
        self.material_undo.push(previous);
        self.material_redo.clear();
        self.material_library.insert(path.clone(), material);
        if let Err(error) = self.material_library.save(&self.assets.root, &path) {
            self.set_status_level(
                crate::status::Level::Error,
                format!("No se pudo crear {path}: {error}"),
            );
            return;
        }
        self.assets.reload(ctx);
        self.rebuild_guids();
        self.selected_asset = Some(path.clone());
        self.material_name_buffer.clear();
        self.set_status(format!("Creado {path}"));
    }

    /// Ruta del fichero lateral de luces junto a la escena actual.
    pub(crate) fn light_map_path(&self) -> PathBuf {
        Self::sidecar_path(&self.path, LIGHT_MAP_FILE)
    }

    /// Lee luces y asignaciones sin mutar la escena (E49, ADR-050).
    pub(crate) fn load_lights(&mut self) {
        self.light_map = crate::lights::load(self.path.parent().unwrap_or_else(|| Path::new(".")));
        let ids: Vec<u64> = self
            .document
            .scene()
            .entities
            .iter()
            .map(|entity| entity.id)
            .collect();
        self.light_map.retain(&ids);
        self.light_map_stamp = crate::import_options::stamp(&self.light_map_path());
        self.light_undo.clear();
        self.light_redo.clear();
    }

    fn write_lights_for(&mut self, path: &Path) -> Result<(), String> {
        let sidecar = Self::sidecar_path(path, LIGHT_MAP_FILE);
        match crate::scripts::atomic_write(&sidecar, &self.light_map.encode()) {
            Ok(()) => {
                self.light_map_stamp = crate::import_options::stamp(&sidecar);
                Ok(())
            }
            Err(error) => Err(format!("No se pudo guardar {}: {error}", sidecar.display())),
        }
    }

    fn restore_lights(&mut self, snapshot: LightMap) {
        self.light_map = snapshot;
        if let Err(error) = self.write_lights_for(&self.path.clone()) {
            self.set_status_level(crate::status::Level::Error, error);
        }
    }

    /// Crea, edita o retira la luz de una entidad con undo/redo (ADR-050).
    pub(crate) fn commit_light(&mut self, id: u64, light: Option<Light>) {
        if !self.ensure_editable() {
            return;
        }
        if self.light_map.get(id) == light.as_ref() {
            return;
        }
        let previous = self.light_map.clone();
        if self.try_apply(EditorCommand::LightEdit).is_err() {
            return;
        }
        self.light_undo.push(previous);
        self.light_redo.clear();
        match light {
            Some(light) => self.light_map.set(id, light),
            None => self.light_map.remove(id),
        }
        if let Err(error) = self.write_lights_for(&self.path.clone()) {
            self.set_status_level(crate::status::Level::Error, error);
            return;
        }
        self.set_status(if light.is_some() {
            format!("Luz guardada para #{id}")
        } else {
            format!("Luz retirada de #{id}")
        });
    }

    fn load_script_diagnostics(&mut self) {
        let path = self.script_diagnostics_path();
        self.script_diag_stamp = crate::import_options::stamp(&path);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for error in crate::scripts::diagnostics(&text) {
            self.set_status_level(crate::status::Level::Error, error);
        }
    }

    /// Recalcula el informe de validación sin mutar la escena (E46).
    pub(crate) fn run_validation(&mut self) {
        let text = std::fs::read_to_string(self.script_diagnostics_path()).unwrap_or_default();
        self.validation_issues = crate::validation::validate(
            self.document.scene(),
            &self.assets.entries,
            &self.guid_index,
            &self.script_map,
            &text,
        );
        self.set_status(format!(
            "Validación: {} problema(s)",
            self.validation_issues.len()
        ));
    }

    /// Edita un enlace como un único paso de undo, apareado con un comando
    /// atómico del documento para que la pila del editor siga alineada.
    pub(crate) fn apply_script_binding(
        &mut self,
        id: u64,
        binding: Option<crate::scripts::Binding>,
    ) {
        if !self.ensure_editable() || self.script_map.get(id) == binding.as_ref() {
            return;
        }
        if self
            .try_apply(EditorCommand::SetTransforms {
                transforms: Vec::new(),
            })
            .is_err()
        {
            return;
        }
        self.script_map.set(id, binding);
        self.script_map_stamp = crate::import_options::stamp(&self.script_map_path());
        self.set_status(format!("Comportamiento actualizado para #{id}"));
    }

    /// Sondeo de diagnósticos y de cambios externos del fichero lateral.
    pub(crate) fn tick_scripts(&mut self, now_ms: u64) {
        const INTERVAL_MS: u64 = 750;
        if now_ms < self.script_last_poll + INTERVAL_MS {
            return;
        }
        self.script_last_poll = now_ms;
        let diagnostics_path = self.script_diagnostics_path();
        let stamp = crate::import_options::stamp(&diagnostics_path);
        if stamp != self.script_diag_stamp {
            self.script_diag_stamp = stamp;
            self.load_script_diagnostics();
        }
        let map_path = self.script_map_path();
        let stamp = crate::import_options::stamp(&map_path);
        if stamp != 0 && stamp != self.script_map_stamp && !self.document.is_dirty() {
            self.load_scripts();
            self.set_status("Enlaces de scripts recargados desde disco");
        }
        let light_path = self.light_map_path();
        let stamp = crate::import_options::stamp(&light_path);
        if stamp != 0 && stamp != self.light_map_stamp && !self.document.is_dirty() {
            self.load_lights();
            self.set_status("Luces recargadas desde disco");
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

    /// Mensaje con severidad elegida explícitamente (errores de consola, avisos
    /// de reimportado y opciones de asset inválidas).
    pub(crate) fn set_status_level(
        &mut self,
        level: crate::status::Level,
        message: impl Into<String>,
    ) {
        let message = message.into();
        let elapsed = self.start_time.elapsed().as_secs_f32();
        crate::status::push_level(&mut self.status_log, level, message.clone(), elapsed);
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
        let _ = self.try_apply(command);
    }

    /// Igual que `apply`, pero devuelve el error para la consola de comandos.
    pub(crate) fn try_apply(&mut self, command: EditorCommand) -> Result<(), String> {
        if !self.ensure_editable() {
            return Err("Detén la simulación antes de editar".to_owned());
        }
        match self.document.apply(command) {
            Ok(()) => {
                self.revision += 1;
                self.set_status("");
                let previous = self.script_map.clone();
                self.script_undo.push(previous);
                self.script_redo.clear();
                let ids: Vec<u64> = self
                    .document
                    .scene()
                    .entities
                    .iter()
                    .map(|entity| entity.id)
                    .collect();
                self.script_map.retain(&ids);
                self.material_map.retain(&ids);
                self.light_map.retain(&ids);
                Ok(())
            }
            Err(error) => {
                let message = error.to_string();
                self.set_status_level(crate::status::Level::Error, message.clone());
                Err(message)
            }
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

    /// Vista de las preferencias que se persisten (ADR-031/044).
    pub(crate) fn editor_settings(&self) -> crate::settings::EditorSettings {
        crate::settings::EditorSettings {
            assets_root: self.assets_root_text.clone(),
            center: self.view_center,
            zoom: self.zoom,
            show_grid: self.show_grid,
            snap: self.snap,
            transform_space: self.transform_space,
            shading_mode: self.shading_mode,
            asset_view: self.asset_view,
        }
    }

    /// Persiste las preferencias de la GUI (ADR-031). Un fallo de escritura no
    /// altera el estado del editor.
    pub(crate) fn save_settings(&self) {
        let _ = crate::settings::save(&self.editor_settings());
    }

    pub(crate) fn undo(&mut self) {
        if !self.ensure_editable() {
            return;
        }
        if self.document.undo() {
            if let Some(previous) = self.script_undo.pop() {
                self.script_redo
                    .push(std::mem::replace(&mut self.script_map, previous));
            }
            if let Some(previous) = self.material_undo.pop() {
                let current = self.material_snapshot();
                self.material_redo.push(current);
                self.restore_materials(previous);
            }
            if let Some(previous) = self.light_undo.pop() {
                let current = self.light_map.clone();
                self.light_redo.push(current);
                self.restore_lights(previous);
            }
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
            if let Some(next) = self.script_redo.pop() {
                self.script_undo
                    .push(std::mem::replace(&mut self.script_map, next));
            }
            if let Some(next) = self.material_redo.pop() {
                let current = self.material_snapshot();
                self.material_undo.push(current);
                self.restore_materials(next);
            }
            if let Some(next) = self.light_redo.pop() {
                let current = self.light_map.clone();
                self.light_undo.push(current);
                self.restore_lights(next);
            }
            self.revision += 1;
            self.set_status("");
        } else {
            self.set_status("Nada para rehacer");
        }
    }

    /// Deshace hasta dejar `remaining` operaciones, con el mismo efecto sobre
    /// `scripts.map.txt` que una sucesión de `undo` (E45).
    pub(crate) fn undo_to(&mut self, remaining: usize) {
        if !self.ensure_editable() {
            return;
        }
        let target = remaining.min(self.document.undo_len());
        while self.document.undo_len() > target {
            self.undo();
        }
    }

    /// Rehace hasta dejar `remaining` operaciones pendientes (E45).
    pub(crate) fn redo_to(&mut self, remaining: usize) {
        if !self.ensure_editable() {
            return;
        }
        let target = remaining.min(self.document.redo_len());
        while self.document.redo_len() > target {
            self.redo();
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
                self.load_scripts();
                self.load_materials();
                self.load_lights();
                self.refresh_guids();
                let (rewritten, broken) = self.rewrite_asset_paths();
                let suffix = Self::describe_guid_changes(rewritten, &broken);
                self.set_status(format!("Abierta {path:?}{suffix}"));
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

    /// Escribe el mapa lateral tras un guardado correcto de la escena.
    fn write_scripts_for(&mut self, path: &Path) -> Result<(), String> {
        let sidecar = Self::sidecar_path(path, crate::scripts::MAP_FILE);
        match crate::scripts::atomic_write(&sidecar, &self.script_map.encode()) {
            Ok(()) => {
                self.script_map_stamp = crate::import_options::stamp(&sidecar);
                Ok(())
            }
            Err(error) => Err(format!("No se pudo guardar {}: {error}", sidecar.display())),
        }
    }

    fn save_to(&mut self, path: PathBuf) -> bool {
        if !self.ensure_editable() {
            return false;
        }
        self.refresh_guids();
        let (rewritten, broken) = self.rewrite_asset_paths();
        let suffix = Self::describe_guid_changes(rewritten, &broken);
        match self.document.save_to_path(&path) {
            Ok(()) => {
                self.path = path.clone();
                match self
                    .write_scripts_for(&path)
                    .and_then(|()| self.write_materials_for(&path))
                    .and_then(|()| self.write_lights_for(&path))
                {
                    Ok(()) => {
                        self.set_status(format!("Guardada {path:?}{suffix}"));
                        self.remember(path);
                        true
                    }
                    Err(error) => {
                        self.set_status_level(crate::status::Level::Error, error);
                        false
                    }
                }
            }
            Err(error) => {
                self.set_status(format!("No se pudo guardar {path:?}: {error}"));
                false
            }
        }
    }

    /// Guardado usado por la consola: no escribe en `status_log` para que el
    /// resultado del comando sea el único mensaje registrado.
    pub(crate) fn console_save(&mut self, path: Option<&str>) -> Result<String, String> {
        if !self.ensure_editable() {
            return Err("Detén la simulación antes de editar".to_owned());
        }
        let target = path.map(PathBuf::from).unwrap_or_else(|| self.path.clone());
        if target.as_os_str().is_empty() {
            return Err("La escena no tiene ruta: usa save <ruta>".to_owned());
        }
        self.refresh_guids();
        let (rewritten, broken) = self.rewrite_asset_paths();
        let suffix = Self::describe_guid_changes(rewritten, &broken);
        match self.document.save_to_path(&target) {
            Ok(()) => {
                self.path = target.clone();
                self.write_scripts_for(&target)?;
                self.write_materials_for(&target)?;
                self.write_lights_for(&target)?;
                self.remember(target.clone());
                Ok(format!("Guardada {target:?}{suffix}"))
            }
            Err(error) => Err(format!("No se pudo guardar {target:?}: {error}")),
        }
    }

    /// Apertura usada por la consola; respeta el aviso de cambios sin guardar.
    pub(crate) fn console_load(&mut self, text: &str) -> Result<String, String> {
        if !self.ensure_editable() {
            return Err("Detén la simulación antes de editar".to_owned());
        }
        let path = PathBuf::from(text);
        if self.document.is_dirty() {
            self.pending_action = Some(PendingAction::Open(path));
            return Ok("Cambios sin guardar: confirma antes de abrir otra escena".to_owned());
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
                self.load_scripts();
                self.load_materials();
                self.load_lights();
                self.refresh_guids();
                let (rewritten, broken) = self.rewrite_asset_paths();
                let suffix = Self::describe_guid_changes(rewritten, &broken);
                self.remember(path.clone());
                Ok(format!("Abierta {path:?}{suffix}"))
            }
            Err(error) => Err(format!("No se pudo abrir {path:?}: {error}")),
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
        self.script_map = crate::scripts::ScriptMap::default();
        self.script_undo.clear();
        self.script_redo.clear();
        self.script_map_stamp = 0;
        self.script_diag_stamp = 0;
        self.material_map = crate::materials::MaterialMap::default();
        self.material_undo.clear();
        self.material_redo.clear();
        self.material_map_stamp = 0;
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

    /// Guarda la selección actual como plantilla en un solo paso de undo (E15).
    pub(crate) fn save_prefab(&mut self, name: &str) {
        if !self.ensure_editable() {
            return;
        }
        let name = name.trim();
        if name.is_empty() {
            self.set_status("Escribe un nombre para la plantilla");
            return;
        }
        let ids = self.document.selection().to_vec();
        if ids.is_empty() {
            self.set_status("Selecciona entidades para guardar una plantilla");
            return;
        }
        let entities = crate::helpers::selection_entities(self.document.scene(), &ids);
        let count = entities.len();
        let history = self.document.undo_len();
        self.apply(EditorCommand::SavePrefab {
            name: name.to_owned(),
            entities,
        });
        if self.document.undo_len() > history {
            self.prefab_name_buffer.clear();
            self.set_status(format!(
                "Guardada plantilla «{name}» con {count} entidad(es)"
            ));
        }
    }

    /// Instancia una plantilla tantas veces como indique el menú «+ Crear» (E15).
    pub(crate) fn instantiate_prefab(&mut self, name: &str) {
        if !self.ensure_editable() {
            return;
        }
        if !crate::prefabs::prefab_names(self.document.scene())
            .iter()
            .any(|candidate| candidate == name)
        {
            self.set_status(format!("Plantilla no encontrada: {name}"));
            return;
        }
        let copies = crate::prefabs::clamp_instance_count(self.prefab_instance_count);
        let entities = crate::prefabs::instantiate(self.document.scene(), name, copies);
        if entities.is_empty() {
            self.set_status(format!("No se pudo instanciar «{name}»"));
            return;
        }
        let created = entities.len();
        let history = self.document.undo_len();
        self.apply(EditorCommand::PasteEntities { entities });
        if self.document.undo_len() > history {
            self.set_status(format!(
                "Instanciada «{name}» ×{copies} ({created} entidad(es))"
            ));
        }
    }

    /// Aplica los cambios de una plantilla a sus instancias en un solo paso (E43).
    pub(crate) fn apply_prefab(&mut self, name: &str) {
        if !self.ensure_editable() {
            return;
        }
        let changes = crate::prefabs::propagate(self.document.scene(), name);
        if changes.transforms.is_empty() && changes.components.is_empty() {
            self.set_status(format!("No hay instancias de «{name}» que actualizar"));
            return;
        }
        let skipped = changes.skipped;
        let history = self.document.undo_len();
        self.apply(EditorCommand::ApplyPrefab {
            name: name.to_owned(),
            transforms: changes.transforms,
            components: changes.components,
        });
        if self.document.undo_len() > history {
            let message = if skipped == 0 {
                format!("Actualizadas las instancias de «{name}»")
            } else {
                format!(
                    "Actualizadas las instancias de «{name}»; {skipped} omitida(s) por estructura distinta"
                )
            };
            if skipped == 0 {
                self.set_status(message);
            } else {
                self.set_status_level(crate::status::Level::Warn, message);
            }
        }
    }

    /// Sustituye una plantilla por la selección actual, con descendientes (E43).
    pub(crate) fn update_prefab(&mut self, name: &str) {
        if !self.ensure_editable() {
            return;
        }
        let selected = self.document.selection().to_vec();
        if selected.is_empty() {
            self.set_status("Selecciona la instancia con los cambios");
            return;
        }
        let entities = crate::prefabs::template_from_scene(self.document.scene(), &selected);
        let count = entities.len();
        let history = self.document.undo_len();
        self.apply(EditorCommand::UpdatePrefab {
            name: name.to_owned(),
            entities,
        });
        if self.document.undo_len() > history {
            self.set_status(format!(
                "Actualizada plantilla «{name}» con {count} entidad(es)"
            ));
        }
    }

    /// Borra una plantilla y deja las instancias como entidades normales (E15).
    pub(crate) fn delete_prefab(&mut self, name: &str) {
        if !self.ensure_editable() {
            return;
        }
        let references = crate::prefabs::count_prefab_refs(self.document.scene(), name);
        let history = self.document.undo_len();
        self.apply(EditorCommand::DeletePrefab {
            name: name.to_owned(),
        });
        if self.document.undo_len() > history {
            self.set_status(format!(
                "Borrada plantilla «{name}»; {references} referencia(s) limpiadas"
            ));
        }
    }

    pub(crate) fn has_clipboard(&self) -> bool {
        !self.clipboard.is_empty()
    }

    /// Lee los `.meta` existentes sin crear ficheros; se usa al abrir y guardar.
    fn refresh_guids(&mut self) {
        if self.assets.error.is_some() {
            self.guid_index = crate::guids::GuidIndex::default();
            return;
        }
        self.guid_index = crate::guids::load(&self.assets.root, &self.assets.entries);
    }

    /// Genera los `.meta` ausentes y persiste el índice tras un escaneo explícito.
    fn rebuild_guids(&mut self) {
        if self.assets.error.is_some() {
            self.guid_index = crate::guids::GuidIndex::default();
            return;
        }
        self.guid_index = crate::guids::rebuild(&self.assets.root, &self.assets.entries);
    }

    /// Reescribe en memoria las rutas resueltas por GUID en un solo paso de
    /// undo. Las referencias desconocidas se devuelven para informar, sin tocarlas.
    fn rewrite_asset_paths(&mut self) -> (usize, Vec<String>) {
        if self.assets.error.is_some() || self.guid_index.is_empty() {
            return (0, Vec::new());
        }
        let broken = crate::guids::broken_paths(self.document.scene(), &self.guid_index);
        let changes = crate::guids::path_changes(self.document.scene(), &self.guid_index);
        if changes.is_empty() {
            return (0, broken);
        }
        let count = changes.len();
        let history = self.document.undo_len();
        self.apply(EditorCommand::SetComponents { changes });
        if self.document.undo_len() > history {
            (count, broken)
        } else {
            (0, broken)
        }
    }

    fn describe_guid_changes(rewritten: usize, broken: &[String]) -> String {
        let mut suffix = String::new();
        if rewritten > 0 {
            suffix.push_str(&format!("; {rewritten} ruta(s) GUID actualizada(s)"));
        }
        if !broken.is_empty() {
            suffix.push_str(&format!(
                "; {} referencia(s) rota(s): {}",
                broken.len(),
                broken.join(", ")
            ));
        }
        suffix
    }

    pub(crate) fn reload_assets(&mut self, ctx: &Context) {
        self.assets = AssetLibrary::new(PathBuf::from(&self.assets_root_text));
        self.assets.reload(ctx);
        self.rebuild_guids();
        self.load_materials();
        let (rewritten, broken) = self.rewrite_asset_paths();
        let suffix = Self::describe_guid_changes(rewritten, &broken);
        let status = match &self.assets.error {
            Some(error) => error.clone(),
            None => format!(
                "{} asset(s), {} GUID(s) en {}{suffix}",
                self.assets.entries.len(),
                self.guid_index.len(),
                self.assets.root.display()
            ),
        };
        self.set_status(status);
        self.save_settings();
    }

    /// Carga las opciones del asset seleccionado solo cuando cambia la ruta,
    /// para que los campos editables no se sobrescriban en cada frame.
    pub(crate) fn refresh_asset_options(&mut self) {
        let Some(path) = self.selected_asset.clone() else {
            self.asset_options_path = None;
            return;
        };
        if self.asset_options_path.as_deref() == Some(path.as_str()) {
            return;
        }
        self.asset_options = crate::import_options::load_for(&self.assets.root, &path);
        self.asset_options_path = Some(path);
    }

    /// Persiste las opciones en el `.meta` y recarga las miniaturas afectadas.
    pub(crate) fn save_asset_options(&mut self, ctx: &Context) {
        let Some(path) = self.selected_asset.clone() else {
            return;
        };
        let options = self.asset_options;
        match crate::import_options::save_for(&self.assets.root, &path, &options) {
            Ok(()) => {
                let changed_texture = self.assets.kind(&path) == Some(AssetKind::Image);
                self.assets.reload(ctx);
                self.refresh_guids();
                self.asset_options_path = None;
                self.refresh_asset_options();
                let suffix = if changed_texture {
                    " (miniatura actualizada)"
                } else {
                    ""
                };
                self.set_status(format!("Opciones guardadas en {path}.meta{suffix}"));
            }
            Err(error) => self.set_status_level(
                crate::status::Level::Error,
                format!("No se pudieron guardar opciones de {path}: {error}"),
            ),
        }
    }

    /// Sondeo cooperativo de cambios en disco. No crea hilos y solo muta la
    /// escena a través de `rewrite_asset_paths` cuando una ruta resuelta cambia.
    pub(crate) fn tick_reimport(&mut self, ctx: &Context, now_ms: u64) {
        const INTERVAL_MS: u64 = 750;
        let samples = crate::import_options::sample(&self.assets.root, &self.assets.entries);
        let missing = self.reimport.missing(&samples);
        let changed = self.reimport.poll(&samples, now_ms, INTERVAL_MS);
        if changed.is_empty() && missing.is_empty() {
            return;
        }
        self.assets.reload(ctx);
        self.refresh_guids();
        self.asset_options_path = None;
        self.refresh_asset_options();
        let (rewritten, broken) = self.rewrite_asset_paths();
        let suffix = Self::describe_guid_changes(rewritten, &broken);
        let message = if missing.is_empty() {
            format!(
                "Reimportado(s): {} asset(s) cambiado(s){suffix}",
                changed.len()
            )
        } else {
            format!(
                "Reimportado(s): {} cambiado(s), {} desaparecido(s){suffix}",
                changed.len(),
                missing.len()
            )
        };
        self.set_status(message);
    }

    pub(crate) fn create_entity_from_asset(&mut self, asset: &str, position: [f32; 2]) {
        if !self.ensure_editable() {
            return;
        }
        let Some(kind) = self.assets.kind(asset) else {
            self.set_status(format!("Asset no cargado: {asset}"));
            return;
        };
        if kind == AssetKind::Material {
            self.assign_material_to_selection(Some(asset));
            return;
        }
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
            AssetKind::Material => unreachable!("los materiales se asignan arriba"),
        };
        self.apply(EditorCommand::AddComponent { id, component });
        let scale = if kind == AssetKind::Mesh {
            crate::import_options::load_for(&self.assets.root, asset)
                .gltf
                .scale
        } else {
            1.0
        };
        self.apply(EditorCommand::SetTransform {
            id,
            transform: Transform {
                translation: [position[0], position[1], 0.0],
                scale: [scale; 3],
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
                        AssetKind::Material => "material",
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
                    AssetKind::Material => unreachable!("ninguna plantilla crea materiales"),
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
        if template == EntityTemplate::Light3D {
            self.commit_light(id, Some(Light::default()));
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

    /// Avanza la vista previa de la timeline sin mutar la escena (E42).
    pub(crate) fn tick_timeline(&mut self, dt: f32) {
        if !self.timeline_state.playing {
            return;
        }
        let Some(id) = self.document.selection().first().copied() else {
            return;
        };
        let Some((_, clips)) = crate::timeline::selected_clips(self.document.scene(), id) else {
            return;
        };
        let Some(clip) = clips.get(self.timeline_state.clip).cloned() else {
            return;
        };
        crate::timeline::advance(&mut self.timeline_state, dt, &clip);
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
            toggle_space: input.key_pressed(egui::Key::Q),
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
        if actions.toggle_space {
            self.transform_space = self.transform_space.toggle();
            self.save_settings();
            self.set_status(format!("Gizmo en espacio {}", self.transform_space.label()));
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
        let now_ms = (ctx.input(|input| input.time) * 1000.0) as u64;
        self.tick_play(dt);
        self.tick_timeline(dt);
        self.tick_reimport(&ctx, now_ms);
        self.tick_scripts(now_ms);
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
        let mut validation_open = self.show_validation;
        if validation_open {
            egui::Window::new("Validación de escena")
                .open(&mut validation_open)
                .show(&ctx, |ui| crate::validation::validation_ui(self, ui));
            self.show_validation = validation_open;
        }
        let mut history_open = self.show_history;
        if history_open {
            egui::Window::new("Historial")
                .open(&mut history_open)
                .show(&ctx, |ui| crate::history::history_ui(self, ui));
            self.show_history = history_open;
        }
        let mut timeline_open = self.show_timeline;
        if timeline_open {
            egui::Window::new("Timeline de animación")
                .open(&mut timeline_open)
                .show(&ctx, |ui| crate::timeline::timeline_ui(self, ui));
            self.show_timeline = timeline_open;
        }
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
    fn material_assignment_is_undoable_and_written_to_sidecar() {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        let root = std::env::temp_dir().join(format!("lunar_mat_app_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        app.assets.root = root.clone();
        app.path = root.join("scene.json");
        app.document = EditorDocument::new();
        app.apply(EditorCommand::CreateEntity { id: 1, name: None });
        app.material_library
            .insert("m.material.json".to_owned(), Material::default());

        app.assign_material_to_selection(Some("m.material.json"));
        assert_eq!(app.material_map.get(1), Some("m.material.json"));
        let sidecar = std::fs::read_to_string(app.material_map_path()).expect("sidecar");
        assert!(sidecar.contains("1=m.material.json"), "{sidecar}");
        assert!(app.document.can_undo());
        assert_eq!(EditorCommand::MaterialEdit.label(), "Editar material");

        app.undo();
        assert_eq!(app.material_map.get(1), None);
        let sidecar = std::fs::read_to_string(app.material_map_path()).expect("sidecar");
        assert!(!sidecar.contains("1=m.material.json"), "{sidecar}");

        app.redo();
        assert_eq!(app.material_map.get(1), Some("m.material.json"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn light_edits_are_undoable_and_written_to_sidecar() {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        let root = std::env::temp_dir().join(format!("lunar_light_app_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        app.path = root.join("scene.json");
        app.document = EditorDocument::new();
        app.apply(EditorCommand::CreateEntity { id: 1, name: None });
        let history = app.document.undo_len();

        app.commit_light(1, Some(Light::default()));
        assert_eq!(app.light_map.get(1), Some(&Light::default()));
        assert_eq!(app.document.undo_len(), history + 1);
        let sidecar = std::fs::read_to_string(app.light_map_path()).expect("sidecar");
        assert!(sidecar.contains("1=d:"), "{sidecar}");

        app.undo();
        assert_eq!(app.light_map.get(1), None);
        let sidecar = std::fs::read_to_string(app.light_map_path()).expect("sidecar");
        assert!(!sidecar.contains("1="), "{sidecar}");

        app.redo();
        assert_eq!(app.light_map.get(1), Some(&Light::default()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn light_template_creates_a_default_light() {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        let root = std::env::temp_dir().join(format!("lunar_light_tpl_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        app.path = root.join("scene.json");
        app.document = EditorDocument::new();

        app.create_from_template(EntityTemplate::Light3D);

        assert_eq!(app.document.scene().entities.len(), 1);
        assert_eq!(
            app.document.scene().entities[0].name.as_deref(),
            Some("Luz 3D 1")
        );
        assert!(app.light_map.get(1).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn creating_a_material_writes_json_and_guid() {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        let root = std::env::temp_dir().join(format!("lunar_mat_create_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        app.assets.root = root.clone();
        app.assets.entries.clear();
        app.path = root.join("scene.json");
        app.material_name_buffer = "Ladrillo rojo".to_owned();

        app.create_material(&ctx);

        let path = "Ladrillo_rojo.material.json";
        assert!(root.join(path).exists());
        let text = std::fs::read_to_string(root.join(path)).expect("material json");
        assert_eq!(
            crate::materials::Material::decode(&text).name,
            "Ladrillo_rojo"
        );
        assert!(root.join(format!("{path}.meta")).exists());
        assert_eq!(app.selected_asset.as_deref(), Some(path));
        assert!(app.material_library.get(path).is_some());

        app.undo();
        assert!(
            !root.join(path).exists(),
            "el undo retira el fichero creado"
        );
        assert!(
            !root.join(format!("{path}.meta")).exists(),
            "el undo retira también el GUID huérfano"
        );
        let _ = std::fs::remove_dir_all(&root);
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
    fn presentation_preferences_are_included_in_saved_settings() {
        let mut app = fresh_app();
        app.shading_mode = crate::view3d::ShadingMode::Lit;
        app.asset_view = AssetView::Grid;

        let restored = crate::settings::decode(&crate::settings::encode(&app.editor_settings()));

        assert_eq!(restored.shading_mode, crate::view3d::ShadingMode::Lit);
        assert_eq!(restored.asset_view, AssetView::Grid);
    }

    #[test]
    fn the_animation_timeline_advances_without_touching_the_scene() {
        use lunar_scenes::{AnimationClip, SpriteMeta};

        let mut entity = SceneEntity::new(1);
        entity.components.push(Component::Sprite {
            asset: "hero.png".to_owned(),
            meta: SpriteMeta {
                frame_width: 16,
                frame_height: 16,
                columns: 2,
                rows: 1,
                fps: 4.0,
                looping: true,
            },
            animations: vec![AnimationClip {
                name: "caminar".to_owned(),
                frames: vec![0, 1],
                fps: 4.0,
                looping: true,
            }],
        });
        let mut scene = Scene::new();
        scene.entities.push(entity);
        let mut app = fresh_app();
        app.document = EditorDocument::from_scene(scene).expect("document");
        app.select_clicked(1, false, false);
        app.timeline_state.playing = true;
        app.tick_timeline(0.25);
        assert_eq!(app.timeline_state.elapsed, 0.25);
        let clip = match &app.document.scene().entities[0].components[0] {
            Component::Sprite { animations, .. } => animations[0].clone(),
            _ => panic!("se esperaba un sprite"),
        };
        assert_eq!(
            crate::timeline::frame_at(&clip, app.timeline_state.elapsed),
            1
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
    fn history_jumps_repeat_normal_undo_and_redo_steps() {
        let mut app = fresh_app();
        app.apply(EditorCommand::CreateEntity { id: 1, name: None });
        app.apply_script_binding(
            1,
            Some(crate::scripts::Binding::new(
                crate::scripts::Behavior::Patrol,
            )),
        );
        let history = app.document.undo_len();
        assert_eq!(history, app.script_undo.len());
        assert!(app.script_map.get(1).is_some());

        app.undo_to(history - 1);

        assert_eq!(app.document.undo_len(), history - 1);
        assert_eq!(app.script_undo.len(), history - 1);
        assert!(app.script_map.get(1).is_none());

        app.redo_to(0);

        assert_eq!(app.document.undo_len(), history);
        assert_eq!(app.script_undo.len(), history);
        assert!(app.script_map.get(1).is_some());
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

    fn app_with_prefab() -> EditorApp {
        let mut app = fresh_app();
        app.document
            .apply(EditorCommand::CreateEntity {
                id: 1,
                name: Some("Escena".to_owned()),
            })
            .expect("create entity");
        app.document
            .apply(EditorCommand::SavePrefab {
                name: "base".to_owned(),
                entities: vec![SceneEntity::new(2)],
            })
            .expect("save prefab");
        app
    }

    #[test]
    fn saving_a_prefab_from_the_selection_uses_one_undo_step() {
        let mut app = app_with_entities(2);
        app.select_clicked(1, false, false);
        app.select_clicked(2, true, false);
        app.prefab_name_buffer = "  enemigo  ".to_owned();
        let history = app.document.undo_len();

        app.save_prefab(&app.prefab_name_buffer.clone());

        assert_eq!(app.document.undo_len(), history + 1);
        assert_eq!(app.document.scene().prefabs.len(), 1);
        assert_eq!(app.document.scene().prefabs[0].name, "enemigo");
        assert_eq!(app.document.scene().prefabs[0].entities.len(), 2);
        assert!(app.prefab_name_buffer.is_empty());
        assert!(app.status.contains("Guardada plantilla"));

        app.document.undo();
        assert!(app.document.scene().prefabs.is_empty());
        app.document.redo();
        assert_eq!(app.document.scene().prefabs.len(), 1);
    }

    #[test]
    fn saving_a_prefab_needs_a_name_and_a_selection() {
        let mut app = app_with_entities(1);
        app.save_prefab("   ");
        assert!(app.status.contains("nombre"));
        assert!(app.document.scene().prefabs.is_empty());

        app.document.select(&[]);
        app.save_prefab("sin_seleccion");
        assert!(app.status.contains("Selecciona"));
        assert!(app.document.scene().prefabs.is_empty());
    }

    #[test]
    fn instantiating_a_prefab_uses_fresh_ids_and_one_undo_step() {
        let mut app = app_with_prefab();
        app.prefab_instance_count = 2;
        let history = app.document.undo_len();

        app.instantiate_prefab("base");

        let ids: Vec<u64> = app
            .document
            .scene()
            .entities
            .iter()
            .map(|entity| entity.id)
            .collect();
        assert_eq!(ids, vec![1, 2, 3]);
        assert_eq!(app.document.selection(), &[2, 3]);
        assert_eq!(app.document.undo_len(), history + 1);
        assert!(app.status.contains("Instanciada"));
        app.document.undo();
        assert_eq!(app.document.scene().entities.len(), 1);
        app.document.redo();
        assert_eq!(app.document.scene().entities.len(), 3);
    }

    #[test]
    fn updating_then_applying_a_prefab_keeps_each_instance_in_place() {
        let mut app = app_with_prefab();
        app.prefab_instance_count = 2;
        app.instantiate_prefab("base");
        let rotated = Transform {
            rotation: [0.0, 0.707, 0.0, 0.707],
            ..Transform::default()
        };
        app.document
            .apply(EditorCommand::SetTransform {
                id: 2,
                transform: rotated,
            })
            .expect("rotate instance");
        app.document.select(&[2]);

        app.update_prefab("base");

        assert_eq!(
            app.document.scene().prefabs[0].entities[0].transform,
            rotated
        );
        assert!(app.status.contains("Actualizada plantilla"));

        let before_apply = app.document.scene().clone();
        let history = app.document.undo_len();
        app.apply_prefab("base");

        assert_eq!(app.document.undo_len(), history + 1);
        assert!(app.status.contains("Actualizadas"));
        for id in [2, 3] {
            let entity = app
                .document
                .scene()
                .entities
                .iter()
                .find(|entity| entity.id == id)
                .expect("instance");
            assert_eq!(entity.transform.rotation, rotated.rotation);
        }
        let translations: Vec<[f32; 3]> = app
            .document
            .scene()
            .entities
            .iter()
            .filter(|entity| entity.id != 1)
            .map(|entity| entity.transform.translation)
            .collect();
        assert_eq!(translations, vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]]);
        assert!(app.document.undo());
        assert_eq!(app.document.scene(), &before_apply);
    }

    #[test]
    fn an_unknown_prefab_cannot_be_instantiated() {
        let mut app = app_with_prefab();
        let history = app.document.undo_len();
        app.instantiate_prefab("inexistente");
        assert_eq!(app.document.undo_len(), history);
        assert_eq!(app.document.scene().entities.len(), 1);
        assert!(app.status.contains("no encontrada"));
    }

    #[test]
    fn deleting_a_prefab_keeps_instances_and_is_undoable() {
        let mut app = app_with_prefab();
        app.instantiate_prefab("base");
        let history = app.document.undo_len();

        app.delete_prefab("base");

        assert!(app.document.scene().prefabs.is_empty());
        assert_eq!(app.document.scene().entities.len(), 2);
        assert!(
            app.document.scene().entities[1]
                .components
                .iter()
                .all(|component| !matches!(component, Component::PrefabRef { .. }))
        );
        assert_eq!(app.document.undo_len(), history + 1);
        assert!(app.status.contains("referencia(s)"));

        app.document.undo();
        assert_eq!(app.document.scene().prefabs.len(), 1);
        assert!(
            app.document.scene().entities[1]
                .components
                .iter()
                .any(|component| matches!(component, Component::PrefabRef { .. }))
        );
    }

    #[test]
    fn prefab_actions_are_locked_during_play() {
        let mut app = app_with_prefab();
        app.select_clicked(1, false, false);
        app.play();
        let history = app.document.undo_len();

        app.save_prefab("bloqueada");
        app.instantiate_prefab("base");
        app.apply_prefab("base");
        app.update_prefab("base");
        app.delete_prefab("base");

        assert_eq!(app.document.undo_len(), history);
        assert!(app.document.scene().prefabs.len() == 1);
        assert_eq!(app.document.scene().entities.len(), 1);
        assert!(app.status.contains("Detén"));
        app.stop_play();
    }

    fn temp_guid_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "lunar_e16_{label}_{}_{}",
            std::process::id(),
            crate::guids::fresh_seed()
        ));
        let _ = std::fs::create_dir_all(&root);
        root
    }

    #[test]
    fn reloading_guids_rewrites_moved_asset_paths_in_one_undo_step() {
        let ctx = Context::default();
        let root = temp_guid_root("move");
        let _ = std::fs::write(root.join("jump.wav"), b"wav");
        let mut app = fresh_app();
        app.assets = AssetLibrary::new(root.clone());
        app.assets.reload(&ctx);
        app.rebuild_guids();

        let guid = app
            .guid_index
            .guid_for("jump.wav")
            .expect("guid for jump.wav")
            .to_owned();
        assert!(crate::guids::is_guid(&guid));
        assert_eq!(
            crate::guids::parse_meta(&std::fs::read_to_string(root.join("jump.wav.meta")).unwrap()),
            Some(guid.clone())
        );
        app.document
            .apply(EditorCommand::CreateEntity { id: 1, name: None })
            .expect("create entity");
        app.document
            .apply(EditorCommand::AddComponent {
                id: 1,
                component: Component::Audio {
                    asset: "jump.wav".to_owned(),
                    volume: 0.5,
                    autoplay: false,
                },
            })
            .expect("add audio");
        let history = app.document.undo_len();

        std::fs::rename(root.join("jump.wav"), root.join("moved.wav")).unwrap();
        std::fs::rename(root.join("jump.wav.meta"), root.join("moved.wav.meta")).unwrap();
        app.assets.reload(&ctx);
        app.rebuild_guids();
        let (rewritten, broken) = app.rewrite_asset_paths();

        assert_eq!(rewritten, 1);
        assert!(broken.is_empty());
        assert_eq!(
            app.document.scene().entities[0].components[0].asset_path(),
            Some("moved.wav")
        );
        assert_eq!(app.document.undo_len(), history + 1);
        assert_eq!(
            crate::guids::parse_meta(
                &std::fs::read_to_string(root.join("moved.wav.meta")).unwrap()
            ),
            Some(guid.clone())
        );
        assert!(
            std::fs::read_to_string(root.join(crate::guids::INDEX_FILE))
                .unwrap()
                .contains(&format!("{guid}=moved.wav"))
        );
        app.document.undo();
        assert_eq!(
            app.document.scene().entities[0].components[0].asset_path(),
            Some("jump.wav")
        );
        app.document.redo();
        assert_eq!(
            app.document.scene().entities[0].components[0].asset_path(),
            Some("moved.wav")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_asset_paths_are_reported_without_being_rewritten() {
        let ctx = Context::default();
        let root = temp_guid_root("broken");
        let _ = std::fs::write(root.join("jump.wav"), b"wav");
        let mut app = fresh_app();
        app.assets = AssetLibrary::new(root.clone());
        app.assets.reload(&ctx);
        app.rebuild_guids();
        app.document
            .apply(EditorCommand::CreateEntity { id: 1, name: None })
            .expect("create entity");
        app.document
            .apply(EditorCommand::AddComponent {
                id: 1,
                component: Component::Image {
                    asset: "perdido.png".to_owned(),
                },
            })
            .expect("add broken image");
        let history = app.document.undo_len();

        let (rewritten, broken) = app.rewrite_asset_paths();

        assert_eq!(rewritten, 0);
        assert_eq!(broken, ["perdido.png".to_owned()]);
        assert_eq!(app.document.undo_len(), history);
        assert_eq!(
            app.document.scene().entities[0].components[0].asset_path(),
            Some("perdido.png")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reimport_watcher_reports_a_missing_asset_in_the_console() {
        let ctx = Context::default();
        let root = temp_guid_root("reimport");
        let file = root.join("jump.wav");
        let _ = std::fs::write(&file, b"wav");
        let mut app = fresh_app();
        app.assets = AssetLibrary::new(root.clone());
        app.assets.reload(&ctx);
        app.rebuild_guids();
        let samples = crate::import_options::sample(&app.assets.root, &app.assets.entries);
        let _ = app.reimport.poll(&samples, 0, 0);

        std::fs::remove_file(&file).unwrap();
        app.tick_reimport(&ctx, 0);

        assert!(app.status.contains("desaparecido(s)"), "{}", app.status);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn script_bindings_survive_save_load_and_one_undo_step() {
        let ctx = Context::default();
        let root = std::env::temp_dir().join(format!("lunar_scripts_app_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let scene = root.join("escena.json");
        let mut app = EditorApp::new(None, &ctx);
        app.document = EditorDocument::new();
        app.document
            .apply(EditorCommand::CreateEntity {
                id: 4,
                name: Some("caja".to_owned()),
            })
            .unwrap();
        let mut binding = crate::scripts::Binding::new(crate::scripts::Behavior::Patrol);
        binding.params.insert("speed".to_owned(), "3.5".to_owned());
        app.apply_script_binding(4, Some(binding.clone()));

        app.console_save(Some(&scene.to_string_lossy()))
            .expect("save scene and script map");
        assert!(app.script_map_path().exists());
        let text = std::fs::read_to_string(app.script_map_path()).unwrap();
        assert!(text.contains("4=Patrol"), "{text}");
        assert!(text.contains("speed=3.5"), "{text}");

        let loaded = EditorApp::new(Some(scene.clone()), &ctx);
        assert_eq!(loaded.script_map.get(4), Some(&binding));

        app.undo();
        assert!(app.script_map.get(4).is_none());
        app.redo();
        assert_eq!(app.script_map.get(4), Some(&binding));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn validation_report_reads_diagnostics_without_changing_the_scene() {
        let ctx = Context::default();
        let root = std::env::temp_dir().join(format!("lunar_validation_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let mut app = EditorApp::new(None, &ctx);
        app.document = EditorDocument::new();
        app.path = root.join("escena.json");
        let diagnostics = app.script_diagnostics_path();
        std::fs::write(&diagnostics, "problema del juego\n").expect("diagnostics");
        let before = app.document.scene().clone();

        app.run_validation();

        assert_eq!(app.document.scene(), &before);
        assert!(
            app.validation_issues
                .iter()
                .any(|issue| issue.message.contains("problema del juego"))
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn script_diagnostics_reach_the_console_without_touching_the_scene() {
        let ctx = Context::default();
        let root = std::env::temp_dir().join(format!("lunar_scripts_diag_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let scene = root.join("escena.json");
        let mut app = EditorApp::new(None, &ctx);
        app.document = EditorDocument::new();
        app.path = scene;
        let diagnostics = app.script_diagnostics_path();
        std::fs::write(&diagnostics, "línea 2: speed no válido\n").unwrap();
        let before = app.document.scene().clone();

        app.tick_scripts(5_000);

        assert_eq!(app.document.scene(), &before);
        assert!(app.status_log.iter().any(|entry| {
            entry.level == crate::status::Level::Error && entry.message.contains("speed no válido")
        }));
        std::fs::write(&diagnostics, "").unwrap();
        app.tick_scripts(10_000);
        assert_eq!(
            app.script_diag_stamp,
            crate::import_options::stamp(&diagnostics)
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
