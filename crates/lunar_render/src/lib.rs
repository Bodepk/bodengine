//! Render sobre `wgpu` (ADR-009).
//!
//! La lógica de juego no depende de este crate. Aquí viven el dispositivo, la
//! surface, la configuración y la validación de tamaños antes de tocar la GPU.

mod viewport;

pub use viewport::{InvalidViewport, Viewport, ViewportState};

use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use wgpu::{
    Adapter, Color, CommandEncoderDescriptor, CurrentSurfaceTexture, Device, DeviceDescriptor,
    Instance, LoadOp, Operations, Queue, RenderPassColorAttachment, RenderPassDescriptor,
    RequestAdapterOptions, StoreOp, Surface, SurfaceConfiguration, SurfaceTexture,
    TextureViewDescriptor,
};

/// Color con el que se limpia el frame del smoke H2.
pub const DEFAULT_CLEAR_COLOR: Color = Color {
    r: 0.05,
    g: 0.08,
    b: 0.14,
    a: 1.0,
};

/// Resultado de intentar presentar un frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderOutcome {
    /// Se limpió y presentó una textura.
    Presented,
    /// No había un tamaño válido o la surface estaba oculta; no se hizo nada.
    Skipped,
}

/// Fallo controlado durante el arranque o la presentación.
#[derive(Debug)]
pub enum RenderError {
    /// No hay adapter compatible con la surface.
    Adapter,
    /// `wgpu` no pudo crear el dispositivo.
    Device(String),
    /// La surface no ofrece una configuración usable.
    SurfaceConfig,
    /// La surface se perdió y debe recrearse.
    SurfaceLost,
    /// La surface devolvió un error de validación.
    SurfaceValidation,
    /// La surface sigue desactualizada tras reconfigurar una vez.
    SurfaceOutdated,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Adapter => f.write_str("no hay adapter de wgpu compatible con la surface"),
            Self::Device(message) => write!(f, "no se pudo crear el dispositivo: {message}"),
            Self::SurfaceConfig => f.write_str("la surface no ofrece una configuración válida"),
            Self::SurfaceLost => f.write_str("la surface se perdió; hay que recrearla"),
            Self::SurfaceValidation => {
                f.write_str("wgpu reportó un error de validación en la surface")
            }
            Self::SurfaceOutdated => {
                f.write_str("la surface sigue desactualizada tras reconfigurar")
            }
        }
    }
}

impl Error for RenderError {}

/// Espera un futuro de `wgpu` sin añadir un runtime async al stack (ADR-009).
pub fn block_on<F: Future>(instance: &Instance, future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = pin!(future);

    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => {
                instance.poll_all(false);
                std::thread::yield_now();
            }
        }
    }
}

/// Estado GPU de la ventana.
pub struct Renderer<'surface> {
    surface: Surface<'surface>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    viewport: ViewportState,
    clear_color: Color,
    needs_reconfigure: bool,
}

impl<'surface> Renderer<'surface> {
    /// Inicializa adapter, dispositivo y surface.
    ///
    /// `viewport == None` es válido: la ventana puede nacer minimizada. En ese
    /// caso se configura un tamaño 1×1 y los frames se omiten hasta que llegue
    /// un tamaño válido.
    pub fn new_blocking(
        instance: &Instance,
        surface: Surface<'surface>,
        viewport: Option<Viewport>,
    ) -> Result<Self, RenderError> {
        let adapter: Adapter = block_on(
            instance,
            instance.request_adapter(&RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..RequestAdapterOptions::default()
            }),
        )
        .map_err(|_| RenderError::Adapter)?;

        let (device, queue): (Device, Queue) = block_on(
            instance,
            adapter.request_device(&DeviceDescriptor::default()),
        )
        .map_err(|error| RenderError::Device(error.to_string()))?;

        let initial = viewport.unwrap_or(Viewport {
            width: 1,
            height: 1,
        });
        let config = surface
            .get_default_config(&adapter, initial.width, initial.height)
            .ok_or(RenderError::SurfaceConfig)?;
        surface.configure(&device, &config);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            viewport: ViewportState::new(viewport),
            clear_color: DEFAULT_CLEAR_COLOR,
            needs_reconfigure: false,
        })
    }

    /// Cambia el color de limpieza.
    pub fn set_clear_color(&mut self, clear_color: Color) {
        self.clear_color = clear_color;
    }

    /// Devuelve el tamaño actualmente renderizable.
    #[must_use]
    pub const fn viewport(&self) -> Option<Viewport> {
        self.viewport.current()
    }

    /// Aplica un evento de resize. Los tamaños degenerados omiten el frame.
    pub fn resize(&mut self, width: u32, height: u32) {
        if let Some(viewport) = self.viewport.update(width, height) {
            self.config.width = viewport.width;
            self.config.height = viewport.height;
            self.surface.configure(&self.device, &self.config);
            self.needs_reconfigure = false;
        }
    }

    /// Limpia y presenta un frame, o lo omite sin panic.
    pub fn render(&mut self) -> Result<RenderOutcome, RenderError> {
        if self.viewport.current().is_none() {
            return Ok(RenderOutcome::Skipped);
        }
        if self.needs_reconfigure {
            self.reconfigure();
            self.needs_reconfigure = false;
        }

        let texture = match self.acquire()? {
            Some(texture) => texture,
            None => return Ok(RenderOutcome::Skipped),
        };
        self.present(texture)?;
        Ok(RenderOutcome::Presented)
    }

    fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    fn acquire(&mut self) -> Result<Option<SurfaceTexture>, RenderError> {
        match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(texture) => Ok(Some(texture)),
            CurrentSurfaceTexture::Suboptimal(texture) => {
                self.needs_reconfigure = true;
                Ok(Some(texture))
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => Ok(None),
            CurrentSurfaceTexture::Outdated => {
                self.reconfigure();
                match self.surface.get_current_texture() {
                    CurrentSurfaceTexture::Success(texture)
                    | CurrentSurfaceTexture::Suboptimal(texture) => Ok(Some(texture)),
                    CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => Ok(None),
                    CurrentSurfaceTexture::Outdated => Err(RenderError::SurfaceOutdated),
                    CurrentSurfaceTexture::Lost => Err(RenderError::SurfaceLost),
                    CurrentSurfaceTexture::Validation => Err(RenderError::SurfaceValidation),
                }
            }
            CurrentSurfaceTexture::Lost => Err(RenderError::SurfaceLost),
            CurrentSurfaceTexture::Validation => Err(RenderError::SurfaceValidation),
        }
    }

    fn present(&mut self, texture: SurfaceTexture) -> Result<(), RenderError> {
        let view = texture
            .texture
            .create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());
        {
            encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("lunar clear"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(self.clear_color),
                        store: StoreOp::Store,
                    },
                })],
                ..RenderPassDescriptor::default()
            });
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(texture);
        Ok(())
    }
}
