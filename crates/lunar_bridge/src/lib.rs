//! Frontera `cxx` hacia la API pública C++ (ADR-004).
//!
//! `EngineConfig` es un struct compartido: `cxx` traduce su campo `String` a
//! `rust::String`, que sí puede cruzar la frontera. `Engine` es opaco y se
//! entrega a C++ como `rust::Box<Engine>`.

use lunar_core::{Engine as CoreEngine, EngineConfig as CoreEngineConfig, ManualClock};

/// Motor opaco expuesto a C++.
pub struct Engine {
    inner: CoreEngine,
    clock: ManualClock,
}

impl Engine {
    fn title(&self) -> String {
        self.inner.config.title.clone()
    }

    fn width(&self) -> u32 {
        self.inner.config.width
    }

    fn height(&self) -> u32 {
        self.inner.config.height
    }

    fn tick_count(&self) -> u64 {
        self.inner.tick_count
    }

    fn frame_count(&self) -> u64 {
        self.inner.frame_count
    }

    /// Avanza el reloj inyectado y devuelve el resultado del frame.
    fn tick(&mut self, elapsed_ns: u64) -> ffi::FrameStats {
        self.clock
            .advance(std::time::Duration::from_nanos(elapsed_ns));
        let tick = self.inner.update(&self.clock);
        ffi::FrameStats {
            substeps: tick.substeps,
            alpha: tick.alpha,
            dropped_ns: tick.dropped_time.as_nanos() as u64,
            frame_count: self.inner.frame_count,
            tick_count: self.inner.tick_count,
        }
    }
}

/// Crea el motor opaco desde una configuración compartida.
fn new_engine(config: ffi::EngineConfig) -> Box<Engine> {
    let config = CoreEngineConfig::new(config.title, config.width, config.height);
    Box::new(Engine {
        inner: CoreEngine::new(config),
        clock: ManualClock::default(),
    })
}

#[cxx::bridge]
mod ffi {
    /// Configuración compartida con C++.
    struct EngineConfig {
        title: String,
        width: u32,
        height: u32,
    }

    /// Resultado de un frame, sin `Duration` en la ABI.
    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct FrameStats {
        substeps: u32,
        alpha: f32,
        dropped_ns: u64,
        frame_count: u64,
        tick_count: u64,
    }

    extern "Rust" {
        type Engine;

        fn new_engine(config: EngineConfig) -> Box<Engine>;
        fn title(self: &Engine) -> String;
        fn width(self: &Engine) -> u32;
        fn height(self: &Engine) -> u32;
        fn tick_count(self: &Engine) -> u64;
        fn frame_count(self: &Engine) -> u64;
        fn tick(self: &mut Engine, elapsed_ns: u64) -> FrameStats;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ffi::EngineConfig {
        ffi::EngineConfig {
            title: "smoke".to_owned(),
            width: 800,
            height: 600,
        }
    }

    #[test]
    fn shared_config_reaches_opaque_engine() {
        let engine = new_engine(config());
        assert_eq!(engine.title(), "smoke");
        assert_eq!((engine.width(), engine.height()), (800, 600));
    }

    #[test]
    fn tick_uses_injected_time_instead_of_system_clock() {
        let mut engine = new_engine(config());
        let fixed_dt = lunar_core::FIXED_DT.as_nanos() as u64;
        let mut final_stats = ffi::FrameStats::default();

        // El primer frame solo ancla el reloj, como en `lunar_core::Engine`.
        for _ in 0..61 {
            final_stats = engine.tick(fixed_dt);
        }

        assert_eq!(final_stats.substeps, 1);
        assert_eq!(final_stats.tick_count, 60);
        assert_eq!(final_stats.frame_count, 61);
        assert_eq!(engine.tick_count(), 60);
    }
}
