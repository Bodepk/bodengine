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
/// Margen por debajo del colisionador más bajo para situar el suelo.
pub(crate) const FLOOR_MARGIN: f32 = 1.0;
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

/// Suelo de la simulación, derivado de la escena para que ninguna entidad
/// situada por debajo del origen "salte" hacia arriba al empezar a jugar.
#[must_use]
pub(crate) fn floor_y(scene: &Scene) -> f32 {
    let lowest = scene
        .entities
        .iter()
        .filter(|entity| {
            entity
                .components
                .iter()
                .any(|component| matches!(component, Component::Collider2D { .. }))
        })
        .map(|entity| entity.transform.translation[1])
        .fold(f32::INFINITY, f32::min);
    if lowest.is_finite() {
        lowest - FLOOR_MARGIN
    } else {
        0.0
    }
}

/// Integra velocidad y altura con un suelo simple. Puro y determinista.
#[must_use]
pub(crate) fn step_vertical(y: f32, velocity: f32, dt: f32, floor: f32) -> (f32, f32) {
    let dt = dt.clamp(0.0, MAX_STEP);
    let velocity = velocity + GRAVITY * dt;
    let y = y + velocity * dt;
    if y <= floor {
        (floor, 0.0)
    } else {
        (y, velocity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lunar_scenes::{SceneEntity, Shape2D};

    fn entity_with_collider(id: u64, y: f32, body: BodyType) -> SceneEntity {
        let mut entity = SceneEntity::new(id);
        entity.transform.translation = [0.0, y, 0.0];
        entity.components.push(Component::Collider2D {
            shape: Shape2D::Ball { radius: 0.5 },
            body,
        });
        entity
    }

    #[test]
    fn only_dynamic_colliders_take_part() {
        let mut scene = Scene::new();
        scene.entities.extend([
            SceneEntity::new(1),
            entity_with_collider(2, 0.0, BodyType::Fixed),
            entity_with_collider(3, 0.0, BodyType::Dynamic),
        ]);
        assert_eq!(dynamic_ids(&scene), [3]);
    }

    #[test]
    fn floor_comes_from_the_lowest_collider() {
        let mut scene = Scene::new();
        scene.entities.extend([
            entity_with_collider(1, 3.0, BodyType::Dynamic),
            entity_with_collider(2, -11.9, BodyType::Fixed),
        ]);
        assert_eq!(floor_y(&scene), -12.9);
        assert_eq!(floor_y(&Scene::new()), 0.0);
    }

    #[test]
    fn gravity_pulls_down_and_accelerates() {
        let floor = -100.0;
        let (y, velocity) = step_vertical(2.0, 0.0, 0.1, floor);
        assert!(y < 2.0);
        assert!(velocity < 0.0);
        let (y2, velocity2) = step_vertical(y, velocity, 0.1, floor);
        assert!(y2 < y);
        assert!(velocity2 < velocity);
    }

    #[test]
    fn the_floor_stops_the_fall() {
        let (y, velocity) = step_vertical(0.01, -5.0, MAX_STEP, 0.0);
        assert_eq!(y, 0.0);
        assert_eq!(velocity, 0.0);
    }

    #[test]
    fn non_positive_steps_are_no_ops() {
        assert_eq!(step_vertical(3.0, -1.0, 0.0, -100.0), (3.0, -1.0));
        assert_eq!(step_vertical(3.0, -1.0, -1.0, -100.0), (3.0, -1.0));
    }
}
