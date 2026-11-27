# ADR-036: Opciones de importación y reimportado en el editor

- Estado: Aceptada
- Fecha: 2026-11-26
- Relacionada: ADR-034 (GUIDs y meta-archivos), ADR-012/013/014 (assets)

## Contexto

El content browser ya recarga assets y el editor guarda GUIDs estables en ficheros `.meta` (ADR-034). Faltan dos capacidades para crear contenido real sin reiniciar el editor: conservar opciones de importación por asset y detectar cambios del fichero fuente en disco. `lunar_assets` expone cargas inmutables y no ofrece parámetros por textura, glTF o audio; añadirlos cambiaría la API pública del motor y está fuera de este hito.

## Decisión

- Las opciones viven **en el `.meta` de ADR-034**, como líneas `clave=valor` adicionales, con un prefijo por tipo (`texture.`, `gltf.`, `audio.`). El parser ignora claves desconocidas y valores inválidos, conservando el resto del fichero. No se añade JSON ni base de datos.
- Se definen estructuras internas del editor (`TextureImportOptions`, `GltfImportOptions`, `AudioImportOptions`) con valores por defecto deterministas, serialización/deserialización pura y tests de ida/vuelta. El `.meta` conserva `guid=` y cualquier otra clave ya presente.
- El editor aplica las opciones **solo en su vista previa**: filtro y wrap en las miniaturas `egui`, escala al crear una entidad desde un glTF. Los campos `materials`/`animations` y `channels`/`sample_rate` se guardan y se muestran, pero no alteran la API de `lunar_assets` ni el runtime; se documentan como opciones de metadatos pendientes de consumo por el motor.
- El reimportado usa **polling de `modified()`** sobre los ficheros conocidos, sin hilos ni crates nuevos. Un `ReimportWatcher` puro recibe la marca de tiempo actual por parámetro (`now: u64`, nanosegundos o ticks inyectables en tests), compara con la última muestra y devuelve la lista de rutas cambiadas; la capa de UI llama a `reload` y refleja el resultado en la barra de estado y en la consola.
- Las referencias rotas se detectan recorriendo los componentes con ruta de asset (`Image`, `Sprite`, `Audio`, `Mesh`) y comparando con el índice de GUIDs y los ficheros presentes. Se listan en la consola como avisos, sin mutar la escena ni borrar componentes.
- Toda mutación de escena derivada de un reimportado (por ejemplo, reescribir una ruta resuelta) sigue pasando por `EditorDocument::apply`, con un único comando atómico cuando haya varios cambios.

## Consecuencias

- No aparecen dependencias nuevas en `lunar_editor_ui`, `lunar_editor`, `lunar_assets` ni en el workspace.
- No cambia el esquema de escenas v2, el JSON ni la API pública de la bridge C++.
- El watcher es cooperativo: solo actúa mientras la app recibe frames; no hay acceso concurrente a la escena desde otro hilo.
- El hot reload cubre archivos de asset y sus opciones, no código C++ (ADR-033).
