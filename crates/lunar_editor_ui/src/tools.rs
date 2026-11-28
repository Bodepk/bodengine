//! Herramientas de transformación y plantillas de creación del editor.
//!
//! Son datos puros: la GUI solo decide qué herramienta está activa y qué
//! plantilla se ha pedido. Ninguna de ellas crea componentes nuevos ni toca
//! el esquema v2 de ADR-008; las plantillas reutilizan los componentes que ya
//! existen y las cámaras/luces son marcadores de transformación hasta que
//! exista una simulación de Play (decisión B de esta fase).

use lunar_scenes::Transform;

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
    /// Luz del editor con sidecar propio (E49, ADR-050).
    Light3D,
    /// Primitivas de caja para probar luces y materiales (ADR-052).
    Cube,
    Plane,
    Wall,
    Column,
    Ramp,
}

impl EntityTemplate {
    /// Orden estable del menú.
    pub(crate) const ALL: [Self; 13] = [
        Self::Empty,
        Self::Sprite,
        Self::Audio,
        Self::Collider2D,
        Self::Mesh,
        Self::Camera2D,
        Self::Light2D,
        Self::Light3D,
        Self::Cube,
        Self::Plane,
        Self::Wall,
        Self::Column,
        Self::Ramp,
    ];

    /// Primitivas 3D que se agrupan en su propio submenú.
    pub(crate) const PRIMITIVES: [Self; 5] = [
        Self::Cube,
        Self::Plane,
        Self::Wall,
        Self::Column,
        Self::Ramp,
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
            Self::Light3D => "Luz 3D (marcador)",
            Self::Cube => "Cubo 1×1",
            Self::Plane => "Plano 8×8",
            Self::Wall => "Muro 4×3",
            Self::Column => "Columna 0.5×3",
            Self::Ramp => "Rampa inclinada",
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
            Self::Light3D => format!("Luz 3D {id}"),
            Self::Cube => format!("Cubo {id}"),
            Self::Plane => format!("Plano {id}"),
            Self::Wall => format!("Muro {id}"),
            Self::Column => format!("Columna {id}"),
            Self::Ramp => format!("Rampa {id}"),
        }
    }

    /// Asset del panel que necesita la plantilla, si alguno.
    #[must_use]
    pub(crate) fn asset_kind(self) -> Option<AssetKind> {
        match self {
            Self::Sprite => Some(AssetKind::Image),
            Self::Audio => Some(AssetKind::Audio),
            Self::Mesh => Some(AssetKind::Mesh),
            Self::Empty
            | Self::Collider2D
            | Self::Camera2D
            | Self::Light2D
            | Self::Light3D
            | Self::Cube
            | Self::Plane
            | Self::Wall
            | Self::Column
            | Self::Ramp => None,
        }
    }

    /// Transformación inicial de la plantilla. Las primitivas nacen apoyadas en
    /// el suelo y con un tamaño legible para probar luces y materiales.
    #[must_use]
    pub(crate) fn transform(self) -> Option<Transform> {
        let scale = |scale: [f32; 3]| Transform {
            scale,
            ..Transform::default()
        };
        match self {
            Self::Cube => Some(Transform {
                translation: [0.0, 0.5, 0.0],
                ..Transform::default()
            }),
            Self::Plane => Some(Transform {
                translation: [0.0, 0.05, 0.0],
                ..scale([8.0, 0.1, 8.0])
            }),
            Self::Wall => Some(Transform {
                translation: [0.0, 1.5, 0.0],
                ..scale([4.0, 3.0, 0.2])
            }),
            Self::Column => Some(Transform {
                translation: [0.0, 1.5, 0.0],
                ..scale([0.5, 3.0, 0.5])
            }),
            Self::Ramp => Some(Transform {
                translation: [0.0, 0.5, 0.0],
                // Unos 20° alrededor de X: rampa transitable sin colisionador.
                rotation: [-0.174_108, 0.0, 0.0, 0.984_731],
                ..scale([3.0, 0.2, 2.0])
            }),
            Self::Empty
            | Self::Sprite
            | Self::Audio
            | Self::Collider2D
            | Self::Mesh
            | Self::Camera2D
            | Self::Light2D
            | Self::Light3D => None,
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
    fn primitives_carry_a_usable_initial_transform() {
        assert!(EntityTemplate::Cube.transform().is_some());
        assert!(EntityTemplate::Plane.transform().is_some());
        assert!(EntityTemplate::Wall.transform().is_some());
        assert!(EntityTemplate::Column.transform().is_some());
        assert!(EntityTemplate::Ramp.transform().is_some());
        assert!(EntityTemplate::Empty.transform().is_none());
        let plane = EntityTemplate::Plane.transform().expect("plano");
        assert!(plane.scale[0] > plane.scale[1]);
        assert!(plane.scale[2] > plane.scale[1]);
        let ramp = EntityTemplate::Ramp.transform().expect("rampa");
        assert!(ramp.rotation[0].abs() > 0.0);
        for template in EntityTemplate::PRIMITIVES {
            assert!(!template.name(1).starts_with("Cámara"));
            assert!(!template.name(1).starts_with("Luz"));
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
