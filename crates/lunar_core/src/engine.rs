//! Estado mínimo del motor y bucle de paso fijo (ADR-005).

use std::time::Duration;

use lunar_ecs::{Schedule, World};

use crate::clock::{Clock, FixedStepClock, Tick};
use crate::input::InputState;

/// Configuración del motor. `title` es Rust-side: la firma FFI se valida en H3 (ADR-004).
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

impl EngineConfig {
    /// Crea una configuración.
    #[must_use]
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            title: title.into(),
            width,
            height,
        }
    }
}

/// Motor mínimo: mundo ECS, planificación de sistemas, input y reloj.
pub struct Engine {
    pub config: EngineConfig,
    pub world: World,
    pub schedule: Schedule,
    input: InputState,
    step: Option<FixedStepClock>,
    pub tick_count: u64,
    pub frame_count: u64,
}

impl Engine {
    /// Crea un motor con la configuración dada.
    #[must_use]
    pub fn new(config: EngineConfig) -> Self {
        Self {
            config,
            world: World::new(),
            schedule: Schedule::new(),
            input: InputState::default(),
            step: None,
            tick_count: 0,
            frame_count: 0,
        }
    }

    /// Procesa un frame usando el reloj inyectado.
    pub fn update<C: Clock>(&mut self, clock: &C) -> Tick {
        let now = clock.elapsed();
        let tick = match &mut self.step {
            Some(step) => step.tick(now),
            None => {
                self.step = Some(FixedStepClock::new(now));
                Tick {
                    substeps: 0,
                    alpha: 0.0,
                    dropped_time: Duration::ZERO,
                }
            }
        };

        for _ in 0..tick.substeps {
            self.schedule.run(&mut self.world);
            self.tick_count += 1;
        }
        self.frame_count += 1;
        self.input.end_frame();
        tick
    }

    /// Estado de entrada mutable para la capa de plataforma.
    pub fn input_mut(&mut self) -> &mut InputState {
        &mut self.input
    }

    /// Estado de entrada de solo lectura.
    #[must_use]
    pub const fn input(&self) -> &InputState {
        &self.input
    }

    /// Reinicia el reloj de paso fijo y los contadores (ADR-028).
    ///
    /// No toca el mundo ECS ni el input: el replay se encarga de reaplicar el
    /// estado de entrada de cada frame desde una escena equivalente.
    pub fn reset_timing(&mut self) {
        self.step = None;
        self.tick_count = 0;
        self.frame_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{FIXED_DT, MAX_SUBSTEPS, ManualClock};
    use lunar_ecs::Query;

    // El derive de bevy necesita el crate presente; es solo dev-dependency (ADR-002).
    #[derive(bevy_ecs::component::Component)]
    struct Counter(u32);

    fn increment(mut counters: Query<&mut Counter>) {
        for mut counter in &mut counters {
            counter.0 += 1;
        }
    }

    #[test]
    fn engine_counts_sixty_ticks_from_injected_clock() {
        let mut engine = Engine::new(EngineConfig::new("test", 800, 600));
        engine.schedule.add_system(increment);
        let entity = engine.world.spawn(Counter(0));
        let mut clock = ManualClock::default();

        // El primer frame solo inicializa el reloj con el tiempo inyectado.
        engine.update(&clock);
        for _ in 0..60 {
            clock.advance(FIXED_DT);
            engine.update(&clock);
        }

        assert_eq!(engine.tick_count, 60);
        assert_eq!(engine.frame_count, 61);
        assert_eq!(engine.world.get::<Counter>(entity).unwrap().0, 60);
    }

    #[test]
    fn engine_caps_long_frames() {
        let mut engine = Engine::new(EngineConfig::new("test", 800, 600));
        let mut clock = ManualClock::default();
        engine.update(&clock);
        clock.advance(Duration::from_secs(2));

        let tick = engine.update(&clock);
        assert_eq!(tick.substeps, MAX_SUBSTEPS);
        assert_eq!(engine.tick_count, u64::from(MAX_SUBSTEPS));
        assert!(!tick.dropped_time.is_zero());
    }

    #[test]
    fn first_frame_initializes_clock_without_ticks() {
        let mut engine = Engine::new(EngineConfig::new("test", 800, 600));
        let mut clock = ManualClock::default();
        clock.advance(Duration::from_secs(10));

        let tick = engine.update(&clock);
        assert_eq!(tick.substeps, 0);
        assert_eq!(engine.tick_count, 0);
    }

    #[test]
    fn reset_timing_clears_counters_and_step() {
        let mut engine = Engine::new(EngineConfig::new("test", 800, 600));
        let mut clock = ManualClock::default();
        engine.update(&clock);
        clock.advance(FIXED_DT);
        engine.update(&clock);
        assert_eq!(engine.tick_count, 1);
        assert_eq!(engine.frame_count, 2);

        engine.reset_timing();
        assert_eq!(engine.tick_count, 0);
        assert_eq!(engine.frame_count, 0);
        let tick = engine.update(&clock);
        assert_eq!(tick.substeps, 0);
        assert_eq!(engine.frame_count, 1);
    }
}
