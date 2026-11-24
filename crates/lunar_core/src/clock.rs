//! Reloj de paso fijo e inyectable (ADR-005).

use std::time::Duration;

/// Duración de un tick de simulación: ~60 Hz.
pub const FIXED_DT: Duration = Duration::from_nanos(16_666_667);

/// Número máximo de substeps por frame. Evita la espiral de la muerte.
pub const MAX_SUBSTEPS: u32 = 5;

/// Fuente de tiempo. Se inyecta para poder testear sin dormir.
pub trait Clock {
    /// Tiempo transcurrido desde un origen arbitrario pero estable.
    fn elapsed(&self) -> Duration;
}

/// Reloj basado en `Instant`, para la aplicación real.
#[derive(Debug, Clone)]
pub struct SystemClock {
    start: std::time::Instant,
}

impl SystemClock {
    /// Crea un reloj que empieza ahora.
    #[must_use]
    pub fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// Reloj manual para tests.
#[derive(Debug, Clone, Default)]
pub struct ManualClock {
    elapsed: Duration,
}

impl ManualClock {
    /// Avanza el reloj manual.
    pub fn advance(&mut self, delta: Duration) {
        self.elapsed += delta;
    }

    /// Fija el tiempo transcurrido.
    pub fn set(&mut self, elapsed: Duration) {
        self.elapsed = elapsed;
    }
}

impl Clock for ManualClock {
    fn elapsed(&self) -> Duration {
        self.elapsed
    }
}

/// Resultado de procesar un frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    /// Substeps de simulación que debe ejecutar el juego.
    pub substeps: u32,
    /// Factor de interpolación para el render, en `[0, 1)`.
    pub alpha: f32,
    /// Tiempo descartado por exceder `MAX_SUBSTEPS`.
    pub dropped_time: Duration,
}

/// Acumulador de paso fijo. No depende de GPU, ventana ni reloj del sistema.
#[derive(Debug, Clone)]
pub struct FixedStepClock {
    last: Duration,
    accumulator: Duration,
}

impl FixedStepClock {
    /// Crea el acumulador con un tiempo inicial.
    #[must_use]
    pub const fn new(now: Duration) -> Self {
        Self {
            last: now,
            accumulator: Duration::ZERO,
        }
    }

    /// Procesa el tiempo `now` y devuelve los substeps pendientes.
    pub fn tick(&mut self, now: Duration) -> Tick {
        let delta = now.checked_sub(self.last).unwrap_or(Duration::ZERO);
        self.last = now;
        self.accumulator += delta;

        let mut substeps = 0;
        while self.accumulator >= FIXED_DT && substeps < MAX_SUBSTEPS {
            self.accumulator -= FIXED_DT;
            substeps += 1;
        }

        let dropped_time = if substeps == MAX_SUBSTEPS && self.accumulator >= FIXED_DT {
            let dropped = self.accumulator;
            self.accumulator = Duration::ZERO;
            dropped
        } else {
            Duration::ZERO
        };

        Tick {
            substeps,
            alpha: self.accumulator.as_secs_f32() / FIXED_DT.as_secs_f32(),
            dropped_time,
        }
    }

    /// Tiempo acumulado pendiente de simular.
    #[must_use]
    pub const fn accumulator(&self) -> Duration {
        self.accumulator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixty_frames_of_fixed_dt_produce_sixty_ticks() {
        let mut clock = FixedStepClock::new(Duration::ZERO);
        let mut manual = ManualClock::default();
        let mut total = 0;

        for _ in 0..60 {
            manual.advance(FIXED_DT);
            total += clock.tick(manual.elapsed()).substeps;
        }

        assert_eq!(total, 60);
    }

    #[test]
    fn fractional_frames_accumulate() {
        let mut clock = FixedStepClock::new(Duration::ZERO);
        let mut manual = ManualClock::default();

        manual.advance(Duration::from_millis(8));
        assert_eq!(clock.tick(manual.elapsed()).substeps, 0);

        manual.advance(Duration::from_millis(8));
        assert_eq!(clock.tick(manual.elapsed()).substeps, 0);

        manual.advance(Duration::from_millis(8));
        let tick = clock.tick(manual.elapsed());
        assert_eq!(tick.substeps, 1);
        assert!(clock.accumulator() < FIXED_DT);
    }

    #[test]
    fn two_second_frame_caps_substeps() {
        let mut clock = FixedStepClock::new(Duration::ZERO);
        let mut manual = ManualClock::default();

        manual.advance(Duration::from_secs(2));
        let tick = clock.tick(manual.elapsed());

        assert_eq!(tick.substeps, MAX_SUBSTEPS);
        assert!(tick.dropped_time > Duration::ZERO);
        assert_eq!(clock.accumulator(), Duration::ZERO);
        assert_eq!(tick.alpha, 0.0);
    }

    #[test]
    fn alpha_stays_in_unit_interval() {
        let mut clock = FixedStepClock::new(Duration::ZERO);
        let mut manual = ManualClock::default();

        for millis in [5, 10, 10, 20, 30] {
            manual.advance(Duration::from_millis(millis));
            let tick = clock.tick(manual.elapsed());
            assert!((0.0..1.0).contains(&tick.alpha));
        }
    }

    #[test]
    fn clock_can_be_reset_with_injected_time() {
        let mut clock = FixedStepClock::new(Duration::from_secs(10));
        let tick = clock.tick(Duration::from_secs(10));
        assert_eq!(tick.substeps, 0);

        let tick = clock.tick(Duration::from_secs(10) + FIXED_DT);
        assert_eq!(tick.substeps, 1);
    }
}
