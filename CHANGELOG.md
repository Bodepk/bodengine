# Changelog

Todas las decisiones de compatibilidad están en [`docs/adr`](docs/adr/ADR-000-index.md).

## [No publicado]

### Añadido

- Editor gráfico opcional `lunar_editor_ui` con `egui`/`eframe` 0.37: jerarquía, inspector de nombre/transformación/padre/componentes, selección, undo/redo, abrir/guardar JSON y atajos de teclado (ADR-019).
- Feature raíz `editor_ui`, que implica `editor` y queda fuera de `default`.
- Pipeline de sprites en `lunar_render`: `SpriteId`, `SpriteDraw` y `SpriteError`, con hojas de fotogramas, blending alfa y sampler `Nearest` (ADR-018).
- Ejemplo `sprite_platformer`: plataformas 2D con física Rapier, animación del jugador, cámara, manzanas y meta, usando `platformer.scene.json`.
- Sprites CC0 de Pixel Adventure 1 copiados al ejemplo con nombres descriptivos y procedencia en `assets/ASSETS.md`.
- Editor visual en `lunar_editor_ui` (ADR-020): panel de assets con ruta configurable y recarga recursiva, texturas `egui` de PNG/JPEG, lienzo 2D con rejilla, zoom/paneo, selección múltiple, arrastre de assets, gizmos de escala/rotación y jerarquía reordenable o reparentable por arrastre.
- Comandos `MoveEntity` y `DuplicateEntity` en `lunar_editor`, validados y deshechos como los demás; `Ctrl+D` duplica la selección en la GUI.
- API C++ de primera clase en `lunar_bridge` (ADR-021): clase abstracta `Game`, bucle de ventana gestionado por Rust, servicios de input, escenas, imágenes, física 2D y viewport, y órdenes de dibujo `SpriteDrawCpp` que Rust convierte en `SpriteDraw`.
- Ejemplo `examples/cpp_platformer`: el plataformas canónico con toda la lógica de juego en C++ y los mismos assets y escena v1; modo `--headless` para verificación determinista.
- SDK C++ instalable en `sdk/cpp` (ADR-022): paquete CMake con headers públicos, header generado, staticlib, target importado `lunar::bridge` y versión exacta del workspace.
- Ejemplo `examples/cpp_sdk_template`: proyecto C++ externo que consume el SDK con `find_package` y verifica el reloj manual sin abrir ventana; CI lo compila y ejecuta en los tres sistemas operativos.
- Input de ratón en C++ (ADR-023): botones, posición del cursor, scroll por frame y conversión pantalla→NDC mediante structs POD nuevos; `Game` gana callbacks virtuales opcionales.
- Audio desde C++ (ADR-024): `load_sound`, `play_sound`, `stop_sound`, `stop_all_sounds`, `set_volume`, `set_master_volume`, `master_volume` e `is_sound_playing`, con handles opacos y rutas validadas dentro de `assets_dir`.

### Seguridad y compatibilidad

- Ni `default`, `dim2`, `dim3`, `assets`, `scenes` ni el build sin features incluyen `lunar_editor_ui`, `egui` ni `eframe`; CI verifica los árboles de dependencias.
- La GUI delega cada mutación a `EditorDocument::apply`; el núcleo `lunar_editor` no gana dependencias nuevas.
- La API de sprites no añade tipos `wgpu` públicos ni nuevas dependencias; `lunar_scenes` mantiene el esquema de versión 1.
- El pipeline de sprites vive en `lunar_render`, detrás de la feature `render`; los builds sin render siguen sin incluirlo.
- CI verifica que `sprite_platformer` arrastra solo `dim2`, `assets` y `scenes`, sin 3D, audio, bridge ni GUI.
- La GUI no usa `lunar_render` ni expone `wgpu`; `editor_ui` sigue fuera de `default` y `eframe` sin backend `glow`.
- No se añaden dependencias externas: `lunar_editor_ui` solo incorpora el crate interno `lunar_assets`, sin `rfd` ni diálogo nativo.
- El esquema JSON permanece en versión 1 y `Component::Image` no gana campos de animación; el editor previewa la imagen completa.
- La bridge C++ no expone `Renderer`, `Surface`, `ImageData`, escenas enteras ni Rapier por valor: solo handles, structs POD y órdenes de dibujo; `wgpu` sigue fuera de la API pública.
- `lunar_bridge` no añade features ni dependencias externas; `bridge` y `default` pasan a incluir servicios 2D, mientras `dim2`, `dim3` y el build sin features siguen sin arrastrar la bridge.
- El plataformas Rust se conserva como banco de pruebas sin `lunar_bridge`; CI verifica por separado el árbol de la bridge y ejecuta el juego C++ en modo headless en Linux, Windows y macOS.
- Los ejemplos C++ enlazan explícitamente en macOS los frameworks que la staticlib Rust no propaga (`AppKit`, `Metal`, `MetalKit`, `QuartzCore`, `Security`, etc.).
- El SDK usa `ExactVersion` porque antes de 1.0 cualquier cambio de versión puede romper la ABI C++; la plantilla externa no referencia rutas del workspace y CI lo verifica.
- El empaquetado no añade gestores de paquetes ni dependencias nuevas: solo CMake 3.20 y Cargo.
- H14 no cambia el layout de `ViewportInfo`, `FrameStats` ni `EntityInfo`; `winit` y los tipos de eventos siguen confinados en `lunar_platform` y los nuevos callbacks de `Game` tienen implementación por defecto.
- H15 solo añade un crate interno (`lunar_audio`) a la bridge: `rodio`, `cpal`, `SoundData` y los handles de `rodio` no cruzan la FFI; sin dispositivo `play_sound` devuelve `0` y los tests no dependen de hardware de audio. Los gates `dim2`, `dim3` y bare siguen sin arrastrar audio o bridge.

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
