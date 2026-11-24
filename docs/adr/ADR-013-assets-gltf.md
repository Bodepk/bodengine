# ADR-013: Assets glTF y crate `lunar_assets`

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

H7 necesita cargar escenas y mallas glTF 2.0 sin crear ventana, dispositivo GPU ni reloj. `gltf` está aprobado en ADR-001, pero aún no tiene versión fijada ni frontera definida. ADR-012 confinó `image` a `lunar_assets` y dejó JPEG para cuando glTF lo necesitara. ADR-008 todavía no fija cómo se referenciarán los assets en escenas serializadas.

## Decisión

- `gltf` queda fijado en **1.4.1** (MSRV 1.61), con `default-features = false` y las features `import`, `names` y `guess_mime_type`. Queda confinado a `lunar_assets`; ningún otro crate del motor lo menciona.
- La feature de `image` se amplía a **`png` + `jpeg`** para cubrir las texturas que decodifica glTF. No se añaden otros formatos.
- `lunar_assets` gana una feature opcional **`gltf`**. El crate raíz mantiene `assets` como feature image-only y añade **`gltf = ["assets", "lunar_assets/gltf"]`**, incluida en `default`. Así `--features assets` sigue sin arrastrar glTF.
- `GltfStore` entrega `GltfHandle(u64)` opaco, monótono por instancia. `load_from_slice` acepta GLB autocontenido; `load_from_path` usa `gltf::import` y resuelve buffers/imágenes externas relativas al archivo. No hay registro global ni caché ruta→handle: eso se decide en ADR-008 antes de H9.
- `GltfDocument` es una instantánea propia y completamente poseída: escenas, nodos (matriz 4×4), mallas, primitivas, materiales PBR, texturas, imágenes RGBA8 y datos de accessors. Ningún tipo de `gltf` ni de `image` aparece en la API pública.
- Los accessors se copian a bytes contiguos; se eliminan los `byte_stride` cuando están presentes. Los accessors sparse se rechazan explícitamente. Las imágenes se convierten a RGBA8 y se validan con los límites de ADR-012; los formatos de 16/32 bits se rechazan.
- Los errores son propios (`GltfLoadError`) y no exponen `gltf::Error`, `image::ImageError` ni `std::io::Error`.
- El crate sigue sin depender de `winit`, `wgpu`, `bevy_ecs`, `rapier` ni `cxx`. La bridge C++ continúa sin `AssetHandle` hasta que exista un consumidor real.
- H7 no decide conversores a GPU ni caché de texturas: la capa de render los añadirá cuando corresponda.

## Consecuencias

- `dim2`, `dim3` y `--no-default-features` no arrastran `gltf`, `gltf-json` ni `image`; se comprueba en CI con `cargo tree -e features`.
- `--features assets` incluye `image` pero excluye `gltf`; se comprueba como gate adicional.
- Las referencias a texturas dentro de `GltfDocument` son índices locales al propio documento, no handles globales ni rutas serializadas.
- Cambiar de versión de `gltf` no obliga a tocar render, física ni FFI.
