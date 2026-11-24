//! Smoke del H1: ventana 800x600, contador de ticks y cierre con ESC.

use lunar::core::{Engine, EngineConfig, Key, SystemClock};
use lunar::platform::{PlatformApp, WindowConfig, run};

struct Smoke {
    engine: Engine,
    clock: SystemClock,
}

impl PlatformApp for Smoke {
    fn on_key(&mut self, key: Key, pressed: bool) {
        let input = self.engine.input_mut();
        if pressed {
            input.press(key);
        } else {
            input.release(key);
        }
    }

    fn on_resize(&mut self, width: u32, height: u32) {
        // H2 conectará aquí el render; en H1 basta con no hacer nada (incluido 0x0).
        let _ = (width, height);
    }

    fn on_frame(&mut self) -> bool {
        let tick = self.engine.update(&self.clock);
        if self.engine.frame_count.is_multiple_of(60) {
            println!(
                "frame {} | ticks {} | substeps {} | alpha {:.3}",
                self.engine.frame_count, self.engine.tick_count, tick.substeps, tick.alpha
            );
        }
        true
    }
}

fn main() {
    let window = WindowConfig::new("Lunar H1 smoke", 800, 600);
    let engine = Engine::new(EngineConfig::new("Lunar H1 smoke", 800, 600));
    let mut app = Smoke {
        engine,
        clock: SystemClock::new(),
    };

    if let Err(error) = run(window, &mut app) {
        eprintln!("error en el bucle de eventos: {error}");
        std::process::exit(1);
    }
}
