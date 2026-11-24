//! Física 2D con `rapier2d` (ADR-010).
//!
//! El estado de Rapier vive aquí y no se expone: la API pública usa
//! `lunar_math::Vec2` e identificadores opacos.

use std::collections::HashMap;
use std::fmt;

use lunar_math::Vec2;
use rapier2d::prelude::{
    ColliderBuilder, ColliderHandle, PhysicsWorld, QueryFilter, Ray, RigidBodyBuilder,
};

/// Identificador opaco de un cuerpo rígido.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodyId(u64);

/// Identificador opaco de un colisionador.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColliderId(u64);

/// Tipo de cuerpo rígido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyType2D {
    /// Simulado por fuerzas y colisiones.
    Dynamic,
    /// No se mueve.
    Fixed,
    /// Se mueve por posición asignada, sin ser afectado por fuerzas.
    KinematicPositionBased,
}

/// Forma de un colisionador 2D.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape2D {
    /// Círculo de radio dado.
    Ball { radius: f32 },
    /// Rectángulo con semiextensiones.
    Cuboid { half_extents: Vec2 },
}

impl Shape2D {
    fn is_valid(self) -> bool {
        match self {
            Self::Ball { radius } => radius.is_finite() && radius > 0.0,
            Self::Cuboid { half_extents } => {
                half_extents.x.is_finite()
                    && half_extents.y.is_finite()
                    && half_extents.x > 0.0
                    && half_extents.y > 0.0
            }
        }
    }
}

/// Descripción de un cuerpo rígido.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyDesc2D {
    body_type: BodyType2D,
    position: Vec2,
    rotation: f32,
    linvel: Vec2,
}

impl BodyDesc2D {
    /// Crea una descripción con el tipo y la posición dados.
    #[must_use]
    pub const fn new(body_type: BodyType2D, position: Vec2) -> Self {
        Self {
            body_type,
            position,
            rotation: 0.0,
            linvel: Vec2::new(0.0, 0.0),
        }
    }

    /// Asigna la rotación inicial en radianes.
    #[must_use]
    pub const fn with_rotation(mut self, rotation: f32) -> Self {
        self.rotation = rotation;
        self
    }

    /// Asigna la velocidad lineal inicial.
    #[must_use]
    pub const fn with_linear_velocity(mut self, linvel: Vec2) -> Self {
        self.linvel = linvel;
        self
    }

    fn is_valid(self) -> bool {
        self.position.x.is_finite()
            && self.position.y.is_finite()
            && self.rotation.is_finite()
            && self.linvel.x.is_finite()
            && self.linvel.y.is_finite()
    }

    fn builder(self) -> RigidBodyBuilder {
        let mut builder = match self.body_type {
            BodyType2D::Dynamic => RigidBodyBuilder::dynamic(),
            BodyType2D::Fixed => RigidBodyBuilder::fixed(),
            BodyType2D::KinematicPositionBased => RigidBodyBuilder::kinematic_position_based(),
        };
        builder = builder
            .translation(to_vector(self.position))
            .rotation(self.rotation)
            .linvel(to_vector(self.linvel));
        builder
    }
}

/// Descripción de un colisionador adjunto a un cuerpo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColliderDesc2D {
    shape: Shape2D,
    density: f32,
    restitution: f32,
    friction: f32,
}

impl ColliderDesc2D {
    /// Crea una descripción con materiales por defecto.
    #[must_use]
    pub const fn new(shape: Shape2D) -> Self {
        Self {
            shape,
            density: 1.0,
            restitution: 0.0,
            friction: 0.5,
        }
    }

    /// Asigna la densidad.
    #[must_use]
    pub const fn with_density(mut self, density: f32) -> Self {
        self.density = density;
        self
    }

    /// Asigna la restitución.
    #[must_use]
    pub const fn with_restitution(mut self, restitution: f32) -> Self {
        self.restitution = restitution;
        self
    }

    /// Asigna la fricción.
    #[must_use]
    pub const fn with_friction(mut self, friction: f32) -> Self {
        self.friction = friction;
        self
    }

    fn is_valid(self) -> bool {
        self.shape.is_valid()
            && self.density.is_finite()
            && self.density > 0.0
            && self.restitution.is_finite()
            && self.restitution >= 0.0
            && self.friction.is_finite()
            && self.friction >= 0.0
    }

    fn builder(self) -> ColliderBuilder {
        let builder = match self.shape {
            Shape2D::Ball { radius } => ColliderBuilder::ball(radius),
            Shape2D::Cuboid { half_extents } => {
                ColliderBuilder::cuboid(half_extents.x, half_extents.y)
            }
        };
        builder
            .density(self.density)
            .restitution(self.restitution)
            .friction(self.friction)
    }
}

/// Estado observable de un cuerpo rígido.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyState2D {
    pub position: Vec2,
    pub rotation: f32,
    pub linvel: Vec2,
}

/// Resultado de un raycast.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit2D {
    pub collider: ColliderId,
    pub toi: f32,
}

/// Error devuelto cuando el paso temporal no es finito y positivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidTimestep;

impl fmt::Display for InvalidTimestep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("el paso de física debe ser finito y mayor que cero")
    }
}

impl std::error::Error for InvalidTimestep {}

/// Mundo físico 2D. No contiene ventana, GPU ni reloj propio.
pub struct PhysicsWorld2D {
    world: PhysicsWorld,
    bodies: HashMap<BodyId, rapier2d::dynamics::RigidBodyHandle>,
    colliders: HashMap<ColliderId, ColliderHandle>,
    collider_by_handle: HashMap<ColliderHandle, ColliderId>,
    colliders_by_body: HashMap<BodyId, Vec<ColliderId>>,
    next_body: u64,
    next_collider: u64,
}

impl PhysicsWorld2D {
    /// Crea un mundo con la gravedad dada.
    #[must_use]
    pub fn new(gravity: Vec2) -> Self {
        let world = PhysicsWorld {
            gravity: to_vector(gravity),
            ..Default::default()
        };
        Self {
            world,
            bodies: HashMap::new(),
            colliders: HashMap::new(),
            collider_by_handle: HashMap::new(),
            colliders_by_body: HashMap::new(),
            next_body: 0,
            next_collider: 0,
        }
    }

    /// Devuelve la gravedad actual.
    #[must_use]
    pub fn gravity(&self) -> Vec2 {
        from_vector(&self.world.gravity)
    }

    /// Cambia la gravedad.
    pub fn set_gravity(&mut self, gravity: Vec2) {
        self.world.gravity = to_vector(gravity);
    }

    /// Añade un cuerpo rígido y devuelve su identificador.
    pub fn add_body(&mut self, desc: BodyDesc2D) -> Option<BodyId> {
        if !desc.is_valid() {
            return None;
        }
        let handle = self.world.insert_body(desc.builder());
        let id = BodyId(self.next_body);
        self.next_body += 1;
        self.bodies.insert(id, handle);
        Some(id)
    }

    /// Añade un colisionador adjunto a un cuerpo existente.
    pub fn add_collider(&mut self, body: BodyId, desc: ColliderDesc2D) -> Option<ColliderId> {
        if !desc.is_valid() {
            return None;
        }
        let parent = *self.bodies.get(&body)?;
        let handle = self.world.insert_collider(desc.builder(), Some(parent));
        let id = ColliderId(self.next_collider);
        self.next_collider += 1;
        self.colliders.insert(id, handle);
        self.collider_by_handle.insert(handle, id);
        self.colliders_by_body.entry(body).or_default().push(id);
        Some(id)
    }

    /// Elimina un cuerpo y todos sus colisionadores. Devuelve `false` si el id es desconocido.
    pub fn remove_body(&mut self, body: BodyId) -> bool {
        let Some(handle) = self.bodies.remove(&body) else {
            return false;
        };
        self.world.remove_body(handle);
        if let Some(ids) = self.colliders_by_body.remove(&body) {
            for id in ids {
                if let Some(handle) = self.colliders.remove(&id) {
                    self.collider_by_handle.remove(&handle);
                }
            }
        }
        true
    }

    /// Número de cuerpos vivos.
    #[must_use]
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Número de colisionadores vivos.
    #[must_use]
    pub fn collider_count(&self) -> usize {
        self.colliders.len()
    }

    /// Devuelve el estado de un cuerpo, o `None` si el id no existe.
    #[must_use]
    pub fn body_state(&self, body: BodyId) -> Option<BodyState2D> {
        let handle = self.bodies.get(&body)?;
        let rigid_body = self.world.bodies.get(*handle)?;
        Some(BodyState2D {
            position: from_vector(&rigid_body.translation()),
            rotation: rigid_body.rotation().angle(),
            linvel: from_vector(&rigid_body.linvel()),
        })
    }

    /// Aplica un impulso a un cuerpo dinámico. Devuelve `false` si el id no existe.
    pub fn apply_impulse(&mut self, body: BodyId, impulse: Vec2) -> bool {
        if !impulse.x.is_finite() || !impulse.y.is_finite() {
            return false;
        }
        let Some(handle) = self.bodies.get(&body).copied() else {
            return false;
        };
        let Some(rigid_body) = self.world.bodies.get_mut(handle) else {
            return false;
        };
        rigid_body.apply_impulse(to_vector(impulse), true);
        true
    }

    /// Avanza la simulación con el `dt` dado, en segundos.
    pub fn step(&mut self, dt: f32) -> Result<(), InvalidTimestep> {
        if !dt.is_finite() || dt <= 0.0 {
            return Err(InvalidTimestep);
        }
        self.world.integration_parameters.dt = dt;
        self.world.step();
        Ok(())
    }

    /// Lanza un rayo y devuelve el primer colisionador alcanzado.
    #[must_use]
    pub fn cast_ray(&self, origin: Vec2, direction: Vec2, max_toi: f32) -> Option<RayHit2D> {
        if !origin.x.is_finite()
            || !origin.y.is_finite()
            || !direction.x.is_finite()
            || !direction.y.is_finite()
            || !max_toi.is_finite()
            || max_toi <= 0.0
            || direction.length_squared() == 0.0
        {
            return None;
        }
        let ray = Ray::new(to_vector(origin), to_vector(direction));
        let (handle, toi) = self
            .world
            .cast_ray(&ray, max_toi, true, QueryFilter::default())?;
        Some(RayHit2D {
            collider: *self.collider_by_handle.get(&handle)?,
            toi,
        })
    }
}

impl Default for PhysicsWorld2D {
    fn default() -> Self {
        Self::new(Vec2::new(0.0, -9.81))
    }
}

fn to_vector(v: Vec2) -> rapier2d::math::Vec2 {
    rapier2d::math::Vec2::new(v.x, v.y)
}

fn from_vector(v: &rapier2d::math::Vec2) -> Vec2 {
    Vec2::new(v.x, v.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn falling_ball() -> (PhysicsWorld2D, BodyId) {
        let mut world = PhysicsWorld2D::default();
        let body = world
            .add_body(BodyDesc2D::new(BodyType2D::Dynamic, Vec2::new(0.0, 4.0)))
            .unwrap();
        world
            .add_collider(body, ColliderDesc2D::new(Shape2D::Ball { radius: 0.5 }))
            .unwrap();
        (world, body)
    }

    #[test]
    fn gravity_moves_dynamic_bodies_down() {
        let (mut world, body) = falling_ball();
        for _ in 0..60 {
            world.step(DT).unwrap();
        }
        let state = world.body_state(body).unwrap();
        assert!(state.position.y < 0.0, "y={}", state.position.y);
        assert!(state.linvel.y < 0.0, "vy={}", state.linvel.y);
    }

    #[test]
    fn fixed_bodies_do_not_move() {
        let mut world = PhysicsWorld2D::default();
        let ground = world
            .add_body(BodyDesc2D::new(BodyType2D::Fixed, Vec2::new(0.0, -1.0)))
            .unwrap();
        world
            .add_collider(
                ground,
                ColliderDesc2D::new(Shape2D::Cuboid {
                    half_extents: Vec2::new(10.0, 0.1),
                }),
            )
            .unwrap();

        for _ in 0..30 {
            world.step(DT).unwrap();
        }
        assert_eq!(
            world.body_state(ground).unwrap().position,
            Vec2::new(0.0, -1.0)
        );
    }

    #[test]
    fn ball_rests_on_cuboid_ground() {
        let mut world = PhysicsWorld2D::default();
        let ground = world
            .add_body(BodyDesc2D::new(BodyType2D::Fixed, Vec2::new(0.0, -1.0)))
            .unwrap();
        world
            .add_collider(
                ground,
                ColliderDesc2D::new(Shape2D::Cuboid {
                    half_extents: Vec2::new(10.0, 0.1),
                }),
            )
            .unwrap();
        let ball = world
            .add_body(BodyDesc2D::new(BodyType2D::Dynamic, Vec2::new(0.0, 3.0)))
            .unwrap();
        world
            .add_collider(ball, ColliderDesc2D::new(Shape2D::Ball { radius: 0.5 }))
            .unwrap();

        for _ in 0..180 {
            world.step(DT).unwrap();
        }
        let state = world.body_state(ball).unwrap();
        assert!(
            (-0.6..=-0.2).contains(&state.position.y),
            "y={}",
            state.position.y
        );
        assert!(state.linvel.y.abs() < 0.1, "vy={}", state.linvel.y);
    }

    #[test]
    fn invalid_timestep_does_not_step() {
        let (mut world, body) = falling_ball();
        let before = world.body_state(body).unwrap();
        assert_eq!(world.step(0.0), Err(InvalidTimestep));
        assert_eq!(world.step(f32::NAN), Err(InvalidTimestep));
        assert_eq!(world.step(-DT), Err(InvalidTimestep));
        assert_eq!(world.body_state(body).unwrap(), before);
    }

    #[test]
    fn impulse_changes_linear_velocity() {
        let (mut world, body) = falling_ball();
        let before = world.body_state(body).unwrap().linvel;
        assert!(world.apply_impulse(body, Vec2::new(10.0, 0.0)));
        let after = world.body_state(body).unwrap().linvel;
        assert!(after.x > before.x);
    }

    #[test]
    fn raycast_returns_the_collider_id() {
        let mut world = PhysicsWorld2D::new(Vec2::new(0.0, 0.0));
        let body = world
            .add_body(BodyDesc2D::new(BodyType2D::Fixed, Vec2::new(0.0, 0.0)))
            .unwrap();
        let collider = world
            .add_collider(body, ColliderDesc2D::new(Shape2D::Ball { radius: 1.0 }))
            .unwrap();
        world.step(DT).unwrap();

        let hit = world
            .cast_ray(Vec2::new(0.0, 5.0), Vec2::new(0.0, -1.0), 10.0)
            .unwrap();
        assert_eq!(hit.collider, collider);
        assert!((3.9..4.1).contains(&hit.toi), "toi={}", hit.toi);
    }

    #[test]
    fn invalid_descriptions_are_rejected() {
        let mut world = PhysicsWorld2D::default();
        assert!(
            world
                .add_body(BodyDesc2D::new(
                    BodyType2D::Dynamic,
                    Vec2::new(f32::NAN, 0.0)
                ))
                .is_none()
        );
        let body = world
            .add_body(BodyDesc2D::new(BodyType2D::Dynamic, Vec2::new(0.0, 0.0)))
            .unwrap();
        assert!(
            world
                .add_collider(body, ColliderDesc2D::new(Shape2D::Ball { radius: 0.0 }))
                .is_none()
        );
        assert_eq!(world.body_count(), 1);
        assert_eq!(world.collider_count(), 0);
    }

    #[test]
    fn removing_a_body_invalidates_its_id() {
        let (mut world, body) = falling_ball();
        assert_eq!(world.collider_count(), 1);
        assert!(world.remove_body(body));
        assert!(!world.remove_body(body));
        assert_eq!(world.body_count(), 0);
        assert_eq!(world.collider_count(), 0);
        assert!(world.body_state(body).is_none());
    }
}
