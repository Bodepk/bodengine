# ADR-018: Juego de plataformas con sprites

- Estado: Aceptada
- Fecha: 2026-11-24
- Nota: el apartado «Fuera de alcance» que excluía la bridge C++ queda supersedido por ADR-021.

## Contexto

Después de `v0.1.0` y del editor gráfico (ADR-019) queda pendiente una demo realmente jugable con sprites. El usuario aportó `C:\Users\bode9\Documents\Assets\Pixel Adventure 1.zip`, eligió un plataformas 2D, eligió al personaje `Ninja Frog` y pidió que la decisión se documentara antes de implementar.

Solo se han inspeccionado rutas, nombres de archivo y dimensiones leídas del encabezado PNG; no se ha analizado el contenido artístico. La página oficial del paquete (<https://pixelfrog-assets.itch.io/pixel-adventure-1>) declara licencia Creative Commons Zero (CC0), sin obligación de atribución. Aun así se registrará la procedencia en el ejemplo.

El renderer actual solo dibuja un triángulo (ADR-017) y `lunar_render2d` sigue siendo un placeholder. Para mostrar sprites reales hace falta un camino de texturas que no exponga `wgpu` en la API pública y que no cambie la plataforma.

## Decisión

- Se crea el example **`examples/sprite_platformer`**, miembro del workspace, ejecutable con `cargo run -p sprite_platformer`. Depende de `lunar` con `dim2`, `assets` y `scenes`. No se añade ninguna feature nueva a la fachada ni ninguna dependencia nueva al stack aprobado.
- Se copian del ZIP solo los archivos necesarios a `examples/sprite_platformer/assets/sprites/`, con nombres descriptivos. `examples/sprite_platformer/assets/ASSETS.md` registra origen, licencia CC0 y nombres originales:

  | Archivo original en el ZIP | Copia local | Dimensiones / fotogramas |
  | --- | --- | --- |
  | `Free/Main Characters/Ninja Frog/Idle (32x32).png` | `player_ninja_frog_idle.png` | 352×32, 11 fotogramas de 32×32 |
  | `Free/Main Characters/Ninja Frog/Run (32x32).png` | `player_ninja_frog_run.png` | 384×32, 12 fotogramas de 32×32 |
  | `Free/Main Characters/Ninja Frog/Jump (32x32).png` | `player_ninja_frog_jump.png` | 32×32, 1 fotograma |
  | `Free/Main Characters/Ninja Frog/Fall (32x32).png` | `player_ninja_frog_fall.png` | 32×32, 1 fotograma |
  | `Free/Terrain/Terrain (16x16).png` | `terrain_tiles.png` | 352×176, teselas de 16×16 |
  | `Free/Traps/Platforms/Brown On (32x8).png` | `platform_brown.png` | 256×8, 8 fotogramas de 32×8 |
  | `Free/Items/Checkpoints/End/End (Idle).png` | `goal_end.png` | 64×64, 1 fotograma |
  | `Free/Items/Fruits/Apple.png` | `collectible_apple.png` | 544×32, 17 fotogramas de 32×32 |
  | `Free/Items/Boxes/Box1/Idle.png` | `crate_idle.png` | 28×24, 1 fotograma |
  | `Free/Background/Blue.png` | `background_blue.png` | 64×64, 1 fotograma |

- `lunar_render` gana un pipeline mínimo de quads texturizados junto al triángulo de ADR-017. La API pública usa solo tipos propios: `SpriteId` opaco, `SpriteDraw { sprite, frame, flip_x, center, size }` en coordenadas NDC y `SpriteError`. `Renderer::define_sprite(rgba, width, height, frame_size)` crea la textura; `Renderer::set_sprites(&[SpriteDraw])` sustituye la lista del frame; `Renderer::clear_sprites()` la vacía. No se exponen tipos `wgpu` nuevos.
- La textura se crea como `Rgba8UnormSrgb` con sampler `Nearest` —los assets son pixel art— y blending alfa estándar. La imagen ya decodificada por `lunar_assets::ImageStore` se sube como RGBA8; el example calcula las UV del fotograma solicitado. Los fotogramas se numeran de izquierda a derecha y, en hojas 2D, de arriba abajo. `frame_size` vale las dimensiones completas para imágenes de un solo fotograma.
- La animación la decide el example, no el motor: selecciona el fotograma según velocidad horizontal, salto o caída. No hay componente de animación ni metadatos de atlas en el esquema de escenas.
- El nivel vive en `examples/sprite_platformer/assets/platformer.scene.json` y usa únicamente `Component::Image`, `Component::Collider2D` y `Transform` de ADR-008. Suelo y plataformas son `Fixed`; el jugador es `Dynamic`; meta y manzanas son entidades visuales sin cuerpo, detectadas por solapamiento AABB en el example. La escala es 32 píxeles = 1 unidad de mundo. No cambia el esquema JSON ni el editor.
- La física usa `lunar_physics2d` con el paso fijo de 60 Hz de ADR-005. Movimiento horizontal mediante velocidad/impulso y salto mediante impulso cuando un raycast corto hacia abajo (`cast_ray`) detecta suelo. No se añaden sensores ni tipos de cuerpo nuevos.
- La entrada reutiliza `Key`: `A`/`ArrowLeft` y `D`/`ArrowRight` mueven, `W`/`ArrowUp` salta y `Escape` sale. No se amplía `Key` ni `map_key`; no hay gamepad.
- La cámara es ortográfica y vive en el example: 9 unidades de alto, centrada en el jugador y limitada a los bordes del nivel. No hay cámara en `lunar_render` ni integración con el editor GUI.
- Fuera de alcance: audio, HUD de texto, pausa, checkpoints activos, guardado de partida, preview en `lunar_editor_ui`, bridge C++ y 3D. Si algún asset falta, el example termina con un error claro que indica la ruta esperada.

## Consecuencias

- No entran dependencias nuevas y los gates `dim2`, `dim3`, `assets`, `scenes`, `editor`, `editor_ui` y bare siguen siendo independientes: el renderer solo gana código detrás de la feature `render`, que ya implica `dim2`.
- `lunar_render2d` y `lunar_render3d` permanecen como placeholders; el pipeline de sprites se añade donde ya vive el dispositivo `wgpu`, sin crear un segundo renderer.
- `lunar_scenes` y `lunar_editor` no cambian: la escena del plataformas es otro consumidor de su formato existente.
- Los PNG se incluyen en el repositorio bajo CC0 con su nota de procedencia; no se necesita atribución legal, pero se mantiene por trazabilidad.
- Una futura vista previa con sprites dentro del editor requiere una ADR nueva, según ADR-019.
