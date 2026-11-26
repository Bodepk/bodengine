//! Simulación local del editor para Play/Pause/Stop (decisión C).
//!
//! No sustituye al runtime ni abre dispositivos: integra la gravedad sobre las
//! entidades con `Collider2D` dinámico y escribe las posiciones en el mapa de
//! vistas previas del lienzo. La escena del documento no se toca mientras se
//! reproduce; `Stop` la restaura desde la instantánea capturada al empezar.

use lunar_scenes::{BodyType, Component, Scene};

/// Estado de la reproducción.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PlayState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

impl PlayState {
    #[must_use]
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Stopped => "Detenido",
            Self::Playing => "En juego",
            Self::Paused => "En pausa",
        }
    }
}

/// Gravedad en unidades de mundo por segundo al cuadrado (Y crece arriba).
pub(crate) const GRAVITY: f32 = -9.81;
/// Suelo de la simulación local.
pub(crate) const FLOOR_Y: f32 = 0.0;
/// Paso máximo para que un tirón de frame no haga saltar la simulación.
pub(crate) const MAX_STEP: f32 = 1.0 / 15.0;

/// Ids con colisionador dinámico, en orden de escena.
#[must_use]
pub(crate) fn dynamic_ids(scene: &Scene) -> Vec<u64> {
    scene
        .entities
        .iter()
        .filter(|entity| {
            entity.components.iter().any(|component| {
                matches!(
                    component,
                    Component::Collider2D {
                        body: BodyType::Dynamic,
                        ..
                    }
                )
            })
        })
        .map(|entity| entity.id)
        .collect()
}

/// Integra velocidad y altura con un suelo simple. Puro y determinista.
#[must_use]
pub(crate) fn step_vertical(y: f32, velocity: f32, dt: f32) -> (f32, f32) {
    let dt = dt.clamp(0.0, MAX_STEP);
    let velocity = velocity + GRAVITY * dt;
    let y = y + velocity * dt;
    if y <= FLOOR_Y {
        (FLOOR_Y, 0.0)
    } else {
        (y, velocity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lunar_scenes::{SceneEntity, Shape2D};

    #[test]
    fn only_dynamic_colliders_take_part() {
        let mut scene = Scene::new();
        let dynamic = SceneEntity::new(1);
        let mut fixed = SceneEntity::new(2);
        fixed.components.push(Component::Collider2D {
            shape: Shape2D::Ball { radius: 0.5 },
            body: BodyType::Fixed,
        });
        let mut moving = SceneEntity::new(3);
        moving.components.push(Component::Collider2D {
            shape: Shape2D::Ball { radius: 0.5 },
            body: BodyType::Dynamic,
        });
        scene.entities.extend([dynamic, fixed, moving]);
        assert_eq!(dynamic_ids(&scene), [3]);
    }

    #[test]
    fn gravity_pulls_down_and_accelerates() {
        let (y, velocity) = step_vertical(2.0, 0.0, 0.1);
        assert!(y < 2.0);
        assert!(velocity < 0.0);
        let (y2, velocity2) = step_vertical(y, velocity, 0.1);
        assert!(y2 < y);
        assert!(velocity2 < velocity);
    }

    #[test]
    fn the_floor_stops_the_fall() {
        let (y, velocity) = step_vertical(0.01, -5.0, MAX_STEP);
        assert_eq!(y, FLOOR_Y);
        assert_eq!(velocity, 0.0);
    }

    #[test]
    fn non_positive_steps_are_no_ops() {
        assert_eq!(step_vertical(3.0, -1.0, 0.0), (3.0, -1.0));
        assert_eq!(step_vertical(3.0, -1.0, -1.0), (3.0, -1.0));
    }
}
