# ADR-026: Documentos de escena y recarga desde C++

- Estado: Aceptada
- Fecha: 2026-11-25
- Supersede parcialmente: el servicio `load_scene` de ADR-021, que trabajaba con un `Scene` suelto

## Contexto

La bridge C++ puede cargar escenas JSON v1 (`load_scene`) y leer entidades como POD, pero no puede guardar, saber si hay cambios sin guardar ni deshacer/rehacer. El núcleo `lunar_editor` ya posee `EditorDocument`, que valida cada mutación y mantiene historial y criterio de guardado, pero ADR-015 lo dejó fuera de la bridge y ADR-021 prohíbe cruzar tipos de editor por la FFI.

## Decisión

- `lunar_bridge` depende de `lunar_editor` de forma incondicional, sin features nuevas ni dependencias externas. `EditorDocument` sustituye al `Scene` suelto como estado de escena de la bridge; las lecturas existentes (`entity_count`, `entity`) pasan por `EditorDocument::scene()`.
- Las rutas de escena se resuelven con `safe_asset_path` dentro de `GameConfig.assets_dir`, igual que imágenes, sonidos y glTF. La ruta relativa y la marca de tiempo del último archivo leído o escrito se guardan en Rust.
- API C++ nueva, sin cambiar struct ni método existente:
  - `save_scene(path)` y `save_scene_as(path)` guardan el documento; `save_scene_current()` reutiliza la última ruta conocida. `scene_path()` devuelve la ruta relativa o una cadena vacía.
  - `scene_is_dirty`, `scene_can_undo`, `scene_can_redo`, `scene_undo` y `scene_redo` exponen el estado del documento como escalares.
  - `editor_create_entity(id, name)`, `editor_delete_entity(id)`, `editor_rename_entity(id, name)` y `editor_set_transform(id, ...)` pasan por `EditorDocument::apply`; una cadena de nombre vacía significa `None`. Ningún `EditorCommand`, `Scene` ni `Component` cruza la FFI.
  - `reload_scene()` vuelve a leer la ruta conocida y reemplaza el documento, incluso con cambios sin guardar (intención explícita). `poll_scene_reload()` es la recarga en caliente opcional: compara fecha, longitud y un hash FNV-1a del archivo, y solo recarga si cambió y el documento está limpio; no usa hilos ni watchers. El hash evita depender de la granularidad del reloj de archivos.
- `load_scene(path)` conserva su firma pero crea un `EditorDocument` con historial vacío, aplicando la normalización de rutas previa. Escribir una escena válida sigue siendo responsabilidad de `Scene::save_to_path` bajo el documento.
- La versión de esquema sigue siendo 1 en esta ADR (ADR-008): no se añaden campos ni se toleran desconocidos. ADR-027 eleva el esquema canónico a v2 y añade un convertidor explícito para v1; la bridge sigue sin exponer esos tipos. `Editor`, `lunar_editor_ui`, `egui`, `Renderer` y `wgpu` siguen sin aparecer en los headers instalados ni en el árbol de `lunar_bridge`.

## Consecuencias

- El tree gate de la bridge pasa a verificar `lunar_editor v` y a seguir prohibiendo `lunar_editor_ui`, `egui` y `eframe`. Los builds `dim2`, `dim3`, bare y `scenes` no arrastran la bridge ni el editor.
- Los tests usan archivos temporales dentro de `temp_dir()` y no dependen de GUI, reloj ni dispositivo: validan guardado, dirty, undo/redo, recarga y rechazo de rutas que salen de `assets_dir`.
- La plantilla C++ del SDK comprueba que los nuevos símbolos compilan y ejecutan sin abrir ventana; `ExactVersion` se mantiene hasta H20.
- La recarga en caliente es cooperativa: C++ decide cuándo sondear. Un documento sucio nunca se sobrescribe desde `poll_scene_reload()`.
