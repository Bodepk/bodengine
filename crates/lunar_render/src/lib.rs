//! Render sobre `wgpu` (ADR-009).
//!
//! La lógica de juego no depende de este crate. Aquí viven el dispositivo, la
//! surface, la configuración y la validación de tamaños antes de tocar la GPU.

mod viewport;

pub use viewport::{InvalidViewport, Viewport, ViewportState};

use std::borrow::Cow;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use wgpu::{
    Adapter, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, BlendComponent,
    BlendFactor, BlendOperation, BlendState, Buffer, BufferDescriptor, BufferUsages, Color,
    ColorTargetState, ColorWrites, CommandEncoderDescriptor, CurrentSurfaceTexture, Device,
    DeviceDescriptor, Extent3d, FilterMode, FragmentState, Instance, LoadOp, Operations, Origin3d,
    PipelineCompilationOptions, PipelineLayoutDescriptor, PrimitiveState, PrimitiveTopology, Queue,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor,
    RequestAdapterOptions, Sampler, SamplerBindingType, SamplerDescriptor, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, StoreOp, Surface, SurfaceConfiguration, SurfaceTexture,
    TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor,
    TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureViewDescriptor,
    TextureViewDimension, VertexAttribute, VertexBufferLayout, VertexFormat, VertexState,
    VertexStepMode,
};

/// Número de quads que caben en el buffer de sprites inicial.
const SPRITE_INITIAL_QUADS: usize = 16;

/// Tamaño de un vértice de sprite: posición + UV, en bytes.
const SPRITE_VERTEX_STRIDE: u64 = 16;

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

/// Identificador opaco de una hoja de sprites ya subida a la GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpriteId(u64);

impl SpriteId {
    /// Devuelve el valor entero del identificador. No es estable entre renderers.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Fallo al definir o dibujar sprites, sin exponer tipos de `wgpu`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpriteError {
    /// La imagen no tiene dimensiones válidas.
    InvalidDimensions { width: u32, height: u32 },
    /// El tamaño de fotograma no cabe en la imagen.
    InvalidFrameSize,
    /// Los bytes RGBA no coinciden con las dimensiones.
    InvalidRgba { expected: usize, actual: usize },
    /// El identificador no pertenece a este renderer.
    UnknownSprite,
    /// El fotograma solicitado no existe en la hoja.
    FrameOutOfRange { frame: u32, count: u32 },
    /// El centro o el tamaño del quad no son finitos, o el tamaño no es positivo.
    InvalidQuad,
    /// Se agotaron los identificadores de sprites.
    HandleOverflow,
}

impl fmt::Display for SpriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions { width, height } => {
                write!(f, "dimensiones de sprite inválidas: {width}×{height}")
            }
            Self::InvalidFrameSize => f.write_str("el tamaño de fotograma no cabe en la imagen"),
            Self::InvalidRgba { expected, actual } => {
                write!(f, "RGBA esperado: {expected} bytes; recibido: {actual}")
            }
            Self::UnknownSprite => f.write_str("identificador de sprite desconocido"),
            Self::FrameOutOfRange { frame, count } => {
                write!(f, "fotograma {frame} fuera de rango (hoja de {count})")
            }
            Self::InvalidQuad => {
                f.write_str("el quad tiene coordenadas no finitas o tamaño inválido")
            }
            Self::HandleOverflow => f.write_str("se agotaron los identificadores de sprite"),
        }
    }
}

impl Error for SpriteError {}

/// Un quad texturizado en coordenadas NDC, con selección de fotograma.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteDraw {
    /// Hoja de sprites a dibujar.
    pub sprite: SpriteId,
    /// Índice del fotograma, empezando en cero.
    pub frame: u32,
    /// Invierte la dirección horizontal de la textura.
    pub flip_x: bool,
    /// Centro del quad en NDC.
    pub center: [f32; 2],
    /// Tamaño del quad en NDC.
    pub size: [f32; 2],
}

#[derive(Debug)]
struct Sprite {
    /// Se conserva explícitamente mientras el bind group referencia su vista.
    #[allow(dead_code)]
    texture: Texture,
    bind_group: BindGroup,
    texture_width: u32,
    texture_height: u32,
    frame_width: u32,
    frame_height: u32,
    columns: u32,
    rows: u32,
}

impl Sprite {
    fn frame_count(&self) -> u32 {
        self.columns * self.rows
    }
}

#[derive(Debug, Clone, Copy)]
struct SpriteBatch {
    sprite: SpriteId,
    offset: u64,
    size: u64,
}

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
    triangle_pipeline: RenderPipeline,
    triangle_buffer: Buffer,
    has_triangle: bool,
    sprite_pipeline: RenderPipeline,
    sprite_bind_group_layout: BindGroupLayout,
    sprite_sampler: Sampler,
    sprite_vertex_buffer: Buffer,
    sprite_vertex_capacity: usize,
    sprites: HashMap<SpriteId, Sprite>,
    sprite_draws: Vec<SpriteDraw>,
    next_sprite: u64,
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
        let (triangle_pipeline, triangle_buffer) =
            create_triangle_resources(&device, config.format);
        let (sprite_pipeline, sprite_bind_group_layout, sprite_sampler, sprite_vertex_buffer) =
            create_sprite_resources(&device, config.format);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            viewport: ViewportState::new(viewport),
            clear_color: DEFAULT_CLEAR_COLOR,
            triangle_pipeline,
            triangle_buffer,
            has_triangle: false,
            sprite_pipeline,
            sprite_bind_group_layout,
            sprite_sampler,
            sprite_vertex_buffer,
            sprite_vertex_capacity: (SPRITE_INITIAL_QUADS * 6) * SPRITE_VERTEX_STRIDE as usize,
            sprites: HashMap::new(),
            sprite_draws: Vec::new(),
            next_sprite: 0,
            needs_reconfigure: false,
        })
    }

    /// Cambia el color de limpieza.
    pub fn set_clear_color(&mut self, clear_color: Color) {
        self.clear_color = clear_color;
    }

    /// Define el triángulo de demo en coordenadas NDC, con color RGBA.
    ///
    /// Se usa para hacer verificable el gancho de frame de ADR-017; no pretende
    /// ser un pipeline de render completo.
    pub fn set_triangle(&mut self, vertices: [[f32; 2]; 3], color: [f32; 4]) {
        let mut bytes = Vec::with_capacity(3 * 6 * std::mem::size_of::<f32>());
        for vertex in vertices {
            for component in vertex {
                bytes.extend_from_slice(&component.to_le_bytes());
            }
            for component in color {
                bytes.extend_from_slice(&component.to_le_bytes());
            }
        }
        self.queue.write_buffer(&self.triangle_buffer, 0, &bytes);
        self.has_triangle = true;
    }

    /// Oculta el triángulo de demo.
    pub fn clear_triangle(&mut self) {
        self.has_triangle = false;
    }

    /// Sube una hoja de sprites RGBA8 y devuelve su identificador opaco.
    ///
    /// `frame_size` es el tamaño de cada fotograma en píxeles. Para imágenes de
    /// un solo fotograma, usa las dimensiones completas.
    pub fn define_sprite(
        &mut self,
        rgba: &[u8],
        width: u32,
        height: u32,
        frame_size: [u32; 2],
    ) -> Result<SpriteId, SpriteError> {
        if width == 0 || height == 0 {
            return Err(SpriteError::InvalidDimensions { width, height });
        }
        let [frame_width, frame_height] = frame_size;
        if frame_width == 0 || frame_height == 0 || frame_width > width || frame_height > height {
            return Err(SpriteError::InvalidFrameSize);
        }
        let Some(pixels) = width.checked_mul(height) else {
            return Err(SpriteError::InvalidDimensions { width, height });
        };
        let Some(expected) = pixels.checked_mul(4) else {
            return Err(SpriteError::InvalidDimensions { width, height });
        };
        let expected = expected as usize;
        if rgba.len() != expected {
            return Err(SpriteError::InvalidRgba {
                expected,
                actual: rgba.len(),
            });
        }

        let texture = self.device.create_texture(&TextureDescriptor {
            label: Some("lunar sprite texture"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            rgba,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("lunar sprite bind group"),
            layout: &self.sprite_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&self.sprite_sampler),
                },
            ],
        });

        let id = SpriteId(
            self.next_sprite
                .checked_add(1)
                .ok_or(SpriteError::HandleOverflow)?,
        );
        self.next_sprite += 1;
        self.sprites.insert(
            id,
            Sprite {
                texture,
                bind_group,
                texture_width: width,
                texture_height: height,
                frame_width,
                frame_height,
                columns: width / frame_width,
                rows: height / frame_height,
            },
        );
        Ok(id)
    }

    /// Sustituye la lista de sprites del próximo frame.
    pub fn set_sprites(&mut self, draws: &[SpriteDraw]) -> Result<(), SpriteError> {
        for draw in draws {
            let sprite = self
                .sprites
                .get(&draw.sprite)
                .ok_or(SpriteError::UnknownSprite)?;
            let count = sprite.frame_count();
            if draw.frame >= count {
                return Err(SpriteError::FrameOutOfRange {
                    frame: draw.frame,
                    count,
                });
            }
            if !draw.center.iter().all(|value| value.is_finite())
                || !draw.size.iter().all(|value| value.is_finite())
                || draw.size[0] <= 0.0
                || draw.size[1] <= 0.0
            {
                return Err(SpriteError::InvalidQuad);
            }
        }
        self.sprite_draws = draws.to_vec();
        Ok(())
    }

    /// Oculta todos los sprites del próximo frame.
    pub fn clear_sprites(&mut self) {
        self.sprite_draws.clear();
    }

    fn sprite_batches_and_bytes(&self) -> (Vec<u8>, Vec<SpriteBatch>) {
        let mut groups: Vec<(SpriteId, Vec<&SpriteDraw>)> = Vec::new();
        for draw in &self.sprite_draws {
            if let Some((_, draws)) = groups.iter_mut().find(|(id, _)| *id == draw.sprite) {
                draws.push(draw);
            } else {
                groups.push((draw.sprite, vec![draw]));
            }
        }

        let mut bytes = Vec::new();
        let mut batches = Vec::with_capacity(groups.len());
        for (sprite_id, draws) in groups {
            let Some(sprite) = self.sprites.get(&sprite_id) else {
                continue;
            };
            let offset = bytes.len() as u64;
            for draw in draws {
                let column = draw.frame % sprite.columns;
                let row = draw.frame / sprite.columns;
                let mut u0 = (column * sprite.frame_width) as f32 / sprite.texture_width as f32;
                let mut u1 =
                    ((column + 1) * sprite.frame_width) as f32 / sprite.texture_width as f32;
                let v0 = (row * sprite.frame_height) as f32 / sprite.texture_height as f32;
                let v1 = ((row + 1) * sprite.frame_height) as f32 / sprite.texture_height as f32;
                if draw.flip_x {
                    std::mem::swap(&mut u0, &mut u1);
                }

                let half_width = draw.size[0] * 0.5;
                let half_height = draw.size[1] * 0.5;
                let [cx, cy] = draw.center;
                push_sprite_vertex(&mut bytes, [cx - half_width, cy + half_height], [u0, v0]);
                push_sprite_vertex(&mut bytes, [cx - half_width, cy - half_height], [u0, v1]);
                push_sprite_vertex(&mut bytes, [cx + half_width, cy + half_height], [u1, v0]);
                push_sprite_vertex(&mut bytes, [cx + half_width, cy + half_height], [u1, v0]);
                push_sprite_vertex(&mut bytes, [cx - half_width, cy - half_height], [u0, v1]);
                push_sprite_vertex(&mut bytes, [cx + half_width, cy - half_height], [u1, v1]);
            }
            let size = bytes.len() as u64 - offset;
            batches.push(SpriteBatch {
                sprite: sprite_id,
                offset,
                size,
            });
        }
        (bytes, batches)
    }

    fn ensure_sprite_buffer(&mut self, bytes: &[u8]) {
        let needed = bytes.len();
        if needed > self.sprite_vertex_capacity {
            let minimum = SPRITE_INITIAL_QUADS * 6 * SPRITE_VERTEX_STRIDE as usize;
            let capacity = needed.next_power_of_two().max(minimum);
            self.sprite_vertex_buffer = self.device.create_buffer(&BufferDescriptor {
                label: Some("lunar sprite vertices"),
                size: capacity as u64,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.sprite_vertex_capacity = capacity;
        }
        self.queue
            .write_buffer(&self.sprite_vertex_buffer, 0, bytes);
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
        let (sprite_bytes, sprite_batches) = self.sprite_batches_and_bytes();
        if !sprite_bytes.is_empty() {
            self.ensure_sprite_buffer(&sprite_bytes);
        }

        let view = texture
            .texture
            .create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("lunar frame"),
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
            if self.has_triangle {
                pass.set_pipeline(&self.triangle_pipeline);
                pass.set_vertex_buffer(0, self.triangle_buffer.slice(..));
                pass.draw(0..3, 0..1);
            }
            if !sprite_batches.is_empty() {
                pass.set_pipeline(&self.sprite_pipeline);
                for batch in &sprite_batches {
                    let bind_group = &self.sprites[&batch.sprite].bind_group;
                    pass.set_bind_group(0, bind_group, &[]);
                    pass.set_vertex_buffer(
                        0,
                        self.sprite_vertex_buffer
                            .slice(batch.offset..batch.offset + batch.size),
                    );
                    let vertices = (batch.size / SPRITE_VERTEX_STRIDE) as u32;
                    pass.draw(0..vertices, 0..1);
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(texture);
        Ok(())
    }
}

fn create_triangle_resources(device: &Device, format: TextureFormat) -> (RenderPipeline, Buffer) {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("lunar triangle shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(include_str!("triangle.wgsl"))),
    });
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("lunar triangle layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let attributes = [
        VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        VertexAttribute {
            format: VertexFormat::Float32x4,
            offset: 8,
            shader_location: 1,
        },
    ];
    let buffers = [Some(VertexBufferLayout {
        array_stride: 24,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("lunar triangle pipeline"),
        layout: Some(&layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &buffers,
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            ..PrimitiveState::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(ColorTargetState {
                format,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("lunar triangle vertices"),
        size: 72,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (pipeline, buffer)
}

fn create_sprite_resources(
    device: &Device,
    format: TextureFormat,
) -> (RenderPipeline, BindGroupLayout, Sampler, Buffer) {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("lunar sprite shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(include_str!("sprite.wgsl"))),
    });
    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("lunar sprite bind group layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("lunar sprite pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let attributes = [
        VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 8,
            shader_location: 1,
        },
    ];
    let buffers = [Some(VertexBufferLayout {
        array_stride: SPRITE_VERTEX_STRIDE,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("lunar sprite pipeline"),
        layout: Some(&layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &buffers,
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            ..PrimitiveState::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(ColorTargetState {
                format,
                blend: Some(BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::SrcAlpha,
                        dst_factor: BlendFactor::OneMinusSrcAlpha,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent {
                        src_factor: BlendFactor::One,
                        dst_factor: BlendFactor::OneMinusSrcAlpha,
                        operation: BlendOperation::Add,
                    },
                }),
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("lunar sprite sampler"),
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        ..Default::default()
    });
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("lunar sprite vertices"),
        size: (SPRITE_INITIAL_QUADS * 6) as u64 * SPRITE_VERTEX_STRIDE,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (pipeline, bind_group_layout, sampler, buffer)
}

fn push_sprite_vertex(bytes: &mut Vec<u8>, position: [f32; 2], uv: [f32; 2]) {
    for component in position {
        bytes.extend_from_slice(&component.to_le_bytes());
    }
    for component in uv {
        bytes.extend_from_slice(&component.to_le_bytes());
    }
}
