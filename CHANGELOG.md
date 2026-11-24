# Changelog

Todas las decisiones de compatibilidad están en [`docs/adr`](docs/adr/ADR-000-index.md).

## [0.1.0] — 2026-11-24

Primera versión funcional del motor Lunar: núcleo Rust, API pública C++ mediante `cxx` y hitos H0–H11 completados.

### Añadido

- Workspace con fachada `lunar` y builds parciales `dim2`, `dim3`, bare, `assets`, `gltf`, `audio`, `scenes` y `editor`.
- Bucle de paso fijo, input, ventana `winit` y render `wgpu` con manejo de resize `0×0`.
- Bridge C++ `cxx` y smoke multiplataforma con CMake.
- Física 2D y 3D con `rapier2d`/`rapier3d` 0.36.1.
- Carga de imágenes PNG/JPEG con `image` 0.25.10 y de documentos glTF 2.0 con `gltf` 1.4.1, mediante handles opacos.
- Reproducción WAV con `rodio` 0.23.0 y `AudioPlayer` sin dispositivo en tests.
- Escenas JSON versionadas con `serde` 1.0.229 y `serde_json` 1.0.154: jerarquía, componentes y validación de rutas y volúmenes.
- Editor headless `lunar_editor`: selección efímera, comandos y undo/redo; queda fuera de `default`.
- Normalización de rutas de assets y creación de directorios al guardar escenas.
- Gancho opcional `on_render` y triángulo de demo sin exponer `wgpu` en la API pública (ADR-017).
- Demo jugable `scene_demo` con assets sintéticos: escena JSON, glTF, PNG, WAV y editor headless.
- CI en Linux, Windows y macOS con fmt, clippy, tests, gates de features, `--all-features` y smoke C++.

### Seguridad y compatibilidad

- `gltf` no entra en builds sin la feature `gltf`; `image` solo con `assets`; `rodio`/`cpal` solo con `audio`; `serde_json` solo con `scenes`; el editor solo con `editor`.
- `on_render` solo existe con la feature `render` y tiene implementación por defecto, así que las apps existentes no cambian.
- El esquema de escenas es versión `1` y no admite migraciones silenciosas (ADR-008).

[0.1.0]: https://github.com/Bodepk/bodengine/releases/tag/v0.1.0
