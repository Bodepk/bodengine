//! Atajos de teclado del editor: resolución pura de teclas y modificadores.

/// Teclas y modificadores leídos del evento de entrada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ShortcutKeys {
    pub(crate) command: bool,
    pub(crate) shift: bool,
    pub(crate) save: bool,
    pub(crate) open: bool,
    pub(crate) undo: bool,
    pub(crate) redo: bool,
    pub(crate) delete: bool,
    pub(crate) duplicate: bool,
    pub(crate) move_tool: bool,
    pub(crate) rotate_tool: bool,
    pub(crate) scale_tool: bool,
}

/// Acciones resultantes de resolver un atajo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ShortcutActions {
    pub(crate) save: bool,
    pub(crate) save_as: bool,
    pub(crate) open: bool,
    pub(crate) undo: bool,
    pub(crate) redo: bool,
    pub(crate) delete: bool,
    pub(crate) duplicate: bool,
    pub(crate) move_tool: bool,
    pub(crate) rotate_tool: bool,
    pub(crate) scale_tool: bool,
}

/// Resuelve las acciones. `Ctrl+Shift+S` guarda como y no sobrescribe la escena
/// actual, y `Ctrl+Shift+Z` rehace en lugar de deshacer. `W`/`E`/`R` cambian la
/// herramienta solo sin modificadores, para no pisar los atajos con `Ctrl`.
#[must_use]
pub(crate) fn resolve(keys: ShortcutKeys) -> ShortcutActions {
    let ShortcutKeys {
        command,
        shift,
        save,
        open,
        undo,
        redo,
        delete,
        duplicate,
        move_tool,
        rotate_tool,
        scale_tool,
    } = keys;
    let plain = !command && !shift;
    ShortcutActions {
        save: command && !shift && save,
        save_as: command && shift && save,
        open: command && open,
        undo: command && !shift && undo,
        redo: command && ((shift && undo) || redo),
        delete,
        duplicate: command && duplicate,
        move_tool: plain && move_tool,
        rotate_tool: plain && rotate_tool,
        scale_tool: plain && scale_tool,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_save_as_are_exclusive() {
        let base = ShortcutKeys {
            save: true,
            ..Default::default()
        };
        assert_eq!(
            resolve(ShortcutKeys {
                command: true,
                ..base
            }),
            ShortcutActions {
                save: true,
                ..Default::default()
            }
        );
        assert_eq!(
            resolve(ShortcutKeys {
                command: true,
                shift: true,
                ..base
            }),
            ShortcutActions {
                save_as: true,
                ..Default::default()
            }
        );
        assert_eq!(resolve(base), ShortcutActions::default());
    }

    #[test]
    fn open_requires_command() {
        let keys = ShortcutKeys {
            command: true,
            open: true,
            ..Default::default()
        };
        assert!(resolve(keys).open);
        assert!(
            !resolve(ShortcutKeys {
                open: true,
                ..Default::default()
            })
            .open
        );
    }

    #[test]
    fn undo_and_redo_keep_their_shortcuts() {
        let undo = ShortcutKeys {
            command: true,
            undo: true,
            ..Default::default()
        };
        assert!(resolve(undo).undo);
        let redo = ShortcutKeys {
            command: true,
            shift: true,
            undo: true,
            ..Default::default()
        };
        let actions = resolve(redo);
        assert!(!actions.undo);
        assert!(actions.redo);
        let redo_y = ShortcutKeys {
            command: true,
            redo: true,
            ..Default::default()
        };
        assert!(resolve(redo_y).redo);
    }

    #[test]
    fn delete_is_global_but_duplicate_needs_command() {
        assert!(
            resolve(ShortcutKeys {
                delete: true,
                ..Default::default()
            })
            .delete
        );
        assert!(
            resolve(ShortcutKeys {
                command: true,
                duplicate: true,
                ..Default::default()
            })
            .duplicate
        );
        assert!(
            !resolve(ShortcutKeys {
                duplicate: true,
                ..Default::default()
            })
            .duplicate
        );
    }

    #[test]
    fn w_e_r_switch_the_tool_without_modifiers() {
        let moved = resolve(ShortcutKeys {
            move_tool: true,
            ..Default::default()
        });
        assert!(moved.move_tool);
        assert!(!moved.rotate_tool);
        assert!(!moved.scale_tool);
        assert!(
            resolve(ShortcutKeys {
                rotate_tool: true,
                ..Default::default()
            })
            .rotate_tool
        );
        assert!(
            resolve(ShortcutKeys {
                scale_tool: true,
                ..Default::default()
            })
            .scale_tool
        );
    }

    #[test]
    fn tool_shortcuts_ignore_modifiers() {
        let keys = ShortcutKeys {
            command: true,
            move_tool: true,
            ..Default::default()
        };
        assert!(!resolve(keys).move_tool);
        let keys = ShortcutKeys {
            shift: true,
            rotate_tool: true,
            ..Default::default()
        };
        assert!(!resolve(keys).rotate_tool);
    }
}
