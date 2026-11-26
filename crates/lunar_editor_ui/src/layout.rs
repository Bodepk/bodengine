//! Invariantes numéricas del layout del editor (E1).
//!
//! Los paneles se dimensionan con estos límites para que el viewport central
//! siga siendo utilizable incluso con todos los paneles laterales al máximo.
//! Las funciones son puras y se comprueban con tests sin ventana ni GPU.

/// Alto de la barra de herramientas, incluyendo márgenes.
pub(crate) const TOOLBAR_HEIGHT: f32 = 36.0;
/// Alto de la barra de estado, incluyendo márgenes.
pub(crate) const STATUS_HEIGHT: f32 = 28.0;
/// Alto de la consola acoplada, incluyendo márgenes.
pub(crate) const CONSOLE_HEIGHT: f32 = 160.0;
/// Tamaño inicial de la ventana.
pub(crate) const WINDOW_SIZE: [f32; 2] = [1280.0, 800.0];
/// Tamaño mínimo de la ventana: garantiza el viewport mínimo con los paneles al máximo.
pub(crate) const MIN_WINDOW_SIZE: [f32; 2] = [1280.0, 720.0];
/// Viewport útil mínimo; se comprueba en los tests de este módulo.
#[cfg(test)]
pub(crate) const MIN_VIEWPORT_SIZE: [f32; 2] = [400.0, 300.0];

/// Ancho de la jerarquía: defecto y rango permitido.
pub(crate) const HIERARCHY_WIDTH: f32 = 230.0;
pub(crate) const HIERARCHY_RANGE: (f32, f32) = (200.0, 260.0);
/// Ancho del navegador de assets: defecto y rango permitido.
pub(crate) const ASSETS_WIDTH: f32 = 240.0;
pub(crate) const ASSETS_RANGE: (f32, f32) = (200.0, 260.0);
/// Ancho del inspector: defecto y rango permitido.
pub(crate) const INSPECTOR_WIDTH: f32 = 330.0;
pub(crate) const INSPECTOR_RANGE: (f32, f32) = (280.0, 360.0);
/// Alto de la consola: rango permitido.
pub(crate) const CONSOLE_RANGE: (f32, f32) = (120.0, 260.0);

/// Limita un tamaño al rango `[mínimo, máximo]`, con Defecto incluido.
#[must_use]
pub(crate) fn clamp(value: f32, range: (f32, f32), default: f32) -> f32 {
    let (min, max) = range;
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default.clamp(min, max)
    }
}

#[must_use]
pub(crate) fn hierarchy_width(value: f32) -> f32 {
    clamp(value, HIERARCHY_RANGE, HIERARCHY_WIDTH)
}

#[must_use]
pub(crate) fn assets_width(value: f32) -> f32 {
    clamp(value, ASSETS_RANGE, ASSETS_WIDTH)
}

#[must_use]
pub(crate) fn inspector_width(value: f32) -> f32 {
    clamp(value, INSPECTOR_RANGE, INSPECTOR_WIDTH)
}

#[must_use]
pub(crate) fn console_height(value: f32) -> f32 {
    clamp(value, CONSOLE_RANGE, CONSOLE_HEIGHT)
}

/// Viewport que queda con los anchos/alto dados para los paneles.
#[cfg(test)]
#[must_use]
pub(crate) fn viewport_for(window: [f32; 2], sides: [f32; 3], console: f32) -> [f32; 2] {
    let [width, height] = window;
    let [hierarchy, assets, inspector] = sides;
    [
        width - hierarchy - assets - inspector,
        height - TOOLBAR_HEIGHT - console - STATUS_HEIGHT,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_inside_their_ranges() {
        assert_eq!(hierarchy_width(HIERARCHY_WIDTH), HIERARCHY_WIDTH);
        assert_eq!(assets_width(ASSETS_WIDTH), ASSETS_WIDTH);
        assert_eq!(inspector_width(INSPECTOR_WIDTH), INSPECTOR_WIDTH);
        assert_eq!(console_height(CONSOLE_HEIGHT), CONSOLE_HEIGHT);
    }

    #[test]
    fn panels_clamp_to_their_ranges() {
        assert_eq!(hierarchy_width(10.0), HIERARCHY_RANGE.0);
        assert_eq!(hierarchy_width(1000.0), HIERARCHY_RANGE.1);
        assert_eq!(assets_width(10.0), ASSETS_RANGE.0);
        assert_eq!(assets_width(1000.0), ASSETS_RANGE.1);
        assert_eq!(inspector_width(10.0), INSPECTOR_RANGE.0);
        assert_eq!(inspector_width(1000.0), INSPECTOR_RANGE.1);
        assert_eq!(console_height(10.0), CONSOLE_RANGE.0);
        assert_eq!(console_height(1000.0), CONSOLE_RANGE.1);
    }

    #[test]
    fn non_finite_sizes_fall_back_to_the_default() {
        assert_eq!(hierarchy_width(f32::NAN), HIERARCHY_WIDTH);
        assert_eq!(assets_width(f32::INFINITY), ASSETS_WIDTH);
        assert_eq!(inspector_width(f32::NEG_INFINITY), INSPECTOR_WIDTH);
    }

    #[test]
    fn minimum_window_keeps_the_minimum_viewport_with_default_panels() {
        let viewport = viewport_for(
            MIN_WINDOW_SIZE,
            [HIERARCHY_WIDTH, ASSETS_WIDTH, INSPECTOR_WIDTH],
            CONSOLE_HEIGHT,
        );
        assert!(viewport[0] >= MIN_VIEWPORT_SIZE[0], "viewport={viewport:?}");
        assert!(viewport[1] >= MIN_VIEWPORT_SIZE[1], "viewport={viewport:?}");
    }

    #[test]
    fn minimum_window_keeps_the_minimum_viewport_with_maximal_panels() {
        let viewport = viewport_for(
            MIN_WINDOW_SIZE,
            [HIERARCHY_RANGE.1, ASSETS_RANGE.1, INSPECTOR_RANGE.1],
            CONSOLE_RANGE.1,
        );
        assert!(viewport[0] >= MIN_VIEWPORT_SIZE[0], "viewport={viewport:?}");
        assert!(viewport[1] >= MIN_VIEWPORT_SIZE[1], "viewport={viewport:?}");
    }
}
