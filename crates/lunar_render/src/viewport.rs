//! Validación de tamaños de surface sin tocar la GPU.
//!
//! `wgpu::Surface::configure` exige dimensiones mayores que cero. Toda ventana
//! minimizada o redimensionada a `0×0` pasa por aquí antes de llegar a `wgpu`.

use std::error::Error;
use std::fmt;

const MAX_DIMENSION: u32 = i32::MAX as u32;

/// Tamaño de surface ya validado para `wgpu`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

/// Tamaño rechazado por ser degenerado o demasiado grande.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidViewport {
    pub width: u32,
    pub height: u32,
}

impl fmt::Display for InvalidViewport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tamaño de surface inválido: {}x{} (se requiere 1..={} en ambos ejes)",
            self.width, self.height, MAX_DIMENSION
        )
    }
}

impl Error for InvalidViewport {}

impl Viewport {
    /// Valida un par de dimensiones físicas.
    pub fn new(width: u32, height: u32) -> Result<Self, InvalidViewport> {
        if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(InvalidViewport { width, height });
        }
        Ok(Self { width, height })
    }
}

/// Estado del último tamaño renderizable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportState {
    current: Option<Viewport>,
}

impl ViewportState {
    /// Crea el estado con un tamaño inicial opcional.
    #[must_use]
    pub const fn new(initial: Option<Viewport>) -> Self {
        Self { current: initial }
    }

    /// Devuelve el tamaño actual, si es renderizable.
    #[must_use]
    pub const fn current(&self) -> Option<Viewport> {
        self.current
    }

    /// Actualiza el tamaño y devuelve `Some` solo si hace falta reconfigurar.
    ///
    /// Un tamaño degenerado guarda `None` y hace que el frame se omita.
    pub fn update(&mut self, width: u32, height: u32) -> Option<Viewport> {
        let viewport = Viewport::new(width, height);
        let next = viewport.ok();
        let changed = self.current != next;
        self.current = next;
        if changed { next } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_positive_dimensions() {
        let viewport = Viewport::new(800, 600).unwrap();
        assert_eq!((viewport.width, viewport.height), (800, 600));
    }

    #[test]
    fn rejects_zero_and_oversized_dimensions() {
        assert!(Viewport::new(0, 600).is_err());
        assert!(Viewport::new(800, 0).is_err());
        assert!(Viewport::new(1, 0).is_err());
        assert!(Viewport::new(0, 1).is_err());
        assert!(Viewport::new(u32::MAX, 1).is_err());
        assert!(Viewport::new(1, u32::MAX).is_err());
        assert!(Viewport::new(MAX_DIMENSION + 1, 1).is_err());
    }

    #[test]
    fn minimize_and_restore_updates_viewport() {
        let mut state = ViewportState::new(Some(Viewport::new(800, 600).unwrap()));

        assert_eq!(state.update(0, 0), None);
        assert_eq!(state.current(), None);
        assert_eq!(state.update(0, 1), None);
        assert_eq!(state.update(1, 0), None);
        assert_eq!(state.update(u32::MAX, 1), None);

        let restored = Viewport::new(800, 600).unwrap();
        assert_eq!(state.update(800, 600), Some(restored));
        assert_eq!(state.current(), Some(restored));
    }

    #[test]
    fn same_size_does_not_reconfigure() {
        let mut state = ViewportState::new(Some(Viewport::new(640, 480).unwrap()));
        assert_eq!(state.update(640, 480), None);
        assert_eq!(state.current(), Some(Viewport::new(640, 480).unwrap()));
    }
}
