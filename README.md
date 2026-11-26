# Lunar

Motor híbrido 2D/3D en construcción: núcleo Rust, API pública C++ mediante `cxx` y editor como crate opcional.

> Estado actual: **H0 a H20 completados** y release `v0.2.0` disponible. El motor y sus hitos aprobados están implementados; además hay un editor GUI opcional con vista 2D, assets arrastrables y jerarquía reordenable (ADR-019/020), un plataformas 2D jugable con sprites (ADR-018), scripting C++ de primera clase con el plataformas portado a C++ (ADR-021), un SDK C++ instalable con `find_package` (ADR-022), input de ratón, scroll y conversión de viewport en C++ (ADR-023), audio desde C++ (ADR-024), servicios 3D en C++ (ADR-025), documentos de escena y recarga desde C++ (ADR-026), escenas v2 con sprites, animación y prefabs (ADR-027), diagnósticos con replay de input y hashes de estado (ADR-028) y la publicación `v0.2.0` (ADR-029). Las decisiones quedan registradas en ADR.

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
  lunar_render3d    Superficie CPU de mallas y órdenes 3D
  lunar_physics2d   Rapier 2D
  lunar_physics3d   Rapier 3D
  lunar_assets      Decodificación PNG/JPEG, glTF 2.0 y handles opacos
  lunar_audio       Reproducción WAV con rodio y handles opacos
  lunar_scenes      Escenas JSON versionadas y validación sin GPU
  lunar_editor      Núcleo de edición headless: comandos, selección y undo/redo
  lunar_editor_ui   GUI opcional con egui: vista 2D, assets, jerarquía e inspector
  lunar_bridge      Frontera cxx / API C++ con servicios 2D, 3D, audio, escenas y bucle para juegos C++
  lunar_runtime     Composición de todos los subsistemas
examples/minimal_game  Smoke 800x600 con render del H2
examples/cpp_smoke     Smoke C++ de la bridge cxx (CMake)
examples/cpp_platformer  Plataformas 2D canónico: lógica en C++, motor en Rust
examples/cpp_sdk_template  Proyecto C++ externo que consume el SDK instalado
examples/scene_demo    Demo jugable: JSON, glTF, PNG, WAV y editor headless
examples/sprite_platformer  Plataformas 2D: sprites, física Rapier y cámara
docs/adr           Decisiones arquitectónicas
tests/fixtures     Datos de prueba sin GPU
```

## Features

`default` activa `dim2`, `dim3`, `assets`, `gltf`, `audio`, `scenes`, `bridge` y `runtime`; el editor queda fuera por decisión de ADR-007/015 y se activa con `--features editor`. La interfaz gráfica es aún más opcional: `--features editor_ui` implica `editor` y añade `lunar_editor_ui` con `egui`/`eframe`. `gltf` implica `assets`, pero `assets` no implica `gltf`. La bridge C++ incorpora los servicios 2D (física, assets, escenas y render) sin feature nueva, así que `bridge` y `default` los arrastran; `dim2`, `dim3` y el build sin features siguen sin incluir la bridge. Los builds parciales:

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

La versión canónica de ese plataformas es la de **C++** (ADR-021): `examples/cpp_platformer` enlaza la staticlib de `lunar_bridge`, hereda de la clase `Game` y concentra en C++ todo el estado, el input, la animación, el AABB de manzanas/meta y la cámara. Rust conserva la ventana, el reloj de 60 Hz, Rapier, la decodificación PNG, el parseo JSON y `Renderer`: C++ solo emite órdenes `SpriteDrawCpp`, sin ver `wgpu`. El ejecutable Rust `sprite_platformer` se mantiene como banco de pruebas de la API Rust.

```bash
cmake -S examples/cpp_platformer -B build/cpp_platformer
cmake --build build/cpp_platformer --config Release
build/cpp_platformer/Release/cpp_platformer.exe            # Windows; abre ventana
build/cpp_platformer/Release/cpp_platformer.exe --headless # verificación determinista
# build/cpp_platformer/cpp_platformer                      # Linux/macOS
```

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

El SDK C++ (ADR-022) se instala como un paquete CMake con `lunar::bridge`, headers y la staticlib. Un proyecto externo solo necesita `find_package`; la plantilla `examples/cpp_sdk_template` comprueba el reloj manual sin abrir ventana:

```bash
cmake -S sdk/cpp -B build/cpp_sdk -DCMAKE_INSTALL_PREFIX="$PWD/build/sdk-prefix"
cmake --build build/cpp_sdk --config Release
cmake --install build/cpp_sdk --config Release
cmake -S examples/cpp_sdk_template -B build/cpp_sdk_template -DCMAKE_PREFIX_PATH="$PWD/build/sdk-prefix"
cmake --build build/cpp_sdk_template --config Release
build/cpp_sdk_template/Release/cpp_sdk_template.exe  # Windows
# build/cpp_sdk_template/cpp_sdk_template            # Linux/macOS
```

La versión del paquete es `0.2.0` y se solicita con `EXACT`: antes de 1.0, un cambio de versión puede romper la ABI C++. La configuración propaga las librerías del sistema que la staticlib Rust no declara por sí sola.

La API C++ también expone ratón, scroll y coordenadas de viewport (ADR-023): `mouse_state`, `cursor_position`, `cursor_ndc`, `screen_to_ndc` y `scroll_delta` son aditivos; `Game` ofrece `on_mouse`, `on_cursor` y `on_scroll` como virtuales opcionales. `winit` sigue confinado en `lunar_platform` y los píxeles físicos se convierten a NDC en Rust.

El audio desde C++ (ADR-024) usa handles opacos `u64`: `load_sound`, `play_sound`, `stop_sound`, `stop_all_sounds`, `set_volume`, `set_master_volume`, `master_volume` e `is_sound_playing`. Las rutas se resuelven dentro de `assets_dir` y `rodio` no aparece en los headers instalados; sin dispositivo `play_sound` devuelve `0` y el juego continúa.

Los servicios 3D desde C++ (ADR-025) añaden física `rapier3d` (`set_gravity3d`, `add_body3d`, `add_collider3d`, `body_state3d`, `apply_impulse3d`, `cast_ray3d`, `step_physics3d`), mallas glTF (`load_gltf`, `define_mesh`, `mesh_vertex_count`) y órdenes validadas (`set_camera3d`, `view_projection3d`, `submit_mesh_draws`, `mesh_draw`). `lunar_render3d` es una superficie CPU sin `wgpu`: los handles son opacos, `MeshDrawCpp` es POD y la presentación GPU de mallas queda para una ADR posterior. `dim3` sigue sin arrastrar assets ni la bridge; la conversión glTF→malla solo se activa en `lunar_bridge`.

Los documentos de escena desde C++ (ADR-026) sustituyen al `Scene` suelto por un `EditorDocument` dentro de Rust: `save_scene`, `save_scene_as`, `save_scene_current`, `scene_path`, `scene_is_dirty`, `scene_can_undo`, `scene_can_redo`, `scene_undo`, `scene_redo` y los comandos `editor_create_entity`, `editor_delete_entity`, `editor_rename_entity` y `editor_set_transform`. `reload_scene()` fuerza la lectura del archivo; `poll_scene_reload()` es la recarga en caliente cooperativa y solo actúa si el archivo cambió (fecha, longitud y hash) y no hay cambios locales. Ningún `EditorCommand`, `Scene`, `Renderer` ni `wgpu` cruza la FFI.

Las escenas v2 (ADR-027) conservan `Component::Image` intacto y añaden `Component::Sprite` con `SpriteMeta` y `AnimationClip`, además de `prefabs` y `Component::PrefabRef`. `Scene::from_json` lee documentos v2 de forma nativa y migra v1 solo con el conversor explícito `v1::upgrade`: cada imagen pasa a sprite de una celda sin clips. `Scene::instantiate_prefab` devuelve entidades con ids remapeados; la validación comprueba celdas, clips y ciclos de prefabs. No se añaden campos a `Image`, ni tipos de animación a la FFI, ni dependencias nuevas.

Los diagnósticos (ADR-028) añaden `FrameMetricsCpp` con coste de CPU/render y número de órdenes, un log estructurado acotado (`log_event`, `log_count`, `log_entry`) y replay determinista de input (`begin_replay_recording`, `load_replay_frames`, `start_replay_playback`). `state_hash()` combina contadores, input, física 2D/3D y el JSON canónico de la escena con FNV-1a; los tiempos de CPU no entran en la huella. `FrameStats` sigue intacto y no hay dependencias nuevas.

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
| H12 | Scripting C++ y plataformas portado a C++ (ADR-021) | ✅ |
| H13 | SDK C++ instalable con `find_package` (ADR-022) | ✅ |
| H14 | Input de ratón, scroll y viewport en C++ (ADR-023) | ✅ |
| H15 | Audio desde C++ con handles opacos (ADR-024) | ✅ |
| H16 | Servicios 3D desde C++ y órdenes de mallas (ADR-025) | ✅ |
| H17 | Documentos de escena y recarga desde C++ (ADR-026) | ✅ |
| H18 | Escenas v2 con sprites, animación y prefabs (ADR-027) | ✅ |
| H19 | Diagnósticos, replay de input y hashes de estado (ADR-028) | ✅ |

Las decisiones y sus consecuencias están en [`docs/adr`](docs/adr/ADR-000-index.md). El historial de versiones está en [`CHANGELOG.md`](CHANGELOG.md).
