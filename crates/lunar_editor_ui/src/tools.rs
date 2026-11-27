//! Herramientas de transformación y plantillas de creación del editor.
//!
//! Son datos puros: la GUI solo decide qué herramienta está activa y qué
//! plantilla se ha pedido. Ninguna de ellas crea componentes nuevos ni toca
//! el esquema v2 de ADR-008; las plantillas reutilizan los componentes que ya
//! existen y las cámaras/luces son marcadores de transformación hasta que
//! exista una simulación de Play (decisión B de esta fase).

use crate::assets::AssetKind;

/// Herramienta activa sobre el lienzo, con los atajos de Unity/Unreal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TransformMode {
    #[default]
    Move,
    Rotate,
    Scale,
}

impl TransformMode {
    /// Texto corto para la barra del lienzo.
    #[must_use]
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Move => "Mover (W)",
            Self::Rotate => "Rotar (E)",
            Self::Scale => "Escalar (R)",
        }
    }
}

/// Espacio de coordenadas del gizmo (E14). `Global` coloca el tirador en el
/// centro del grupo y alinea los ejes con el mundo; `Local` lo coloca en el
/// origen de la entidad ancla y gira los ejes con ella.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TransformSpace {
    #[default]
    Global,
    Local,
}

impl TransformSpace {
    #[must_use]
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Global => "Global",
            Self::Local => "Local",
        }
    }

    #[must_use]
    pub(crate) fn toggle(self) -> Self {
        match self {
            Self::Global => Self::Local,
            Self::Local => Self::Global,
        }
    }
}

/// Plantilla ofrecida en el menú «Crear» de la jerarquía.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntityTemplate {
    Empty,
    Sprite,
    Audio,
    Collider2D,
    Mesh,
    Camera2D,
    Light2D,
}

impl EntityTemplate {
    /// Orden estable del menú.
    pub(crate) const ALL: [Self; 7] = [
        Self::Empty,
        Self::Sprite,
        Self::Audio,
        Self::Collider2D,
        Self::Mesh,
        Self::Camera2D,
        Self::Light2D,
    ];

    #[must_use]
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Empty => "Entidad vacía",
            Self::Sprite => "Sprite (primer PNG/JPEG)",
            Self::Audio => "Audio (primer WAV)",
            Self::Collider2D => "Colisionador 2D",
            Self::Mesh => "Malla (primer glTF)",
            Self::Camera2D => "Cámara 2D (marcador)",
            Self::Light2D => "Luz 2D (marcador)",
        }
    }

    /// Nombre por defecto de la entidad creada.
    #[must_use]
    pub(crate) fn name(self, id: u64) -> String {
        match self {
            Self::Empty => format!("Entidad {id}"),
            Self::Sprite => format!("Sprite {id}"),
            Self::Audio => format!("Audio {id}"),
            Self::Collider2D => format!("Colisionador {id}"),
            Self::Mesh => format!("Malla {id}"),
            Self::Camera2D => format!("Cámara 2D {id}"),
            Self::Light2D => format!("Luz 2D {id}"),
        }
    }

    /// Asset del panel que necesita la plantilla, si alguno.
    #[must_use]
    pub(crate) fn asset_kind(self) -> Option<AssetKind> {
        match self {
            Self::Sprite => Some(AssetKind::Image),
            Self::Audio => Some(AssetKind::Audio),
            Self::Mesh => Some(AssetKind::Mesh),
            Self::Empty | Self::Collider2D | Self::Camera2D | Self::Light2D => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_is_the_default_tool() {
        assert_eq!(TransformMode::default(), TransformMode::Move);
        assert_eq!(TransformMode::Move.label(), "Mover (W)");
        assert_eq!(TransformMode::Rotate.label(), "Rotar (E)");
        assert_eq!(TransformMode::Scale.label(), "Escalar (R)");
    }

    #[test]
    fn global_is_the_default_space_and_toggles() {
        assert_eq!(TransformSpace::default(), TransformSpace::Global);
        assert_eq!(TransformSpace::Global.label(), "Global");
        assert_eq!(TransformSpace::Local.label(), "Local");
        assert_eq!(TransformSpace::Global.toggle(), TransformSpace::Local);
        assert_eq!(TransformSpace::Local.toggle(), TransformSpace::Global);
    }

    #[test]
    fn only_asset_templates_need_a_library_entry() {
        assert_eq!(EntityTemplate::Sprite.asset_kind(), Some(AssetKind::Image));
        assert_eq!(EntityTemplate::Audio.asset_kind(), Some(AssetKind::Audio));
        assert_eq!(EntityTemplate::Mesh.asset_kind(), Some(AssetKind::Mesh));
        for template in [
            EntityTemplate::Empty,
            EntityTemplate::Collider2D,
            EntityTemplate::Camera2D,
            EntityTemplate::Light2D,
        ] {
            assert_eq!(template.asset_kind(), None);
        }
    }

    #[test]
    fn the_menu_has_unique_labels_and_names() {
        let labels: Vec<_> = EntityTemplate::ALL
            .iter()
            .map(|item| item.label())
            .collect();
        let unique: std::collections::BTreeSet<_> = labels.iter().collect();
        assert_eq!(unique.len(), labels.len());
        assert_eq!(EntityTemplate::ALL[0].name(7), "Entidad 7");
        assert_eq!(EntityTemplate::Camera2D.name(9), "Cámara 2D 9");
    }
}
