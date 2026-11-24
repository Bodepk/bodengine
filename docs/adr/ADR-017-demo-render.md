# ADR-017: Gancho de frame y triángulo de demo

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

La demo de `v0.1.0` debe consumir escenas, glTF, imágenes y audio, pero el bucle de `lunar_platform` solo permitía limpiar la pantalla desde `lunar_render`. Sin un punto de extensión, la lógica de juego no puede actualizar lo que se dibuja cada frame sin reimplementar la plataforma.

## Decisión

- `PlatformApp` gana `on_render(&mut lunar_render::Renderer<'_>)`, con implementación por defecto vacía. Solo existe con la feature `render` activa, así que los builds bare no cambian. La plataforma lo invoca justo antes de presentar el frame.
- `Renderer` gana `set_triangle(vertices, color)` y `clear_triangle()`. El triángulo se dibuja en el mismo render pass que el clear, con un pipeline mínimo WGSL y un buffer de vértices posición+color. No se expone `wgpu` en la API pública.
- El color se pasa como `[f32; 4]` y las posiciones ya están en coordenadas NDC. No hay texturas, cámara ni materiales nuevos: el pipeline existe solo para hacer verificable la demo.
- La demo usa `lunar::render::Renderer` a través del gancho; `minimal_game` sigue funcionando sin implementar el método.

## Consecuencias

- No se añaden dependencias y `lunar_render` sigue siendo el único crate de render.
- La API de `PlatformApp` es retrocompatible por el método por defecto.
- Un futuro renderer real podrá sustituir el triángulo sin tocar la plataforma ni las escenas.
