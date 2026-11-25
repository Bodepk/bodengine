//! Panel de assets y caché de texturas del editor visual (ADR-020).
//!
//! El escaneo usa `std::fs` y la decodificación `lunar_assets`. No hay diálogo
//! nativo ni dependencias nuevas: la carpeta se edita como texto.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{Color32, Context, Sense, TextureHandle, TextureOptions, Ui, Vec2};
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

/// Entrada listada en el panel de assets.
#[derive(Debug, Clone)]
pub(crate) struct AssetEntry {
    /// Ruta relativa a la raíz, con `/` como separador (ADR-008).
    pub(crate) path: String,
    pub(crate) kind: AssetKind,
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
                                TextureOptions::NEAREST,
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
    ui.label(format!("{} asset(s)", app.assets.entries.len()));
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        let entries = app.assets.entries.clone();
        for entry in entries {
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(224.0, 46.0), Sense::click_and_drag());
            let background = if response.hovered() {
                Color32::from_gray(60)
            } else {
                Color32::from_gray(42)
            };
            ui.painter().rect_filled(rect, 4.0, background);

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

            if response.drag_started() {
                egui::DragAndDrop::set_payload(
                    ui.ctx(),
                    format!("{ASSET_PAYLOAD_PREFIX}{}", entry.path),
                );
            }
            response.on_hover_text(&entry.path);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
