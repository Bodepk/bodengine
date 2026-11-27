# ADR-046: Validación de escena en el editor

- Estado: Aceptada
- Fecha: 2026-11-27
- Relacionada: ADR-008 (esquema v2), ADR-013/014 (assets), ADR-034 (GUIDs), ADR-036 (opciones de importación), ADR-040 (diagnósticos de scripts)

## Contexto

El editor valida la escena al aplicar comandos y al cargar JSON, pero no muestra un resumen legible de problemas que pueden aparecer fuera de ese camino: assets movidos o borrados, referencias a plantillas eliminadas, fotogramas fuera de la hoja, enlaces de `scripts.map.txt` a entidades inexistentes y diagnósticos escritos por el juego C++ en `scripts.diagnostics.txt`. Hoy esos síntomas se reparten entre la barra de estado y la consola, sin vista conjunta.

## Decisión

- Se añade `validation.rs` con una función pura `validate(scene, entries, guids, scripts, diagnostics_text) -> Vec<Issue>`. `Issue` tiene severidad (`Error`/`Warn`) y mensaje, y no guarda ids de GUI. La función no toca el disco ni muta la escena; los textos laterales se leen en `EditorApp` y se pasan como argumento.
- Comprobaciones: ids duplicados dentro de la escena o dentro de cada plantilla (defensivo, además de `Scene::validate`); `PrefabRef` a plantillas inexistentes; rutas de `Mesh`/`Image`/`Sprite`/`Audio` que no resuelven ni por ruta ni por GUID/alias; clips de sprite con celdas fuera de `SpriteMeta`; enlaces de scripts a entidades borradas; y líneas de `scripts.diagnostics.txt` parseadas con `scripts::diagnostics`.
- La ventana «Validación de escena» se abre desde la barra de herramientas y tiene un botón «Comprobar» que recalcula la lista y la guarda en memoria. Cada fila muestra severidad y mensaje; la cabecera indica cuántos problemas hay. No hay filtros, ni autoreparación, ni escritura en la escena: cada problema sigue resolviéndose con comandos normales y su undo correspondiente.
- El informe no se persiste ni viaja a la bridge C++; es estado de la sesión, como el historial. No se añaden dependencias ni campos al esquema v2.
- Junto con este hito se actualizan `README.md` y `CHANGELOG.md` para que la tabla de hitos y la lista de funcionalidades del editor reflejen E42–E46 y ADR-042…046.

## Consecuencias

- Un solo panel concentra los problemas de assets, plantillas, sprites y scripts, con la misma terminología que los comandos del editor.
- La validación es instantánea y determinista: ante la misma escena y los mismos ficheros laterales produce el mismo informe.
- No se detectan problemas que exigen ejecutar el juego C++ (colisiones, físicas o rendimiento); esos siguen siendo diagnósticos del runtime.
