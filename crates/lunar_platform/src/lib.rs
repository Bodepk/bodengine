//! Único crate que depende de `winit` (ADR-006).
//!
//! Traduce eventos de ventana a tipos de `lunar_core` y crea la surface nativa
//! de `wgpu`. El estado GPU vive en `lunar_render` (ADR-009).

use lunar_core::Key;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::error::EventLoopError;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use std::sync::Arc;

#[cfg(feature = "render")]
use lunar_render::{Renderer, Viewport};
#[cfg(feature = "render")]
use wgpu::rwh::{HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle as RawWindowHandle};

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

    /// Cambio de tamaño. Se reenvía incluso si es `0×0`.
    fn on_resize(&mut self, width: u32, height: u32);

    /// Un frame listo para simular/renderizar. Devuelve `false` para salir.
    fn on_frame(&mut self) -> bool;

    /// Actualiza el render justo antes de presentar el frame (ADR-017).
    #[cfg(feature = "render")]
    fn on_render(&mut self, _renderer: &mut lunar_render::Renderer<'_>) {}
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

/// Mantiene viva la ventana mientras `wgpu` usa su handle nativo.
#[cfg(feature = "render")]
struct NativeWindowHandle(Arc<Window>);

#[cfg(feature = "render")]
impl HasWindowHandle for NativeWindowHandle {
    fn window_handle(&self) -> Result<RawWindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

#[cfg(feature = "render")]
impl HasDisplayHandle for NativeWindowHandle {
    fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

/// Crea la surface a partir de la ventana y delega el estado GPU a `lunar_render`.
#[cfg(feature = "render")]
fn create_renderer(
    instance: &wgpu::Instance,
    window: &Arc<Window>,
) -> Result<Renderer<'static>, String> {
    let surface = instance
        .create_surface(NativeWindowHandle(Arc::clone(window)))
        .map_err(|error| error.to_string())?;
    let size = window.inner_size();
    let viewport = Viewport::new(size.width, size.height).ok();
    Renderer::new_blocking(instance, surface, viewport).map_err(|error| error.to_string())
}

/// Reenvía resize a la aplicación sin validar el tamaño: `0×0` es un evento
/// legítimo de minimizado y la capa de render decide omitirlo.
fn forward_resize(app: &mut impl PlatformApp, width: u32, height: u32) {
    app.on_resize(width, height);
}

struct WinitHost<'a, A: PlatformApp> {
    config: WindowConfig,
    app: &'a mut A,
    window: Option<Arc<Window>>,
    #[cfg(feature = "render")]
    renderer: Option<Renderer<'static>>,
}

impl<A: PlatformApp> ApplicationHandler for WinitHost<'_, A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(self.config.title.clone())
            .with_inner_size(PhysicalSize::new(self.config.width, self.config.height));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("no se pudo crear la ventana: {error}");
                event_loop.exit();
                return;
            }
        };

        #[cfg(feature = "render")]
        {
            let instance = wgpu::Instance::default();
            match create_renderer(&instance, &window) {
                Ok(renderer) => self.renderer = Some(renderer),
                Err(error) => eprintln!("no se pudo inicializar el render: {error}"),
            }
        }

        self.window = Some(window);
        if let Some(window) = &self.window {
            window.request_redraw();
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
            WindowEvent::Resized(size) => {
                #[cfg(feature = "render")]
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
                forward_resize(self.app, size.width, size.height);
            }
            WindowEvent::RedrawRequested => {
                if !self.app.on_frame() {
                    event_loop.exit();
                    return;
                }
                #[cfg(feature = "render")]
                if let Some(renderer) = &mut self.renderer {
                    self.app.on_render(renderer);
                    if let Err(error) = renderer.render() {
                        eprintln!("error al presentar el frame: {error}");
                    }
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
        #[cfg(feature = "render")]
        renderer: None,
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

    #[derive(Default)]
    struct RecordingApp {
        resizes: Vec<(u32, u32)>,
    }

    impl PlatformApp for RecordingApp {
        fn on_key(&mut self, _key: Key, _pressed: bool) {}
        fn on_resize(&mut self, width: u32, height: u32) {
            self.resizes.push((width, height));
        }
        fn on_frame(&mut self) -> bool {
            true
        }
    }

    #[test]
    fn window_config_keeps_values() {
        let config = WindowConfig::new("smoke", 800, 600);
        assert_eq!(config.title, "smoke");
        assert_eq!((config.width, config.height), (800, 600));
    }

    #[test]
    fn zero_sized_resize_is_forwarded_without_panic() {
        let mut app = RecordingApp::default();
        forward_resize(&mut app, 0, 0);
        forward_resize(&mut app, 0, 1);
        forward_resize(&mut app, 800, 600);
        assert_eq!(app.resizes, [(0, 0), (0, 1), (800, 600)]);
    }
}
