# ADR-045: Historial de undo/redo con etiquetas

- Estado: Aceptada
- Fecha: 2026-11-27
- Relacionada: ADR-008 (comandos atómicos y undo/redo), ADR-019/020 (GUI opcional), ADR-041/042/043 (operaciones recientes)

## Contexto

`EditorDocument` guarda instantáneas para `undo`/`redo`, pero solo expone longitudes y booleanos: la GUI puede deshacer, no sabe qué va a deshacer ni puede saltar a un punto anterior del historial. Las barras de herramientas muestran botones genéricos y la consola registra mensajes de estado que se pierden al cerrar la ventana. Añadir etiquetas al documento y una ventana de historial permite inspeccionar y navegar el historial sin cambiar el significado de los comandos.

## Decisión

- `EditorCommand::label()` devuelve una etiqueta estática en español por variante, sin datos del comando. `EditorDocument` mantiene `undo_labels` y `redo_labels` en paralelo a las instantáneas: `apply` registra la etiqueta del comando, `undo` la mueve a la pila de rehacer y `redo` la devuelve. La longitud de cada pila de etiquetas es siempre igual a la de instantáneas.
- Se añaden los accesores de solo lectura `undo_labels()`, `redo_labels()` y `redo_len()`, y los métodos `undo_to(remaining)` / `redo_to(remaining)`, que deshacen o rehacen hasta dejar la pila correspondiente en esa longitud. No cambian la semántica de `apply`, `undo` ni `redo`.
- El módulo `history.rs` dibuja una ventana flotante «Historial» con la pila de deshacer (más reciente primero) y la de rehacer. Cada fila muestra su etiqueta; hacer clic salta a ese punto con `EditorApp::undo_to`/`redo_to`. Los botones «Deshacer» y «Rehacer» siguen disponibles en la barra de herramientas y ahora se abre el historial desde un botón propio.
- El historial no se persiste: es estado de la sesión, igual que la selección. Abrir o crear una escena sigue descartándolo. Las etiquetas no viajan a la bridge C++ ni entran en el JSON de escena.
- No hay dependencias nuevas ni cambios en el esquema v2. Los tests son headless.

## Consecuencias

- El usuario ve qué operación se va a deshacer o rehacer y puede saltar varias operaciones de una vez, conservando el historial de `scripts.map.txt` porque el salto se implementa repitiendo `EditorApp::undo`/`redo`.
- Las etiquetas son genéricas («Crear entidad», «Propagar plantilla»…): no identifican ids ni nombres, para mantenerlas estables y baratas.
- Cualquier comando nuevo debe añadir su etiqueta en `EditorCommand::label()`; el compilador lo exige al ser un `match` exhaustivo.
