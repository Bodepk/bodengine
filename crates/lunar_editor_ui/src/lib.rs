//! Interfaz gráfica opcional del editor (ADR-019).
//!
//! Este crate es solo un frente: abre una ventana con `egui`, muestra la
//! jerarquía y el inspector, y delega cada cambio a `lunar_editor`. No crea
//! dispositivos GPU propios ni expone `wgpu` en su API.

mod app;
mod graph;
mod helpers;
mod hierarchy;
mod inspector;

use std::path::PathBuf;

pub use helpers::next_entity_id;

/// Abre la ventana del editor. El primer argumento, si existe, es un JSON de escena.
pub fn run() -> eframe::Result<()> {
    let scene_path = std::env::args().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Lunar Editor")
            .with_inner_size([1180.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "lunar_editor_ui",
        options,
        Box::new(move |_cc| Ok(Box::new(app::EditorApp::new(scene_path.clone())))),
    )
}
