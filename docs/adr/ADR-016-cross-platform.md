# ADR-016: Pulido multiplataforma H11

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

El CI ya ejecuta fmt/clippy/test/smoke en Linux, Windows y macOS, pero dos detalles rompen la portabilidad del contenido: ADR-008 exige `/` en rutas de assets y un autor en Windows puede escribir `\`, y `save_to_path` falla si el directorio destino no existe. Además, el workflow validaba árboles de dependencias parciales pero no compilaba esas variantes.

## Decisión

- H11 no añade dependencias ni cambia el esquema versionado de ADR-008.
- `lunar_scenes` añade `normalize_asset_path` y `Scene::normalize_asset_paths`: convierten `\` en `/` antes de validar. No se aceptan rutas absolutas ni `..`, y la versión sigue siendo `1`.
- `Scene::save_to_path` crea los directorios padre que falten, con manejo explícito de rutas sin padre. `EditorDocument::save_to_path` hereda el comportamiento sin lógica duplicada.
- El workflow mantiene la matriz Linux/Windows/macOS, añade `cargo check -p lunar --all-features` en cada sistema y compila las variantes `dim2`, `dim3`, bare, `scenes` y `editor` en el job de gates. El smoke C++ sigue ejecutándose en los tres sistemas.
- No se toca el manejo de eventos de `winit`, la superficie de `wgpu` ni la bridge C++: cualquier cambio de API o de dependencias requiere un ADR nuevo.

## Consecuencias

- Escenas escritas en Windows se pueden normalizar antes de guardar sin sorpresas al cargarlas en Linux o macOS.
- Guardar en una ruta nueva crea el directorio sin código externo.
- Los builds parciales y `--all-features` se comprueban en cada sistema, no solo por inspección del árbol de dependencias.
