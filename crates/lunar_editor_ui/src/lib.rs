//! Interfaz gráfica opcional del editor (ADR-019).
//!
//! Este crate es solo un frente: abre una ventana con `egui`, muestra la
//! jerarquía y el inspector, y delega cada cambio a `lunar_editor`. No crea
//! dispositivos GPU propios ni expone `wgpu` en su API.

mod app;
mod assets;
mod canvas;
mod components;
mod config;
mod dialogs;
mod dnd;
mod file_picker;
mod gizmo;
mod guids;
mod helpers;
mod hierarchy;
mod import_options;
mod inspector;
mod layout;
mod play;
mod prefabs;
mod recent;
mod settings;
mod shortcuts;
mod status;
mod tools;
mod view3d;

use std::path::PathBuf;

pub use helpers::next_entity_id;

/// Abre la ventana del editor. El primer argumento, si existe, es un JSON de escena.
pub fn run() -> eframe::Result<()> {
    let scene_path = std::env::args().nth(1).map(PathBuf::from);
    // Icono fijado en el repo: el mismo que eframe 0.37 usa por defecto, para
    // que la ventana y la barra de tareas no dependan de esa versión.
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
        .expect("el icono embebido del editor es válido");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Lunar Editor")
            .with_inner_size(layout::WINDOW_SIZE)
            .with_min_inner_size(layout::MIN_WINDOW_SIZE)
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "lunar_editor_ui",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::EditorApp::new(
                scene_path.clone(),
                &cc.egui_ctx,
            )))
        }),
    )
}
