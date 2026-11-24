//! Fachada sobre `bevy_ecs`: expone una API pequeña y estable al resto del motor.
//!
//! Los tipos de esta capa no mencionan ventana, GPU ni física, así que la lógica
//! del juego se puede testear sin dispositivos externos (ADR-002).

use bevy_ecs::{
    component::Mutable, prelude::Bundle, prelude::IntoScheduleConfigs, system::ScheduleSystem,
};

pub use bevy_ecs::prelude::{Component, Mut, Query, Res, ResMut};

use bevy_ecs::{schedule::Schedule as BevySchedule, world::World as BevyWorld};

/// Identificador opaco de entidad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId(bevy_ecs::entity::Entity);

/// Mundo ECS.
pub struct World(BevyWorld);

impl World {
    /// Crea un mundo vacío.
    #[must_use]
    pub fn new() -> Self {
        Self(BevyWorld::new())
    }

    /// Crea una entidad con un componente y devuelve su identificador.
    pub fn spawn<B: Bundle>(&mut self, bundle: B) -> EntityId {
        EntityId(self.0.spawn(bundle).id())
    }

    /// Devuelve un componente de una entidad.
    #[must_use]
    pub fn get<T: Component>(&self, id: EntityId) -> Option<&T> {
        self.0.get(id.0)
    }

    /// Devuelve un componente mutable de una entidad.
    pub fn get_mut<T: Component<Mutability = Mutable>>(
        &mut self,
        id: EntityId,
    ) -> Option<Mut<'_, T>> {
        self.0.get_mut(id.0)
    }

    /// Elimina una entidad. Devuelve `false` si ya no existía.
    pub fn despawn(&mut self, id: EntityId) -> bool {
        self.0.despawn(id.0)
    }

    /// Comprueba si una entidad sigue viva.
    #[must_use]
    pub fn is_alive(&self, id: EntityId) -> bool {
        self.0.get_entity(id.0).is_ok()
    }

    /// Cuenta las entidades que tienen un componente.
    #[must_use]
    pub fn count<T: Component>(&mut self) -> usize {
        let mut query = self.0.query::<&T>();
        query.iter(&self.0).count()
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

/// Planificación de sistemas del motor.
pub struct Schedule(BevySchedule);

impl Schedule {
    /// Crea una planificación vacía.
    #[must_use]
    pub fn new() -> Self {
        Self(BevySchedule::default())
    }

    /// Añade un sistema sin parámetros de entrada.
    pub fn add_system<M>(
        &mut self,
        system: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        self.0.add_systems(system);
        self
    }

    /// Ejecuta todos los sistemas sobre el mundo.
    pub fn run(&mut self, world: &mut World) {
        self.0.run(&mut world.0);
    }
}

impl Default for Schedule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Debug, PartialEq)]
    struct Position {
        x: f32,
    }

    #[derive(Component, Debug, PartialEq)]
    struct Velocity {
        dx: f32,
    }

    fn move_positions(mut positions: Query<(&mut Position, &Velocity)>) {
        for (mut position, velocity) in &mut positions {
            position.x += velocity.dx;
        }
    }

    #[test]
    fn spawn_get_and_despawn() {
        let mut world = World::new();
        let id = world.spawn(Position { x: 3.0 });

        assert!(world.is_alive(id));
        assert_eq!(world.get::<Position>(id), Some(&Position { x: 3.0 }));
        assert_eq!(world.count::<Position>(), 1);

        world.get_mut::<Position>(id).unwrap().x = 7.0;
        assert_eq!(world.get::<Position>(id).unwrap().x, 7.0);

        assert!(world.despawn(id));
        assert!(!world.is_alive(id));
        assert!(!world.despawn(id));
        assert_eq!(world.count::<Position>(), 0);
    }

    #[test]
    fn schedule_runs_systems() {
        let mut world = World::new();
        let still = world.spawn(Position { x: 1.0 });
        let moving = world.spawn((Position { x: 10.0 }, Velocity { dx: 2.0 }));

        let mut schedule = Schedule::new();
        schedule.add_system(move_positions);
        schedule.run(&mut world);

        assert_eq!(world.get::<Position>(still).unwrap().x, 1.0);
        assert_eq!(world.get::<Position>(moving).unwrap().x, 12.0);
    }
}
