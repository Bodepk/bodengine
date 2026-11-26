//! Directorio de configuración compartido por recientes y preferencias (ADR-031).

use std::path::PathBuf;

/// Devuelve `LunarEditor` dentro del directorio de configuración del usuario.
#[must_use]
pub(crate) fn config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("HOME").map(PathBuf::from);

    base.map(|dir| dir.join("LunarEditor"))
}
