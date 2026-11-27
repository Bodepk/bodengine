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
    pub(crate) copy: bool,
    pub(crate) cut: bool,
    pub(crate) paste: bool,
    pub(crate) move_tool: bool,
    pub(crate) rotate_tool: bool,
    pub(crate) scale_tool: bool,
    pub(crate) toggle_space: bool,
    pub(crate) toggle_play: bool,
    pub(crate) stop_play: bool,
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
    pub(crate) copy: bool,
    pub(crate) cut: bool,
    pub(crate) paste: bool,
    pub(crate) move_tool: bool,
    pub(crate) rotate_tool: bool,
    pub(crate) scale_tool: bool,
    pub(crate) toggle_space: bool,
    pub(crate) toggle_play: bool,
    pub(crate) stop_play: bool,
}

/// Resuelve las acciones. `Ctrl+Shift+S` guarda como y no sobrescribe la escena
/// actual, y `Ctrl+Shift+Z` rehace en lugar de deshacer. `W`/`E`/`R` cambian la
/// herramienta solo sin modificadores, para no pisar los atajos con `Ctrl`.
/// Copiar, cortar y pegar exigen `Ctrl` y descartan `Shift` para no interferir
/// con los atajos de formato del sistema (E13). `Q` alterna el espacio del
/// gizmo solo sin modificadores (E14).
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
        copy,
        cut,
        paste,
        move_tool,
        rotate_tool,
        scale_tool,
        toggle_space,
        toggle_play,
        stop_play,
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
        copy: command && !shift && copy,
        cut: command && !shift && cut,
        paste: command && !shift && paste,
        move_tool: plain && move_tool,
        rotate_tool: plain && rotate_tool,
        scale_tool: plain && scale_tool,
        toggle_space: plain && toggle_space,
        toggle_play: plain && toggle_play,
        stop_play: plain && stop_play,
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
    fn copy_cut_and_paste_require_command_without_shift() {
        for key in ["copy", "cut", "paste"] {
            let base = match key {
                "copy" => ShortcutKeys {
                    copy: true,
                    ..Default::default()
                },
                "cut" => ShortcutKeys {
                    cut: true,
                    ..Default::default()
                },
                _ => ShortcutKeys {
                    paste: true,
                    ..Default::default()
                },
            };
            let value = |actions: ShortcutActions| match key {
                "copy" => actions.copy,
                "cut" => actions.cut,
                _ => actions.paste,
            };
            assert!(!value(resolve(base)), "{key} sin Ctrl no debe actuar");
            assert!(
                value(resolve(ShortcutKeys {
                    command: true,
                    ..base
                })),
                "{key} con Ctrl"
            );
            assert!(
                !value(resolve(ShortcutKeys {
                    command: true,
                    shift: true,
                    ..base
                })),
                "{key} con Ctrl+Shift no debe actuar"
            );
        }
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
    fn q_toggles_the_gizmo_space_without_modifiers() {
        let plain = resolve(ShortcutKeys {
            toggle_space: true,
            ..Default::default()
        });
        assert!(plain.toggle_space);
        let with_ctrl = resolve(ShortcutKeys {
            command: true,
            toggle_space: true,
            ..Default::default()
        });
        assert!(!with_ctrl.toggle_space);
        let with_shift = resolve(ShortcutKeys {
            shift: true,
            toggle_space: true,
            ..Default::default()
        });
        assert!(!with_shift.toggle_space);
    }

    #[test]
    fn space_toggles_play_and_escape_stops() {
        let toggle = resolve(ShortcutKeys {
            toggle_play: true,
            ..Default::default()
        });
        assert!(toggle.toggle_play);
        assert!(!toggle.stop_play);
        let stop = resolve(ShortcutKeys {
            stop_play: true,
            ..Default::default()
        });
        assert!(stop.stop_play);
        assert!(!stop.toggle_play);
        let modified = resolve(ShortcutKeys {
            command: true,
            toggle_play: true,
            stop_play: true,
            ..Default::default()
        });
        assert!(!modified.toggle_play && !modified.stop_play);
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
