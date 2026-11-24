//! Estado de entrada independiente de la ventana (ADR-006).

use std::collections::HashSet;

/// Teclas que el motor expone a la lógica de juego.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Key {
    Escape,
    W,
    A,
    S,
    D,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    #[default]
    Other,
}

/// Estado de teclado en el frame actual.
#[derive(Debug, Clone, Default)]
pub struct InputState {
    down: HashSet<Key>,
    just_pressed: HashSet<Key>,
    just_released: HashSet<Key>,
}

impl InputState {
    /// Registra una pulsación.
    pub fn press(&mut self, key: Key) {
        if self.down.insert(key) {
            self.just_pressed.insert(key);
        }
    }

    /// Registra una liberación.
    pub fn release(&mut self, key: Key) {
        if self.down.remove(&key) {
            self.just_released.insert(key);
        }
    }

    /// Comprueba si la tecla está pulsada.
    #[must_use]
    pub fn is_down(&self, key: Key) -> bool {
        self.down.contains(&key)
    }

    /// Comprueba si la tecla se pulsó en este frame.
    #[must_use]
    pub fn was_pressed(&self, key: Key) -> bool {
        self.just_pressed.contains(&key)
    }

    /// Comprueba si la tecla se liberó en este frame.
    #[must_use]
    pub fn was_released(&self, key: Key) -> bool {
        self.just_released.contains(&key)
    }

    /// Borra los eventos transitorios al final del frame.
    pub fn end_frame(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_and_release_transitions() {
        let mut input = InputState::default();
        assert!(!input.is_down(Key::Escape));

        input.press(Key::Escape);
        assert!(input.is_down(Key::Escape));
        assert!(input.was_pressed(Key::Escape));

        input.end_frame();
        assert!(!input.was_pressed(Key::Escape));
        assert!(input.is_down(Key::Escape));

        input.release(Key::Escape);
        assert!(!input.is_down(Key::Escape));
        assert!(input.was_released(Key::Escape));
    }

    #[test]
    fn repeated_press_does_not_duplicate_transition() {
        let mut input = InputState::default();
        input.press(Key::W);
        input.press(Key::W);
        assert!(input.was_pressed(Key::W));
        input.end_frame();
        assert!(!input.was_pressed(Key::W));
    }
}
