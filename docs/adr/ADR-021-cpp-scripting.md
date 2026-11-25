# ADR-021: Scripting C++ de primera clase

- Estado: Aceptada
- Fecha: 2026-11-24
- Supersede parcialmente: ADR-018 (el apartado «Fuera de alcance: bridge C++» deja de ser válido)

## Contexto

El producto es un motor parecido a Unity/Unreal escrito en Rust, con la lógica de los juegos en C++. El plataformas de ADR-018 y los demás ejemplos son bancos de pruebas, no el resultado final. Hoy `lunar_bridge` solo expone `EngineConfig`, `FrameStats` y un `Engine` opaco con un reloj manual: no hay forma de escribir la lógica de un juego en C++ ni de pedirle al render que dibuje sprites.

El usuario eligió tres restricciones: C++ se **compila y enlaza estáticamente** con el ejecutable del juego; el primer hito entrega una **API utilizable y porta el plataformas** para que deje de estar en Rust; y **Rust dibuja, C++ ordena**, de modo que `wgpu` y `Renderer` nunca cruzan la frontera.

La bridge existente (ADR-004) ya fijó `cxx` 1.0.205, C++17, structs POD compartidos, `rust::String` en structs compartidos, handles opacos y CMake leyendo `OUT_DIR` del JSON de Cargo. ADR-017 fija `on_render` como punto de extensión justo antes de presentar. ADR-018 fija `SpriteId`/`SpriteDraw` en NDC. ADR-012 prohíbe cruzar `ImageData`; ADR-008 mantiene el JSON en versión 1. Ninguna de esas decisiones se cambia aquí: se amplía la superficie C++ apoyándose en ellas.

## Decisión

### Modelo de enlace

- El juego C++ es un ejecutable CMake que enlaza la **staticlib** de `lunar_bridge`, construida por Cargo. Se reutiliza el patrón de `examples/cpp_smoke`: `find_program(cargo)`, `execute_process(... --message-format=json)` en configure para localizar `OUT_DIR/cxxbridge/include`, `add_custom_target` que compila la staticlib y las librerías del sistema por plataforma. No se copia código generado al árbol de fuentes.
- `lunar_bridge` aporta un header propio y versionado, `include/lunar_bridge/game.h`, con una clase abstracta `Game` (`on_key`, `on_resize`, `on_frame`, `on_render`). El juego C++ hereda de ella. En la bridge se declara `type Game` en `unsafe extern "C++"` y `cxx` genera las llamadas virtuales. El bucle de ventana sigue en Rust (`lunar_platform::run`), que recibe `run(config, engine: &mut Engine, game: Pin<&mut Game>)`; `cxx` exige el `Pin` para una referencia mutable a un tipo opaco C++, pero la firma que ve C++ sigue siendo `Game &`.
- El bucle llama, en orden: `on_key` desde el evento de `winit`; `engine.tick_system()` antes de `on_frame()`; `on_frame()`; `on_render()`; presentación. `tick_system()` guarda una copia del `InputState` **antes** de `Engine::update` —que borra `just_pressed`— para que `was_key_pressed` siga siendo cierto durante `on_frame`, igual que en ADR-018. El reloj de paso fijo (60 Hz, `MAX_SUBSTEPS = 5`) y el momento de avanzarlo los decide Rust, nunca C++.

### Servicios C++

- `Engine` (opaco, `rust::Box<Engine>` en C++) conserva `new_engine`, getters y `tick(elapsed_ns)` del smoke y gana los servicios que necesita la lógica de juego, todos con structs POD compartidos o escalares:
  - **Entrada y frame**: `press_key(code, pressed)`, `is_key_down(code)`, `was_key_pressed(code)`, `substeps()`, `alpha()`, `tick_count()`, `frame_count()`. La enumeración `KeyCode` del header usa los mismos valores que `lunar_core::Key`; la conversión se hace en Rust.
  - **Escenas**: `load_scene(path)` lee y normaliza el JSON v1 con `lunar_scenes` y `entity_count()` / `entity(index)` devuelven un `EntityInfo` con id, nombre, transformación 2D, ruta de imagen y componente `Collider2D` (tipo de cuerpo y forma). El esquema no cambia y `Component::Image` no gana campos.
  - **Assets**: `GameConfig` incorpora `assets_dir`; `load_image(asset)` resuelve la ruta relativa dentro de esa carpeta con `lunar_assets` y devuelve un handle opaco (`0` = error). `define_sprite(image, frame_width, frame_height)` devuelve un handle de sprite; la textura GPU se crea diferidamente en el primer `on_render` posterior a `submit_sprites`, cuando Rust ya tiene `Renderer` y llama a `Renderer::define_sprite`. `ImageData`, `Renderer` y `wgpu` no cruzan la bridge.
  - **Física 2D**: `set_gravity(x, y)`, `add_body(kind, x, y)`, `add_collider(body, shape, half_x, half_y)`, `body_state(body)`, `apply_impulse(body, x, y)`, `cast_ray(origin, direction, max_toi)` y `step_physics(dt)` con `lunar_physics2d`. Los ids son `u64`; `BodyState` y `RayHit` son POD con un campo `hit`/`valid`, porque `Option` y `Result` no forman parte de esta ABI. Rapier sigue confinado en su crate.
  - **Viewport**: `viewport()` devuelve anchura y altura en píxeles; el host la actualiza en cada resize y en el primer frame.
- `submit_sprites(draws: &[SpriteDrawCpp])` recibe un `rust::Slice` de un struct compartido con `sprite`, `frame`, `flip_x`, `center_x`, `center_y`, `size_x`, `size_y`. Rust resuelve los handles a `SpriteId`, sube las texturas pendientes y llama a `Renderer::set_sprites`; ante cualquier error llama a `clear_sprites()` y devuelve `false`. La lista se limpia al empezar cada `on_render`. Las coordenadas siguen siendo NDC, como en ADR-018.

### Porte del plataformas

- Se crea `examples/cpp_platformer` con `CMakeLists.txt`, `game.hpp`/`game.cpp` y `main.cpp`. La única lógica de juego en el ejemplo pasa a C++: constantes, estado, movimiento, salto con raycast, substeps, animación, AABB de manzanas/meta, suavizado y clamp de cámara y construcción de las órdenes `SpriteDrawCpp`. Los PNG y el JSON se reutilizan de `examples/sprite_platformer/assets` sin copiarlos.
- Permanece en Rust: evento y presentación de `winit`, `Engine`/`FixedStepClock`, Rapier, decodificación PNG, parseo de escenas y `Renderer`. El ejemplo Rust `sprite_platformer` se conserva como banco de pruebas de la API Rust; el C++ pasa a ser el ejemplo canónico de lógica de juego.
- No se añaden dependencias externas: `lunar_bridge` solo incorpora crates internos del workspace (`lunar_math`, `lunar_platform`, `lunar_render`, `lunar_physics2d`, `lunar_assets`, `lunar_scenes`). `Game` no conoce `egui`, `eframe`, `rodio` ni 3D.

### Features y compatibilidad

- `lunar_bridge` no añade features: la API de juego incorpora de forma incondicional esos crates internos y activa `lunar_platform/render`, porque esta bridge es la API del juego y el primer hito es 2D. La fachada mantiene `bridge = ["dep:lunar_bridge"]` y `default` sin cambios, así que esas dos variantes pasan a arrastrar servicios 2D. El smoke sigue compilando y pasando con la misma staticlib.
- Los gates de `dim2`, `dim3`, bare, `assets`, `scenes` y GUI no se relajan: la bridge no entra en esos árboles salvo cuando se pide `bridge`/`default`. CI mantiene el gate del árbol `editor_ui` que prohíbe `lunar_render` y añade uno para el árbol de `lunar_bridge`: incluye render, física 2D, assets y escenas, pero no `egui`, `eframe` ni `lunar_editor_ui`.
- Se conserva el smoke C++ determinista del reloj manual y se amplía con tests Rust de conversiones `KeyCode`, handles y POD; el ejemplo C++ se compila y su modo `--headless` —que incluye `on_render` sin GPU— se ejecuta en los tres sistemas operativos de la matriz.

## Consecuencias

- ADR-018 sigue siendo la decisión del pipeline de sprites y del plataformas Rust; solo queda sin efecto su exclusión explícita de bridge C++. ADR-004 sigue siendo la decisión de la forma de la ABI; esta ADR la extiende con una clase abstracta C++ y una API de servicios.
- `crates/lunar_bridge` deja de ser un esqueleto y pasa a tener dependencias internas y tests; `lunar_runtime` sigue sin ser tocado porque la composición de subsistemas continúa en la bridge.
- Un juego C++ ya no puede acceder a `Renderer`, `Surface`, `ImageData`, `Scene` ni Rapier por valor: solo a handles, POD y órdenes de dibujo. Eso mantiene `wgpu` fuera de la API pública y facilita cambiar el backend de render sin recompilar la lógica de juego.
- El esquema JSON sigue en versión 1 y la animación sigue siendo decisión del juego, ahora C++. No se introduce audio en C++ en este hito; no hay plugins en runtime, hot reload, gamepad, HUD ni 3D en la bridge.
- Las builds con `bridge` y `default` pasan a incluir física 2D, assets, escenas y render; las variantes que excluyen `bridge` no cambian. CI verifica ambos árboles.
