# ADR-009: Render `wgpu` y tamaños de surface degenerados

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

H2 debe añadir render real con `wgpu` y sobrevivir a ventanas minimizadas o redimensionadas a `0×0`. `wgpu::Surface::configure` exige `width` y `height` mayores que cero: configurar con cero panics. Además, `winit` 0.30 solo puede crear la surface a partir del `Window` nativo, y `wgpu` 30 expone esa frontera mediante `raw-window-handle` 0.6.

## Decisión

- Fijar `wgpu` = 30.0.2 (MSRV 1.87.0, compatible con la toolchain 1.100.0).
- `lunar_render` posee el estado GPU de presentación (`Surface`, `Device`, `Queue`, configuración y clear). `lunar_platform` crea el `Instance` y lo usa para sondear los futures durante el arranque; la lógica de juego sigue sin depender de GPU.
- `lunar_platform` mantiene el confinamiento de `winit` (ADR-006) y es el único lugar que convierte `winit::Window` en una surface de `wgpu`, mediante un envoltorio `Arc<Window>` que delega `HasWindowHandle`/`HasDisplayHandle`. La surface resultante es `'static`: el envoltorio mantiene viva la ventana.
- `Viewport` valida tamaños antes de tocar la GPU: rechaza `0`, `u32::MAX` y cualquier dimensión mayor que `i32::MAX`. `ViewportState` guarda el último tamaño renderizable y devuelve `None` mientras la ventana esté degenerada.
- Un frame con `ViewportState` sin tamaño válido se omite sin configurar la surface ni presentar texturas. Al restaurar un tamaño válido se reconfigura la surface.
- `CurrentSurfaceTexture::Occluded`, `Timeout`, `Lost` y `Validation` se tratan como omisión/error controlado, nunca como panic. `Outdated` provoca una reconfiguración y un único reintento.
- Para no añadir un crate de runtime async fuera del stack, `lunar_render::block_on` sondea el futuro de `wgpu` con `Waker::noop()`, `Instance::poll_all(false)` e `yield_now()`. Es código de arranque, no una dependencia nueva.

## Consecuencias

- Los tests de validación de viewport y de transiciones minimizado/restaurado son puros y corren sin GPU.
- El smoke H2 abre una ventana 800×600, limpia a un color y sigue cerrando con ESC; el resize a `0×0` y la restauración no panics.
- La dependencia `wgpu` en `lunar_platform` está detrás de la feature `render`; `--no-default-features` mantiene la plataforma sin render.
- ADR-004 permanece pendiente: esta decisión no toca la firma C++ ni la representación FFI de `EngineConfig`.
