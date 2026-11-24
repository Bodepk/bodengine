# ADR-012: Assets de imagen y crate `lunar_assets`

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

H6 necesita cargar imágenes sin crear ventana, dispositivo GPU ni reloj. `image` está aprobado en ADR-001, pero aún no existe un crate que lo confine. Además, ADR-004 pospuso `AssetHandle` en la bridge C++ hasta que existiera un subsistema de assets, y ADR-008 aún no fija cómo se referenciarán los assets en escenas serializadas.

## Decisión

- Se añade el crate **`lunar_assets`** al workspace, expuesto por la feature opcional `assets` del crate raíz (`default` la activa). `image` queda confinado a este crate; ningún otro crate del motor lo menciona.
- `image` queda fijado en **0.25.10** (MSRV 1.88), con `default-features = false` y la feature `png`. JPEG, WebP y el resto de formatos se incorporan cuando un hito lo necesite (H7 para texturas glTF).
- `ImageStore` posee las imágenes decodificadas. `load_from_bytes` y `load_from_path` devuelven un `ImageHandle(u64)` opaco, monótono por instancia del store. No hay registro global ni caché ruta→handle: ese formato se decide en ADR-008 antes de H9.
- `ImageData { width, height, pixels }` contiene RGBA8 en orden de filas. No es un tipo `#[repr(C)]` ni cruza la FFI; los conversores a texturas GPU vivirán en la capa de render cuando corresponda.
- Los errores son propios (`ImageLoadError`) y no exponen `image::ImageError` ni `std::io::Error` en la API pública.
- Se rechazan imágenes vacías o por encima de `MAX_DIMENSION`/`MAX_PIXELS` antes de copiar píxeles al store.
- El crate no depende de `winit`, `wgpu`, `bevy_ecs`, `rapier` ni `cxx`. Sus tests decodifican PNG generado en memoria y usan un archivo temporal, sin GPU.
- La bridge C++ sigue sin `AssetHandle`: igual que `WorldHandle` en H4, se añadirá cuando exista un consumidor C++ real, no solo por rellenar la ABI.

## Consecuencias

- El gate 2D/3D no se altera: `--features dim2`/`dim3` no arrastran `image` porque no activan `assets`.
- `--no-default-features` excluye `image`; el gate correspondiente se comprueba en CI con `cargo tree -e features`.
- La representación de handles en escenas (rutas, UUIDs o índices estables) sigue pendiente de ADR-008; H6 no la decide.
- Cambiar de versión de `image` o añadir formatos no obliga a tocar los otros crates. H7 activa `jpeg` junto a `png` para texturas glTF (ADR-013); WebP sigue sin activarse.
