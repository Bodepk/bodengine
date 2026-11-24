//! Único crate que depende de `winit` (ADR-006).
//!
//! Traduce eventos de ventana a tipos de `lunar_core`; el resto del motor no ve winit.

use lunar_core::Key;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::error::EventLoopError;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// Configuración de la ventana nativa.
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

impl WindowConfig {
    /// Crea una configuración de ventana.
    #[must_use]
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            title: title.into(),
            width,
            height,
        }
    }
}

/// Interfaz que la aplicación implementa para recibir eventos y frames.
pub trait PlatformApp {
    /// Tecla pulsada o liberada.
    fn on_key(&mut self, key: Key, pressed: bool);

    /// Cambio de tamaño. H2 añadirá aquí el render.
    fn on_resize(&mut self, width: u32, height: u32);

    /// Un frame listo para simular/renderizar. Devuelve `false` para salir.
    fn on_frame(&mut self) -> bool;
}

/// Traduce una tecla física de winit a una tecla del motor.
#[must_use]
pub fn map_key(key: PhysicalKey) -> Key {
    let PhysicalKey::Code(code) = key else {
        return Key::Other;
    };
    match code {
        KeyCode::Escape => Key::Escape,
        KeyCode::KeyW => Key::W,
        KeyCode::KeyA => Key::A,
        KeyCode::KeyS => Key::S,
        KeyCode::KeyD => Key::D,
        KeyCode::ArrowUp => Key::ArrowUp,
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        _ => Key::Other,
    }
}

struct WinitHost<'a, A: PlatformApp> {
    config: WindowConfig,
    app: &'a mut A,
    window: Option<Window>,
}

impl<A: PlatformApp> ApplicationHandler for WinitHost<'_, A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(self.config.title.clone())
            .with_inner_size(PhysicalSize::new(self.config.width, self.config.height));
        match event_loop.create_window(attributes) {
            Ok(window) => self.window = Some(window),
            Err(error) => {
                eprintln!("no se pudo crear la ventana: {error}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                let key = map_key(event.physical_key);
                let pressed = matches!(event.state, winit::event::ElementState::Pressed);
                self.app.on_key(key, pressed);
                if pressed && key == Key::Escape {
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(size) => self.app.on_resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                if !self.app.on_frame() {
                    event_loop.exit();
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// Crea la ventana y ejecuta el bucle de eventos hasta que se solicite salir.
pub fn run(config: WindowConfig, app: &mut impl PlatformApp) -> Result<(), EventLoopError> {
    let event_loop = EventLoop::new()?;
    let mut host = WinitHost {
        config,
        app,
        window: None,
    };
    event_loop.run_app(&mut host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_named_keys() {
        assert_eq!(map_key(PhysicalKey::Code(KeyCode::Escape)), Key::Escape);
        assert_eq!(map_key(PhysicalKey::Code(KeyCode::KeyW)), Key::W);
        assert_eq!(
            map_key(PhysicalKey::Code(KeyCode::ArrowLeft)),
            Key::ArrowLeft
        );
    }

    struct DummyApp;

    impl PlatformApp for DummyApp {
        fn on_key(&mut self, _key: Key, _pressed: bool) {}
        fn on_resize(&mut self, _width: u32, _height: u32) {}
        fn on_frame(&mut self) -> bool {
            true
        }
    }

    #[test]
    fn window_config_keeps_values() {
        let config = WindowConfig::new("smoke", 800, 600);
        assert_eq!(config.title, "smoke");
        assert_eq!((config.width, config.height), (800, 600));
        let _ = DummyApp;
    }
}
