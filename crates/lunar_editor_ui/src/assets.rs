//! Panel de assets y caché de texturas del editor visual (ADR-020).
//!
//! El escaneo usa `std::fs` y la decodificación `lunar_assets`. No hay diálogo
//! nativo ni dependencias nuevas: la carpeta se edita como texto.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{Color32, Context, Sense, TextureHandle, Ui, Vec2};
use lunar_assets::ImageStore;

use crate::app::EditorApp;

/// Prefijo del payload que viaja del panel de assets al lienzo.
pub(crate) const ASSET_PAYLOAD_PREFIX: &str = "lunar-asset:";
/// Prefijo del payload que viaja entre filas de la jerarquía.
pub(crate) const ENTITY_PAYLOAD_PREFIX: &str = "lunar-entity:";

/// Tipo de asset según su extensión.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssetKind {
    Image,
    Audio,
    Mesh,
}

impl AssetKind {
    /// Clasifica una ruta relativa ya normalizada, usada por el payload de DnD.
    #[must_use]
    pub(crate) fn from_path(path: &str) -> Option<Self> {
        classify_asset(Path::new(path))
    }
}

/// Forma de presentar los assets del content browser (E18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum AssetView {
    #[default]
    List,
    Grid,
}

impl AssetView {
    #[must_use]
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::List => "Lista",
            Self::Grid => "Miniaturas",
        }
    }
}

/// Entrada listada en el panel de assets.
#[derive(Debug, Clone)]
pub(crate) struct AssetEntry {
    /// Ruta relativa a la raíz, con `/` como separador (ADR-008).
    pub(crate) path: String,
    pub(crate) kind: AssetKind,
}

impl AssetEntry {
    /// Carpeta que contiene el asset, vacía si está en la raíz.
    #[must_use]
    pub(crate) fn folder(&self) -> &str {
        self.path.rsplit_once('/').map_or("", |(folder, _)| folder)
    }
}

/// Filtros del content browser. Son puros para poder probarlos sin GUI.
#[derive(Debug, Clone, Default)]
pub(crate) struct AssetFilter {
    /// Carpeta exacta, o cadena vacía para todas.
    pub(crate) folder: String,
    /// Texto a buscar en la ruta, sin distinguir mayúsculas.
    pub(crate) query: String,
    /// Tipo, o `None` para todos.
    pub(crate) kind: Option<AssetKind>,
}

impl AssetFilter {
    #[must_use]
    pub(crate) fn matches(&self, entry: &AssetEntry) -> bool {
        (self.folder.is_empty() || entry.folder() == self.folder)
            && (self.kind.is_none() || self.kind == Some(entry.kind))
            && (self.query.is_empty()
                || entry
                    .path
                    .to_ascii_lowercase()
                    .contains(&self.query.to_ascii_lowercase()))
    }

    /// Carpetas presentes en la biblioteca, ordenadas y sin repetir.
    #[must_use]
    pub(crate) fn folders(entries: &[AssetEntry]) -> Vec<String> {
        let mut folders: Vec<String> = entries
            .iter()
            .map(AssetEntry::folder)
            .filter(|folder| !folder.is_empty())
            .map(str::to_owned)
            .collect();
        folders.sort();
        folders.dedup();
        folders
    }
}

/// Clasifica un archivo por extensión; devuelve `None` si no es un asset soportado.
#[must_use]
pub(crate) fn classify_asset(path: &Path) -> Option<AssetKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" => Some(AssetKind::Image),
        "wav" => Some(AssetKind::Audio),
        "glb" | "gltf" => Some(AssetKind::Mesh),
        _ => None,
    }
}

/// Convierte una ruta absoluta en relativa a la raíz, normalizada a `/`.
#[must_use]
pub(crate) fn relative_asset_path(root: &Path, full: &Path) -> Option<String> {
    let relative = full.strip_prefix(root).ok()?;
    let text = relative.to_string_lossy().replace('\\', "/");
    (!text.is_empty()).then_some(text)
}

/// Caché de texturas y lista de assets de una carpeta configurable.
pub(crate) struct AssetLibrary {
    pub(crate) root: PathBuf,
    pub(crate) entries: Vec<AssetEntry>,
    pub(crate) error: Option<String>,
    textures: HashMap<String, TextureHandle>,
    dimensions: HashMap<String, [u32; 2]>,
}

impl AssetLibrary {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            entries: Vec::new(),
            error: None,
            textures: HashMap::new(),
            dimensions: HashMap::new(),
        }
    }

    /// Vuelve a escanear la carpeta y recrea las texturas.
    pub(crate) fn reload(&mut self, ctx: &Context) {
        self.entries.clear();
        self.textures.clear();
        self.dimensions.clear();
        self.error = None;

        if !self.root.is_dir() {
            self.error = Some(format!("No es una carpeta: {}", self.root.display()));
            return;
        }
        let mut full_paths = Vec::new();
        collect_files(&self.root, &self.root, &mut full_paths);
        full_paths.sort();

        let mut store = ImageStore::new();
        for full in full_paths {
            let Some(kind) = classify_asset(&full) else {
                continue;
            };
            let Some(path) = relative_asset_path(&self.root, &full) else {
                continue;
            };
            if kind == AssetKind::Image {
                match store.load_from_path(&full) {
                    Ok(handle) => {
                        if let Some(image) = store.get(handle) {
                            let dimensions = [image.width, image.height];
                            let texture = ctx.load_texture(
                                format!("editor-asset:{path}"),
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [dimensions[0] as usize, dimensions[1] as usize],
                                    image.as_rgba(),
                                ),
                                crate::import_options::load_for(&self.root, &path)
                                    .texture
                                    .egui(),
                            );
                            self.textures.insert(path.clone(), texture);
                            self.dimensions.insert(path.clone(), dimensions);
                        }
                    }
                    Err(error) => {
                        self.error = Some(format!("{}: {error}", full.display()));
                        continue;
                    }
                }
            }
            self.entries.push(AssetEntry { path, kind });
        }
    }

    pub(crate) fn texture(&self, path: &str) -> Option<&TextureHandle> {
        self.textures.get(path)
    }

    pub(crate) fn dimensions(&self, path: &str) -> Option<[u32; 2]> {
        self.dimensions.get(path).copied()
    }

    pub(crate) fn kind(&self, path: &str) -> Option<AssetKind> {
        self.entries
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.kind)
    }
}

impl std::fmt::Debug for AssetLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetLibrary")
            .field("root", &self.root)
            .field("entries", &self.entries)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

fn collect_files(root: &Path, directory: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path != root {
                collect_files(root, &path, out);
            }
        } else {
            out.push(path);
        }
    }
}

fn draw_list(app: &mut EditorApp, ui: &mut Ui, visible: &[AssetEntry]) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        for entry in visible {
            asset_row_ui(app, ui, entry);
        }
    });
}

fn draw_grid(app: &mut EditorApp, ui: &mut Ui, visible: &[AssetEntry]) {
    let columns = (ui.available_width() / 104.0).floor().max(1.0) as usize;
    egui::ScrollArea::vertical().show(ui, |ui| {
        for row in visible.chunks(columns) {
            ui.horizontal(|ui| {
                for entry in row {
                    asset_tile_ui(app, ui, entry);
                }
            });
        }
    });
}

fn asset_background(app: &EditorApp, entry: &AssetEntry, hovered: bool) -> Color32 {
    if app.selected_asset.as_deref() == Some(entry.path.as_str()) {
        Color32::from_rgb(50, 70, 100)
    } else if hovered {
        Color32::from_gray(60)
    } else {
        Color32::from_gray(42)
    }
}

fn asset_interactions(app: &mut EditorApp, ui: &Ui, entry: &AssetEntry, response: egui::Response) {
    if response.clicked() {
        app.selected_asset = Some(entry.path.clone());
    }
    if response.double_clicked() {
        app.create_entity_from_asset(&entry.path, app.view_center);
    }
    if response.drag_started() {
        crate::dnd::set_asset(
            ui.ctx(),
            crate::dnd::AssetDrag::new(entry.path.clone(), entry.kind),
        );
    }
    response.on_hover_text(&entry.path);
}

fn asset_row_ui(app: &mut EditorApp, ui: &mut Ui, entry: &AssetEntry) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(224.0, 46.0), Sense::click_and_drag());
    ui.painter()
        .rect_filled(rect, 4.0, asset_background(app, entry, response.hovered()));

    if entry.kind == AssetKind::Image {
        if let Some(texture) = app.assets.texture(&entry.path) {
            let thumb = egui::Rect::from_min_size(rect.min, Vec2::new(42.0, 42.0));
            ui.painter().image(
                texture.id(),
                thumb,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    } else {
        let label = match entry.kind {
            AssetKind::Audio => "A",
            AssetKind::Mesh => "M",
            AssetKind::Image => "I",
        };
        ui.painter().text(
            rect.min + Vec2::new(12.0, 12.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(18.0),
            Color32::LIGHT_BLUE,
        );
    }

    let text_position = rect.min + Vec2::new(48.0, 8.0);
    ui.painter().text(
        text_position,
        egui::Align2::LEFT_TOP,
        entry.path.clone(),
        egui::FontId::proportional(12.0),
        Color32::from_gray(220),
    );
    if let Some(dimensions) = app.assets.dimensions(&entry.path) {
        ui.painter().text(
            text_position + Vec2::new(0.0, 16.0),
            egui::Align2::LEFT_TOP,
            format!("{}×{} px", dimensions[0], dimensions[1]),
            egui::FontId::proportional(10.0),
            Color32::GRAY,
        );
    }
    asset_interactions(app, ui, entry, response);
}

fn asset_tile_ui(app: &mut EditorApp, ui: &mut Ui, entry: &AssetEntry) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(96.0, 82.0), Sense::click_and_drag());
    ui.painter()
        .rect_filled(rect, 4.0, asset_background(app, entry, response.hovered()));

    if entry.kind == AssetKind::Image {
        if let Some(texture) = app.assets.texture(&entry.path) {
            let thumb =
                egui::Rect::from_min_size(rect.min + Vec2::new(2.0, 2.0), Vec2::new(92.0, 52.0));
            ui.painter().image(
                texture.id(),
                thumb,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    } else {
        let label = match entry.kind {
            AssetKind::Audio => "A",
            AssetKind::Mesh => "M",
            AssetKind::Image => "I",
        };
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(22.0),
            Color32::LIGHT_BLUE,
        );
    }

    ui.painter().text(
        rect.min + Vec2::new(4.0, 58.0),
        egui::Align2::LEFT_TOP,
        entry.path.clone(),
        egui::FontId::proportional(10.0),
        Color32::from_gray(220),
    );
    asset_interactions(app, ui, entry, response);
}

pub(crate) fn assets_ui(app: &mut EditorApp, ui: &mut Ui) {
    ui.heading("Assets");
    ui.add(
        egui::TextEdit::singleline(&mut app.assets_root_text)
            .hint_text("carpeta de assets")
            .desired_width(210.0),
    );
    if ui.button("Recargar").clicked() {
        let ctx = ui.ctx().clone();
        app.reload_assets(&ctx);
    }
    if let Some(error) = &app.assets.error {
        ui.colored_label(Color32::RED, error);
    }

    let folders = AssetFilter::folders(&app.assets.entries);
    let folder_label = if app.asset_folder.is_empty() {
        "Todas".to_owned()
    } else {
        app.asset_folder.clone()
    };
    ui.horizontal(|ui| {
        ui.label("Carpeta");
        egui::ComboBox::from_id_salt("asset-folder")
            .selected_text(folder_label.clone())
            .width(120.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.asset_folder, String::new(), "Todas");
                for folder in &folders {
                    ui.selectable_value(&mut app.asset_folder, folder.clone(), folder.as_str());
                }
            });
    });
    ui.add(
        egui::TextEdit::singleline(&mut app.asset_search)
            .hint_text("Buscar…")
            .desired_width(210.0),
    );
    let mut kind = app.asset_kind_filter;
    ui.horizontal(|ui| {
        if ui.selectable_label(kind.is_none(), "Todo").clicked() {
            kind = None;
        }
        if ui
            .selectable_label(kind == Some(AssetKind::Image), "Img")
            .clicked()
        {
            kind = Some(AssetKind::Image);
        }
        if ui
            .selectable_label(kind == Some(AssetKind::Audio), "Audio")
            .clicked()
        {
            kind = Some(AssetKind::Audio);
        }
        if ui
            .selectable_label(kind == Some(AssetKind::Mesh), "Malla")
            .clicked()
        {
            kind = Some(AssetKind::Mesh);
        }
    });
    app.asset_kind_filter = kind;

    ui.horizontal(|ui| {
        ui.label("Vista");
        let mut view = app.asset_view;
        if ui
            .selectable_label(view == AssetView::List, AssetView::List.label())
            .clicked()
        {
            view = AssetView::List;
        }
        if ui
            .selectable_label(view == AssetView::Grid, AssetView::Grid.label())
            .clicked()
        {
            view = AssetView::Grid;
        }
        app.asset_view = view;
    });

    let filter = AssetFilter {
        folder: app.asset_folder.clone(),
        query: app.asset_search.clone(),
        kind: app.asset_kind_filter,
    };
    let visible: Vec<AssetEntry> = app
        .assets
        .entries
        .iter()
        .filter(|entry| filter.matches(entry))
        .cloned()
        .collect();
    ui.label(format!(
        "{} de {} asset(s)",
        visible.len(),
        app.assets.entries.len()
    ));
    ui.separator();

    match app.asset_view {
        AssetView::List => draw_list(app, ui, &visible),
        AssetView::Grid => draw_grid(app, ui, &visible),
    }

    ui.separator();
    app.refresh_asset_options();
    let selected = app
        .selected_asset
        .clone()
        .filter(|path| app.assets.kind(path).is_some());
    match selected {
        Some(path) => {
            ui.label(format!("Seleccionado: {path}"));
            let guid = app.guid_index.guid_for(&path).unwrap_or("sin .meta");
            ui.label(format!("GUID: {guid}"));
            let kind = match app.assets.kind(&path) {
                Some(AssetKind::Image) => "imagen",
                Some(AssetKind::Audio) => "audio",
                Some(AssetKind::Mesh) => "malla",
                None => "desconocido",
            };
            ui.label(format!("Tipo: {kind}"));
            if let Some(dimensions) = app.assets.dimensions(&path) {
                ui.label(format!("{}×{} px", dimensions[0], dimensions[1]));
            }
            ui.separator();
            ui.label("Opciones de importación");
            use crate::import_options::{TextureFilter, TextureWrap};
            let kind = app.assets.kind(&path);
            let mut changed = false;
            if kind == Some(AssetKind::Image) {
                let mut filter = app.asset_options.texture.filter;
                let mut wrap = app.asset_options.texture.wrap;
                ui.horizontal(|ui| {
                    ui.label("Filtro");
                    changed |= ui
                        .radio_value(&mut filter, TextureFilter::Nearest, "Nearest")
                        .clicked();
                    changed |= ui
                        .radio_value(&mut filter, TextureFilter::Linear, "Linear")
                        .clicked();
                });
                ui.horizontal(|ui| {
                    ui.label("Wrap");
                    changed |= ui
                        .radio_value(&mut wrap, TextureWrap::Clamp, "Clamp")
                        .clicked();
                    changed |= ui
                        .radio_value(&mut wrap, TextureWrap::Repeat, "Repeat")
                        .clicked();
                    changed |= ui
                        .radio_value(&mut wrap, TextureWrap::Mirror, "Mirror")
                        .clicked();
                });
                app.asset_options.texture.filter = filter;
                app.asset_options.texture.wrap = wrap;
            } else if kind == Some(AssetKind::Mesh) {
                let mut scale = app.asset_options.gltf.scale;
                ui.horizontal(|ui| {
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut scale)
                                .speed(0.05)
                                .range(0.01..=100.0),
                        )
                        .on_hover_text("Escala glTF")
                        .changed();
                    ui.label("Escala glTF");
                });
                changed |= ui
                    .checkbox(&mut app.asset_options.gltf.materials, "Materiales")
                    .clicked();
                changed |= ui
                    .checkbox(&mut app.asset_options.gltf.animations, "Animaciones")
                    .clicked();
                app.asset_options.gltf.scale = scale;
            } else if kind == Some(AssetKind::Audio) {
                let mut channels = app.asset_options.audio.channels;
                let mut rate = app.asset_options.audio.sample_rate;
                ui.horizontal(|ui| {
                    changed |= ui
                        .add(egui::DragValue::new(&mut channels).speed(1.0).range(1..=2))
                        .on_hover_text("Canales")
                        .changed();
                    ui.label("Canales");
                });
                ui.horizontal(|ui| {
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut rate)
                                .speed(100.0)
                                .range(8_000..=192_000),
                        )
                        .on_hover_text("Sample rate")
                        .changed();
                    ui.label("Sample rate");
                });
                app.asset_options.audio.channels = channels;
                app.asset_options.audio.sample_rate = rate;
            }
            if changed {
                let ctx = ui.ctx().clone();
                app.save_asset_options(&ctx);
            }
            if ui.button("Crear en el centro").clicked() {
                let center = app.view_center;
                app.create_entity_from_asset(&path, center);
            }
        }
        None => {
            ui.label("Sin asset seleccionado");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Context;

    #[test]
    fn classifies_supported_extensions() {
        assert_eq!(classify_asset(Path::new("A.PNG")), Some(AssetKind::Image));
        assert_eq!(classify_asset(Path::new("a.jpeg")), Some(AssetKind::Image));
        assert_eq!(
            classify_asset(Path::new("boom.wav")),
            Some(AssetKind::Audio)
        );
        assert_eq!(classify_asset(Path::new("ship.glb")), Some(AssetKind::Mesh));
        assert_eq!(classify_asset(Path::new("notes.txt")), None);
    }

    #[test]
    fn relative_paths_use_forward_slashes() {
        let root = Path::new("assets");
        let full = root.join("sprites").join("frog.png");
        assert_eq!(
            relative_asset_path(root, &full).as_deref(),
            Some("sprites/frog.png")
        );
        assert_eq!(relative_asset_path(root, Path::new("other/frog.png")), None);
    }

    #[test]
    fn folders_are_sorted_and_unique() {
        let entries = [
            AssetEntry {
                path: "sprites/hero.png".to_owned(),
                kind: AssetKind::Image,
            },
            AssetEntry {
                path: "audio/jump.wav".to_owned(),
                kind: AssetKind::Audio,
            },
            AssetEntry {
                path: "sprites/frog.png".to_owned(),
                kind: AssetKind::Image,
            },
            AssetEntry {
                path: "root.png".to_owned(),
                kind: AssetKind::Image,
            },
        ];
        assert_eq!(AssetFilter::folders(&entries), ["audio", "sprites"]);
        assert_eq!(entries[3].folder(), "");
    }

    #[test]
    fn filters_combine_folder_query_and_kind() {
        let hero = AssetEntry {
            path: "sprites/hero.png".to_owned(),
            kind: AssetKind::Image,
        };
        let frog = AssetEntry {
            path: "sprites/frog.png".to_owned(),
            kind: AssetKind::Image,
        };
        let jump = AssetEntry {
            path: "audio/jump.wav".to_owned(),
            kind: AssetKind::Audio,
        };
        let filter = AssetFilter {
            folder: "sprites".to_owned(),
            query: "HERO".to_owned(),
            kind: Some(AssetKind::Image),
        };
        assert!(filter.matches(&hero));
        assert!(!filter.matches(&frog));
        assert!(!filter.matches(&jump));
        let broad = AssetFilter::default();
        assert!(broad.matches(&hero) && broad.matches(&frog) && broad.matches(&jump));
        let audio = AssetFilter {
            kind: Some(AssetKind::Audio),
            ..Default::default()
        };
        assert!(audio.matches(&jump));
        assert!(!audio.matches(&hero));
    }

    #[test]
    fn asset_panel_draws_search_filters_and_selection() {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.assets.entries.clear();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(900.0, 700.0),
            )),
            ..Default::default()
        };
        let mut first = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Assets").show(ctx, |ui| assets_ui(&mut app, ui));
        });
        first.textures_delta.clear();
        let mut output = ctx.run_pass(input, |ctx| {
            egui::Window::new("Assets").show(ctx, |ui| assets_ui(&mut app, ui));
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
            texts.iter().any(|text| text.contains("Assets")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|text| text.contains("Buscar")));
        assert!(
            texts
                .iter()
                .any(|text| text.contains("Sin asset seleccionado"))
        );
    }

    #[test]
    fn asset_panel_shows_the_guid_of_the_selected_asset() {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.assets.entries.clear();
        app.assets.entries.push(AssetEntry {
            path: "hero.png".to_owned(),
            kind: AssetKind::Image,
        });
        app.selected_asset = Some("hero.png".to_owned());
        let id = "0123456789abcdef0123456789abcdef";
        app.guid_index = crate::guids::GuidIndex::from_pairs(
            &[("hero.png".to_owned(), id.to_owned())],
            &crate::guids::GuidIndex::default(),
        );
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(900.0, 700.0),
            )),
            ..Default::default()
        };
        let mut first = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Assets").show(ctx, |ui| assets_ui(&mut app, ui));
        });
        first.textures_delta.clear();
        let mut output = ctx.run_pass(input, |ctx| {
            egui::Window::new("Assets").show(ctx, |ui| assets_ui(&mut app, ui));
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
            texts.iter().any(|text| text == &format!("GUID: {id}")),
            "{texts:?}"
        );
    }

    fn rendered_asset_panel(app: &mut EditorApp, ctx: &Context) -> Vec<String> {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(900.0, 700.0),
            )),
            ..Default::default()
        };
        let mut first = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("Assets").show(ctx, |ui| assets_ui(app, ui));
        });
        first.textures_delta.clear();
        let mut output = ctx.run_pass(input, |ctx| {
            egui::Window::new("Assets").show(ctx, |ui| assets_ui(app, ui));
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
        texts
    }

    #[test]
    fn asset_panel_shows_import_options_for_the_selected_kind() {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        app.assets.entries.clear();
        app.assets.entries.push(AssetEntry {
            path: "hero.png".to_owned(),
            kind: AssetKind::Image,
        });
        app.selected_asset = Some("hero.png".to_owned());
        let texts = rendered_asset_panel(&mut app, &ctx);
        assert!(
            texts
                .iter()
                .any(|text| text.contains("Opciones de importación"))
        );
        assert!(texts.iter().any(|text| text.contains("Filtro")));
        assert!(texts.iter().any(|text| text.contains("Wrap")));

        app.assets.entries.clear();
        app.assets.entries.push(AssetEntry {
            path: "ship.glb".to_owned(),
            kind: AssetKind::Mesh,
        });
        app.selected_asset = Some("ship.glb".to_owned());
        let texts = rendered_asset_panel(&mut app, &ctx);
        assert!(texts.iter().any(|text| text.contains("Escala glTF")));
        assert!(texts.iter().any(|text| text.contains("Materiales")));
    }

    #[test]
    fn asset_panel_offers_list_and_thumbnail_views() {
        let ctx = Context::default();
        let mut app = EditorApp::new(None, &ctx);
        assert_eq!(app.asset_view, AssetView::List);
        let texts = rendered_asset_panel(&mut app, &ctx);
        assert!(texts.iter().any(|text| text == "Vista"));
        assert!(texts.iter().any(|text| text == "Lista"));
        assert!(texts.iter().any(|text| text == "Miniaturas"));

        app.asset_view = AssetView::Grid;
        app.assets.entries.clear();
        app.assets.entries.push(AssetEntry {
            path: "hero.png".to_owned(),
            kind: AssetKind::Image,
        });
        app.assets.entries.push(AssetEntry {
            path: "jump.wav".to_owned(),
            kind: AssetKind::Audio,
        });
        let texts = rendered_asset_panel(&mut app, &ctx);
        assert!(texts.iter().any(|text| text == "hero.png"));
        assert!(texts.iter().any(|text| text == "jump.wav"));
    }
}
