# ADR-030: Inspector completo y edición de componentes

- Estado: Aceptada
- Fecha: 2026-11-26
- Supersede parcialmente: el punto «fuera de alcance: edición de fotogramas» de ADR-020 y el uso global de volumen/autoplay del inspector de ADR-020

## Contexto

ADR-027 elevó el esquema a versión 2 y añadió `Component::Sprite` con `SpriteMeta` y `AnimationClip`, además de `Component::PrefabRef` y plantillas en la escena. El inspector de `lunar_editor_ui`, sin embargo, solo sabía **añadir y borrar** componentes: no podía tocar la rejilla, los clips ni las referencias a prefabs. El volumen y el autoplay eran dos controles globales que no se guardaban en la escena, y la rotación se mostraba como un cuaternión crudo `(x, y, z, w)`, poco útil para un editor 2D.

El núcleo `lunar_editor` solo expone comandos para **sustituir** un componente completo (`AddComponent`) o quitarlo (`RemoveComponent`). Para editar un campo hace falta una operación aditiva que mantenga la única puerta de mutación `EditorDocument::apply`, con validación y undo/redo.

## Decisión

- Se añade el comando `EditorCommand::SetComponent { id, index, component }` en `lunar_editor`. Sustituye un componente por índice, pasa por `apply`, valida la escena resultante y es deshecho como cualquier otro comando. Un índice inexistente devuelve `EditorError::ComponentIndex` y no cambia el documento. No cambia el esquema JSON ni la versión 2.
- El inspector edita una **copia** del componente activo, guardada en `EditorApp` como `ComponentEdit { id, index, component }`, y confirma cada cambio con `SetComponent`. Las cadenas se confirman al perder el foco o pulsar Enter; los valores numéricos, al cambiar. Si la validación rechaza el valor, el documento queda intacto y el error se muestra en la barra de estado.
- Todos los componentes v2 son editables desde la GUI: ruta de asset de `Mesh`/`Image`/`Sprite`/`Audio`, `SpriteMeta` (celda, columnas, filas, fps y bucle), clips de animación (nombre, fotogramas como celdas numéricas, fps y bucle), forma y tipo de cuerpo de los colisionadores 2D/3D, y `PrefabRef` elegido entre los prefabs de la escena. No se interpreta ni reproduce la animación: solo se editan sus datos.
- El volumen y el autoplay **viven en cada componente `Audio`**; desaparecen los controles globales del inspector. Crear una entidad desde un asset de audio sigue usando valores iniciales fijos (`0.5`, `false`).
- La rotación se presenta en **grados** como tres ángulos Euler XYZ. Las funciones puras de conversión viven en `lunar_editor_ui`; el esquema sigue guardando el cuaternión `(x, y, z, w)` de ADR-008/020. Una rotación solo en Z produce exactamente el cuaternión canónico `(0, 0, sin(grados/2), cos(grados/2))` que ya usa el lienzo.
- No hay dependencias nuevas. La GUI sigue siendo opcional, fuera de `default`, sin `rfd`, sin `glow`, sin `serde_json` y sin exponer `lunar_render` ni `wgpu`.

## Consecuencias

- ADR-020 queda enmendada en dos puntos: la edición de fotogramas deja de estar fuera de alcance como datos serializados (sigue sin haber timeline ni reproducción), y volumen/autoplay dejan de ser globales.
- `lunar_editor` gana un comando con tests de índice inválido y undo; `lunar_editor_ui` gana un módulo de edición de componentes y helpers de conversión con tests puros, sin ventana ni GPU.
- La escena abierta puede tener sprites migrados de v1: guardar sigue emitiendo v2, y el inspector puede editar sus metadatos sin cambiar el convertidor.
- CI mantiene todos los gates: `fmt`, `clippy -D warnings`, tests del workspace, árboles de features y build con todas las features.
