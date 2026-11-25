//! Único crate que depende de `winit` (ADR-006).
//!
//! Traduce eventos de ventana a tipos de `lunar_core` y crea la surface nativa
//! de `wgpu`. El estado GPU vive en `lunar_render` (ADR-009).

use lunar_core::Key;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::error::EventLoopError;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
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

    /// Botón de ratón pulsado o liberado; el código sigue a `MouseButtonCode` (ADR-023).
    fn on_mouse_button(&mut self, _button: u32, _pressed: bool) {}

    /// Cursor en píxeles físicos relativos a la esquina superior izquierda.
    fn on_cursor(&mut self, _x: f64, _y: f64) {}

    /// Scroll acumulado del evento. `pixels` distingue `PixelDelta` de `LineDelta`.
    fn on_scroll(&mut self, _x: f32, _y: f32, _pixels: bool) {}

    /// Actualiza el render justo antes de presentar el frame (ADR-017).
    #[cfg(feature = "render")]
    fn on_render(&mut self, _renderer: &mut lunar_render::Renderer<'_>) {}
}

/// Traduce un botón de ratón a un código POD compartido con C++ (ADR-023).
#[must_use]
pub fn map_mouse_button(button: MouseButton) -> u32 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 3,
        MouseButton::Back => 4,
        MouseButton::Forward => 5,
        MouseButton::Other(_) => 0,
    }
}

/// Normaliza una rueda de ratón a dos componentes finitas y una unidad.
fn map_scroll(delta: MouseScrollDelta) -> Option<(f32, f32, bool)> {
    let (x, y, pixels) = match delta {
        MouseScrollDelta::LineDelta(x, y) => (x, y, false),
        MouseScrollDelta::PixelDelta(position) => (position.x as f32, position.y as f32, true),
    };
    (x.is_finite() && y.is_finite()).then_some((x, y, pixels))
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

fn forward_mouse_button(app: &mut impl PlatformApp, button: MouseButton, pressed: bool) {
    app.on_mouse_button(map_mouse_button(button), pressed);
}

fn forward_cursor(app: &mut impl PlatformApp, x: f64, y: f64) {
    app.on_cursor(x, y);
}

fn forward_scroll(app: &mut impl PlatformApp, delta: MouseScrollDelta) {
    if let Some((x, y, pixels)) = map_scroll(delta) {
        app.on_scroll(x, y, pixels);
    }
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
            WindowEvent::CursorMoved { position, .. } => {
                forward_cursor(self.app, position.x, position.y);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                forward_mouse_button(self.app, button, matches!(state, ElementState::Pressed));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                forward_scroll(self.app, delta);
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

    #[test]
    fn maps_mouse_buttons_and_scroll_deltas() {
        assert_eq!(map_mouse_button(MouseButton::Left), 1);
        assert_eq!(map_mouse_button(MouseButton::Right), 2);
        assert_eq!(map_mouse_button(MouseButton::Middle), 3);
        assert_eq!(map_mouse_button(MouseButton::Back), 4);
        assert_eq!(map_mouse_button(MouseButton::Forward), 5);
        assert_eq!(map_mouse_button(MouseButton::Other(7)), 0);
        assert_eq!(
            map_scroll(MouseScrollDelta::LineDelta(1.0, -2.0)),
            Some((1.0, -2.0, false))
        );
        assert_eq!(
            map_scroll(MouseScrollDelta::PixelDelta(
                winit::dpi::PhysicalPosition::new(4.0, 5.0)
            )),
            Some((4.0, 5.0, true))
        );
        assert_eq!(map_scroll(MouseScrollDelta::LineDelta(f32::NAN, 1.0)), None);
    }

    #[derive(Default)]
    struct RecordingApp {
        resizes: Vec<(u32, u32)>,
        buttons: Vec<(u32, bool)>,
        cursor: Vec<(f64, f64)>,
        scroll: Vec<(f32, f32, bool)>,
    }

    impl PlatformApp for RecordingApp {
        fn on_key(&mut self, _key: Key, _pressed: bool) {}
        fn on_resize(&mut self, width: u32, height: u32) {
            self.resizes.push((width, height));
        }
        fn on_mouse_button(&mut self, button: u32, pressed: bool) {
            self.buttons.push((button, pressed));
        }
        fn on_cursor(&mut self, x: f64, y: f64) {
            self.cursor.push((x, y));
        }
        fn on_scroll(&mut self, x: f32, y: f32, pixels: bool) {
            self.scroll.push((x, y, pixels));
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

    #[test]
    fn mouse_events_are_forwarded_as_pod_values() {
        let mut app = RecordingApp::default();
        forward_mouse_button(&mut app, MouseButton::Left, true);
        forward_cursor(&mut app, 120.0, 240.0);
        forward_scroll(&mut app, MouseScrollDelta::LineDelta(0.0, 3.0));
        forward_scroll(
            &mut app,
            MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(8.0, 9.0)),
        );

        assert_eq!(app.buttons, [(1, true)]);
        assert_eq!(app.cursor, [(120.0, 240.0)]);
        assert_eq!(app.scroll, [(0.0, 3.0, false), (8.0, 9.0, true)]);
    }
}
