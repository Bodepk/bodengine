# ADR-023: Input de ratón y coordenadas de viewport en C++

- Estado: Aceptada
- Fecha: 2026-11-25
- Amplía: ADR-004, ADR-006, ADR-021 y ADR-022

## Contexto

ADR-021 expuso a C++ el teclado, el viewport en píxeles y las órdenes de sprites en NDC, pero dejó fuera el ratón: `lunar_platform` ignora `CursorMoved`, `MouseWheel` y `MouseInput`, `lunar_core::InputState` solo conoce teclas y la clase `Game` solo recibe `on_key`. Para juegos y herramientas C++ reales faltan botones, scroll y una conversión explícita de píxeles a NDC.

La frontera de ADR-006 exige que `winit` siga confinado en `lunar_platform`; ADR-021 prohíbe cruces de `Renderer`, `wgpu` o `ImageData`, y ADR-022 fija que los headers son parte del contrato distribuido. Cualquier solución debe ser aditiva: no cambiar el layout de `ViewportInfo`, `FrameStats` ni `EntityInfo`, y no convertir métodos de `Game` en virtuales puros, porque hay juegos C++ y un SDK instalado que ya los implementan.

## Decisión

- `lunar_platform` traduce los tres eventos de ratón de winit dentro del único crate que depende de winit:
  - `map_mouse_button(MouseButton) -> u32` devuelve `0` para botones no reconocidos, `1` izquierdo, `2` derecho, `3` medio, `4` atrás y `5` adelante; `Other(u16)` se reporta como `0`.
  - `PlatformApp` gana `on_mouse_button(u32, bool)`, `on_cursor(f64, f64)` y `on_scroll(f32, f32, bool)` con implementación por defecto vacía, así que las apps actuales no se rompen.
  - `CursorMoved` entrega píxeles físicos relativos a la esquina superior izquierda, sin conversión DPI nueva. `MouseWheel` entrega dos componentes y un flag `pixels`: `true` para `PixelDelta`, `false` para `LineDelta`. Se descartan valores no finitos.
- `lunar_bridge::Engine` mantiene un estado de ratón privado con los cinco botones, transiciones `pressed`/`released`, posición del cursor y acumulación de scroll del frame. La instantánea de transiciones se toma antes de `Engine::update`, igual que la de teclado, para que `on_frame()` en C++ pueda consultarlas.
- La API C++ añade solo structs POD nuevos y métodos nuevos:
  - `MouseButtonState` con `down`, `pressed` y `released`;
  - `CursorPoint` con `x`, `y` y `valid`;
  - `ScrollDelta` con `x`, `y`, `pixels` y `valid`;
  - `mouse_state(button)`, `cursor_position()`, `cursor_ndc()`, `scroll_delta()` y `screen_to_ndc(x, y)` como consultas;
  - `press_mouse`, `set_cursor` y `add_scroll` para simulación determinista y pruebas headless.
- `cursor_ndc()` y `screen_to_ndc()` aplican `x / ancho * 2 - 1` y `1 - y / alto * 2` sobre el viewport vigente. Si el viewport es `0×0`, los valores no son finitos o los píxeles no son finitos, devuelven `valid = false` y componentes `0.0`. No se modifica `ViewportInfo`.
- `Game` gana `on_mouse`, `on_cursor` y `on_scroll` como virtuales con cuerpo vacío. El orden del frame sigue siendo el de ADR-021: eventos → `on_frame()` → `on_render()` → present; los callbacks de ratón ocurren en el momento del evento y los getters reflectan la instantánea del frame.
- `scroll_delta()` devuelve la acumulación del frame y se reinicia después de cada `tick`/`tick_system`; `pixels` conserva la unidad del último evento acumulado. No se introducen filtros ni `scale_factor`.
- No se añaden dependencias ni features. `winit`, `MouseButton`, `PhysicalPosition` y `MouseScrollDelta` no aparecen en headers instalados; la bridge solo publica escalares y structs POD.

## Consecuencias

- La ABI de los structs existentes no cambia; se añaden símbolos al target `lunar::bridge` y métodos virtuales por defecto a `Game`. Como ADR-022 usa `ExactVersion`, el SDK se regenera y se distribuye junto a la nueva versión del workspace en v0.2.0.
- El comportamiento es verificable sin GPU: tests de Round-trip de botones, transiciones que sobreviven a `tick_system()`, conversión pantalla→NDC con viewport `0×0` y eventos traducidos por `PlatformApp`.
- CI añade comprobaciones C++ en la plantilla del SDK para usar los nuevos getters sin implementar los nuevos callbacks; el plataformas C++ sigue siendo el ejemplo canónico y no necesita cambios obligatorios.
- Gamepad, HUD y texto siguen fuera de alcance; serán decisiones separadas.
