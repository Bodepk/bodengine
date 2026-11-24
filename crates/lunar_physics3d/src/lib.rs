//! Física 3D con `rapier3d` (ADR-011).
//!
//! El estado de Rapier vive aquí y no se expone: la API pública usa
//! `lunar_math::Vec3`, `lunar_math::Quat` e identificadores opacos.

use std::collections::HashMap;
use std::fmt;

use lunar_math::{Quat, Vec3};
use rapier3d::prelude::{
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
pub enum BodyType3D {
    /// Simulado por fuerzas y colisiones.
    Dynamic,
    /// No se mueve.
    Fixed,
    /// Se mueve por posición asignada, sin ser afectado por fuerzas.
    KinematicPositionBased,
}

/// Forma de un colisionador 3D.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape3D {
    /// Esfera de radio dado.
    Ball { radius: f32 },
    /// Caja con semiextensiones.
    Cuboid { half_extents: Vec3 },
    /// Cápsula alineada con el eje Y.
    Capsule { half_height: f32, radius: f32 },
}

impl Shape3D {
    fn is_valid(self) -> bool {
        match self {
            Self::Ball { radius } => radius.is_finite() && radius > 0.0,
            Self::Cuboid { half_extents } => {
                half_extents.x.is_finite()
                    && half_extents.y.is_finite()
                    && half_extents.z.is_finite()
                    && half_extents.x > 0.0
                    && half_extents.y > 0.0
                    && half_extents.z > 0.0
            }
            Self::Capsule {
                half_height,
                radius,
            } => half_height.is_finite() && half_height > 0.0 && radius.is_finite() && radius > 0.0,
        }
    }
}

/// Descripción de un cuerpo rígido.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyDesc3D {
    body_type: BodyType3D,
    position: Vec3,
    rotation: Quat,
    linvel: Vec3,
}

impl BodyDesc3D {
    /// Crea una descripción con el tipo y la posición dados, sin rotación ni velocidad.
    #[must_use]
    pub const fn new(body_type: BodyType3D, position: Vec3) -> Self {
        Self {
            body_type,
            position,
            rotation: Quat::IDENTITY,
            linvel: Vec3::new(0.0, 0.0, 0.0),
        }
    }

    /// Asigna la rotación inicial. Se normaliza antes de crear el cuerpo.
    #[must_use]
    pub const fn with_rotation(mut self, rotation: Quat) -> Self {
        self.rotation = rotation;
        self
    }

    /// Asigna la velocidad lineal inicial.
    #[must_use]
    pub const fn with_linear_velocity(mut self, linvel: Vec3) -> Self {
        self.linvel = linvel;
        self
    }

    fn is_valid(self) -> bool {
        is_vec3_valid(self.position)
            && is_vec3_valid(self.linvel)
            && normalize_quat(self.rotation).is_some()
    }

    fn builder(self) -> RigidBodyBuilder {
        let mut builder = match self.body_type {
            BodyType3D::Dynamic => RigidBodyBuilder::dynamic(),
            BodyType3D::Fixed => RigidBodyBuilder::fixed(),
            BodyType3D::KinematicPositionBased => RigidBodyBuilder::kinematic_position_based(),
        };
        let mut pose = rapier3d::math::Pose::from_translation(to_vector(self.position));
        pose.rotation = to_rotation(self.rotation);
        builder = builder.pose(pose).linvel(to_vector(self.linvel));
        builder
    }
}

/// Descripción de un colisionador adjunto a un cuerpo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColliderDesc3D {
    shape: Shape3D,
    density: f32,
    restitution: f32,
    friction: f32,
}

impl ColliderDesc3D {
    /// Crea una descripción con materiales por defecto.
    #[must_use]
    pub const fn new(shape: Shape3D) -> Self {
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
            Shape3D::Ball { radius } => ColliderBuilder::ball(radius),
            Shape3D::Cuboid { half_extents } => {
                ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z)
            }
            Shape3D::Capsule {
                half_height,
                radius,
            } => ColliderBuilder::capsule_y(half_height, radius),
        };
        builder
            .density(self.density)
            .restitution(self.restitution)
            .friction(self.friction)
    }
}

/// Estado observable de un cuerpo rígido.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyState3D {
    pub position: Vec3,
    pub rotation: Quat,
    pub linvel: Vec3,
}

/// Resultado de un raycast.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit3D {
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

/// Mundo físico 3D. No contiene ventana, GPU ni reloj propio.
pub struct PhysicsWorld3D {
    world: PhysicsWorld,
    bodies: HashMap<BodyId, rapier3d::dynamics::RigidBodyHandle>,
    colliders: HashMap<ColliderId, ColliderHandle>,
    collider_by_handle: HashMap<ColliderHandle, ColliderId>,
    colliders_by_body: HashMap<BodyId, Vec<ColliderId>>,
    next_body: u64,
    next_collider: u64,
}

impl PhysicsWorld3D {
    /// Crea un mundo con la gravedad dada.
    #[must_use]
    pub fn new(gravity: Vec3) -> Self {
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
    pub fn gravity(&self) -> Vec3 {
        from_vector(&self.world.gravity)
    }

    /// Cambia la gravedad.
    pub fn set_gravity(&mut self, gravity: Vec3) {
        self.world.gravity = to_vector(gravity);
    }

    /// Añade un cuerpo rígido y devuelve su identificador.
    pub fn add_body(&mut self, desc: BodyDesc3D) -> Option<BodyId> {
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
    pub fn add_collider(&mut self, body: BodyId, desc: ColliderDesc3D) -> Option<ColliderId> {
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
    pub fn body_state(&self, body: BodyId) -> Option<BodyState3D> {
        let handle = self.bodies.get(&body)?;
        let rigid_body = self.world.bodies.get(*handle)?;
        Some(BodyState3D {
            position: from_vector(&rigid_body.translation()),
            rotation: from_rotation(rigid_body.rotation()),
            linvel: from_vector(&rigid_body.linvel()),
        })
    }

    /// Aplica un impulso a un cuerpo dinámico. Devuelve `false` si el id no existe.
    pub fn apply_impulse(&mut self, body: BodyId, impulse: Vec3) -> bool {
        if !is_vec3_valid(impulse) {
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
    pub fn cast_ray(&self, origin: Vec3, direction: Vec3, max_toi: f32) -> Option<RayHit3D> {
        if !is_vec3_valid(origin)
            || !is_vec3_valid(direction)
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
        Some(RayHit3D {
            collider: *self.collider_by_handle.get(&handle)?,
            toi,
        })
    }
}

impl Default for PhysicsWorld3D {
    fn default() -> Self {
        Self::new(Vec3::new(0.0, -9.81, 0.0))
    }
}

fn is_vec3_valid(v: Vec3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
}

fn normalize_quat(q: Quat) -> Option<Quat> {
    if !q.x.is_finite() || !q.y.is_finite() || !q.z.is_finite() || !q.w.is_finite() {
        return None;
    }
    let length_squared = q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w;
    if !length_squared.is_finite() || length_squared <= 0.0 {
        return None;
    }
    let length = length_squared.sqrt();
    Some(Quat::new(
        q.x / length,
        q.y / length,
        q.z / length,
        q.w / length,
    ))
}

fn to_vector(v: Vec3) -> rapier3d::math::Vec3 {
    rapier3d::math::Vec3::new(v.x, v.y, v.z)
}

fn from_vector(v: &rapier3d::math::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn to_rotation(q: Quat) -> rapier3d::math::Rot3 {
    let q = normalize_quat(q).unwrap_or(Quat::IDENTITY);
    rapier3d::math::Rot3::from_xyzw(q.x, q.y, q.z, q.w)
}

fn from_rotation(r: &rapier3d::math::Rot3) -> Quat {
    normalize_quat(Quat::new(r.x, r.y, r.z, r.w)).unwrap_or(Quat::IDENTITY)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn assert_vec3_close(actual: Vec3, expected: Vec3, epsilon: f32) {
        assert!(
            (actual.x - expected.x).abs() <= epsilon
                && (actual.y - expected.y).abs() <= epsilon
                && (actual.z - expected.z).abs() <= epsilon,
            "actual={actual:?}, expected={expected:?}"
        );
    }

    fn assert_quat_close(actual: Quat, expected: Quat, epsilon: f32) {
        assert!(
            (actual.x - expected.x).abs() <= epsilon
                && (actual.y - expected.y).abs() <= epsilon
                && (actual.z - expected.z).abs() <= epsilon
                && (actual.w - expected.w).abs() <= epsilon,
            "actual={actual:?}, expected={expected:?}"
        );
    }

    fn falling_ball() -> (PhysicsWorld3D, BodyId) {
        let mut world = PhysicsWorld3D::default();
        let body = world
            .add_body(BodyDesc3D::new(
                BodyType3D::Dynamic,
                Vec3::new(0.0, 4.0, 0.0),
            ))
            .unwrap();
        world
            .add_collider(body, ColliderDesc3D::new(Shape3D::Ball { radius: 0.5 }))
            .unwrap();
        (world, body)
    }

    fn ground(world: &mut PhysicsWorld3D) -> BodyId {
        let body = world
            .add_body(BodyDesc3D::new(
                BodyType3D::Fixed,
                Vec3::new(0.0, -1.0, 0.0),
            ))
            .unwrap();
        world
            .add_collider(
                body,
                ColliderDesc3D::new(Shape3D::Cuboid {
                    half_extents: Vec3::new(10.0, 0.1, 10.0),
                }),
            )
            .unwrap();
        body
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
        let mut world = PhysicsWorld3D::default();
        let body = ground(&mut world);
        for _ in 0..30 {
            world.step(DT).unwrap();
        }
        assert_vec3_close(
            world.body_state(body).unwrap().position,
            Vec3::new(0.0, -1.0, 0.0),
            0.0,
        );
    }

    #[test]
    fn ball_rests_on_cuboid_ground() {
        let mut world = PhysicsWorld3D::default();
        ground(&mut world);
        let ball = world
            .add_body(BodyDesc3D::new(
                BodyType3D::Dynamic,
                Vec3::new(0.0, 3.0, 0.0),
            ))
            .unwrap();
        world
            .add_collider(ball, ColliderDesc3D::new(Shape3D::Ball { radius: 0.5 }))
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
    fn capsule_rests_on_cuboid_ground() {
        let mut world = PhysicsWorld3D::default();
        ground(&mut world);
        let capsule = world
            .add_body(BodyDesc3D::new(
                BodyType3D::Dynamic,
                Vec3::new(0.0, 3.0, 0.0),
            ))
            .unwrap();
        world
            .add_collider(
                capsule,
                ColliderDesc3D::new(Shape3D::Capsule {
                    half_height: 0.5,
                    radius: 0.25,
                }),
            )
            .unwrap();
        for _ in 0..180 {
            world.step(DT).unwrap();
        }
        let state = world.body_state(capsule).unwrap();
        assert!(
            (-0.4..=0.0).contains(&state.position.y),
            "y={}",
            state.position.y
        );
        assert!(state.linvel.y.abs() < 0.1, "vy={}", state.linvel.y);
    }

    #[test]
    fn quaternion_rotation_is_preserved() {
        let mut world = PhysicsWorld3D::new(Vec3::new(0.0, 0.0, 0.0));
        let angle = std::f32::consts::FRAC_PI_2;
        let half = angle * 0.5;
        let expected = Quat::new(0.0, half.sin(), 0.0, half.cos());
        let body = world
            .add_body(
                BodyDesc3D::new(BodyType3D::Dynamic, Vec3::new(0.0, 0.0, 0.0))
                    .with_rotation(Quat::new(0.0, 7.0 * half.sin(), 0.0, 7.0 * half.cos())),
            )
            .unwrap();
        world.step(DT).unwrap();
        assert_quat_close(world.body_state(body).unwrap().rotation, expected, 1e-5);
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
        assert!(world.apply_impulse(body, Vec3::new(10.0, 0.0, 0.0)));
        let after = world.body_state(body).unwrap().linvel;
        assert!(after.x > before.x);
    }

    #[test]
    fn raycast_returns_the_collider_id() {
        let mut world = PhysicsWorld3D::new(Vec3::new(0.0, 0.0, 0.0));
        let body = world
            .add_body(BodyDesc3D::new(BodyType3D::Fixed, Vec3::new(0.0, 0.0, 0.0)))
            .unwrap();
        let collider = world
            .add_collider(body, ColliderDesc3D::new(Shape3D::Ball { radius: 1.0 }))
            .unwrap();
        world.step(DT).unwrap();

        let hit = world
            .cast_ray(Vec3::new(0.0, 5.0, 0.0), Vec3::new(0.0, -1.0, 0.0), 10.0)
            .unwrap();
        assert_eq!(hit.collider, collider);
        assert!((3.9..4.1).contains(&hit.toi), "toi={}", hit.toi);
    }

    #[test]
    fn invalid_descriptions_are_rejected() {
        let mut world = PhysicsWorld3D::default();
        assert!(
            world
                .add_body(BodyDesc3D::new(
                    BodyType3D::Dynamic,
                    Vec3::new(f32::NAN, 0.0, 0.0)
                ))
                .is_none()
        );
        assert!(
            world
                .add_body(
                    BodyDesc3D::new(BodyType3D::Dynamic, Vec3::new(0.0, 0.0, 0.0))
                        .with_rotation(Quat::new(0.0, 0.0, 0.0, 0.0))
                )
                .is_none()
        );
        let body = world
            .add_body(BodyDesc3D::new(
                BodyType3D::Dynamic,
                Vec3::new(0.0, 0.0, 0.0),
            ))
            .unwrap();
        assert!(
            world
                .add_collider(body, ColliderDesc3D::new(Shape3D::Ball { radius: 0.0 }))
                .is_none()
        );
        assert!(
            world
                .add_collider(
                    body,
                    ColliderDesc3D::new(Shape3D::Capsule {
                        half_height: f32::INFINITY,
                        radius: 0.5
                    })
                )
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
