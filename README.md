# Lunar

Motor híbrido 2D/3D en construcción: núcleo Rust, API pública C++ mediante `cxx` y editor como crate opcional.

> Estado actual: **H0 a H11 completados** y demo de `v0.1.0` disponible. El motor y sus hitos aprobados están implementados; además hay un editor GUI opcional con vista 2D, assets arrastrables y jerarquía reordenable (ADR-019/020) y un plataformas 2D jugable con sprites (ADR-018). Las decisiones quedan registradas en ADR.

## Principios

1. Claridad y corrección antes que optimización.
2. Toda la lógica testeable vive sin GPU; el render es una capa de presentación.
3. Cada hito termina con `cargo fmt --check`, `cargo clippy -- -D warnings` y `cargo test --workspace` en verde.
4. Un commit atómico por cambio, siguiendo Conventional Commits.
5. Sin dependencias fuera del stack aprobado sin justificación y aprobación (ver ADR-001).

## Stack fijado

| Uso | Crate | Versión |
| --- | --- | --- |
| Ventana y eventos | `winit` | 0.30.13 |
| ECS | `bevy_ecs` | 0.20.1 |
| Render | `wgpu` | 30.0.2 |
| Física 2D/3D | `rapier2d` / `rapier3d` | 0.36.1 |
| Assets de imagen | `image` | 0.25.10 (PNG y JPEG) |
| Assets glTF | `gltf` | 1.4.1 |
| Audio | `rodio` | 0.23.0 (playback + WAV) |
| Escenas | `serde` / `serde_json` | 1.0.229 / 1.0.154 |
| GUI del editor | `egui`, `eframe` | 0.37.0 |
| FFI C++ | `cxx`, `cxx-build` | 1.0.205 |

Toolchain fijada en `rust-toolchain.toml`: Rust 1.100.0.

## Estructura

```text
crates/
  lunar_math        Matemáticas POD, sin dependencias de GPU
  lunar_ecs         Fachada sobre bevy_ecs
  lunar_core        Reloj de paso fijo, input, estado del engine
  lunar_platform    winit confinado; crea la surface nativa de wgpu
  lunar_render      Render wgpu, viewport y presentación
  lunar_render2d    Render 2D (marcador de feature)
  lunar_render3d    Render 3D (marcador de feature)
  lunar_physics2d   Rapier 2D
  lunar_physics3d   Rapier 3D
  lunar_assets      Decodificación PNG/JPEG, glTF 2.0 y handles opacos
  lunar_audio       Reproducción WAV con rodio y handles opacos
  lunar_scenes      Escenas JSON versionadas y validación sin GPU
  lunar_editor      Núcleo de edición headless: comandos, selección y undo/redo
  lunar_editor_ui   GUI opcional con egui: vista 2D, assets, jerarquía e inspector
  lunar_bridge      Frontera cxx / API C++
  lunar_runtime     Composición de todos los subsistemas
examples/minimal_game  Smoke 800x600 con render del H2
examples/cpp_smoke     Smoke C++ de la bridge cxx (CMake)
examples/scene_demo    Demo jugable: JSON, glTF, PNG, WAV y editor headless
examples/sprite_platformer  Plataformas 2D: sprites, física Rapier y cámara
docs/adr           Decisiones arquitectónicas
tests/fixtures     Datos de prueba sin GPU
```

## Features

`default` activa `dim2`, `dim3`, `assets`, `gltf`, `audio`, `scenes`, `bridge` y `runtime`; el editor queda fuera por decisión de ADR-007/015 y se activa con `--features editor`. La interfaz gráfica es aún más opcional: `--features editor_ui` implica `editor` y añade `lunar_editor_ui` con `egui`/`eframe`. `gltf` implica `assets`, pero `assets` no implica `gltf`. Los builds parciales:

```bash
cargo build -p lunar --no-default-features --features dim2
cargo build -p lunar --no-default-features --features dim3
cargo build -p lunar --no-default-features
```

La última variante excluye también `wgpu`, `image`, `gltf`, `rodio`, `cpal` y `serde_json`: `lunar_platform/render` se activa desde la feature `render` del crate raíz, y `assets` no la activan `dim2` ni `dim3`.

El workflow de CI verifica con `cargo tree -e features` que los árboles 2D-only y 3D-only no arrastran crates de la otra dimensión ni assets, audio o escenas; que `assets` incluye `image` pero no `gltf`; que `gltf` incluye el loader fijado en 1.4.1; que `scenes` incluye `serde`/`serde_json` sin requerir GPU; que el editor no aparece salvo con `--features editor`; y que la GUI no aparece salvo con `--features editor_ui`. Además compila cada variante parcial y `--all-features` en Linux, Windows y macOS.

## Comandos

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p minimal_game
cargo run -p scene_demo
cargo run -p sprite_platformer
cargo run -p lunar_editor_ui                         # escena nueva; assets en examples/sprite_platformer/assets
cargo run -p lunar_editor_ui -- ruta/a/scene.json    # abre un JSON existente
```

El editor gráfico (ADR-019 y ADR-020) muestra una vista 2D con rejilla opcional, zoom anclado al puntero con la rueda, paneo con el botón central o derecho y selección múltiple con Shift/Ctrl+clic. El panel de assets escanea de forma recursiva la carpeta escrita en el campo de texto y se actualiza con «Recargar»: arrastra `.png`/`.jpg`/`.jpeg` al lienzo para crear entidades con su sprite; también se listan `.wav` y `.glb`/`.gltf`. Arrastra las entidades para moverlas; con una sola selección aparecen tiradores de escala uniforme y rotación. En la jerarquía, soltar sobre otra entidad cambia el padre, soltar entre filas reordena y soltar en el área raíz desaparenta; `Ctrl+D` duplica la selección.

No hay diálogo nativo de carpetas (`rfd` no está en el stack) ni render propio: el preview usa `egui::TextureHandle` y la GUI no usa `lunar_render` ni expone `wgpu`. Toda mutación pasa por `EditorDocument::apply`, con deshacer/rehacer y guardado JSON v1. Atajos: `Ctrl+S`, `Ctrl+Z`, `Ctrl+Shift+Z`/`Ctrl+Y`, `Ctrl+D` y `Supr`.

El plataformas (ADR-018) abre una ventana 960×540 y consume `examples/sprite_platformer/assets/platformer.scene.json`. `A`/`D` o las flechas mueven, `W`/`ArrowUp` salta y `Escape` sale; recoger manzanas y llegar a la meta se informa por stdout. Usa sprites CC0 de Pixel Adventure 1, con procedencia en `assets/ASSETS.md`. `lunar_render` dibuja hojas de sprites mediante `SpriteId`/`SpriteDraw` sin exponer tipos `wgpu` nuevos.

La demo abre una ventana 800×600 y consume `examples/scene_demo/assets/demo.scene.json`: lee el glTF, toma el color del PNG, reproduce el WAV con el volumen de la escena y muestra el editor headless. WASD mueve el triángulo, `W` reproduce el sonido, `S` lo detiene, `↑` renombra la entidad, `↓` deshace, `←` rehace y `Esc` cierra. Para regenerar los assets sintéticos:

```bash
python examples/scene_demo/tools/generate_assets.py
```

El smoke de la API pública C++ se construye con CMake; genera la staticlib Rust y
localiza el header de `cxx` desde el `OUT_DIR` de Cargo:

```bash
cmake -S examples/cpp_smoke -B build/cpp_smoke
cmake --build build/cpp_smoke --config Release
build/cpp_smoke/Release/cpp_smoke.exe   # Windows
# build/cpp_smoke/cpp_smoke             # Linux/macOS
```

## Hitos

| Hito | Contenido | Estado |
| --- | --- | --- |
| H0 | Workspace, ADR, CI, toolchain | ✅ |
| H1 | Math, ECS, bucle 60 Hz, winit, smoke | ✅ |
| H2 | wgpu y resize a 0×0 | ✅ |
| H3 | FFI cxx y ejemplo C++ | ✅ |
| H4 | Física 2D (`rapier2d`) | ✅ |
| H5 | Física 3D (`rapier3d`) | ✅ |
| H6 | Assets de imagen (`image`, PNG) | ✅ |
| H7 | Assets glTF (`gltf`, mallas, materiales, texturas) | ✅ |
| H8 | Audio (`rodio`, WAV, reproductor sin dispositivo en tests) | ✅ |
| H9 | Escenas JSON versionadas (`serde`/`serde_json`, jerarquía validada) | ✅ |
| H10 | Editor headless (`lunar_editor`, comandos y undo/redo) | ✅ |
| H11 | Pulido multiplataforma (rutas, guardado y gates por SO) | ✅ |

Las decisiones y sus consecuencias están en [`docs/adr`](docs/adr/ADR-000-index.md). El historial de versiones está en [`CHANGELOG.md`](CHANGELOG.md).
