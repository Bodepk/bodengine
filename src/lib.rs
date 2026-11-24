//! Fachada pública del motor Lunar.
//!
//! Los submódulos se implementan por hitos. Las features `dim2`/`dim3` permiten
//! construir versiones parciales del motor; ver ADR-002.

pub use lunar_core as core;
pub use lunar_ecs as ecs;
pub use lunar_math as math;
pub use lunar_platform as platform;

#[cfg(feature = "assets")]
pub use lunar_assets as assets;
#[cfg(feature = "audio")]
pub use lunar_audio as audio;
#[cfg(feature = "bridge")]
pub use lunar_bridge as bridge;
#[cfg(feature = "dim2")]
pub use lunar_physics2d as physics2d;
#[cfg(feature = "dim3")]
pub use lunar_physics3d as physics3d;
#[cfg(feature = "render")]
pub use lunar_render as render;
#[cfg(feature = "dim2")]
pub use lunar_render2d as render2d;
#[cfg(feature = "dim3")]
pub use lunar_render3d as render3d;
#[cfg(feature = "runtime")]
pub use lunar_runtime as runtime;
